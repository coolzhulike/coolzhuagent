//! 远端被用户删除后的显式重新绑定入口；不清空聊天室、不自动补建会话。
use super::journal::{Journal, RemoteBinding};
use axum::{extract::Query, http::{HeaderMap, StatusCode}, routing::get, Json, Router};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BindingQuery { session_id: String }

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RebindRequest {
    session_id: String,
    room_id: String,
    expected_bindings: Vec<RemoteBinding>,
}

#[derive(Serialize)]
pub(crate) struct BindingView {
    session_id: String,
    room_id: String,
    bindings: Vec<RemoteBinding>,
}

pub(crate) fn routes() -> Router {
    Router::new().route("/api/backends/devin/binding", get(status).post(rebind))
}

fn current_scope(session: &str, expected_room: Option<&str>) -> crate::ApiResult<(Journal, String, String)> {
    let store = crate::session_store().lock().map_err(|_|
        crate::api_error(StatusCode::INTERNAL_SERVER_ERROR,"会话存储不可用。"))?;
    let saved = store.find_session(session).ok_or_else(|| crate::api_error(StatusCode::NOT_FOUND,"模型会话不存在。"))?;
    if crate::agent_session_backend::AgentSessionBackend::for_provider(&saved.provider)
        != crate::agent_session_backend::AgentSessionBackend::DevinAcp {
        return Err(crate::api_error(StatusCode::BAD_REQUEST,"此会话没有使用 Devin。"));
    }
    let room = store.active_chat_room_id().ok_or_else(|| crate::api_error(StatusCode::NOT_FOUND,"尚未选择聊天室。"))?;
    if expected_room.is_some_and(|expected| expected != room) {
        return Err(crate::api_error(StatusCode::CONFLICT,"聊天室已切换，请刷新后重新操作。"));
    }
    let workspace = crate::workspace_identity(&crate::active_workspace_path());
    let journal = Journal::open(&store.path).map_err(|reason| crate::api_error(StatusCode::INTERNAL_SERVER_ERROR,&reason))?;
    Ok((journal,workspace,room))
}

async fn status(Query(query): Query<BindingQuery>) -> crate::ApiResult<Json<BindingView>> {
    let (journal,workspace,room) = current_scope(&query.session_id,None)?;
    let bindings = journal.remote_bindings(&workspace,&room,&query.session_id)
        .map_err(|reason| crate::api_error(StatusCode::INTERNAL_SERVER_ERROR,&reason))?;
    Ok(Json(BindingView { session_id:query.session_id,room_id:room,bindings }))
}

async fn rebind(headers: HeaderMap, Json(request): Json<RebindRequest>) -> crate::ApiResult<Json<BindingView>> {
    super::auth::require_local_action(&headers)?;
    let (journal,workspace,room) = current_scope(&request.session_id,Some(&request.room_id))?;
    journal.detach_remote_bindings(&workspace,&room,&request.session_id,&request.expected_bindings)
        .map_err(|reason| crate::api_error(StatusCode::CONFLICT,&reason))?;
    let bindings = journal.remote_bindings(&workspace,&room,&request.session_id)
        .map_err(|reason| crate::api_error(StatusCode::INTERNAL_SERVER_ERROR,&reason))?;
    Ok(Json(BindingView { session_id:request.session_id,room_id:room,bindings }))
}
