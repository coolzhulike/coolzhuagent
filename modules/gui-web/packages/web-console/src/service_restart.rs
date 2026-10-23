//! 重启需要用户级随机凭据、当前 PID 和空闲工程；接受后关闭新任务入口。
use super::*;
static TOKEN:OnceLock<String>=OnceLock::new();
fn shutdown_state() -> &'static tokio::sync::watch::Sender<bool> {
    static STATE: OnceLock<tokio::sync::watch::Sender<bool>> = OnceLock::new();
    STATE.get_or_init(|| tokio::sync::watch::channel(false).0)
}
pub(super) fn begin_shutdown() {
    // 持久状态覆盖已经订阅和刚开始订阅的连接，不能复用只唤醒一个等待者的 Notify。
    shutdown_state().send_replace(true);
}
pub(super) fn until_shutdown<S: futures_core::Stream>(stream: S) -> impl futures_core::Stream<Item = S::Item> {
    close_on_signal(stream, shutdown_state().subscribe())
}
fn close_on_signal<S: futures_core::Stream>(stream: S, mut signal: tokio::sync::watch::Receiver<bool>) -> impl futures_core::Stream<Item = S::Item> {
    futures_util::StreamExt::take_until(stream, async move {
        loop {
            let stopping = *signal.borrow_and_update();
            if stopping || signal.changed().await.is_err() { break; }
        }
    })
}
pub(super) struct Registration(PathBuf);
impl Drop for Registration {fn drop(&mut self){let _=std::fs::remove_file(&self.0);}}
pub(super) fn register()->Result<Registration,String> {
    let root=workspace_projects::state_root()?;std::fs::create_dir_all(&root).map_err(|e|e.to_string())?;
    let token=random_hex_identifier(32,"服务重启标识")?;
    let path=app_launcher::instance_restart::token_path(&root,std::process::id());
    #[cfg(unix)] {use std::os::unix::fs::OpenOptionsExt;use std::io::Write;
        let mut file=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&path).map_err(|e|e.to_string())?;
        file.write_all(json!({"token":token}).to_string().as_bytes()).map_err(|e|e.to_string())?;
    }
    #[cfg(not(unix))] std::fs::write(&path,json!({"token":token}).to_string()).map_err(|e|e.to_string())?;
    TOKEN.set(token).map_err(|_|"服务重复登记")?;Ok(Registration(path))
}
#[derive(Deserialize)]pub(super) struct Request{pid:u32,token:String}
pub(super) async fn restart(Json(request):Json<Request>)->ApiResult<Json<JsonValue>> {
    if request.pid!=std::process::id()||TOKEN.get()!=Some(&request.token){return Err(api_error(StatusCode::FORBIDDEN,"服务重启身份不匹配"));}
    let change=workspace_activity::begin_workspace_change().map_err(|e|api_error(StatusCode::CONFLICT,&e))?;
    let c=open_session_connection(&default_session_sqlite_path()).map_err(sqlite_api_error)?;
    let active:bool=c.query_row("SELECT EXISTS(SELECT 1 FROM runtime_runs WHERE state IN ('accepted','running','stop_requested')) OR EXISTS(SELECT 1 FROM computer_use_runs WHERE state NOT IN ('succeeded','failed','blocked','cancelled','timed_out'))",[],|r|r.get(0)).map_err(sqlite_api_error)?;
    if active{return Err(api_error(StatusCode::CONFLICT,"仍有运行中的任务，请完成或停止后重启"));}
    // 让确认响应先送达，随后优雅退出；不允许退出窗口内重新接收任务。
    std::mem::forget(change);
    tokio::spawn(async{tokio::time::sleep(Duration::from_millis(80)).await;web_console_shutdown_signal().notify_one();});
    Ok(Json(json!({"accepted":true,"pid":std::process::id()})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn 退出会关闭全部空闲实时连接并让服务器排空() {
        let (signal, receiver) = tokio::sync::watch::channel(false);
        let router = Router::new().route("/events", get(move || {
            let receiver = receiver.clone();
            async move {
                let stream = async_stream::stream! {
                    yield Ok::<Event, std::convert::Infallible>(Event::default().data("ready"));
                    std::future::pending::<()>().await;
                };
                Sse::new(close_on_signal(stream, receiver))
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/events", listener.local_addr().unwrap());
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).with_graceful_shutdown(async { let _ = stopped.await; }).await.unwrap();
        });
        let client = reqwest::Client::new();
        let mut first = client.get(&url).send().await.unwrap();
        let mut second = client.get(&url).send().await.unwrap();
        assert!(first.chunk().await.unwrap().is_some());
        assert!(second.chunk().await.unwrap().is_some());
        signal.send_replace(true);
        stop.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(3), server).await.expect("实时连接不应阻止优雅退出").unwrap();
        assert!(first.chunk().await.unwrap().is_none());
        assert!(second.chunk().await.unwrap().is_none());
    }
    #[tokio::test]
    async fn 退出后的新订阅立即结束() {
        use futures_util::StreamExt;
        let (_signal, receiver) = tokio::sync::watch::channel(true);
        let stream = close_on_signal(futures_util::stream::pending::<()>(), receiver);
        futures_util::pin_mut!(stream);
        assert!(tokio::time::timeout(Duration::from_secs(1), stream.next()).await.unwrap().is_none());
    }
}
