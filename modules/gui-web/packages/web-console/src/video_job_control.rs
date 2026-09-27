//! 视频等待有自己的停止令牌；父聊天返回 pending 后仍可停止等待，不声称取消云端生成。
use std::{collections::HashMap, sync::{Arc, Mutex, OnceLock}};
use axum::{extract::Path, Json};

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VideoJobScope {
    pub(crate) workspace_id: String,
    pub(crate) session_id: String,
    #[serde(rename = "chat_room_id")]
    pub(crate) room_id: String,
}
struct Entry { scope: VideoJobScope, cancellation: Arc<crate::ChatTurnCancellation> }
fn jobs() -> &'static Mutex<HashMap<String, Entry>> {
    static JOBS: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) struct Registration {
    key: String,
    pub(crate) cancellation: Arc<crate::ChatTurnCancellation>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut jobs) = jobs().lock() {
            if jobs.get(&self.key).is_some_and(|entry| Arc::ptr_eq(&entry.cancellation, &self.cancellation)) {
                jobs.remove(&self.key);
            }
        }
    }
}

pub(crate) fn register(key: &str, scope: VideoJobScope) -> Result<Registration, String> {
    if [key, &scope.workspace_id, &scope.session_id, &scope.room_id].iter().any(|part| part.trim().is_empty()) {
        return Err("缺少视频任务真实工程、模型会话或聊天室标识，未发送新请求".into());
    }
    let mut jobs = jobs().lock().map_err(|_| "视频控制状态不可用，未发送新请求")?;
    if jobs.len() >= 128 { return Err("同时等待的视频任务过多，未发送新请求".into()); }
    if jobs.contains_key(key) { return Err("视频任务标识已存在，未重复提交".into()); }
    let cancellation = Arc::new(crate::ChatTurnCancellation::new());
    jobs.insert(key.to_string(), Entry {scope, cancellation:Arc::clone(&cancellation)});
    Ok(Registration {key:key.to_string(), cancellation})
}

pub(crate) fn public_scope(key: &str) -> Option<VideoJobScope> {
    let workspace_id = crate::workspace_identity(&crate::active_workspace_path());
    jobs().lock().ok()?.get(key).filter(|entry| entry.scope.workspace_id == workspace_id)
        .map(|entry| entry.scope.clone())
}

pub(crate) fn can_interrupt(key: &str) -> bool {
    let workspace_id = crate::workspace_identity(&crate::active_workspace_path());
    jobs().lock().ok().and_then(|jobs| jobs.get(key).map(|entry|
        entry.scope.workspace_id == workspace_id && !entry.cancellation.is_requested())).unwrap_or(false)
}

#[derive(serde::Deserialize)]
pub(crate) struct InterruptRequest { session_id: String, chat_room_id: String }
#[derive(serde::Serialize)]
pub(crate) struct InterruptResponse {
    task_id: String,
    status: &'static str,
    outcome: &'static str,
    cloud_cancellation_confirmed: bool,
    idempotent: bool,
}

pub(crate) async fn interrupt(Path(task_id): Path<String>, Json(payload): Json<InterruptRequest>)
    -> crate::ApiResult<(crate::StatusCode, Json<InterruptResponse>)> {
    let workspace_id = crate::workspace_identity(&crate::active_workspace_path());
    let jobs = jobs().lock().map_err(|_| crate::api_error(crate::StatusCode::INTERNAL_SERVER_ERROR, "视频控制状态不可用"))?;
    let Some(entry) = jobs.get(&task_id).filter(|entry| entry.scope.workspace_id == workspace_id
        && entry.scope.session_id == payload.session_id && entry.scope.room_id == payload.chat_room_id) else {
        return Err(crate::api_error(crate::StatusCode::NOT_FOUND, "未找到此会话仍在等待的视频任务；请刷新其结果状态"));
    };
    let requested = entry.cancellation.request();
    Ok((crate::StatusCode::ACCEPTED, Json(InterruptResponse {
        task_id, status:"stop_requested", outcome:"local_wait_stop_requested",
        cloud_cancellation_confirmed:false, idempotent:!requested,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn wrong_scope_cannot_stop_job_and_finished_registration_does_not_accept_cancel() {
        let _guard=crate::tests::config_test_guard();
        let workspace_id=crate::workspace_identity(&crate::active_workspace_path());
        let registration=register("video-control-test",VideoJobScope {workspace_id,session_id:"owner".into(),room_id:"room".into()}).unwrap();
        assert!(interrupt(Path("video-control-test".into()),Json(InterruptRequest {session_id:"other".into(),chat_room_id:"room".into()})).await.is_err());
        assert!(!registration.cancellation.is_requested());
        let (status,Json(result))=interrupt(Path("video-control-test".into()),Json(InterruptRequest {session_id:"owner".into(),chat_room_id:"room".into()})).await.unwrap();
        assert_eq!(status,crate::StatusCode::ACCEPTED); assert!(!result.cloud_cancellation_confirmed);
        tokio::time::timeout(std::time::Duration::from_secs(1),registration.cancellation.cancelled()).await.unwrap();
        assert!(registration.cancellation.is_requested());
        drop(registration);
        assert!(interrupt(Path("video-control-test".into()),Json(InterruptRequest {session_id:"owner".into(),chat_room_id:"room".into()})).await.is_err());
    }
}
