//! 聊天传输共享的真实运行接纳与非流式收尾；模型、工具和权限仍由既有执行链负责。
use std::{path::PathBuf, sync::Arc};
use crate::{ApiResult, ChatTurnCancellation, ChatTurnGuard, ChatTurnStatus, FrozenParentContext,
    PreparedChatDispatch, SendMessageResponse, root_execution_budget::{self, RootExecutionBudget}};

#[derive(Debug, serde::Serialize)]
pub(crate) struct ChatRunReceipt {
    pub(crate) turn_id: String,
    pub(crate) run_id: String,
    pub(crate) status: ChatTurnStatus,
}

pub(crate) struct AcceptedChatRun {
    pub(crate) turn_id: String,
    pub(crate) run_id: String,
    pub(crate) claim_token: String,
    pub(crate) db_path: PathBuf,
    pub(crate) cancellation: Arc<ChatTurnCancellation>,
    pub(crate) root_budget: RootExecutionBudget,
    pub(crate) parent: Option<FrozenParentContext>,
    pub(crate) guard: ChatTurnGuard,
}

/// 在任何模型/工具执行之前建 durable run。SSE 和非流式使用同一个入口与 Drop 收尾。
pub(crate) fn accept(result: &PreparedChatDispatch, entry: &'static str) -> ApiResult<AcceptedChatRun> {
    let root_budget = root_execution_budget::current().unwrap_or_else(||
        RootExecutionBudget::for_session(result.conversation_session_id.as_deref()));
    if root_budget.is_expired() {
        return Err(crate::api_error(crate::StatusCode::REQUEST_TIMEOUT, root_execution_budget::EXPIRED_REASON));
    }
    let turn_id = crate::new_chat_turn_id();
    let db_path = result.db_path.clone();
    let workspace_id = result.workspace_id.clone();
    let (run_id, claim_token) = crate::new_runtime_run_identifiers()
        .map_err(|error| crate::api_error(crate::StatusCode::INTERNAL_SERVER_ERROR, &error))?;
    crate::create_chat_runtime_run_sqlite(&db_path, &run_id, &claim_token, &workspace_id,
        result.conversation_session_id.as_deref(), &result.chat_room_id, &turn_id)
        .map_err(crate::sqlite_api_error)?;
    let cancellation = crate::register_chat_turn_with_run(&turn_id, Some(run_id.clone()),
        result.conversation_session_id.clone(), result.chat_room_id.clone());
    let mut guard = ChatTurnGuard::new_runtime(turn_id.clone(), run_id.clone(), claim_token.clone(), db_path.clone())
        .with_workspace_pin(Arc::clone(&result.workspace_pin));
    if let Some(scope) = crate::chat_run_scope_for(&workspace_id, &result.chat_room_id,
        result.conversation_session_id.as_deref(), &turn_id, &run_id) {
        guard = guard.with_scope(scope);
    }
    let mut parent = crate::frozen_parent_context_at(entry, &workspace_id, Some(&result.chat_room_id),
        result.conversation_session_id.as_deref(), Some(&turn_id), Some(&run_id));
    if let Some(parent) = parent.as_mut() {
        parent.root_budget = Some(root_budget.clone());
        parent.runtime_db_path = Some(db_path.clone());
        parent.computer_use_turn_scope = result.computer_use_turn_scope.clone();
        if parent.computer_use_turn_scope.native_browser() {
            parent.native_browser_binding = Some(crate::native_browser_host::capture_panel_binding(parent));
        }
        let snapshots = result.targets.iter().filter_map(|agent| {
            match crate::host_child_agent::HostModelSnapshot::capture(
                agent, Some(&result.chat_room_id), parent,
            ) {
                Ok(snapshot) => Some((agent.id.clone(), Arc::new(snapshot))),
                Err(error) => {
                    crate::diag_log(&format!("[HOST-AGENT] 接纳时无法解析会话 {} 的子执行配置：{error}", agent.id));
                    None
                }
            }
        }).collect();
        parent.host_model_snapshots = Arc::new(snapshots);
    }
    crate::chat_insights::record_source_messages(&db_path, &run_id, &result.messages).map_err(crate::sqlite_api_error)?;
    Ok(AcceptedChatRun { turn_id, run_id, claim_token, db_path, cancellation, root_budget, parent, guard })
}

pub(crate) async fn run_nonstream(result: PreparedChatDispatch, force_relay: bool)
    -> ApiResult<crate::Json<SendMessageResponse>> {
    let AcceptedChatRun { turn_id, run_id, claim_token, db_path, cancellation,
        root_budget, parent, mut guard } = accept(&result, if force_relay { "chat-relay" } else { "chat-send" })?;
    let scoped_budget = root_budget.clone();
    root_execution_budget::scope(scoped_budget, Box::pin(async move {
        let accepted_agent_ids = result.targets.iter().map(|agent| agent.id.clone()).collect::<Vec<_>>();
        let initial_messages = result.messages.clone();
        let initial_tasks = result.tasks.clone();
        let work = async {
            if !crate::start_chat_runtime_run_sqlite(&db_path, &run_id, &claim_token).map_err(crate::sqlite_api_error)? {
                return Err(crate::api_error(crate::StatusCode::CONFLICT, "聊天运行已停止或不再属于当前执行者"));
            }
            // 用户输入先持久化；中断不会丢失输入，后续 append_once 沿用原消息 id 去重。
            crate::persist_chat_dispatch(&result.chat_room_id, result.conversation_session_id.as_deref(),
                &result.targets, result.messages.clone())?;
            crate::run_prepared_chat_dispatch(result, force_relay, parent).await
        };
        let outcome = crate::await_chat_turn(&cancellation,
            crate::CHAT_CANCELLATION.scope(cancellation.clone(), work)).await;
        let expired = root_budget.is_expired();
        let (mut response, requested) = match outcome {
            Ok(Ok(crate::Json(response))) => {
                let status = if response.execution_failed { ChatTurnStatus::Failed } else { ChatTurnStatus::Completed };
                (response, status)
            },
            failure => {
                let (reason, status) = if expired {
                    (root_execution_budget::EXPIRED_REASON.to_string(), ChatTurnStatus::Failed)
                } else if cancellation.is_timed_out() {
                    ("接力步骤执行时限已到，已停止本轮；可能发生的工具结果未确认，不自动重试。".to_string(), ChatTurnStatus::Failed)
                } else if cancellation.is_requested() {
                    ("本轮已停止；已经发生的工具结果仍保留，不自动重放。".to_string(), ChatTurnStatus::Interrupted)
                } else {
                    let detail = match failure {
                        Ok(Err(error)) => crate::api_error_message(error),
                        _ => "聊天执行等待已结束，结果未确认".to_string(),
                    };
                    (detail, ChatTurnStatus::Failed)
                };
                (SendMessageResponse { accepted_agent_ids, messages: initial_messages, tasks: initial_tasks,
                    notes: vec![reason], runtime: None, execution_failed: true }, status)
            }
        };
        // 收尾不放进已到期的业务 timeout；提交失败明确返回 commit_pending。
        let status = guard.finish(if expired { ChatTurnStatus::Failed } else { requested });
        response.runtime = Some(ChatRunReceipt { turn_id, run_id, status });
        if status == ChatTurnStatus::Completed {
            crate::emit_backend_pet_event("chat.completed", Some("Chat response completed"), "chat");
        } else {
            crate::emit_backend_pet_event("chat.failed", response.notes.last().map(String::as_str), "chat");
        }
        Ok(crate::Json(response))
    })).await
}
