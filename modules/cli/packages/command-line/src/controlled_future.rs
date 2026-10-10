//! 同步CLI进入异步请求前捕获根控制，避免线程局部上下文在异步迁移后丢失。
use std::future::Future;
use runtime::managed_process::{ExecutionControl, Interruption};

pub(crate) async fn wait<F: Future>(control: &Option<ExecutionControl>, future: F) -> Result<F::Output, Interruption> {
    let Some(control) = control else { return Ok(future.await); };
    if let Some(reason) = control.interruption() { return Err(reason); }
    tokio::pin!(future);
    loop {
        tokio::select! {
            biased;
            _ = tokio::time::sleep(std::time::Duration::from_millis(20)) => {
                if let Some(reason) = control.interruption() { return Err(reason); }
            }
            result = &mut future => {
                // 就绪结果与取消同时发生时，不把迟到结果交给渲染或工具派发。
                if let Some(reason) = control.interruption() { return Err(reason); }
                return Ok(result);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn already_cancelled_never_polls_request() {
        let control=ExecutionControl::new(None,None);
        control.cancel();
        let result=wait(&Some(control),async {panic!("已取消请求不得启动")}).await;
        assert_eq!(result,Err(Interruption::Cancelled));
    }

    #[tokio::test]
    async fn timed_out_owned_socket_is_dropped() {
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address=listener.local_addr().unwrap();
        let server=tokio::spawn(async move {
            let (mut socket,_)=listener.accept().await.unwrap();
            let mut byte=[0];
            assert_eq!(socket.read(&mut byte).await.unwrap(),1);
            match tokio::time::timeout(std::time::Duration::from_secs(2),socket.read(&mut byte)).await.unwrap() {
                Ok(0) => (),
                Err(error) if error.kind()==std::io::ErrorKind::ConnectionReset => (),
                other => panic!("取消后连接仍未关闭：{other:?}"),
            }
        });
        let socket=tokio::net::TcpStream::connect(address).await.unwrap();
        let control=ExecutionControl::new(Some(std::time::Duration::from_millis(80)),None);
        let result=wait(&Some(control),async move {
            let mut socket=socket;
            socket.write_all(&[1]).await.unwrap();
            let mut byte=[0];
            socket.read(&mut byte).await
        }).await;
        assert!(matches!(result,Err(Interruption::TimedOut)));
        server.await.unwrap(); // Windows可观察EOF或RST，均证明连接已关闭。
    }
}
