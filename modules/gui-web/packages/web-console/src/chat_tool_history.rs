//! 聊天工具状态与跨轮历史投影。原始审计仍保存，模型历史只带必要事实。
use super::*;

pub(super) fn call_id(agent_id: &str, turn_key: &str, tool_use_id: &str) -> String {
    format!("tool-call-{:016x}", hash_bytes(format!("{agent_id}\u{1f}{turn_key}\u{1f}{tool_use_id}").as_bytes()))
}

pub(super) fn status(agent_id: &str, turn_key: &str, tool_use_id: &str, tool_name: &str, status: &str, summary: &str) -> JsonValue {
    let summary = if matches!(status, "completed" | "failed" | "interrupted" | "not-executed") {
        let label = match status { "completed" => "执行已完成", "failed" => "执行失败", "interrupted" => "执行已中止", _ => "调用未执行" };
        let paths = artifact_paths(summary);
        if paths.is_empty() { label.to_string() } else { format!("{label}；相关文件：{}", paths.join("；")) }
    } else { compact_message_snippet(summary, 280) };
    json!({ "call_id": call_id(agent_id, turn_key, tool_use_id), "tool_use_id": tool_use_id,
        "tool_name": tool_name, "status": status, "summary": summary })
}

pub(super) fn status_for_chat_turn(room_id: &str, chat_turn_id: &str, agent_id: &str, turn_key: &str,
    tool_use_id: &str, tool_name: &str, phase: &str, summary: &str) -> JsonValue {
    let mut event = status(agent_id, turn_key, tool_use_id, tool_name, phase, summary);
    // UI 使用公开聊天轮次判定生命周期，不把 provider trace 或调用散列误当 chat-turn ID。
    event["chat_room_id"] = json!(room_id);
    event["turn_id"] = json!(chat_turn_id);
    event
}

fn artifact_paths(text: &str) -> Vec<String> {
    static PATHS: Lazy<Regex> = Lazy::new(|| Regex::new(r#""(?:filePath|file_path|path|artifact_path)"\s*:\s*("(?:\\.|[^"\\])*")"#).expect("路径字段正则"));
    let mut paths = Vec::new();
    for captures in PATHS.captures_iter(text) {
        if let Ok(path) = serde_json::from_str::<String>(&captures[1]) {
            if path.len() <= 1024 && !path.chars().any(char::is_control) && !paths.contains(&path) {
                paths.push(path);
                if paths.len() >= 3 { break; }
            }
        }
    }
    paths
}

pub(super) fn dispatch_status(route: &str, is_error: bool) -> &'static str {
    if route.ends_with("dry-run") { "not-executed" }
    else if is_error { "failed" } else { "completed" }
}

pub(super) fn dispatch_summary(dispatch: &ModelToolDispatchResult) -> String {
    let outcome = match dispatch_status(&dispatch.route, dispatch.is_error) {
        "not-executed" => "未执行", "failed" => "失败", _ => "已完成",
    };
    format!("工具 `{}` {outcome}：{}", dispatch.name, dispatch.summary_text)
}

pub(super) fn project_dto(message: &ChatMessageDto) -> Option<ChatMessageDto> {
    project(&PersistedChatMessage::from(message.clone())).map(|message| chat_message_dto_from_persisted(&message))
}

pub(super) fn project_memory(bead: &MemoryBeadDto) -> Option<MemoryBeadDto> {
    // 旧压缩记录没有逐消息 kind / origin ID，无法安全证明其正文不含思考；只停止自动召回，不删除。
    if bead.source == "context:auto-compact" { return None; }
    if bead.source != "chat-room:auto-extract" { return Some(bead.clone()); }
    // 旧自动 decision 分支只由 reasoning 产生；用户手工 decision 不受影响。
    if bead.kind == "decision" { return None; }
    if bead.kind != "tool" { return Some(bead.clone()); }
    let (_, content) = bead.summary.split_once(": ")?;
    if content.starts_with("工具执行事实：") { return Some(bead.clone()); }
    let message = PersistedChatMessage { id: bead.origin_message_id.clone().unwrap_or_default(),
        author: String::new(), role: "assistant".into(), target: String::new(), content: content.into(),
        kind: "tool-summary".into(), attachments: Vec::new(), created_at: bead.created_at };
    let fact = project(&message)?;
    let mut safe = bead.clone(); safe.summary = fact.content;
    safe.token_count = Some(estimate_bead_tokens(&safe.summary));
    Some(safe)
}

pub(super) fn project(message: &PersistedChatMessage) -> Option<PersistedChatMessage> {
    // 原始用户输入始终是对话，不因内容碰巧包含内部术语而被过滤。
    if message.role.eq_ignore_ascii_case("user") { return Some(message.clone()); }
    let kind = message.kind.trim().to_ascii_lowercase().replace('_', "-");
    if message.role.eq_ignore_ascii_case("tool") || matches!(kind.as_str(),
        "reasoning" | "tool-call" | "tool-result" | "computer-use" | "vision-computer-use") {
        return None;
    }
    if kind != "tool-summary" { return Some(message.clone()); }
    let content = message.content.trim().strip_prefix("工具 `")?;
    let (name, outcome) = content.split_once('`')?;
    if name.is_empty() || name.len() > 128 || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')) {
        return None;
    }
    let outcome_label = if outcome.starts_with(" 未执行：")
        || outcome.contains("（预览，未执行真实操作）") { "未执行" }
        else if outcome.starts_with(" 已完成：") { "已完成" }
        else if outcome.starts_with(" 失败：") { "失败" }
        else { return None; };
    // 只提取完整 JSON 字符串中的文件路径，不把 read_file 正文或写入代码作为“摘要”再送回。
    let paths = artifact_paths(outcome);
    let mut result = message.clone();
    result.kind = "tool-fact".to_string();
    result.content = format!("工具执行事实：`{name}` {outcome_label}。");
    if !paths.is_empty() { result.content.push_str(&format!("相关文件：{}。", paths.join("；"))); }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn message(id: &str, kind: &str, content: &str) -> PersistedChatMessage {
        PersistedChatMessage { id: id.into(), author: "系统工具执行 Agent".into(), role: "assistant".into(),
            target: "聊天".into(), content: content.into(), kind: kind.into(), attachments: Vec::new(), created_at: 1 }
    }

    #[test]
    fn chat_tool_history_preserves_outcome_and_artifact_without_raw_arguments_or_results() {
        let original = message("call", "tool-summary", r#"工具 `write_file` 已完成：{"filePath":"C:\\work\\pelican.html","content":"RAW-TOOL-SECRET-LONG-CODE"}"#);
        let fact = project(&original).unwrap();
        assert!(fact.content.contains("write_file") && fact.content.contains("已完成"));
        assert!(fact.content.contains(r"C:\work\pelican.html"));
        assert!(!fact.content.contains("RAW-TOOL-SECRET"));
        assert!(original.content.contains("RAW-TOOL-SECRET"), "投影不能修改旧审计存储");
        for kind in ["reasoning", "tool-call", "tool-result", "tool_result", "computer-use"] {
            assert!(project(&message("audit", kind, "PRIVATE-THINKING-OR-AUDIT")).is_none());
        }
        let mut user = message("user", "text", "请解释 tool-result 与 reasoning 的区别");
        user.role = "user".into();
        assert_eq!(project(&user).unwrap().content, user.content);
        assert!(project(&message("unknown", "tool-summary", "模型打算调用工具，尚无结果")).is_none());
    }

    #[test]
    fn chat_tool_status_correlates_requested_running_and_terminal_without_parameters() {
        let phases = ["requested", "running", "completed"].map(|phase| status("agent", "turn", "provider-call", "write_file", phase, "文件已写入"));
        assert_eq!(phases[0]["call_id"], phases[1]["call_id"]);
        assert_eq!(phases[1]["call_id"], phases[2]["call_id"]);
        assert_ne!(call_id("agent", "turn", "provider-call"), call_id("agent", "turn2", "provider-call"));
        assert!(phases.iter().all(|phase| phase.get("input").is_none()));
        assert_eq!(dispatch_status("runtime-dry-run", false), "not-executed");
        assert_eq!(dispatch_status("runtime-failed", true), "failed");
        assert_eq!(dispatch_status("runtime-executed", false), "completed");
        assert_ne!(call_id("agent", "turn", "provider-call"), call_id("agent", "turn:feedback-2", "provider-call"));
        let terminal = status("agent", "turn", "call", "write_file", "completed", r#"{"filePath":"artifact.html","content":"RAW-ARGUMENT-MARKER","oldString":"PRIVATE-OLD","originalFile":"PRIVATE-FILE"}"#);
        assert!(terminal["summary"].as_str().unwrap().contains("artifact.html"));
        assert!(!terminal.to_string().contains("RAW-ARGUMENT-MARKER"));
        assert!(!terminal.to_string().contains("PRIVATE-"));
    }

    #[test]
    fn chat_tool_history_filters_audit_before_budget_and_compaction_without_mutating_store() {
        let _lock = crate::tests::config_test_guard();
        let isolated = crate::multimodal_input::tests::IsolatedState::install("http://127.0.0.1:1");
        let agent = isolated.agent("target-text");
        let mut user = message("user", "text", "请保留文件并等待下一轮修改");
        user.role = "user".into();
        let private = "PRIVATE-AUDIT-".repeat(10_000);
        let history = vec![user, message("thinking", "reasoning", &private),
            message("tool-raw", "tool-result", &private),
            message("tool-fact", "tool-summary", r#"工具 `write_file` 已完成：{"path":"artifact.html","content":"PRIVATE-AUDIT"}"#),
            message("answer", "assistant-reply", "artifact.html 已保存。")];
        let assembly = build_context_assembly(&agent, &history, "接着完善颜色", &[], ContextBuildOptions {
            history_token_budget: 200, memory_token_budget: 0, max_prompt_tokens: 8000,
            image_token_estimate: 512, max_memory_beads: 0, history_floor_millis: None, chat_room_id: None,
        });
        assert_eq!(assembly.history_selection.selected_ids, ["user", "tool-fact", "answer"]);
        assert!(!assembly.truncated, "内部审计应在预算和滚动摘要前剔除，不能挤掉正常对话");
        let actual = format!("{} {:?}", assembly.system_prompt, assembly.messages);
        assert!(!actual.contains("PRIVATE-AUDIT"));
        assert!(actual.contains("artifact.html") && actual.contains("工具执行事实"));
        assert_eq!(history[1].content, private, "原存储内容保持不变");
        let compact = context_compaction_memory_summary(&history, context_lifecycle_policy()).unwrap();
        assert!(!compact.contains("PRIVATE-AUDIT"));
        assert!(compact.contains("artifact.html"));
    }

    #[test]
    fn chat_tool_memory_stops_legacy_audit_recall_but_keeps_manual_and_final_reply_memories() {
        let automatic = MemoryBeadDto { id: "old-thought".into(), kind: "decision".into(),
            source: "chat-room:auto-extract".into(), summary: "PRIVATE-OLD-THINKING".into(),
            origin_message_id: Some("thinking".into()), ..MemoryBeadDto::default() };
        assert!(project_memory(&automatic).is_none());
        let mut manual = automatic.clone(); manual.source = "manual".into();
        assert_eq!(project_memory(&manual).unwrap().summary, manual.summary);
        let mut reply = automatic.clone(); reply.kind = "experience".into(); reply.summary = "用户最终答复的有效经验".into();
        assert!(project_memory(&reply).is_some());
        let mut compact = automatic.clone(); compact.source = "context:auto-compact".into();
        assert!(project_memory(&compact).is_none());
        compact.source = "context:auto-compact:v2".into();
        assert!(project_memory(&compact).is_some());
        assert!(evaluate_auto_memory_candidate(&chat_message_dto_from_persisted(&message("thought", "reasoning", "PRIVATE-THINKING"))).is_none());
        let tool = message("tool", "tool-summary", r#"工具 `write_file` 已完成：{"path":"artifact.html","content":"PRIVATE-CODE"}"#);
        let projected = project_dto(&chat_message_dto_from_persisted(&tool)).unwrap();
        assert!(!projected.content.contains("PRIVATE-CODE"));
        assert_eq!(evaluate_auto_memory_candidate(&projected).unwrap().kind, "tool");
        assert_eq!(automatic.summary, "PRIVATE-OLD-THINKING", "兼容处理不得修改已保存记录");
    }

    #[test]
    fn chat_tool_planner_request_preserves_planner_contract_and_session_reasoning() {
        let _lock = crate::tests::config_test_guard();
        let isolated = crate::multimodal_input::tests::IsolatedState::install("http://127.0.0.1:1");
        let mut agent = isolated.agent("target-text"); agent.reasoning_effort = "medium".into();
        let request = agent_planner_message_request(&agent, vec![InputMessage::user_text("计划下一步")], "只输出动作 JSON".into(), 4096);
        assert_eq!(request.system.as_deref(), Some("只输出动作 JSON"));
        assert_eq!(request.model, agent.model);
        assert_eq!(request.reasoning_effort.as_deref(), Some("medium"));
        assert!(request.tools.is_none() && request.tool_choice.is_none() && !request.stream);
        assert!(request.max_tokens > 0 && request.max_tokens <= 4096);
    }

    #[test]
    fn chat_tool_cancellation_checker_keeps_actual_token_after_scope_cleanup() {
        let token = Arc::new(ChatTurnCancellation::new());
        let scope = ToolTurnCancellationScope::install("synthetic-provider-trace", token.clone());
        let check = tool_turn_cancellation_checker("synthetic-provider-trace");
        assert!(!check());
        token.request();
        assert!(check(), "实际 host token 的 request 必须传给工具检查器");
        drop(scope);
        assert!(check(), "清理映射不能让已运行的同步动作丢失取消状态");
        assert!(!tool_turn_cancellation_checker("synthetic-provider-trace")());
        assert!(!tool_turn_cancellation_checker("unknown-provider-trace")());
    }

    #[tokio::test]
    async fn chat_tool_stream_two_calls_and_next_user_turn_keep_protocol_and_trim_internal_history() {
        use http_body_util::BodyExt;
        let _lock = crate::tests::config_test_guard();
        struct DevPermissions(bool);
        impl Drop for DevPermissions { fn drop(&mut self) { test_dev_open_tool_permissions_enabled().store(self.0, Ordering::SeqCst); } }
        let _permissions = DevPermissions(test_dev_open_tool_permissions_enabled().swap(true, Ordering::SeqCst));
        let file = tempfile::tempdir().unwrap();
        let path = file.path().join("artifact.html").display().to_string();
        let raw_code = format!("<html>RAW-ARGUMENT-MARKER-{}</html>", "x".repeat(10_000));
        let captures = Arc::new(Mutex::new(Vec::<JsonValue>::new()));
        let seen = captures.clone();
        let tool_path = path.clone();
        let tool_code = raw_code.clone();
        let mock = Router::new().route("/v1/chat/completions", post(move |Json(request): Json<JsonValue>| {
            let seen = seen.clone(); let path = tool_path.clone(); let code = tool_code.clone();
            async move {
                let index = { let mut requests = seen.lock().unwrap(); requests.push(request.clone()); requests.len() };
                if index == 1 {
                    let arguments = json!({"path":path,"content":code}).to_string();
                    let chunks = vec![
                        json!({"choices":[{"index":0,"delta":{"role":"assistant","reasoning_content":"GENUINE-REASONING-ONLY"},"finish_reason":null}]}),
                        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call-write","type":"function","function":{"name":"write_file","arguments":""}}]},"finish_reason":null}]}),
                        json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":arguments}}]},"finish_reason":null}]}),
                        json!({"choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}],"usage":{"prompt_tokens":20,"completion_tokens":10}}),
                    ];
                    let body = chunks.into_iter().map(|mut v| {
                        v["id"] = json!("mock-stream"); v["object"] = json!("chat.completion.chunk");
                        v["model"] = json!("target-text"); v["created"] = json!(1);
                        format!("data: {v}\n\n")
                    }).collect::<String>() + "data: [DONE]\n\n";
                    return ([(header::CONTENT_TYPE, "text/event-stream")], body).into_response();
                }
                let message = if index == 2 {
                    json!({"role":"assistant","content":null,"reasoning_content":"VERIFY-ONLY", "tool_calls":[{"id":"call-read","type":"function","function":{"name":"read_file","arguments":json!({"path":path}).to_string()}}]})
                } else { json!({"role":"assistant","content":"文件已完成并读取确认。"}) };
                Json(json!({"id":"mock","object":"chat.completion","model":request["model"],"choices":[{"index":0,"message":message,"finish_reason":"stop"}],"usage":{"prompt_tokens":30,"completion_tokens":5}})).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move { axum::serve(listener, mock).await.unwrap(); });
        let isolated = crate::multimodal_input::tests::IsolatedState::install(&url);
        {
            let mut config = workspace_config().lock().unwrap();
            config.model.enable_real_llm = true; config.model.enable_llm_tools = true;
            config.model.llm_tool_exposure = Some("all".into()); config.tool.dev_open_permissions = true;
            config.session_model_limits.entry("target-text".into()).or_default().tool_allowlist = Some(vec!["write_file".into(), "read_file".into()]);
        }
        {
            let mut store = session_store().lock().unwrap();
            store.state.chat_rooms.push(PersistedChatRoom { id:"room-tools".into(), name:"合成工具房间".into(), created_at:1, updated_at:1, messages:Vec::new() });
            store.state.active_chat_room_id = Some("room-tools".into()); store.save().unwrap();
        }
        let payload: SendMessageRequest = serde_json::from_value(json!({"target_agent_ids":["target-text"],"session_id":"target-text","chat_room_id":"room-tools","text":"请使用 write_file 创建一个 HTML 文件，然后 read_file 检查。"})).unwrap();
        let response = api_chat_send_stream(Json(payload)).await.unwrap().into_response();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let sse = String::from_utf8(bytes.to_vec()).unwrap();
        if !std::path::Path::new(&path).exists() {
            panic!("未创建预期文件；合成请求数={}；事件={}", captures.lock().unwrap().len(), sse);
        }
        let events = sse.split("\n\n").filter_map(|frame| {
            let kind = frame.lines().find_map(|line| line.strip_prefix("event: "))?;
            let value: JsonValue = serde_json::from_str(frame.lines().find_map(|line| line.strip_prefix("data: "))?).ok()?;
            Some((kind.to_string(), value))
        }).collect::<Vec<_>>();
        let public_turn = &events.iter().find(|(kind, _)| kind == "started").unwrap().1["turn_id"];
        assert!(events.iter().filter(|(kind, _)| kind == "tool_status").all(|(_, value)| value["chat_room_id"] == "room-tools" && &value["turn_id"] == public_turn));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw_code, "真实工具执行写入合成文件");
        assert_eq!(captures.lock().unwrap().len(), 3);
        for id in ["call-write", "call-read"] {
            let phases = events.iter().filter(|(kind, value)| kind == "tool_status" && value["tool_use_id"] == id).map(|(_, value)| value["status"].as_str().unwrap()).collect::<Vec<_>>();
            assert_eq!(phases, ["requested", "running", "completed"]);
        }
        for (_, value) in &events {
            if value["kind"] == "reasoning" { assert!(!value.to_string().contains("RAW-ARGUMENT-MARKER")); }
        }
        assert!(events.iter().filter(|(kind, _)| kind == "tool_status").all(|(_, value)| !value.to_string().contains("RAW-ARGUMENT-MARKER")), "工具状态只能显示执行事实与文件引用，不能携带原始代码");
        let requests = captures.lock().unwrap().clone();
        let assistant = requests[1]["messages"].as_array().unwrap().iter().find(|m| m.get("tool_calls").is_some()).unwrap();
        assert_eq!(assistant["reasoning_content"], "GENUINE-REASONING-ONLY");
        assert!(assistant["tool_calls"].to_string().contains("RAW-ARGUMENT-MARKER"));
        assert!(!assistant["reasoning_content"].to_string().contains("RAW-ARGUMENT-MARKER"));
        assert!(requests[1]["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool" && m["tool_call_id"] == "call-write"));
        assert!(requests[2]["messages"].as_array().unwrap().iter().any(|m| m["role"] == "tool" && m["tool_call_id"] == "call-read"));

        let history = session_store().lock().unwrap().state.chat_rooms.iter().find(|r| r.id == "room-tools").unwrap().messages.clone();
        assert!(history.iter().any(|m| m.kind == "reasoning"), "原审计仍保存");
        assert!(history.iter().any(|m| m.kind == "tool-result"));
        let agent = isolated.agent("target-text");
        call_agent_model_with_tool_loop(&agent, "只需简短确认上轮文件产物", &[], &history, None, None).await.unwrap();
        let requests = captures.lock().unwrap();
        let next = requests[3]["messages"].to_string();
        assert!(!next.contains("GENUINE-REASONING-ONLY"), "下一轮合成请求仍含内部思考：{next}");
        assert!(!next.contains("RAW-ARGUMENT-MARKER"));
        assert!(!next.contains("tool_call_id:"));
        assert!(next.contains("artifact.html"));
        assert!(next.contains("工具执行事实"));
        assert!(next.len() < 6000, "工具审计和长代码不应挤满下一轮历史");
        task.abort();
    }
}
