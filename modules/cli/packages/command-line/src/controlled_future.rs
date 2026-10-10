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
    async fn actual_provider_handshake_and_stalled_sse_are_cancelled_without_zeroing_remote_usage() {
        use tokio::io::{AsyncReadExt,AsyncWriteExt};
        for send_prefix in [false,true] {
            let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint=format!("http://{}/v1",listener.local_addr().unwrap());
            let control=ExecutionControl::new(None,None);
            let cancel=control.clone();
            let server=tokio::spawn(async move {
                let (mut socket,_)=listener.accept().await.unwrap();
                let mut request=Vec::new();
                loop {
                    let mut buffer=[0;1024];
                    let count=socket.read(&mut buffer).await.unwrap();
                    assert!(count>0,"取消必须发生在完整请求已到达后");
                    request.extend_from_slice(&buffer[..count]);
                    assert!(request.len()<32768,"夹具请求超过边界");
                    if let Some(end)=request.windows(4).position(|bytes|bytes==b"\r\n\r\n") {
                        let head=String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                        let length=head.lines().find_map(|line|line.strip_prefix("content-length:").map(|value|value.trim().parse::<usize>().unwrap())).unwrap();
                        if request.len()>=end+4+length {break;}
                    }
                }
                assert!(request.starts_with(b"POST /v1/chat/completions "));
                if send_prefix {
                    socket.write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 1000000\r\nconnection: close\r\n\r\ndata: {\"id\":\"test\",\"model\":\"local-test\",\"choices\":[{\"delta\":{\"content\":\"prefix\"},\"finish_reason\":null}]}\n\n").await.unwrap();
                    socket.flush().await.unwrap();
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                cancel.cancel();
                let mut byte=[0];
                match tokio::time::timeout(std::time::Duration::from_secs(2),socket.read(&mut byte)).await.unwrap() {
                    Ok(0)=>(),
                    Err(error) if error.kind()==std::io::ErrorKind::ConnectionReset=>(),
                    other=>panic!("Provider取消后连接未关闭：{other:?}"),
                }
            });
            let client=api::ProviderClient::from_custom_openai_compatible("local-test",&endpoint,None).unwrap();
            let identity=client.endpoint_identity();
            let request=api::MessageRequest {model:"local-test".into(),max_tokens:64,messages:vec![api::InputMessage::user_text("test")],system:None,tools:None,tool_choice:None,reasoning_effort:None,stream:true};
            let captured=Some(control);
            let response=wait(&captured,client.stream_message(&request)).await;
            if send_prefix {
                let mut stream=response.unwrap().unwrap();
                let mut count=0;
                loop {
                    match wait(&captured,stream.next_event()).await {
                        Ok(Ok(Some(_)))=>count+=1,
                        Err(Interruption::Cancelled)=>break,
                        other=>panic!("SSE不应正常结束：{other:?}"),
                    }
                }
                assert!(count>0,"必须证明流已开始后取消");
                drop(stream);
            } else {assert!(matches!(response,Err(Interruption::Cancelled)));}
            server.await.unwrap();
            let report=api::local_endpoint_drain(&identity,std::time::Duration::from_millis(20)).await;
            assert!(report.remote_result_unknown()>0,"本端取消不得将远端结果未知归零");
            assert!(!report.permits_automatic_switch());
        }
    }

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
