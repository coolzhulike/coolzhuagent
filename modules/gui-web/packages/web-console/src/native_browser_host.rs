//! 原生宿主资源登记：独立凭据、短租约、后台工程/房间核验。
//! 固定只读观察与短租约；不提供输入接口，也不回退 Chrome，登记不是 Browser Use 验收。
use std::{path::PathBuf, sync::{Mutex, OnceLock}, time::{Duration, Instant}};
use axum::{extract::DefaultBodyLimit, http::{HeaderMap, StatusCode}, routing::post, Json, Router};
use native_browser_protocol::{token_filename, HostIdentity, HostReceipt, HostState, ObservationReply, ObservationRequest,
    PageObservation, PanelResource, LEASE_MILLIS, MAX_STATE_BYTES, MAX_OBSERVATION_BYTES, STATE_PATH, OBSERVATION_PATH};

struct PendingObservation {
    host_id: String,
    request: ObservationRequest,
    delivered: bool,
    sender: std::sync::mpsc::Sender<ObservationReply>,
}

fn pending() -> &'static Mutex<Option<PendingObservation>> {
    static PENDING: OnceLock<Mutex<Option<PendingObservation>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

// 取消、提前返回或展开 panic 均释放自己的请求；迟到回包不能清掉后来者。
struct PendingObservationGuard(String);
impl Drop for PendingObservationGuard {
    fn drop(&mut self) {
        if let Ok(mut pending) = pending().lock() {
            if pending.as_ref().is_some_and(|request| request.request.request_id == self.0) {
                *pending = None;
            }
        }
    }
}

struct RegisteredHost {
    host_id: String,
    identity: Option<HostIdentity>,
    sequence: u64,
    seen: Instant,
    resource: Option<(crate::CanonicalWorkspaceId, PanelResource)>,
}

fn registry() -> &'static Mutex<Option<RegisteredHost>> {
    static REGISTRY: OnceLock<Mutex<Option<RegisteredHost>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(None))
}

fn readable_host(host: Option<&RegisteredHost>) -> Result<&RegisteredHost, &'static str> {
    let host = host.ok_or("native_browser_host_unavailable")?;
    if host.seen.elapsed() >= Duration::from_millis(LEASE_MILLIS) {
        return Err("native_browser_resource_changed");
    }
    if host.resource.is_none() {
        return Err("native_browser_panel_unavailable");
    }
    Ok(host)
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
        .route(OBSERVATION_PATH, post(receive_observation).layer(DefaultBodyLimit::max(MAX_OBSERVATION_BYTES)))
        .merge(crate::native_browser_input::routes())
}

pub(super) fn authenticated(headers: &HeaderMap) -> bool {
    let Some(value) = headers.get("authorization").and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer ")) else { return false; };
    let Ok(token) = host_token() else { return false; };
    authenticated_token(value, token)
}

fn authenticated_token(value: &str, token: &str) -> bool {
    value.len() == 64 && value == token
}

fn accepts_sequence(current: &RegisteredHost, state: &HostState, elapsed: Duration) -> bool {
    if current.host_id == state.host_id {
        // 租约过期不能让同一宿主的旧消息重新变成当前资源。
        state.sequence > current.sequence
    } else {
        elapsed >= Duration::from_millis(LEASE_MILLIS)
    }
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
    if state.identity.as_ref().is_some_and(|identity|
        state.host_id != format!("native-{}", identity.boot_id) || verify_process(identity).is_err()) {
        // 有身份却验证失败时撤销资源，不沿用上一条有效资源。
        let mut registered = registry().lock().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        if registered.as_ref().is_some_and(|host| host.host_id == state.host_id && state.sequence > host.sequence) {
            *registered = Some(RegisteredHost {host_id:state.host_id, identity:None,
                sequence:state.sequence, seen:Instant::now(),resource:None});
        }
        return Err(StatusCode::CONFLICT);
    }
    let mut registered = registry().lock().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if let Some(current) = registered.as_ref() {
        if !accepts_sequence(current, &state, current.seen.elapsed()) {
            return Err(StatusCode::CONFLICT);
        }
    }
    // 一旦环境校验失败立即撤销旧登记；不能在错误后仍给旧资源续约。
    let resource = match state.resource.map(resolve_resource).transpose() {
        Ok(resource) => resource,
        Err(error) => {
            // 撤销资源时仍保留已认证宿主的序号，避免旧有效消息在错误后复活。
            *registered = Some(RegisteredHost {host_id:state.host_id, identity:state.identity, sequence:state.sequence,
                seen:Instant::now(), resource:None});
            return Err(error);
        }
    };
    *registered = Some(RegisteredHost {host_id:state.host_id, identity:state.identity, sequence:state.sequence, seen:Instant::now(), resource});
    let resource_registered = registered.as_ref().is_some_and(|host| host.resource.is_some());
    let observation = pending().lock().ok().and_then(|mut pending| {
        let request = pending.as_mut()?;
        let current = registered.as_ref()?;
        if request.delivered || request.host_id != current.host_id
            || current.resource.as_ref().map(|(_, resource)| resource) != Some(&request.request.resource) {
            return None;
        }
        request.delivered = true;
        Some(request.request.clone())
    });
    let input=registered.as_ref().and_then(|host| crate::native_browser_input::deliver(&host.host_id,host.resource.as_ref().map(|(_,r)|r)));
    Ok(Json(HostReceipt {accepted:true, resource_registered, observation,input}))
}

async fn receive_observation(headers: HeaderMap, Json(reply): Json<ObservationReply>) -> Result<StatusCode, StatusCode> {
    if !authenticated(&headers) { return Err(StatusCode::UNAUTHORIZED); }
    if reply.request_id.len() != 32 || !reply.request_id.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !reply.resource.valid_shape() || reply.observation.is_some() == reply.error.is_some()
        || reply.observation.as_ref().is_some_and(|observation| !observation.valid_shape())
        || reply.error.as_ref().is_some_and(|error| error.len() > 96
            || !error.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'_')) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let registered = registry().lock().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let current = registered.as_ref().ok_or(StatusCode::CONFLICT)?;
    if current.host_id != reply.host_id || current.seen.elapsed() >= Duration::from_millis(LEASE_MILLIS)
        || current.resource.as_ref().map(|(_, resource)| resource) != Some(&reply.resource) {
        return Err(StatusCode::CONFLICT);
    }
    let mut pending = pending().lock().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let request = pending.as_ref().ok_or(StatusCode::CONFLICT)?;
    if !request.delivered || request.host_id != reply.host_id
        || request.request.request_id != reply.request_id || request.request.resource != reply.resource {
        return Err(StatusCode::CONFLICT);
    }
    let request = pending.take().ok_or(StatusCode::CONFLICT)?;
    request.sender.send(reply).map_err(|_| StatusCode::CONFLICT)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) struct NativeObservation {
    pub resource: PanelResource,
    pub page: PageObservation,
    pub host_id: String,
    pub request_id: String,
}

/// 冻结面板身份，不把 URL/导航修订当文档身份；同一面板合法导航后仍须重新观察。
#[derive(Debug, Clone)]
pub(super) struct FrozenPanelBinding {
    host_id: String,
    identity: Option<HostIdentity>,
    workspace: crate::CanonicalWorkspaceId,
    resource: PanelResource,
}

impl FrozenPanelBinding {
    fn matches(&self, host_id: &str, identity: Option<&HostIdentity>, workspace: &crate::CanonicalWorkspaceId, resource: &PanelResource) -> bool {
        self.host_id == host_id && &self.workspace == workspace
            && self.identity.as_ref() == identity
            && self.resource.workspace_path == resource.workspace_path
            && self.resource.room_id == resource.room_id && self.resource.label == resource.label
            && self.resource.generation == resource.generation
    }
}

pub(super) fn capture_panel_binding(parent: &crate::FrozenParentContext) -> Result<FrozenPanelBinding, String> {
    let registered = registry().lock().map_err(|_| "native_browser_unavailable")?;
    let host = readable_host(registered.as_ref()).map_err(str::to_string)?;
    let (workspace, resource) = host.resource.as_ref().ok_or("native_browser_panel_unavailable")?;
    if workspace != &parent.workspace_id || parent.room_id.as_deref() != Some(resource.room_id.as_str()) {
        return Err("native_browser_resource_changed".into());
    }
    Ok(FrozenPanelBinding {host_id:host.host_id.clone(),identity:host.identity.clone(),workspace:workspace.clone(),resource:resource.clone()})
}

/// 输入必须有真实OS宿主身份；旧版无identity仅保留只读兼容。
pub(super) fn input_process(parent:&crate::FrozenParentContext,resource:&PanelResource) -> Result<(String,HostIdentity),String> {
    let frozen=parent.native_browser_binding.as_ref().ok_or("native_browser_binding_missing")?.as_ref().map_err(Clone::clone)?;
    let registered=registry().lock().map_err(|_|"native_browser_unavailable")?;
    let host=readable_host(registered.as_ref()).map_err(str::to_string)?;
    let (workspace,current)=host.resource.as_ref().ok_or("native_browser_unavailable")?;
    if current!=resource || workspace!=&parent.workspace_id || parent.room_id.as_deref()!=Some(resource.room_id.as_str())
        || !frozen.matches(&host.host_id,host.identity.as_ref(),workspace,current) { return Err("native_browser_resource_changed".into()); }
    let process=host.identity.clone().ok_or("native_browser_executor_unverified")?;
    verify_process(&process)?;Ok((host.host_id.clone(),process))
}

/// 登记和每次未来输入消费均须调用；独立认证令牌不替代实际OS实例证据。
pub(super) fn verify_process(identity: &HostIdentity) -> Result<(), String> {
    if !identity.valid_shape() { return Err("native_browser_executor_unverified".into()); }
    #[cfg(windows)]
    {
        let process = windows_process_guard::capture_live_process_identity(identity.pid)
            .map_err(|_| "native_browser_executor_unverified")?;
        let actual = process.image_path().map(PathBuf::from).and_then(|path| path.canonicalize().ok())
            .ok_or("native_browser_executor_unverified")?;
        let claimed = PathBuf::from(&identity.canonical_executable).canonicalize()
            .map_err(|_| "native_browser_executor_unverified")?;
        let own = std::env::current_exe().map_err(|_| "native_browser_executor_unverified")?;
        let sibling = own.parent().map(|parent| parent.join("coolzhu-tauri-shell.exe"))
            .and_then(|path| path.canonicalize().ok());
        // 正式包只信任同bin目录；源码运行沿用固定构建候选目录，不读取网页指定的路径。
        let trusted = if let Some(sibling) = sibling { actual == sibling } else {
            crate::desktop_pet_executable_candidates(std::path::Path::new(env!("CARGO_MANIFEST_DIR")), Some(&own))
                .iter().filter_map(|path| path.canonicalize().ok()).any(|path| path == actual)
        };
        if process.creation_time_filetime() != identity.creation_time_filetime || actual != claimed || !trusted {
            return Err("native_browser_executor_unverified".into());
        }
        Ok(())
    }
    #[cfg(not(windows))]
    { Err("native_browser_executor_unsupported".into()) }
}

// 停止是既有运行的收尾状态，不是工程或房间错绑；身份不匹配仍按原规则拒绝。
fn observation_parent_error(error: crate::FrozenRelationViolation) -> &'static str {
    match error {
        crate::FrozenRelationViolation::ParentRunNotExecutable { state }
            if matches!(state.as_str(), "stop_requested" | "interrupted") => "native_observation_cancelled",
        _ => "native_browser_parent_changed",
    }
}

/// 只允许已有父运行内部调用。未生产接线时没有公共接口能凭页面文本创建请求。
pub(super) fn observe(parent: &crate::FrozenParentContext, remaining: Duration,
    cancelled: &dyn Fn() -> bool) -> Result<NativeObservation, String> {
    let db = parent.runtime_db_path.as_ref().ok_or("native_browser_parent_missing")?;
    let frozen = parent.native_browser_binding.as_ref().ok_or("native_browser_binding_missing")?
        .as_ref().map_err(Clone::clone)?;
    if cancelled() || parent.root_budget.as_ref().is_some_and(|budget| budget.is_expired()) {
        return Err("native_observation_cancelled".into());
    }
    crate::validate_frozen_parent_relations(db, parent).map_err(observation_parent_error)?;
    let mut random = [0u8;16];
    getrandom::fill(&mut random).map_err(|_| "native_observation_unavailable")?;
    let id: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let (sender, receiver) = std::sync::mpsc::channel();
    {
        let registered = registry().lock().map_err(|_| "native_browser_unavailable")?;
        let host = readable_host(registered.as_ref()).map_err(str::to_string)?;
        let (workspace, resource) = host.resource.as_ref().ok_or("native_browser_unavailable")?;
        if host.seen.elapsed() >= Duration::from_millis(LEASE_MILLIS)
            || workspace != &parent.workspace_id || parent.room_id.as_deref() != Some(resource.room_id.as_str())
            || !frozen.matches(&host.host_id, host.identity.as_ref(), workspace, resource) {
            return Err("native_browser_resource_changed".into());
        }
        let mut pending = pending().lock().map_err(|_| "native_observation_unavailable")?;
        if pending.is_some() { return Err("native_observation_busy".into()); }
        *pending = Some(PendingObservation {host_id:host.host_id.clone(),
            request:ObservationRequest {request_id:id.clone(), resource:resource.clone()}, delivered:false, sender});
    }
    let _pending_guard = PendingObservationGuard(id);
    let start = Instant::now();
    let timeout = remaining.min(Duration::from_secs(5));
    let result = loop {
        if cancelled() || parent.root_budget.as_ref().is_some_and(|budget| budget.is_expired()) {
            break Err("native_observation_cancelled".into());
        }
        if start.elapsed() >= timeout { break Err("native_observation_timeout".into()); }
        match receiver.recv_timeout((timeout - start.elapsed().min(timeout)).min(Duration::from_millis(50))) {
            Ok(reply) => {
                if cancelled() || parent.root_budget.as_ref().is_some_and(|budget| budget.is_expired()) {
                    break Err("native_observation_cancelled".into());
                }
                if let Err(error) = crate::validate_frozen_parent_relations(db, parent) {
                    break Err(observation_parent_error(error).into());
                }
                let current_matches = registry().lock().is_ok_and(|registered| registered.as_ref().is_some_and(|host|
                    host.host_id == reply.host_id && host.seen.elapsed() < Duration::from_millis(LEASE_MILLIS)
                    && host.resource.as_ref().is_some_and(|(workspace, resource)| workspace == &parent.workspace_id
                        && resource == &reply.resource && frozen.matches(&host.host_id, host.identity.as_ref(), workspace, resource))));
                if !current_matches { break Err("native_browser_resource_changed".into()); }
                break reply.observation.map(|page| NativeObservation {resource:reply.resource,
                    page, host_id:reply.host_id, request_id:reply.request_id})
                    .ok_or_else(|| reply.error.unwrap_or_else(|| "native_observation_failed".into()));
            },
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
            Err(_) => break Err("native_observation_cancelled".into()),
        }
    };
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_stop_does_not_hide_an_actual_parent_identity_failure() {
        for state in ["stop_requested", "interrupted"] {
            assert_eq!(observation_parent_error(crate::FrozenRelationViolation::ParentRunNotExecutable {
                state: state.into(),
            }), "native_observation_cancelled");
        }
        assert_eq!(observation_parent_error(crate::FrozenRelationViolation::ParentRunRoomMismatch {
            actual: Some("another-room".into()),
        }), "native_browser_parent_changed");
        assert_eq!(observation_parent_error(crate::FrozenRelationViolation::ParentRunNotExecutable {
            state: "completed".into(),
        }), "native_browser_parent_changed");
    }

    #[test]
    fn browser_observation_distinguishes_disconnected_host_from_unavailable_panel() {
        assert_eq!(readable_host(None).err(), Some("native_browser_host_unavailable"));
        let current = RegisteredHost {host_id:"native-host-0123456789".into(), identity:None, sequence:7,
            seen:Instant::now(), resource:None};
        assert_eq!(readable_host(Some(&current)).err(), Some("native_browser_panel_unavailable"));
        let expired = RegisteredHost {seen:Instant::now() - Duration::from_millis(LEASE_MILLIS), ..current};
        assert_eq!(readable_host(Some(&expired)).err(), Some("native_browser_resource_changed"));
    }

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
        assert!(serde_json::from_str::<ObservationRequest>(r#"{"request_id":"01234567890123456789012345678901","resource":null,"method":"Runtime.evaluate"}"#).is_err());
        let oversized = PageObservation {url:"https://example.invalid/".into(), title:String::new(),
            nodes:vec![native_browser_protocol::ObservedNode {role:"button".into(),name:"字".repeat(257)}], truncated:false,
            document_token:None,node_handles:Vec::new(),viewport:None,focused_node_index:None,loading:false,navigation_target:None};
        assert!(!oversized.valid_shape());
    }

    #[test]
    fn frozen_panel_binding_rejects_replacement_without_rejecting_same_panel_navigation() {
        let workspace = crate::canonical_workspace_identity("ws-00000000000000ff").unwrap();
        let resource = PanelResource {workspace_path:"workspace".into(),room_id:"room-1".into(),
            label:"browser-panel-7".into(),generation:7,navigation_revision:1};
        let binding = FrozenPanelBinding {host_id:"native-host-0123456789".into(),identity:None,workspace:workspace.clone(),resource:resource.clone()};
        let mut live = resource.clone();
        live.navigation_revision = 2;
        assert!(binding.matches(&binding.host_id, None, &workspace, &live));
        assert!(!binding.matches("native-host-replacement", None, &workspace, &live));
        live.generation = 8; live.label = "browser-panel-8".into();
        assert!(!binding.matches(&binding.host_id, None, &workspace, &live));
        live = resource.clone(); live.room_id = "room-2".into();
        assert!(!binding.matches(&binding.host_id, None, &workspace, &live));
        live = resource; live.workspace_path = "another-workspace".into();
        assert!(!binding.matches(&binding.host_id, None, &workspace, &live));
    }

    #[test]
    fn native_host_lease_rejects_replay_and_competing_host() {
        let current = RegisteredHost {host_id:"native-host-0123456789".into(), identity:None, sequence:7,
            seen:Instant::now(), resource:None};
        let mut state = HostState {host_id:current.host_id.clone(), sequence:7, identity:None, resource:None};
        assert!(!accepts_sequence(&current, &state, Duration::ZERO));
        assert!(!accepts_sequence(&current, &state, Duration::from_millis(LEASE_MILLIS)));
        state.sequence = 8;
        assert!(accepts_sequence(&current, &state, Duration::ZERO));
        state.host_id = "native-other-0123456789".into();
        assert!(!accepts_sequence(&current, &state, Duration::ZERO));
        assert!(accepts_sequence(&current, &state, Duration::from_millis(LEASE_MILLIS)));
    }
}
