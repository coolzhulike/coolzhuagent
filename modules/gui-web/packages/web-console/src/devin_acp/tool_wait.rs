//! 长程工具的等待状态由桥持有；HTTP 等待结束不销毁执行 future。
//! 不另起脱管任务，桥关闭时丢弃 future，沿用原执行器的取消和释放收尾。
use crate::*;
use std::{future::Future, pin::Pin, task::Poll};

pub(super) struct PendingTool {
    pub id: String,
    pub request_id: JsonValue,
    pub input: JsonValue,
    future: Option<Pin<Box<dyn Future<Output = Result<JsonValue, String>> + Send>>>,
    result: Option<Result<JsonValue, String>>,
}

impl PendingTool {
    pub fn new(id: String, request_id: JsonValue, input: JsonValue,
        future: impl Future<Output = Result<JsonValue, String>> + Send + 'static) -> Self {
        Self { id, request_id, input, future: Some(Box::pin(future)), result: None }
    }
    pub fn unfinished(&self) -> bool { self.result.is_none() }
    fn poll(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<JsonValue, String>> {
        if let Some(result) = &self.result { return Poll::Ready(result.clone()); }
        match self.future.as_mut().expect("等待工具必须持有执行 future").as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                self.future = None;
                self.result = Some(result.clone());
                Poll::Ready(result)
            }
        }
    }
}

pub(super) async fn wait(slot: &std::sync::Mutex<Option<PendingTool>>, id: &str, limit: Duration)
    -> Result<JsonValue, String> {
    let pending = std::future::poll_fn(|cx| {
        let mut guard = match slot.lock() { Ok(guard) => guard, Err(_) => return Poll::Ready(Err("工具等待状态不可用。".into())) };
        let Some(job) = guard.as_mut().filter(|job| job.id == id) else {
            return Poll::Ready(Err("该工具句柄不属于当前会话轮次。".into()));
        };
        job.poll(cx)
    });
    match tokio::time::timeout(limit, pending).await {
        Ok(result) => result,
        Err(_) => Ok(json!({"content":[{"type":"text","text":json!({
            "status":"running","job_id":id,"goal_achieved":false,
            "next_tool":"computer_use_wait","message":"任务仍在执行；调用 computer_use_wait 等待同一 job_id。不要重新调用 computer_use_perform，也不要结束本轮或声称完成。"
        }).to_string()}],"isError":false})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    #[tokio::test]
    async fn waiting_disconnect_preserves_one_execution_and_owner_drop_releases_it() {
        struct Guard(Arc<AtomicBool>);
        impl Drop for Guard { fn drop(&mut self) { self.0.store(true, Ordering::Release); } }
        let released = Arc::new(AtomicBool::new(false));
        let starts = Arc::new(AtomicUsize::new(0));
        let (tx, rx) = tokio::sync::oneshot::channel();
        let flag = released.clone(); let count = starts.clone();
        let slot = std::sync::Mutex::new(Some(PendingTool::new("one".into(),json!(1),json!({}),async move {
            let _guard = Guard(flag); count.fetch_add(1,Ordering::AcqRel);
            rx.await.map_err(|_|"发送端结束".to_string())?;
            Ok(json!({"result":"真实收尾"}))
        })));
        let running = wait(&slot,"one",Duration::from_millis(1)).await.unwrap();
        assert!(running["content"][0]["text"].as_str().unwrap().contains("running"));
        assert!(!released.load(Ordering::Acquire));
        assert!(wait(&slot,"other-turn",Duration::from_millis(1)).await.is_err());
        tx.send(()).unwrap();
        assert_eq!(wait(&slot,"one",Duration::from_secs(1)).await.unwrap(),json!({"result":"真实收尾"}));
        assert_eq!(wait(&slot,"one",Duration::from_secs(1)).await.unwrap(),json!({"result":"真实收尾"}));
        assert_eq!(starts.load(Ordering::Acquire),1);
        assert!(released.load(Ordering::Acquire));
        let flag = Arc::new(AtomicBool::new(false)); let guard_flag = flag.clone();
        let slot = std::sync::Mutex::new(Some(PendingTool::new("drop".into(),json!(2),json!({}),async move {
            let _guard = Guard(guard_flag); std::future::pending::<()>().await; Ok(json!({}))
        })));
        wait(&slot,"drop",Duration::from_millis(1)).await.unwrap();
        slot.lock().unwrap().take();
        assert!(flag.load(Ordering::Acquire));
    }
}
