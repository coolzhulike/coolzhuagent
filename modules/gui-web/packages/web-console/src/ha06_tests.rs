//! HA06本地协议与真实隔离工具故障；结果保留原身份和实际效果。
use super::*;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
static LOSE_RECEIPT:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
pub(crate) fn maybe_lose_dispatch_receipt(name:&str){
    if name=="bash" && LOSE_RECEIPT.swap(false,std::sync::atomic::Ordering::SeqCst){panic!("HA06_EXPECTED_WORKER_RECEIPT_LOSS");}
}
struct LostReceiptGuard;
impl Drop for LostReceiptGuard{fn drop(&mut self){LOSE_RECEIPT.store(false,std::sync::atomic::Ordering::SeqCst);}}
fn save(name:&str,value:JsonValue) {
    if let Ok(dir)=std::env::var("COOLZHU_HARNESS_EVIDENCE_DIR") {
        std::fs::write(Path::new(&dir).join(format!("ha06-{name}.json")),serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
}
fn prepare(room:&str,tools:&[&str]) {
    let mut config=workspace_config().lock().unwrap();config.model.enable_real_llm=true;config.model.enable_llm_tools=true;
    config.model.llm_tool_exposure=Some("all".into());config.tool.execution.max_feedback_rounds=6;
    config.session_model_limits.entry("target-text".into()).or_default().tool_allowlist=Some(tools.iter().map(|v|v.to_string()).collect());drop(config);
    let mut store=session_store().lock().unwrap();store.state.chat_rooms.push(PersistedChatRoom{id:room.into(),name:room.into(),created_at:1,updated_at:1,messages:vec![]});
    store.state.active_chat_room_id=Some(room.into());store.save().unwrap();drop(store);
    set_chat_room_permission_profile_sqlite(&default_session_sqlite_path(),room,ROOM_PERMISSION_FULL_ACCESS).unwrap();
}
async fn send(room:&str,stream:bool)->String {
    let payload=serde_json::from_value(json!({"target_agent_ids":["target-text"],"session_id":"target-text","chat_room_id":room,"text":format!("{room}：执行工具完成当前指定任务，仅按真实回执判断。")})).unwrap();
    if stream {
        let response=Box::pin(api_chat_send_stream(Json(payload))).await.unwrap().into_response();
        String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    } else {let Json(response)=Box::pin(api_chat_send(Json(payload))).await.unwrap();serde_json::to_string(&response).unwrap()}
}
fn response(request:&JsonValue,call:Option<(&str,JsonValue)>,text:&str)->Json<JsonValue> {
    let finish=if call.is_some(){"tool_calls"}else{"stop"};
    let message=match call {Some((name,input))=>json!({"role":"assistant","content":null,"tool_calls":[{"id":"ha06-call","type":"function","function":{"name":name,"arguments":input.to_string()}}]}),None=>json!({"role":"assistant","content":text})};
    Json(json!({"id":"ha06","object":"chat.completion","model":request["model"],"choices":[{"index":0,"message":message,"finish_reason":finish}],"usage":{"prompt_tokens":10,"completion_tokens":3}}))
}
struct Hooks(Option<runtime::HookRunner>);
impl Drop for Hooks {fn drop(&mut self){*tool_dispatch_service::DISPATCH_HOOK_RUNNER_OVERRIDE.lock().unwrap()=self.0.take();}}
fn hooks(pre:Vec<String>,post:Vec<String>)->Hooks {
    let runner=runtime::HookRunner::new(runtime::RuntimeHookConfig::new(pre,post)).with_authorization(
        runtime::PermissionPolicy::new(runtime::PermissionMode::DangerFullAccess)
            .with_tool_requirement(runtime::HOOK_CAPABILITY_PRE_TOOL_USE,runtime::PermissionMode::DangerFullAccess)
            .with_tool_requirement(runtime::HOOK_CAPABILITY_POST_TOOL_USE,runtime::PermissionMode::DangerFullAccess));
    Hooks(tool_dispatch_service::DISPATCH_HOOK_RUNNER_OVERRIDE.lock().unwrap().replace(runner))
}
#[derive(Clone,Copy,Debug)]
enum Case {MissingTool,ParamFixed,ParamInvalid,ParamProtocol,ParamExecutionFailure,ParamTimeout,ProtocolFixed,ProtocolInvalid,Cap,PostDenied,SettlementFailed,JoinLostReceipt}
async fn loop_case(case:Case) {
    let calls=Arc::new(Mutex::new(Vec::<JsonValue>::new()));let seen=calls.clone();
    let router=Router::new().route("/v1/chat/completions",post(move |Json(request):Json<JsonValue>|{
        let seen=seen.clone();async move {
            let index={let mut seen=seen.lock().unwrap();seen.push(request.clone());seen.len()-1};assert!(index<8);
            if matches!(case,Case::ParamTimeout) && index==1 {tokio::time::sleep(Duration::from_secs(15)).await;}
            let path=active_workspace_path().join("input.txt");
            if matches!(case,Case::ProtocolFixed|Case::ProtocolInvalid) || (matches!(case,Case::ParamProtocol)&&index>0) {
                return response(&request,None,if index==0||matches!(case,Case::ProtocolInvalid|Case::ParamProtocol){"<tool_call>{\"name\":\"read_file\"}</tool_call>"}else{"HA06_REPAIRED"});
            }
            let call=match case {
                Case::MissingTool|Case::PostDenied|Case::SettlementFailed|Case::JoinLostReceipt=>Some(("bash",json!({"command":"echo effect>>effects.txt","cwd":active_workspace_path(),"timeout":10}))),
                Case::ParamInvalid=>Some(("read_file",json!({}))),
                Case::ParamProtocol|Case::ParamTimeout=>Some(("read_file",json!({}))),
                Case::ParamExecutionFailure if index==0=>Some(("read_file",json!({}))),
                Case::ParamExecutionFailure=>Some(("read_file",json!({"path":active_workspace_path().join("does-not-exist.txt")}))),
                Case::ParamFixed if index==0=>Some(("read_file",json!({}))),
                Case::ParamFixed if index==1=>Some(("read_file",json!({"path":path}))),
                Case::Cap=>Some(("read_file",json!({"path":path}))),
                _=>None,
            };response(&request,call,"HA06_DONE")
        }
    }));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let url=format!("http://{}",listener.local_addr().unwrap());
    let server=tokio::spawn(async move {axum::serve(listener,router).await.unwrap();});
    let _isolated=multimodal_input::tests::IsolatedState::install(&url);let room=format!("ha06-{case:?}");
    prepare(&room,if matches!(case,Case::PostDenied|Case::SettlementFailed|Case::JoinLostReceipt){&["read_file","bash"]}else{&["read_file"]});
    let _lost=matches!(case,Case::JoinLostReceipt).then(||{LOSE_RECEIPT.store(true,std::sync::atomic::Ordering::SeqCst);LostReceiptGuard});
    std::fs::write(active_workspace_path().join("input.txt"),"fixture").unwrap();
    if matches!(case,Case::Cap){workspace_config().lock().unwrap().tool.execution.max_feedback_rounds=1;}
    let _hook=matches!(case,Case::PostDenied).then(||hooks(vec![],vec!["echo HA06_POST_DENIED 1>&2 & exit /b 2".into()]));
    let db=open_session_connection(&default_session_sqlite_path()).unwrap();
    computer_use_store::ComputerUseRunStore::open(&default_session_sqlite_path()).unwrap();
    if matches!(case,Case::SettlementFailed){db.execute_batch("CREATE TRIGGER ha06_fail_settlement BEFORE UPDATE ON tool_calls WHEN NEW.status='completed' BEGIN SELECT RAISE(FAIL,'HA06_STORAGE_FAULT'); END;").unwrap();}
    let budget=root_execution_budget::RootExecutionBudget::establish(if matches!(case,Case::ParamTimeout){8_000}else{30_000});let deadline=budget.deadline_unix_ms();let began=Instant::now();
    let output=tokio::time::timeout(Duration::from_secs(35),root_execution_budget::scope(budget,send(&room,false))).await.unwrap();
    let raw=calls.lock().unwrap().clone();let effect=std::fs::read_to_string(active_workspace_path().join("effects.txt")).unwrap_or_default().lines().count();
    let rows:Vec<(String,String,String)>=db.prepare("SELECT tool_call_id,status,source_request_key FROM tool_calls ORDER BY created_at_unix_ms").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();
    let purposes:Vec<(String,String,Option<String>)>=db.prepare("SELECT request_kind,logical_request_id,json_extract(request_binding_json,'$.parent_request_id') FROM chat_usage_events WHERE dispatched=1 ORDER BY id").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap().collect::<Result<_,_>>().unwrap();
    let humans:i64=db.query_row("SELECT count(*) FROM chat_room_messages WHERE role='user' AND room_id=?1",[&room],|r|r.get(0)).unwrap();
    save(&format!("{case:?}"),json!({"requests":raw,"host_response":output,"effects":effect,"tool_rows":rows,"purposes":purposes,"root_deadline_ms":deadline,"elapsed_ms":began.elapsed().as_millis(),"human_input_count":humans}));
    assert_eq!(humans,1);
    let expected=match case {Case::MissingTool|Case::PostDenied|Case::SettlementFailed|Case::JoinLostReceipt=>1,Case::ParamFixed=>3,_=>2};
    assert_eq!(raw.len(),expected,"{case:?}: {output}");
    match case {
        Case::MissingTool=>{assert_eq!(effect,0);assert!(rows.is_empty());assert!(output.contains("tool_unavailable"));},
        Case::ParamFixed=>{assert_eq!(rows.len(),1);assert_eq!(rows[0].1,"completed");assert!(output.contains("HA06_DONE"));},
        Case::ParamInvalid|Case::ParamProtocol=>{assert!(rows.is_empty());assert!(output.contains("parameter_repair_limit"));},
        Case::ParamExecutionFailure=>{assert_eq!(rows.len(),1);assert_eq!(rows[0].1,"failed");assert!(output.contains("parameter_repair_failed"));},
        Case::ParamTimeout=>{assert!(rows.is_empty());assert!(began.elapsed()<Duration::from_secs(12));assert!(!output.contains("HA06_DONE"));},
        Case::ProtocolFixed=>{assert!(rows.is_empty());assert!(output.contains("HA06_REPAIRED"));},
        Case::ProtocolInvalid=>{assert!(rows.is_empty());assert!(output.contains("未能完成原请求"));},
        Case::Cap=>{assert_eq!(rows.len(),1);assert!(!output.contains("HA06_DONE"));},
        Case::PostDenied=>{assert_eq!(effect,1);assert_eq!(rows.len(),1);assert_ne!(rows[0].1,"completed");assert!(output.contains("不可自动重试"));},
        Case::SettlementFailed=>{assert_eq!(effect,1);assert_eq!(rows.len(),1);assert_eq!(rows[0].1,"cancelled_outcome_unknown");assert!(output.contains("终态记录未确认"));},
        Case::JoinLostReceipt=>{assert_eq!(effect,1);assert_eq!(rows.len(),1);assert_eq!(rows[0].1,"completed");assert!(output.contains("worker_outcome_unknown"));assert!(!output.contains("HA06_DONE"));},
    }
    if matches!(case,Case::ParamFixed|Case::ParamInvalid|Case::ParamProtocol|Case::ParamExecutionFailure|Case::ParamTimeout){assert_eq!(purposes.iter().filter(|p|p.0=="parameter_repair").count(),1);assert_eq!(purposes.iter().filter(|p|p.0=="text_recovery").count(),0);}
    if matches!(case,Case::ProtocolFixed|Case::ProtocolInvalid){assert_eq!(purposes.iter().filter(|p|p.0=="text_recovery").count(),1);}
    if raw.len()>1 {assert_ne!(purposes[0].1,purposes[1].1);assert_eq!(purposes[1].2.as_ref(),Some(&purposes[0].1));}
    server.abort();
}
#[tokio::test]
async fn terminal_and_pre_effect_repairs_have_exact_request_limits(){let _guard=tests::config_test_guard();for case in [Case::MissingTool,Case::ParamFixed,Case::ParamInvalid,Case::ParamProtocol,Case::ParamExecutionFailure,Case::ParamTimeout,Case::ProtocolFixed,Case::ProtocolInvalid,Case::Cap]{loop_case(case).await;}}
#[tokio::test]
async fn post_hook_and_settlement_faults_preserve_one_effect_without_resampling(){let _guard=tests::config_test_guard();for case in [Case::PostDenied,Case::SettlementFailed,Case::JoinLostReceipt]{loop_case(case).await;}}

#[tokio::test]
async fn lost_receipt_same_identity_cannot_replay_by_changing_parameters_or_tool(){
    let _guard=tests::config_test_guard();let _isolated=multimodal_input::tests::IsolatedState::install("http://127.0.0.1:1");prepare("ha06-identity",&["bash","write_file"]);
    let root=active_workspace_path();let mut parent=FrozenParentContext::new("ha06-identity",&workspace_identity(&root),None,None,None,None).unwrap();parent.runtime_db_path=Some(default_session_sqlite_path());
    let input=json!({"command":"echo effect>>identity.txt","cwd":root,"timeout":10});
    async fn invoke(parent:&FrozenParentContext,source:&str,name:&str,input:&JsonValue)->ApiResult<ToolDispatchResponse>{
        tool_invocation_identity::scope(source.into(),run_model_tool_dispatch_for_session_with_identity(name,input,Some("target-text"),Some("same-id"),Some("ha06-id-turn"),Some("ha06-identity"),Some(parent),None)).await
    }
    // 原回执故意丢弃；已发生效果仍由原持久身份保护。
    drop(invoke(&parent,"request-1","bash",&input).await.unwrap());
    let original=computer_use_store::ComputerUseRunStore::open(&default_session_sqlite_path()).unwrap();
    let identity=tool_invocation_identity::ModelToolIdentity::from_source(None,"request-1","same-id").unwrap();
    let before=original.tool_call_record(&identity.execution_id).unwrap().unwrap().status;
    let changed=json!({"command":"echo changed>>identity.txt","cwd":root,"timeout":10});
    let write=json!({"path":root.join("identity.txt"),"content":"replacement"});
    let mut refusals=vec![];
    for (name,input) in [("bash",&input),("bash",&changed),("write_file",&write)] {
        let error=invoke(&parent,"request-1",name,input).await.err().expect("同一身份必须拒绝");refusals.push(error.1.error.clone());}
    assert_eq!(std::fs::read_to_string(root.join("identity.txt")).unwrap().lines().count(),1);
    assert_eq!(original.tool_call_record(&identity.execution_id).unwrap().unwrap().status,before);
    invoke(&parent,"request-2","bash",&input).await.unwrap();
    let effects=std::fs::read_to_string(root.join("identity.txt")).unwrap();assert_eq!(effects.lines().count(),2);
    save("identity",json!({"original_id":identity.execution_id,"original_status":before,"same_identity_refusals":refusals,"effects_after_distinct_request":effects.lines().count()}));
}

#[tokio::test]
async fn stream_failure_before_complete_response_has_zero_tool_effects(){
    let _guard=tests::config_test_guard();let calls=Arc::new(Mutex::new(Vec::<JsonValue>::new()));let seen=calls.clone();
    let router=Router::new().route("/v1/chat/completions",post(move|Json(request):Json<JsonValue>|{let seen=seen.clone();async move{
        seen.lock().unwrap().push(request.clone());
        // 工具参数尚未闭合且没有完成标记，真实HTTP响应已结束，不能按完整调用执行。
        let body="data: {\"id\":\"cut\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"cut-call\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"{\"}}]}}]}\n\n";
        ([(header::CONTENT_TYPE,"text/event-stream")],body).into_response()
    }}));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let url=format!("http://{}",listener.local_addr().unwrap());let server=tokio::spawn(async move{axum::serve(listener,router).await.unwrap();});
    let _isolated=multimodal_input::tests::IsolatedState::install(&url);prepare("ha06-cut",&["bash"]);
    let output=tokio::time::timeout(Duration::from_secs(15),send("ha06-cut",true)).await.unwrap();
    let db=open_session_connection(&default_session_sqlite_path()).unwrap();let tools:i64=db.query_row("SELECT COUNT(*) FROM tool_calls",[],|r|r.get(0)).unwrap();
    save("stream-cut",json!({"requests":calls.lock().unwrap().clone(),"host_response":output,"tool_registrations":tools,"effects":0}));
    assert_eq!(tools,0);assert!(output.contains("event: error")||output.contains("parameter_repair_limit"));server.abort();
}

#[test]
fn post_hook_process_probe(){
    let Some(dir)=std::env::var_os("COOLZHU_HA06_POST_PROBE") else{return;};let dir=PathBuf::from(dir);
    std::fs::write(dir.join("ha06-post.pid"),std::process::id().to_string()).unwrap();
    std::thread::sleep(Duration::from_secs(10));std::fs::write(dir.join("ha06-post-late.txt"),"late").unwrap();
}
#[tokio::test]
async fn consumer_disconnect_after_effect_preserves_unknown_and_drains_hook(){
    let _guard=tests::config_test_guard();let _isolated=multimodal_input::tests::IsolatedState::install("http://127.0.0.1:1");prepare("ha06-drop",&["bash"]);
    let root=active_workspace_path();let mut parent=FrozenParentContext::new("ha06-drop",&workspace_identity(&root),None,None,None,None).unwrap();parent.runtime_db_path=Some(default_session_sqlite_path());
    let command=format!("set \"COOLZHU_HA06_POST_PROBE={}\" && \"{}\" --exact ha06_tests::post_hook_process_probe --nocapture",root.display(),std::env::current_exe().unwrap().display());
    let _hooks=hooks(vec![],vec![command]);let input=json!({"command":"echo effect>>drop-effects.txt","cwd":root,"timeout":10});
    let began=Instant::now();
    let mut dispatch=Box::pin(tool_invocation_identity::scope("ha06-drop-source".into(),run_model_tool_dispatch_for_session_with_identity("bash",&input,Some("target-text"),Some("drop-id"),Some("ha06-drop-turn"),Some("ha06-drop"),Some(&parent),None)));
    let started=root.join("ha06-post.pid");
    tokio::select!{
        result=&mut dispatch=>panic!("后置屏障前意外结束：{result:?}"),
        ()=async{tokio::time::timeout(Duration::from_secs(8),async{while !started.exists(){tokio::time::sleep(Duration::from_millis(15)).await;}}).await.unwrap();}=>{}
    }
    let pid:u32=std::fs::read_to_string(&started).unwrap().parse().unwrap();assert!(windows_process_guard::capture_live_process_identity(pid).is_ok());
    assert_eq!(std::fs::read_to_string(root.join("drop-effects.txt")).unwrap().lines().count(),1);
    drop(dispatch);
    tokio::time::timeout(Duration::from_secs(4),async{while windows_process_guard::capture_live_process_identity(pid).is_ok(){tokio::time::sleep(Duration::from_millis(30)).await;}}).await.expect("后置hook必须真实清理");
    let store=computer_use_store::ComputerUseRunStore::open(&default_session_sqlite_path()).unwrap();
    let id=tool_invocation_identity::ModelToolIdentity::from_source(None,"ha06-drop-source","drop-id").unwrap();let state=store.tool_call_record(&id.execution_id).unwrap().unwrap().status;
    assert_eq!(state,"cancelled_outcome_unknown");assert!(!root.join("ha06-post-late.txt").exists());
    let duplicate=tool_invocation_identity::scope("ha06-drop-source".into(),run_model_tool_dispatch_for_session_with_identity("bash",&input,Some("target-text"),Some("drop-id"),Some("ha06-drop-turn"),Some("ha06-drop"),Some(&parent),None)).await;
    assert!(duplicate.is_err());assert_eq!(std::fs::read_to_string(root.join("drop-effects.txt")).unwrap().lines().count(),1);
    save("drop-after-effect",json!({"execution_id":id.execution_id,"state":state,"effects":1,"hook_pid":pid,"hook_drained":true,"late_effect":false,"elapsed_ms":began.elapsed().as_millis(),"replay_refused":true}));
}
