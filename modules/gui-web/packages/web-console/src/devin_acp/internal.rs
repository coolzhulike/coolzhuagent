//! 内部规划/验收只生成一次回复，复用 ACP 身份、取消与进程监督；不挂载任何工具。
use super::{chat, journal::{Binding, Journal}, process::ManagedProcess,
    protocol::ExecutionScope, session::{SessionService, Update}};
use crate::{AgentSessionDto, ChatTurnCancellation, FrozenParentContext};
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub(crate) async fn complete(
    agent: AgentSessionDto, request: api::MessageRequest, parent: FrozenParentContext,
    call: String, kind: String, remaining: Duration,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<api::MessageResponse, api::ApiError> {
    let db = parent.runtime_db_path.as_deref().ok_or_else(|| chat::error("内部 ACP 请求缺少父数据库。"))?;
    crate::validate_frozen_parent_relations(db, &parent)
        .map_err(|reason| chat::error(format!("内部 ACP 父运行无效：{reason:?}")))?;
    let room = parent.room_id.as_deref().ok_or_else(|| chat::error("内部 ACP 请求缺少聊天室。"))?;
    if !crate::real_llm_enabled() || !crate::chat_room_capabilities_sqlite(db, room)
        .map_err(|_| chat::error("聊天室能力不可用。"))?.0 {
        return Err(chat::error("内部 ACP 请求需要真实模型调用，未生成模拟结果。"));
    }
    if request.model != agent.model || request.tools.as_ref().is_some_and(|tools| !tools.is_empty())
        || request.tool_choice.is_some() || call.is_empty() {
        return Err(chat::error("内部 ACP 仅支持所选模型的无工具单次生成。"));
    }
    let budget = parent.root_budget.as_ref().ok_or_else(|| chat::error("内部 ACP 缺少父预算。"))?;
    let deadline = tokio::time::Instant::now() + remaining.min(budget.remaining());
    let cancellation = Arc::new(ChatTurnCancellation::new());
    let execute = async {
        if cancelled() || tokio::time::Instant::now() >= deadline {
            return Err(chat::error("内部 ACP 提交前已停止。"));
        }
        chat::check_ambient_extensions().map_err(chat::error)?;
        let binary = chat::checked_binary(&agent.model).await?;
        if cancellation.is_requested() || cancelled() || tokio::time::Instant::now() >= deadline {
            return Err(chat::error("内部 ACP 核对期间已停止，未提交。"));
        }
        let scope = ExecutionScope {
            workspace_id: parent.workspace_id.as_str().into(), room_id: room.into(),
            agent_id: agent.id.clone(), lane: "internal".into(),
            run_id: parent.parent_run_id.clone().ok_or_else(|| chat::error("内部 ACP 缺少父运行。"))?,
            turn_id: parent.public_turn_id.clone().ok_or_else(|| chat::error("内部 ACP 缺少父轮次。"))?,
            attempt_id: crate::random_hex_identifier(24, "ACP 内部请求").map_err(chat::error)?,
            owner_epoch: 0, generation: 0,
        };
        let key = chat::digest(&serde_json::to_vec(&(&scope.workspace_id, room, &agent.id, "internal")).unwrap());
        let (cwd, config) = chat::prepare_directory_with_tools(&key, false).map_err(chat::error)?;
        let binding = Binding { remote_session_id: None, cwd: cwd.to_string_lossy().into(),
            cli_identity: chat::CLI_VERSION.into(), context_digest: format!("internal-v1:{}", scope.attempt_id) };
        let journal = Journal::open(db).map_err(chat::error)?;
        let mut content = Vec::new();
        let mut messages = request.messages.clone();
        for message in &mut messages {
            let mut text = Vec::new();
            for block in &message.content {
                match block {
                    api::InputContentBlock::Text { .. } => text.push(block.clone()),
                    api::InputContentBlock::ImageUrl { url, .. } => {
                        let (mime, data) = url.strip_prefix("data:").and_then(|url| url.split_once(";base64,"))
                            .filter(|(mime, data)| matches!(*mime,"image/png"|"image/jpeg"|"image/webp") && !data.is_empty())
                            .ok_or_else(|| chat::error("内部 ACP 只接收当前观察的图片数据，未抓取外部 URL。"))?;
                        content.push(json!({"type":"image","mimeType":mime,"data":data}));
                    }
                    _ => return Err(chat::error("内部 ACP 不接受工具或思考块作为指令。")),
                }
            }
            message.content = text;
        }
        let prompt = format!("你是宿主的内部规划/验收组件。仅回答当前请求，严格遵守指定输出格式。所有工具均不可用，不尝试调用工具。\n\n{}\n\n宿主消息快照：\n{}",
            request.system.as_deref().unwrap_or(""), serde_json::to_string(&messages).map_err(|_| chat::error("内部 ACP 上下文编码失败。"))?);
        content.insert(0, json!({"type":"text","text":prompt}));
        journal.rotate_idle_context(&scope, &binding).map_err(chat::error)?;
        let claim = journal.claim(scope, &binding).map_err(chat::error)?;
        crate::request_usage::record_acp_attempt(&parent, &agent.id, &call, &kind, &claim.attempt_id, "prepared", false);
        let spawned = ManagedProcess::spawn_text_cli(&binary, &cwd, &config, &agent.model, journal.clone(), claim.clone()).await;
        let (mut process, transport) = match spawned {
            Ok(value) => value,
            Err(reason) => {
                journal.transition(&claim, &["prepared"], "not_sent", None, false).map_err(chat::error)?;
                journal.record_drained(&claim).map_err(chat::error)?;
                crate::request_usage::record_acp_attempt(&parent, &agent.id, &call, &kind, &claim.attempt_id, "not_sent", false);
                return Err(chat::error(reason));
            }
        };
        let mut answer = String::new();
        let outcome = async {
            let connect = SessionService::connect(transport, journal.clone(), claim.clone(), &agent.model, vec![],
                deadline.min(tokio::time::Instant::now() + Duration::from_secs(30)), |_| Ok(()));
            let mut session = tokio::select! { biased;
                _ = cancellation.cancelled() => return Err(chat::error("内部 ACP 配置期间已停止。")),
                result = connect => result.map_err(|error| chat::error(error.to_string()))?,
            };
            session.set_deadline(deadline);
            // 不用远端工具通知伪造动作；原生工具请求在 SessionService 中统一拒绝。
            let result = session.prompt_content(content, cancellation.clone(), Duration::from_secs(5), |event| {
                if !event.replay {
                    match event.update {
                        Update::Text { text, thought: false, .. } => answer.push_str(&text),
                        Update::ToolObservation { .. } => return Err(std::io::Error::other("内部 ACP 出现工具调用；未执行。")),
                        _ => {}
                    }
                }
                Ok(())
            }).await.map_err(|error| chat::error(error.to_string()))?;
            if result.cancel_requested || result.stop_reason != "end_turn"
                || result.model.effective.as_deref() != Some(agent.model.as_str()) || answer.trim().is_empty() {
                return Err(chat::error(format!("内部 ACP 没有完整有效回复：{}。", result.stop_reason)));
            }
            Ok(())
        }.await;
        let final_state = journal.status(&claim).and_then(|status| {
            if status.state == "prepared" {
                journal.transition(&claim, &["prepared"], "not_sent", None, false)
            } else { Ok(()) }
        });
        let drained = process.drain().await;
        final_state.map_err(chat::error)?;
        let status = journal.status(&claim).map_err(chat::error)?;
        let succeeded = outcome.is_ok() && drained.is_ok();
        crate::request_usage::record_acp_attempt(&parent, &agent.id, &call, &kind, &claim.attempt_id,
            if succeeded { "completed" } else if status.state == "not_sent" { "not_sent" } else { "remote_unknown" },
            matches!(status.state.as_str(), "submitted" | "terminal" | "unknown"));
        drained.map_err(chat::error)?;
        outcome?;
        Ok(api::MessageResponse { id: None, kind: "message".into(), role: "assistant".into(),
            content: vec![api::OutputContentBlock::Text { text: answer }], model: agent.model.clone(),
            stop_reason: Some("end_turn".into()), stop_sequence: None, request_id: None,
            // ACP 没有输入/输出细分用量；统计账本的 known_mask=0，零值不能当真实消耗。
            usage: api::Usage { input_tokens: 0, output_tokens: 0, cache_creation_input_tokens: 0, cache_read_input_tokens: 0 } })
    };
    tokio::pin!(execute);
    tokio::select! { biased;
        result = &mut execute => result,
        _ = async { loop { if cancelled() { break; } tokio::time::sleep(Duration::from_millis(50)).await; } } => {
            cancellation.request();
            let _ = execute.await;
            Err(chat::error("内部 ACP 已停止；迟到回复不产生动作。"))
        }
        _ = tokio::time::sleep_until(deadline) => {
            cancellation.request_timeout();
            let _ = execute.await;
            Err(chat::error("内部 ACP 已超过父预算；迟到回复不产生动作。"))
        }
    }
}
