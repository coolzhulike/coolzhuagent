//! poll 定时投递的房间解析；计划领取、权限生命周期及聊天执行各由原模块负责。
use crate::{
    api_error,
    scheduled_execution::{self, ScheduledExecutionGrant},
    ApiResult, ConfigScheduledTask, SendMessageRequest, SessionStore, StatusCode,
    SCHEDULED_TASK_SYSTEM_ROOM_ID,
};
use std::{sync::Arc, time::Duration};

pub(super) fn validate_room(
    store: &SessionStore,
    requested: Option<&str>,
    task_kind: &str,
) -> ApiResult<Option<String>> {
    let Some(requested) = requested else {
        return Ok(None);
    };
    if task_kind.trim() == "goal" {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "目标推进型沿用目标房间，不接受轮询结果聊天室",
        ));
    }
    let room_id = requested.trim();
    if room_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "请选择定时任务的结果聊天室",
        ));
    }
    if !store.state.chat_rooms.iter().any(|room| room.id == room_id) {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "定时任务的结果聊天室不存在或不属于当前工程",
        ));
    }
    Ok(Some(room_id.to_string()))
}

fn prepare(
    task: &ConfigScheduledTask,
    workspace_id: String,
    store: &mut SessionStore,
) -> ApiResult<(Option<Arc<ScheduledExecutionGrant>>, SendMessageRequest)> {
    if !store
        .state
        .sessions
        .iter()
        .any(|session| session.id == task.target_session_id)
    {
        return Err(api_error(
            StatusCode::NOT_FOUND,
            "scheduled task target session does not exist",
        ));
    }
    let room_id = match validate_room(store, task.chat_room_id.as_deref(), &task.task_kind)? {
        Some(room_id) => room_id,
        None => {
            // 只兼容没有绑定字段的旧 poll 任务，不为失效的显式目标创建替代房间。
            store.ensure_scheduled_task_system_room()?;
            SCHEDULED_TASK_SYSTEM_ROOM_ID.to_string()
        }
    };
    let grant = task
        .permissions
        .iter()
        .any(|permission| permission == "full-access")
        .then(|| {
            ScheduledExecutionGrant::new(
                workspace_id.clone(),
                task.target_session_id.clone(),
                room_id.clone(),
                Duration::from_secs(300),
            )
        });
    Ok((
        grant,
        SendMessageRequest {
            expected_workspace_id: Some(workspace_id),
            native_browser_panel: false,
            session_id: Some(task.target_session_id.clone()),
            chat_room_id: Some(room_id),
            target_agent_ids: vec![task.target_session_id.clone()],
            text: task.content.clone(),
            selected_message_ids: None,
            attachments: None,
        },
    ))
}

pub(super) async fn deliver(task: &ConfigScheduledTask) -> ApiResult<()> {
    // 调度器已持有工程 pin；短锁内复核房间和 Agent，不跨模型 await 持锁。
    let (grant, request) = {
        let mut store = crate::session_store().lock().map_err(|_| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "session store lock failed",
            )
        })?;
        prepare(
            task,
            crate::workspace_identity(&crate::active_workspace_path()),
            &mut store,
        )?
    };
    let crate::Json(response) =
        scheduled_execution::run(grant, crate::api_chat_send(crate::Json(request))).await?;
    require_completed(&response)
}

fn require_completed(response: &crate::SendMessageResponse) -> ApiResult<()> {
    // 非流式 HTTP 200 也可能带 failed/interrupted/commit_pending，不能按成功重排。
    if response.execution_failed
        || !response
            .runtime
            .as_ref()
            .is_some_and(|runtime| runtime.status == crate::ChatTurnStatus::Completed)
    {
        let state = match response.runtime.as_ref().map(|runtime| runtime.status) {
            Some(crate::ChatTurnStatus::Failed) => "失败",
            Some(crate::ChatTurnStatus::Interrupted) => "已中断",
            Some(crate::ChatTurnStatus::CommitPending) => "终态提交尚未确认",
            Some(crate::ChatTurnStatus::Running | crate::ChatTurnStatus::InterruptRequested) => {
                "尚未结束"
            }
            Some(crate::ChatTurnStatus::Completed) => "执行结果标记为失败",
            None => "缺少终态回执",
        };
        return Err(api_error(
            StatusCode::CONFLICT,
            &format!("定时会话{state}，未按成功推进计划；请查看原聊天室运行轨迹，不自动重试"),
        ));
    }
    Ok(())
}

pub(super) fn append_status_best_effort(task: &ConfigScheduledTask, status: &str, detail: &str) {
    if let Ok(mut store) = crate::session_store().lock() {
        match task.chat_room_id.as_deref() {
            Some(room_id) => {
                let _ = store.append_scheduled_task_status_in_room(room_id, status, detail);
            }
            None => {
                let _ = store.append_scheduled_task_status_message(status, detail);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 领取指纹取完整序列化字节。此旧格式应逐字节保持，而非仅 JSON 语义相等。
    const LEGACY: &str = r#"{"id":"old-task","target_session_id":"agent","content":"任务","run_at_ms":1,"interval_ms":null,"permissions":["full-access"],"status":"scheduled","created_at_ms":0,"last_run_at_ms":null,"last_error":null,"schedule_kind":"once","wall_hour":0,"wall_minute":0,"weekdays":[],"tz_offset_minutes":0,"task_kind":"poll","goal_id":null}"#;

    #[test]
    fn legacy_claim_fingerprint_bytes_remain_unchanged() {
        let mut task: ConfigScheduledTask = serde_json::from_str(LEGACY).unwrap();
        assert_eq!(serde_json::to_vec(&task).unwrap(), LEGACY.as_bytes());
        task.chat_room_id = Some("original-room".into());
        assert_ne!(serde_json::to_vec(&task).unwrap(), LEGACY.as_bytes());
    }

    #[test]
    fn http_success_without_completed_run_cannot_advance_schedule() {
        use crate::{chat_run_admission::ChatRunReceipt, ChatTurnStatus};
        let mut response = crate::SendMessageResponse {
            accepted_agent_ids: vec!["agent".into()],
            messages: Vec::new(),
            tasks: Vec::new(),
            notes: Vec::new(),
            runtime: None,
            execution_failed: false,
        };
        assert!(require_completed(&response).is_err());
        for status in [
            ChatTurnStatus::Running,
            ChatTurnStatus::InterruptRequested,
            ChatTurnStatus::Interrupted,
            ChatTurnStatus::Failed,
            ChatTurnStatus::CommitPending,
        ] {
            response.runtime = Some(ChatRunReceipt {
                turn_id: "turn".into(),
                run_id: "run".into(),
                status,
            });
            assert!(require_completed(&response).is_err());
        }
        response.runtime.as_mut().unwrap().status = ChatTurnStatus::Completed;
        assert!(require_completed(&response).is_ok());
        response.execution_failed = true;
        assert!(require_completed(&response).is_err());
    }

    #[test]
    fn bound_delivery_uses_exact_scope_and_deleted_room_never_creates_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = SessionStore {
            history_edits: Vec::new(),
            committed_state: None,
            path: temp.path().join("sessions.sqlite3"),
            legacy_json_path: temp.path().join("sessions.json"),
            capacity: crate::SessionStoreCapacity::default(),
            state: crate::PersistedSessionState {
                sessions: vec![serde_json::from_value(serde_json::json!({
                    "id":"agent","name":"agent","provider":"Devin","model":"SWE-2-medium",
                    "api_key_ref":"","created_at":0,"updated_at":0
                }))
                .unwrap()],
                chat_rooms: vec![crate::PersistedChatRoom {
                    id: "original-room".into(),
                    name: "原房间".into(),
                    created_at: 0,
                    updated_at: 0,
                    messages: Vec::new(),
                }],
                active_chat_room_id: Some("original-room".into()),
                ..Default::default()
            },
        };
        let mut task: ConfigScheduledTask = serde_json::from_str(LEGACY).unwrap();
        task.chat_room_id = Some("original-room".into());
        let (grant, request) = prepare(&task, "workspace-a".into(), &mut store).unwrap();
        assert_eq!(request.chat_room_id.as_deref(), Some("original-room"));
        assert_eq!(
            request.expected_workspace_id.as_deref(),
            Some("workspace-a")
        );
        assert_eq!(request.target_agent_ids, ["agent"]);
        assert!(!request.native_browser_panel);
        let grant = grant.unwrap();
        assert!(grant.authorizes("workspace-a", Some("agent"), Some("original-room")));
        assert!(!grant.authorizes(
            "workspace-a",
            Some("agent"),
            Some(SCHEDULED_TASK_SYSTEM_ROOM_ID)
        ));
        assert!(!grant.authorizes("workspace-b", Some("agent"), Some("original-room")));
        for (room, kind, status) in [
            ("", "poll", StatusCode::BAD_REQUEST),
            ("foreign-room", "poll", StatusCode::NOT_FOUND),
            ("original-room", "goal", StatusCode::BAD_REQUEST),
        ] {
            assert_eq!(
                validate_room(&store, Some(room), kind).unwrap_err().0,
                status
            );
        }
        store.state.chat_rooms.clear();
        assert_eq!(
            prepare(&task, "workspace-a".into(), &mut store)
                .err()
                .unwrap()
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            store
                .append_scheduled_task_status_in_room("original-room", "failed", "目标已删除")
                .unwrap_err()
                .0,
            StatusCode::NOT_FOUND
        );
        assert!(store.state.chat_rooms.is_empty());
        assert_eq!(
            store.state.active_chat_room_id.as_deref(),
            Some("original-room")
        );
        assert!(store.state.sessions[0].messages.is_empty());
        assert!(!store.path.exists());

        // 旧任务没有绑定字段，仍幂等创建系统房间，不切换用户正在查看的房间。
        store.state.chat_rooms.push(crate::PersistedChatRoom {
            id: "original-room".into(),
            name: "原房间".into(),
            created_at: 0,
            updated_at: 0,
            messages: Vec::new(),
        });
        task.chat_room_id = None;
        let (legacy_grant, legacy_request) =
            prepare(&task, "workspace-a".into(), &mut store).unwrap();
        assert_eq!(
            legacy_request.chat_room_id.as_deref(),
            Some(SCHEDULED_TASK_SYSTEM_ROOM_ID)
        );
        assert!(legacy_grant.unwrap().authorizes(
            "workspace-a",
            Some("agent"),
            Some(SCHEDULED_TASK_SYSTEM_ROOM_ID)
        ));
        assert_eq!(store.state.chat_rooms.len(), 2);
        assert_eq!(
            store.state.active_chat_room_id.as_deref(),
            Some("original-room")
        );
        prepare(&task, "workspace-a".into(), &mut store).unwrap();
        assert_eq!(store.state.chat_rooms.len(), 2);
        assert!(store.state.sessions[0].messages.is_empty());
    }
}
