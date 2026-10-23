//! HA-04：本地 HTTP 实际字节、在途屏障及宿主效果验证。
use super::*;
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize,Ordering};

static EXECUTOR_ENTRIES: AtomicUsize = AtomicUsize::new(0);
pub(crate) fn record_executor_entry(name: &str) {
    if name == "bash" { EXECUTOR_ENTRIES.fetch_add(1,Ordering::SeqCst); }
}

#[derive(Clone, Copy, Debug)]
enum Change { Add, Revoke, Disable, Control, Retry429, Retry503, Exhaust503 }
impl Change {
    fn retry(self) -> bool { matches!(self, Self::Retry429 | Self::Retry503 | Self::Exhaust503) }
}

fn save_evidence(name: &str, value: &JsonValue) {
    if let Ok(dir) = std::env::var("COOLZHU_HARNESS_EVIDENCE_DIR") {
        std::fs::write(Path::new(&dir).join(format!("{name}.json")),
            serde_json::to_vec_pretty(value).unwrap()).unwrap();
    }
}

fn mock_response(request: &JsonValue, tool: bool) -> axum::response::Response {
    let calls = json!([{"id":"same-provider-call","type":"function","function":{
        "name":"bash","arguments":json!({"command":"echo HA04_EFFECT>ha04-effect.txt", "cwd":active_workspace_path().display().to_string(),"timeout":10}).to_string()}}]);
    if request["stream"] == true {
        let mut calls = calls; calls[0]["index"] = json!(0);
        let delta = if tool { json!({"role":"assistant","tool_calls":calls}) }
            else { json!({"role":"assistant","content":"HA04_COMPLETE"}) };
        let chunk = json!({"id":"ha04-stream","object":"chat.completion.chunk","created":1,"model":request["model"],
            "choices":[{"index":0,"delta":delta,"finish_reason":if tool {"tool_calls"} else {"stop"}}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}});
        ([(header::CONTENT_TYPE,"text/event-stream")],format!("data: {chunk}\n\ndata: [DONE]\n\n")).into_response()
    } else {
        let message = if tool { json!({"role":"assistant","content":null,"tool_calls":calls}) }
            else { json!({"role":"assistant","content":"HA04_COMPLETE"}) };
        Json(json!({"id":"ha04-response","object":"chat.completion","model":request["model"],
            "choices":[{"index":0,"message":message,"finish_reason":if tool {"tool_calls"} else {"stop"}}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}})).into_response()
    }
}

async fn send(streamed: bool, room: &str, text: &str) -> (Vec<request_binding::RequestBinding>, String) {
    let payload = serde_json::from_value(json!({"target_agent_ids":["target-text"],"session_id":"target-text",
        "chat_room_id":room,"text":text})).unwrap();
    if streamed {
        let response = Box::pin(api_chat_send_stream(Json(payload))).await.unwrap().into_response();
        let output = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        let bindings = output.split("\n\n").filter_map(|event| {
            if !event.starts_with("event: request_bound\n") { return None; }
            let data = event.lines().find_map(|line| line.strip_prefix("data: ")).unwrap();
            Some(serde_json::from_str(data).unwrap())
        }).collect();
        if let Some(bound)=output.find("event: request_bound") {
            assert!(output.find("event: started").unwrap() < bound,"{output}");
        } else { assert!(output.contains("event: error"),"没有绑定时必须有明确失败回执：{output}"); }
        (bindings, output)
    } else {
        let Json(response) = Box::pin(api_chat_send(Json(payload))).await.unwrap();
        let output = serde_json::to_string(&response).unwrap();
        (response.request_bindings, output)
    }
}

fn assert_wire(binding: &request_binding::RequestBinding, raw: &str) {
    let mut payload: JsonValue = serde_json::from_str(raw).unwrap();
    let tools = payload.get("tools").cloned().unwrap_or(JsonValue::Null);
    let names: Vec<String> = tools.as_array().into_iter().flatten()
        .map(|tool| tool["function"]["name"].as_str().unwrap().to_owned()).collect();
    assert_eq!(binding.tool_names, names);
    assert_eq!(binding.model, payload["model"]);
    assert_eq!(binding.stream, payload["stream"].as_bool().unwrap_or(false));
    assert_eq!(binding.tools_digest, format!("sha256:{:x}",Sha256::digest(tools.to_string().as_bytes())));
    for name in ["messages","system","tools"] { payload.as_object_mut().unwrap().remove(name); }
    assert_eq!(binding.parameters_digest, format!("sha256:{:x}",Sha256::digest(payload.to_string().as_bytes())));
}

async fn route_case(streamed: bool, change: Change) {
    EXECUTOR_ENTRIES.store(0,Ordering::SeqCst);
    let name = format!("route-{streamed}-{change:?}");
    let raws = Arc::new(Mutex::new(Vec::<String>::new()));
    let events = Arc::new(Mutex::new(Vec::<String>::new()));
    let received = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let (seen, trace, arrival, gate) = (raws.clone(), events.clone(), received.clone(), release.clone());
    let router = Router::new().route("/v1/chat/completions", post(move |body: axum::body::Bytes| {
        let (seen, trace, arrival, gate) = (seen.clone(),trace.clone(),arrival.clone(),gate.clone());
        async move {
            let raw = String::from_utf8(body.to_vec()).unwrap();
            let request: JsonValue = serde_json::from_str(&raw).unwrap();
            let index = { let mut seen=seen.lock().unwrap(); seen.push(raw); seen.len()-1 };
            trace.lock().unwrap().push(format!("http_received:{index}"));
            if index == 0 {
                arrival.notify_one(); gate.notified().await;
                trace.lock().unwrap().push("response_released".into());
            }
            if matches!(change,Change::Exhaust503) || (change.retry() && index == 0) {
                return (if matches!(change,Change::Retry429) {StatusCode::TOO_MANY_REQUESTS} else {StatusCode::SERVICE_UNAVAILABLE},
                    Json(json!({"error":{"message":"HA04_SYNTHETIC_RETRY"}}))).into_response();
            }
            mock_response(&request, index == usize::from(change.retry()))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}",listener.local_addr().unwrap());
    let server = tokio::spawn(async move {axum::serve(listener,router).await.unwrap();});
    let _isolated = multimodal_input::tests::IsolatedState::install(&url);
    {
        let mut config=workspace_config().lock().unwrap();
        config.model.enable_real_llm=true; config.model.enable_llm_tools=true;
        config.model.llm_tool_exposure=Some("all".into());
        let settings=config.session_model_limits.entry("target-text".into()).or_default();
        settings.tool_allowlist=Some(vec![if matches!(change,Change::Add) {"read_file"} else {"bash"}.into()]);
        settings.temperature=Some(0.25); settings.top_p=Some(0.75); settings.reasoning_mode=Some("thinking".into());
    }
    {
        let mut store=session_store().lock().unwrap();
        store.state.chat_rooms.push(PersistedChatRoom{id:name.clone(),name:"HA04 隔离测试".into(),
            created_at:1,updated_at:1,messages:vec![]});
        store.state.active_chat_room_id=Some(name.clone()); store.save().unwrap();
    }
    set_chat_room_permission_profile_sqlite(&default_session_sqlite_path(),&name,ROOM_PERMISSION_FULL_ACCESS).unwrap();
    let mutate = async {
        received.notified().await;
        match change {
            Change::Add | Change::Disable => {
                let mut config=workspace_config().lock().unwrap();
                let settings=config.session_model_limits.get_mut("target-text").unwrap();
                settings.tool_allowlist=Some(vec![if matches!(change,Change::Add) {"bash"} else {"read_file"}.into()]);
                settings.temperature=Some(0.5);
            }
            Change::Revoke => set_chat_room_permission_profile_sqlite(&default_session_sqlite_path(),&name,ROOM_PERMISSION_WORKSPACE_WRITE).unwrap(),
            _ => {}
        }
        events.lock().unwrap().push(format!("change_committed:{change:?}"));
        release.notify_one();
    };
    let ((client_bindings,output),())=tokio::time::timeout(Duration::from_secs(30),async {
        tokio::join!(send(streamed,&name,"HA04_HUMAN_INPUT：调用工具写入虚构测试文件。"),mutate)
    }).await.expect("屏障或请求超时");
    let connection=open_session_connection(&default_session_sqlite_path()).unwrap();
    let bindings:Vec<request_binding::RequestBinding>=connection.prepare(
        "SELECT request_binding_json FROM chat_usage_events WHERE dispatched=1 ORDER BY id").unwrap()
        .query_map([],|row|row.get::<_,String>(0)).unwrap().map(|row|serde_json::from_str(&row.unwrap()).unwrap()).collect();
    let statuses:Vec<String>=connection.prepare("SELECT status FROM chat_usage_events WHERE dispatched=1 ORDER BY id").unwrap()
        .query_map([],|row|row.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    let raw=raws.lock().unwrap().clone(); assert_eq!(bindings.len(),raw.len());
    let host_receipts:Vec<String>=raw.iter().flat_map(|raw| {
        let body:JsonValue=serde_json::from_str(raw).unwrap();
        body["messages"].as_array().unwrap().iter().filter(|message|message["role"]=="tool")
            .filter_map(|message|message["content"].as_str().map(str::to_owned)).collect::<Vec<_>>()
    }).collect();
    // 先保存实际回执，即使后续断言失败也不丢掉故障证据。
    save_evidence(&format!("{name}-observed"),&json!({"raw_http_bodies":raw,"bindings":bindings,"host_response":output,
        "events":events.lock().unwrap().clone(),"executor_entries":EXECUTOR_ENTRIES.load(Ordering::SeqCst),"host_tool_receipts":host_receipts}));
    for (binding,request) in bindings.iter().zip(&raw) { assert_wire(binding,request); }
    for binding in &client_bindings { assert!(bindings.contains(binding)); }
    if !matches!(change,Change::Exhaust503) { assert!(client_bindings.contains(&bindings[usize::from(change.retry())])); }
    let first:JsonValue=serde_json::from_str(&raw[0]).unwrap();
    assert_eq!(first["temperature"],0.25); assert_eq!(first["top_p"],0.75);
    assert_eq!(first["thinking"],json!({"type":"disabled"})); assert!(first.get("reasoning_effort").is_none());
    assert!(first["tools"][0]["function"]["parameters"].is_object()); assert_eq!(first["tool_choice"],"auto");
    let inputs:i64=connection.query_row("SELECT count(*) FROM chat_room_messages WHERE room_id=?1 AND role='user'",[&name],|row|row.get(0)).unwrap();
    assert_eq!(inputs,1,"传输重试/重新采样不能重复接纳人类输入");
    let calls:i64=connection.query_row("SELECT count(*) FROM tool_calls WHERE tool_name='bash'",[],|row|row.get(0)).unwrap();
    let effects=usize::from(active_workspace_path().join("ha04-effect.txt").exists());
    let entries=EXECUTOR_ENTRIES.load(Ordering::SeqCst);
    let allowed=matches!(change,Change::Control|Change::Retry429|Change::Retry503);
    assert_eq!(effects,usize::from(allowed),"{name}: {output}"); assert_eq!(entries,usize::from(allowed),"{name}: {output}");
    if allowed {
        assert_eq!(std::fs::read_to_string(active_workspace_path().join("ha04-effect.txt")).unwrap().trim(),"HA04_EFFECT");
        let correlated:i64=connection.query_row("SELECT count(*) FROM tool_calls t WHERE EXISTS(SELECT 1 FROM chat_usage_events u WHERE u.logical_request_id=t.source_request_key)",[],|r|r.get(0)).unwrap();
        assert_eq!(correlated,1);
    } else if matches!(change,Change::Add) {
        assert!(output.contains("本次未开放的工具"),"{output}");
    } else if matches!(change,Change::Disable) {
        assert!(host_receipts.iter().any(|receipt|receipt.contains("会话工具策略未开放")),"{host_receipts:?}");
    } else if matches!(change,Change::Revoke) {
        assert!(host_receipts.iter().any(|receipt|receipt.contains("route: runtime-dry-run") && receipt.contains("permission_gate: require-confirm")),"{host_receipts:?}");
    }
    if matches!(change,Change::Exhaust503) {
        assert_eq!(bindings.len(),3,"默认上限为2次重试，加首发共3次尝试");
        assert!(bindings.iter().all(|b|b.request_id==bindings[0].request_id && b.parent_request_id.is_none()));
        assert!(statuses.iter().all(|status|status=="http_error"));
        assert_eq!(raw[0],raw[1]);assert_eq!(raw[1],raw[2]);
        assert!(output.contains("503"),"{output}");
    } else if change.retry() {
        assert_eq!(bindings.len(),3); assert_eq!(bindings[0].request_id,bindings[1].request_id);
        assert_ne!(bindings[0].attempt_id,bindings[1].attempt_id); assert_eq!(statuses[0],"http_error");
        assert_eq!(bindings[2].parent_request_id.as_deref(),Some(bindings[1].request_id.as_str()));
    } else if bindings.len()>1 {
        assert_ne!(bindings[0].request_id,bindings[1].request_id);
        assert_eq!(bindings[1].parent_request_id.as_deref(),Some(bindings[0].request_id.as_str()));
    }
    events.lock().unwrap().push(format!("host_settled:registered={calls},entered={entries},effects={effects}"));
    let ordered=events.lock().unwrap().clone();
    assert_eq!(ordered[0],"http_received:0"); assert!(ordered[1].starts_with("change_committed:")); assert_eq!(ordered[2],"response_released");
    save_evidence(&name,&json!({"raw_http_bodies":raw,"bindings":bindings,"client_bindings":client_bindings,
        "statuses":statuses,"events":ordered,"human_input_count":inputs,"registered_call_count":calls,"tool_entry_count":entries,"effects":effects,
        "host_response":output,"host_tool_receipts":host_receipts}));
    if matches!(change,Change::Add) {
        // 下一次接纳取得新配置；同一次运行仍受原冻结能力上限约束。
        let (next,_) = send(streamed,&name,"HA04_NEXT_INPUT：读取新配置。").await;
        assert!(next[0].tool_names.contains(&"bash".into()));
        let raw=raws.lock().unwrap().last().unwrap().clone(); let next_wire:JsonValue=serde_json::from_str(&raw).unwrap();
        assert_eq!(next_wire["temperature"],0.5); assert_wire(&next[0],&raw);
        save_evidence(&format!("{name}-next"),&json!({"raw_http_body":raw,"binding":next[0]}));
    }
    server.abort();
}

#[tokio::test]
async fn both_entries_keep_original_wire_after_settings_change() {
    let _lock=tests::config_test_guard();
    for streamed in [false,true] { route_case(streamed,Change::Add).await; }
}
#[tokio::test]
async fn inflight_permission_revocation_and_tool_disabling_have_zero_effects() {
    let _lock=tests::config_test_guard();
    for streamed in [false,true] { for change in [Change::Control,Change::Revoke,Change::Disable] {route_case(streamed,change).await;} }
}
#[tokio::test]
async fn real_retries_keep_one_admission_and_link_feedback_request() {
    let _lock=tests::config_test_guard();
    for streamed in [false,true] { for change in [Change::Retry429,Change::Retry503,Change::Exhaust503] {route_case(streamed,change).await;} }
}

#[tokio::test]
async fn actual_agent_child_request_inherits_parent_identity_and_capability_limit() {
    let _lock=tests::config_test_guard();
    let raws=Arc::new(Mutex::new(Vec::<String>::new())); let seen=raws.clone();
    let router=Router::new().route("/v1/chat/completions",post(move |bytes:axum::body::Bytes| {
        let seen=seen.clone(); async move {
            let raw=String::from_utf8(bytes.to_vec()).unwrap(); let request:JsonValue=serde_json::from_str(&raw).unwrap();
            let index={let mut seen=seen.lock().unwrap();seen.push(raw);seen.len()-1};
            let message=if index==0 {json!({"role":"assistant","content":null,"tool_calls":[{
                "id":"agent-provider-call","type":"function","function":{"name":"Agent","arguments":json!({
                    "description":"HA04 子请求", "prompt":"直接给出虚构结果", "subagent_type":"general-purpose",
                    "allowed_tools":["read_file","write_file"]}).to_string()}}]})}
                else {json!({"role":"assistant","content":"HA04_CHILD_COMPLETE"})};
            Json(json!({"id":format!("child-{index}"),"object":"chat.completion","model":request["model"],
                "choices":[{"index":0,"message":message,"finish_reason":if index==0 {"tool_calls"} else {"stop"}}],
                "usage":{"prompt_tokens":8,"completion_tokens":4}}))
        }
    }));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url=format!("http://{}",listener.local_addr().unwrap());
    let server=tokio::spawn(async move {axum::serve(listener,router).await.unwrap();});
    let _isolated=multimodal_input::tests::IsolatedState::install(&url);
    {
        let mut config=workspace_config().lock().unwrap();config.model.enable_real_llm=true;config.model.enable_llm_tools=true;
        config.model.llm_tool_exposure=Some("all".into());config.session_model_limits.entry("target-text".into()).or_default()
            .tool_allowlist=Some(vec!["Agent".into(),"read_file".into()]);
        let mut store=session_store().lock().unwrap();store.state.chat_rooms.push(PersistedChatRoom {
            id:"ha04-child".into(),name:"HA04 子请求隔离测试".into(),created_at:1,updated_at:1,messages:vec![]});
        store.state.active_chat_room_id=Some("ha04-child".into());store.save().unwrap();
    }
    set_chat_room_permission_profile_sqlite(&default_session_sqlite_path(),"ha04-child",ROOM_PERMISSION_FULL_ACCESS).unwrap();
    let budget=root_execution_budget::RootExecutionBudget::establish(60_000);
    let root_deadline=budget.deadline_unix_ms();
    let (client,output)=root_execution_budget::scope(budget,send(false,"ha04-child","请使用 Agent 完成虚构子任务。")).await;
    let raw=raws.lock().unwrap().clone();
    save_evidence("actual-child-observed",&json!({"raw_http_bodies":raw,"bindings":client,"host_response":output}));
    assert_eq!(raw.len(),3,"{output}");
    assert_eq!(client.len(),3);for (binding,raw) in client.iter().zip(&raw) {assert_wire(binding,raw);}
    assert_ne!(client[0].request_id,client[1].request_id);assert_ne!(client[1].request_id,client[2].request_id);
    assert_eq!(client[1].parent_request_id.as_deref(),Some(client[0].request_id.as_str()));
    assert_eq!(client[2].parent_request_id.as_deref(),Some(client[0].request_id.as_str()));
    assert_eq!(client[1].tool_names,vec!["read_file".to_string()]);
    let connection=open_session_connection(&default_session_sqlite_path()).unwrap();
    let inputs:i64=connection.query_row("SELECT count(*) FROM chat_room_messages WHERE room_id='ha04-child' AND role='user'",[],|row|row.get(0)).unwrap();
    assert_eq!(inputs,1);
    let calls:i64=connection.query_row("SELECT count(*) FROM tool_calls WHERE tool_name='Agent'",[],|row|row.get(0)).unwrap();assert_eq!(calls,1);
    let child_spec:String=connection.query_row("SELECT spec_json FROM execution_index WHERE id LIKE 'child-execution:%'",[],|row|row.get(0)).unwrap();
    let child_spec:JsonValue=serde_json::from_str(&child_spec).unwrap();
    assert_eq!(child_spec["deadline_ms"].as_u64(),Some(root_deadline));
    save_evidence("actual-child-request",&json!({"raw_http_bodies":raw,"bindings":client,"human_input_count":inputs,"agent_dispatch_count":calls,
        "child_execution":child_spec,"root_deadline_ms":root_deadline,"host_response":output}));
    server.abort();
}

#[tokio::test]
async fn subscription_native_tool_entry_returns_explicit_unsupported() {
    let _lock=tests::config_test_guard();
    let isolated=multimodal_input::tests::IsolatedState::install("http://127.0.0.1:1");
    let mut agent=isolated.agent("target-text");agent.provider="openai_subscription".into();
    let error=call_agent_model_with_tool_loop(&agent,"调用原生工具",&[],&[],None,None,None,None).await.unwrap_err();
    assert!(matches!(error,api::ApiError::UnsupportedCapability{..}));
    let child=execution_service::require_capability("openai_subscription",execution_service::Source::SessionTool).unwrap_err();
    assert!(child.contains("仅开放文本聊天"));
    save_evidence("subscription-unsupported",&json!({"http_native_tool_entry":error.to_string(),"child_entry":child,
        "network_calls":0,"reason":"受影响入口在调用 provider 前返回 UnsupportedCapability；无账号及付费调用"}));
}
