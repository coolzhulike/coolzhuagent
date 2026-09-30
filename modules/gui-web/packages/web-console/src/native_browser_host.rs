//! 原生宿主资源登记：独立凭据、短租约、后台工程/房间核验。
//! 本阶段不提供快照或输入接口，也不回退 Chrome；登记成功不是 Browser Use 验收。
use std::{path::PathBuf, sync::{Mutex, OnceLock}, time::{Duration, Instant}};
use axum::{extract::DefaultBodyLimit, http::{HeaderMap, StatusCode}, routing::post, Json, Router};
use native_browser_protocol::{token_filename, HostReceipt, HostState, PanelResource, LEASE_MILLIS, MAX_STATE_BYTES, STATE_PATH};

struct RegisteredHost {
    host_id: String,
    sequence: u64,
    seen: Instant,
    resource: Option<(crate::CanonicalWorkspaceId, PanelResource)>,
}

fn registry() -> &'static Mutex<Option<RegisteredHost>> {
    static REGISTRY: OnceLock<Mutex<Option<RegisteredHost>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(None))
}

fn token_path() -> Option<PathBuf> {
    #[cfg(test)]
    {
        // 路由测试不得轮换真实宿主凭据；所有测试副作用只进仓库忽略目录。
        return std::env::current_dir().ok().map(|root| root.join("tmp")
            .join(format!("native-browser-host-tests-{}", std::process::id())).join(token_filename(0)));
    }
    #[cfg(not(test))]
    {
        let port = crate::config_web_bind_addr().parse::<std::net::SocketAddr>().ok()?.port();
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
            .map(|root| root.join("CoolzhuAgent/runtime").join(token_filename(port)))
    }
}

fn host_token() -> &'static Result<String, ()> {
    static TOKEN: OnceLock<Result<String, ()>> = OnceLock::new();
    TOKEN.get_or_init(|| {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| ())?;
        let token: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = token_path().ok_or(())?;
        std::fs::create_dir_all(path.parent().ok_or(())?).map_err(|_| ())?;
        std::fs::write(path, token.as_bytes()).map_err(|_| ())?;
        Ok(token)
    })
}

pub(super) fn routes() -> Router {
    // 在服务启动时初始化，不能让网页请求决定凭据生成时机。
    let _ = host_token();
    Router::new().route(STATE_PATH, post(register).layer(DefaultBodyLimit::max(MAX_STATE_BYTES)))
}

fn authenticated(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get("authorization").and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer ")) else { return false; };
    let Ok(token) = host_token() else { return false; };
    authenticated_token(value, token)
}

fn authenticated_token(value: &str, token: &str) -> bool {
    value.len() == 64 && value == token
}

fn accepts_sequence(current: &RegisteredHost, state: &HostState, elapsed: Duration) -> bool {
    elapsed >= Duration::from_millis(LEASE_MILLIS)
        || (current.host_id == state.host_id && state.sequence > current.sequence)
}

fn resolve_resource(resource: PanelResource) -> Result<(crate::CanonicalWorkspaceId, PanelResource), StatusCode> {
    if !resource.valid_shape() { return Err(StatusCode::BAD_REQUEST); }
    let workspace = crate::active_workspace_path();
    let expected = workspace.canonicalize().map_err(|_| StatusCode::CONFLICT)?;
    let claimed = PathBuf::from(&resource.workspace_path).canonicalize().map_err(|_| StatusCode::CONFLICT)?;
    if claimed != expected { return Err(StatusCode::CONFLICT); }
    let id = crate::canonical_workspace_identity(&crate::workspace_identity(&expected))
        .map_err(|_| StatusCode::CONFLICT)?;
    // 冻结该工程的数据库；不用“形状合法”替代实际房间关系，也不创建/迁移数据库。
    let db = crate::default_session_sqlite_path();
    let connection = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| StatusCode::CONFLICT)?;
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM chat_rooms WHERE id = ?1)", [&resource.room_id], |row| row.get(0),
    ).map_err(|_| StatusCode::CONFLICT)?;
    if !exists || crate::workspace_identity(&crate::active_workspace_path()) != id.as_str()
        || crate::default_session_sqlite_path() != db {
        return Err(StatusCode::CONFLICT);
    }
    Ok((id, resource))
}

async fn register(headers: HeaderMap, Json(state): Json<HostState>) -> Result<Json<HostReceipt>, StatusCode> {
    if !authenticated(&headers) { return Err(StatusCode::UNAUTHORIZED); }
    if !state.valid_shape() { return Err(StatusCode::BAD_REQUEST); }
    let mut registered = registry().lock().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if let Some(current) = registered.as_ref() {
        if !accepts_sequence(current, &state, current.seen.elapsed()) {
            return Err(StatusCode::CONFLICT);
        }
    }
    // 一旦环境校验失败立即撤销旧登记；不能在错误后仍给旧资源续约。
    let resource = match state.resource.map(resolve_resource).transpose() {
        Ok(resource) => resource,
        Err(error) => { *registered = None; return Err(error); }
    };
    *registered = Some(RegisteredHost {host_id:state.host_id, sequence:state.sequence, seen:Instant::now(), resource});
    let resource_registered = registered.as_ref().is_some_and(|host| host.resource.is_some());
    Ok(Json(HostReceipt {accepted:true, resource_registered}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_host_auth_never_accepts_empty_or_chrome_nonce() {
        assert!(!authenticated(&HeaderMap::new()));
        // 纯函数验证，不初始化凭据或改动运行中宿主的令牌文件。
        assert!(!authenticated_token("chrome-nonce-is-not-a-native-host-token", &"a".repeat(64)));
        assert!(!authenticated_token("", ""));
    }

    #[test]
    fn panel_resource_rejects_chrome_tab_and_unknown_host_fields() {
        let mut resource = PanelResource {workspace_path:"workspace".into(), room_id:"room-1".into(), label:"browser-panel-7".into(), generation:7, navigation_revision:1};
        assert!(resource.valid_shape());
        resource.label = "7".into();
        assert!(!resource.valid_shape());
        assert!(serde_json::from_str::<HostState>(r#"{"host_id":"host-012345678901","sequence":1,"resource":null,"javascript":"alert(1)"}"#).is_err());
    }

    #[test]
    fn native_host_lease_rejects_replay_and_competing_host() {
        let current = RegisteredHost {host_id:"native-host-0123456789".into(), sequence:7,
            seen:Instant::now(), resource:None};
        let mut state = HostState {host_id:current.host_id.clone(), sequence:7, resource:None};
        assert!(!accepts_sequence(&current, &state, Duration::ZERO));
        state.sequence = 8;
        assert!(accepts_sequence(&current, &state, Duration::ZERO));
        state.host_id = "native-other-0123456789".into();
        assert!(!accepts_sequence(&current, &state, Duration::ZERO));
        assert!(accepts_sequence(&current, &state, Duration::from_millis(LEASE_MILLIS)));
    }
}
