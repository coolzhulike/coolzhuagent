//! 实际CLI适配器的部分工具流取消，禁止提升为完成或派发工具。
use super::*;
use std::sync::atomic::{AtomicUsize,Ordering};

#[test]
fn cancelled_partial_tool_stream_has_interrupted_terminal_and_zero_tool_calls() {
    check_partial_tool_interruption(false);
}

#[test]
fn deadline_partial_tool_stream_has_interrupted_terminal_and_zero_tool_calls() {
    check_partial_tool_interruption(true);
}

fn check_partial_tool_interruption(deadline: bool) {
    let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint=format!("http://{}/v1",listener.local_addr().unwrap());
    let control=runtime::managed_process::ExecutionControl::new(Some(if deadline {Duration::from_millis(800)} else {Duration::from_secs(5)}),None);
    let cancel=control.clone();
    let server=std::thread::spawn(move || {
        let (mut socket,_)=listener.accept().unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut request=Vec::new();
        loop {
            let mut buffer=[0;1024];
            let count=socket.read(&mut buffer).unwrap();
            assert!(count>0);request.extend_from_slice(&buffer[..count]);
            assert!(request.len()<32768);
            if let Some(end)=request.windows(4).position(|bytes|bytes==b"\r\n\r\n") {
                let header=String::from_utf8_lossy(&request[..end]).to_ascii_lowercase();
                let length=header.lines().find_map(|line|line.strip_prefix("content-length:").map(|value|value.trim().parse::<usize>().unwrap())).unwrap();
                if request.len()>=end+4+length {break;}
            }
        }
        let prefix=concat!(
            "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: 1000000\r\nconnection: close\r\n\r\n",
            "data: {\"id\":\"partial\",\"model\":\"local-test\",\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_partial\",\"type\":\"function\",\"function\":{\"name\":\"write_file\",\"arguments\":\"{\"}}]},\"finish_reason\":null}],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n"
        );
        socket.write_all(prefix.as_bytes()).unwrap();socket.flush().unwrap();
        std::thread::sleep(Duration::from_millis(200));
        if !deadline {cancel.cancel();}
        let mut byte=[0];
        match socket.read(&mut byte) {
            Ok(0)=>(),
            Err(error) if error.kind()==io::ErrorKind::ConnectionReset=>(),
            other=>panic!("取消后实际CLI连接未关闭：{other:?}"),
        }
    });
    let client=DefaultRuntimeClient {
        runtime:tokio::runtime::Runtime::new().unwrap(),
        client:ProviderClient::from_custom_openai_compatible("local-test",&endpoint,None).unwrap(),
        model:"local-test".into(),enable_tools:true,emit_output:false,allowed_tools:None,
        tool_registry:GlobalToolRegistry::builtin(),progress_reporter:None,
    };
    struct Executor(Arc<AtomicUsize>);
    impl runtime::ToolExecutor for Executor {
        fn execute(&mut self,_:&str,_:&str)->Result<String,runtime::ToolError>{self.0.fetch_add(1,Ordering::SeqCst);Ok("unexpected".into())}
    }
    let calls=Arc::new(AtomicUsize::new(0));
    let mut conversation=ConversationRuntime::new(Session::new(),client,Executor(calls.clone()),PermissionPolicy::new(PermissionMode::DangerFullAccess),Vec::new());
    let error=runtime::managed_process::with_execution_control(control,||conversation.run_turn("test",None)).unwrap_err();
    server.join().unwrap();
    assert_eq!(calls.load(Ordering::SeqCst),0);
    assert!(error.to_string().contains("输入 7"),"必须保留已收到用量快照：{error}");
    assert!(error.to_string().contains("不完整"));
    let terminal=error.events().last().unwrap();
    assert_eq!(terminal.kind,runtime::AgentEventKind::TurnInterrupted);
    let expected=if deadline {"TimedOut"} else {"Cancelled"};
    assert_eq!(terminal.payload["interruption"],expected);
    assert_eq!(conversation.session().messages.len(),1,"不完整助手/工具内容不得提交会话");
    let directory=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp")
        .join(format!("cli-provider-cancel-{}",generate_session_id()));
    fs::create_dir_all(&directory).unwrap();
    let path=directory.join("main.json");
    let saved=turn_persistence::persist_result::<runtime::TurnSummary>(Err(error),conversation.session(),&path);
    assert!(saved.is_err(),"保存中断事实不得变为成功");
    let history=turn_persistence::load_event_history(&path).unwrap();
    assert_eq!(history.len(),1);
    let recovered=history[0].last().unwrap();
    assert_eq!(recovered.kind,runtime::AgentEventKind::TurnInterrupted);
    assert_eq!(recovered.payload["interruption"],expected);
}
