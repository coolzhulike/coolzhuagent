//! 右侧代码预览使用的显式语言服务入口。配置只保存受信任预设，不接受工作区提供的命令。

use super::*;
use lsp::{LspManager, LspServerConfig};
use lsp_types::Position;

const RUST_PRESET: &str = "rust-analyzer";

#[derive(Serialize, Deserialize)]
struct UserLspConfig {
    schema_version: u32,
    preset: String,
}

#[derive(Serialize)]
pub(super) struct LspStatus {
    configured: bool,
    running: bool,
    preset: Option<&'static str>,
    handle: Option<String>,
    message: String,
}

struct LspWorkspace {
    handle: String,
    root: PathBuf,
    room_db: PathBuf,
    owner: LspOwner,
    rustup: PathBuf,
    binary: PathBuf,
    manager: Arc<LspManager>,
    revoked: AtomicBool,
}

#[derive(Clone, PartialEq, Eq)]
struct LspOwner {
    session_id: String,
    room_id: String,
}

fn current() -> &'static Mutex<Option<Arc<LspWorkspace>>> {
    static CURRENT: OnceLock<Mutex<Option<Arc<LspWorkspace>>>> = OnceLock::new();
    CURRENT.get_or_init(|| Mutex::new(None))
}

fn start_guard() -> &'static tokio::sync::Mutex<()> {
    static START: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    START.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn workspace_generation() -> &'static AtomicU64 {
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    &GENERATION
}

fn canonical_workspace() -> ApiResult<PathBuf> {
    active_workspace_path()
        .canonicalize()
        .map_err(|_| api_error(StatusCode::BAD_REQUEST, "当前工作区不存在或不可访问"))
}

fn config_path(root: &Path) -> ApiResult<PathBuf> {
    let base = env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .or_else(|| user_home_dir().map(|home| home.join(".local/share")))
        .ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, "用户配置目录不可用"))?;
    let canonical_base = base
        .canonicalize()
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "用户配置目录不可访问"))?;
    if canonical_base.starts_with(root) {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "用户配置目录位于当前工作区内，不能用于授权语言服务",
        ));
    }
    let config = canonical_base.join("coolzhuagent").join("lsp-servers.json");
    if config.exists()
        && config
            .canonicalize()
            .is_ok_and(|path| path.starts_with(root))
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "语言服务配置指向当前工作区",
        ));
    }
    Ok(config)
}

fn read_config(root: &Path) -> ApiResult<bool> {
    let path = config_path(root)?;
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => {
            return Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "语言服务配置不可读取",
            ))
        }
    };
    let config: UserLspConfig = serde_json::from_str(&contents)
        .map_err(|_| api_error(StatusCode::CONFLICT, "语言服务配置无效"))?;
    if config.schema_version != 1 || config.preset != RUST_PRESET {
        return Err(api_error(
            StatusCode::CONFLICT,
            "语言服务配置含有不支持的预设",
        ));
    }
    Ok(true)
}

fn fresh_handle() -> ApiResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建语言服务句柄"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn is_active(scope: &LspWorkspace) -> bool {
    !scope.revoked.load(Ordering::Acquire)
        && canonical_workspace().is_ok_and(|root| root == scope.root)
        && owner_is_active(&scope.owner)
}

fn owner_is_active(owner: &LspOwner) -> bool {
    session_store().lock().is_ok_and(|store| {
        store.is_active(&owner.session_id) && store.is_active_chat_room(&owner.room_id)
    })
}

fn request_owner(session_id: Option<&str>, room_id: &str) -> ApiResult<LspOwner> {
    let session_id = session_id.unwrap_or("").trim();
    let room_id = room_id.trim();
    if session_id.is_empty() || room_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "请先选择当前 Agent 和聊天室",
        ));
    }
    let owner = LspOwner {
        session_id: session_id.to_string(),
        room_id: room_id.to_string(),
    };
    if !owner_is_active(&owner) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "当前 Agent 或聊天室已切换，请重新启动语言服务",
        ));
    }
    Ok(owner)
}

// 所有调用者持有 WorkspacePin；再核一次已捕获 root，避免动态 DB 路径读到另一工程。
fn room_db_path(root: &Path) -> ApiResult<PathBuf> {
    if canonical_workspace()?.as_path() != root {
        return Err(api_error(
            StatusCode::CONFLICT,
            "工作区已切换，请重新启动语言服务",
        ));
    }
    let path = default_session_sqlite_path();
    if canonical_workspace()?.as_path() != root {
        return Err(api_error(
            StatusCode::CONFLICT,
            "工作区已切换，请重新启动语言服务",
        ));
    }
    Ok(path)
}

fn launch_grant(root: &Path, room_db: &Path, owner: &LspOwner) -> ApiResult<SessionGrantView> {
    if canonical_workspace()?.as_path() != root {
        return Err(api_error(
            StatusCode::CONFLICT,
            "工作区已切换，请重新启动语言服务",
        ));
    }
    if dev_open_tool_permissions_enabled() {
        return Ok(session_grant_view_for(
            &workspace_identity(root),
            Some(&owner.session_id),
            "PowerShell",
        ));
    }
    let profile =
        chat_room_permission_profile_sqlite(room_db, &owner.room_id).map_err(sqlite_api_error)?;
    if profile != ROOM_PERMISSION_FULL_ACCESS && profile != ROOM_PERMISSION_WORKSPACE_WRITE {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "当前聊天室为只读权限，不能启动语言服务",
        ));
    }
    let room_grant = if profile == ROOM_PERMISSION_FULL_ACCESS {
        SessionGrantView {
            session_authorized: true,
            session_confirmed_twice: true,
        }
    } else {
        SessionGrantView::default()
    };
    let grant = if room_grant.session_authorized {
        room_grant
    } else {
        session_grant_view_for(
            &workspace_identity(root),
            Some(&owner.session_id),
            "PowerShell",
        )
    };
    if !grant.session_authorized || !grant.session_confirmed_twice {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "语言服务需要完整访问权限；请到当前聊天室的权限设置中开启完整访问",
        ));
    }
    Ok(grant)
}

fn check_launch_gate(
    root: &Path,
    room_db: &Path,
    owner: &LspOwner,
    command: &Path,
) -> ApiResult<()> {
    if !owner_is_active(owner) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "当前 Agent 或聊天室已切换，请重新启动语言服务",
        ));
    }
    let grant = launch_grant(root, room_db, owner)?;
    let gate = preview_tool_permission(
        "PowerShell",
        &json!({"command": command.to_string_lossy(), "cwd": root.to_string_lossy()}),
        ToolCaller::WebUi,
        root,
        grant,
        true,
        false,
    );
    if !gate.decision.is_allowed() {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            &format!("当前执行权限不允许启动语言服务：{}", gate.reason),
        ));
    }
    Ok(())
}

fn authorize_running(scope: &LspWorkspace) -> ApiResult<()> {
    check_launch_gate(&scope.root, &scope.room_db, &scope.owner, &scope.rustup)?;
    check_launch_gate(&scope.root, &scope.room_db, &scope.owner, &scope.binary)
}

// 旧请求只能撤销自己看到的实例，不能在等待期间关掉新启动的实例。
fn invalidate_if_current(scope: &Arc<LspWorkspace>) {
    let removed = current().lock().ok().and_then(|mut guard| {
        if guard
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, scope))
        {
            workspace_generation().fetch_add(1, Ordering::AcqRel);
            guard.take()
        } else {
            None
        }
    });
    if let Some(removed) = removed {
        removed.revoked.store(true, Ordering::Release);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = removed.manager.shutdown().await;
            });
        }
    }
}

/// 会话临时授权撤销时，仅在当前实例已失去实际执行权限时关闭它。
pub(super) fn invalidate_if_permission_lost() {
    let scope = current()
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().cloned());
    if let Some(scope) = scope {
        if !is_active(&scope) || authorize_running(&scope).is_err() {
            invalidate_if_current(&scope);
        }
    }
}

fn resolve_handle(handle: &str) -> ApiResult<Arc<LspWorkspace>> {
    let scope = current()
        .lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "语言服务状态锁已损坏"))?
        .as_ref()
        .filter(|scope| scope.handle == handle)
        .cloned()
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "语言服务已停止，请重新启动"))?;
    if !is_active(&scope) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "工作区、Agent 或聊天室已改变，请重新启动语言服务",
        ));
    }
    if let Err(error) = authorize_running(&scope) {
        invalidate_if_current(&scope);
        return Err(error);
    }
    Ok(scope)
}

fn document(scope: &LspWorkspace, path: &str) -> ApiResult<PathBuf> {
    let path = resolve_project_existing_path(&scope.root, Some(path))?;
    if !path.is_file() || !scope.manager.supports_path(&path) {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "当前仅支持工作区内的 Rust 文件",
        ));
    }
    Ok(path)
}

fn relative_path(root: &Path, path: &Path) -> Option<String> {
    path.canonicalize()
        .ok()?
        .strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

fn server_error(error: lsp::LspError) -> ApiError {
    api_error(
        StatusCode::BAD_GATEWAY,
        &format!("语言服务请求失败：{error}"),
    )
}

/// 工作区重载是同步入口；先撤销所有旧句柄，再异步关闭进程。
pub(super) fn invalidate_workspace() {
    workspace_generation().fetch_add(1, Ordering::AcqRel);
    let scope = current().lock().ok().and_then(|mut guard| guard.take());
    if let Some(scope) = scope {
        scope.revoked.store(true, Ordering::Release);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = scope.manager.shutdown().await;
            });
        }
    }
}

pub(super) async fn api_status() -> ApiResult<Json<LspStatus>> {
    let _pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let root = canonical_workspace()?;
    let configured = read_config(&root)?;
    let running = current()
        .lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "语言服务状态锁已损坏"))?
        .as_ref()
        .cloned();
    if running
        .as_ref()
        .is_some_and(|scope| scope.root != root || !is_active(scope))
    {
        if let Some(scope) = &running {
            invalidate_if_current(scope);
        }
    }
    let running = running.filter(|scope| scope.root == root && is_active(scope));
    let alive = if let Some(scope) = &running {
        let allowed_before = authorize_running(scope).is_ok();
        let server_running = allowed_before && scope.manager.is_running().await;
        let allowed_after = server_running && is_active(scope) && authorize_running(scope).is_ok();
        if !allowed_before || (server_running && !allowed_after) {
            invalidate_if_current(scope);
        }
        allowed_after
    } else {
        false
    };
    Ok(Json(LspStatus {
        configured,
        running: alive,
        preset: configured.then_some(RUST_PRESET),
        handle: running
            .as_ref()
            .filter(|_| alive)
            .map(|scope| scope.handle.clone()),
        message: if alive {
            "Rust 语言服务已启动".to_string()
        } else if running.is_some() {
            "Rust 语言服务已停止，可手动重新启动".to_string()
        } else if configured {
            "Rust 语言服务已配置，需手动启动".to_string()
        } else {
            "尚未配置 Rust 语言服务".to_string()
        },
    }))
}

#[derive(Deserialize)]
pub(super) struct ConfigureRequest {
    preset: String,
}

pub(super) async fn api_configure(
    Json(request): Json<ConfigureRequest>,
) -> ApiResult<Json<LspStatus>> {
    let _pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    if request.preset != RUST_PRESET {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "当前仅支持 Rust 语言服务预设",
        ));
    }
    let root = canonical_workspace()?;
    let path = config_path(&root)?;
    if !read_config(&root)? {
        let parent = path.parent().expect("配置路径有父目录");
        std::fs::create_dir_all(parent).map_err(io_api_error)?;
        let canonical_parent = parent.canonicalize().map_err(io_api_error)?;
        if canonical_parent.starts_with(&root) {
            return Err(api_error(
                StatusCode::FORBIDDEN,
                "语言服务配置目录指向工作区",
            ));
        }
        let body = serde_json::to_vec_pretty(&UserLspConfig {
            schema_version: 1,
            preset: RUST_PRESET.to_string(),
        })
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "配置编码失败"))?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(io_api_error)?;
        use std::io::Write;
        file.write_all(&body).map_err(io_api_error)?;
        file.sync_all().map_err(io_api_error)?;
    }
    api_status().await
}

fn trusted_rustup(root: &Path) -> ApiResult<(PathBuf, PathBuf)> {
    let home = user_home_dir()
        .and_then(|home| home.canonicalize().ok())
        .ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, "用户目录不可用"))?;
    #[cfg(windows)]
    let rustup = home.join(".cargo/bin/rustup.exe");
    #[cfg(not(windows))]
    let rustup = home.join(".cargo/bin/rustup");
    let rustup = rustup
        .canonicalize()
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "未找到用户安装的 rustup"))?;
    if rustup.starts_with(root) || !rustup.starts_with(&home.join(".cargo")) {
        return Err(api_error(StatusCode::FORBIDDEN, "rustup 安装路径不可信"));
    }
    Ok((home, rustup))
}

async fn trusted_rust_analyzer(root: &Path, home: &Path, rustup: &Path) -> ApiResult<PathBuf> {
    use tokio::io::AsyncReadExt;
    let mut command = tokio::process::Command::new(rustup);
    command
        .args(["which", "--toolchain", "stable", RUST_PRESET])
        .current_dir(home)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    let (mut child, _job) =
        windows_process_guard::ChildProcessJob::spawn_managed_async(&mut command)
            .map_err(io_api_error)?;
    #[cfg(not(windows))]
    let mut child = command.spawn().map_err(io_api_error)?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| api_error(StatusCode::SERVICE_UNAVAILABLE, "rustup 标准输出不可用"))?;
    let output = tokio::time::timeout(Duration::from_secs(5), async move {
        let mut bytes = Vec::new();
        stdout.take(4097).read_to_end(&mut bytes).await?;
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, bytes))
    })
    .await
    .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "查找 rust-analyzer 超时"))?
    .map_err(io_api_error)?;
    if !output.0.success() || output.1.len() > 4096 {
        return Err(api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "stable 工具链尚未安装 rust-analyzer 组件",
        ));
    }
    let binary = PathBuf::from(String::from_utf8_lossy(&output.1).trim());
    let binary = binary.canonicalize().map_err(|_| {
        api_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "rust-analyzer 安装路径无效",
        )
    })?;
    if !binary.is_file()
        || binary.starts_with(root)
        || !binary.starts_with(home.join(".rustup/toolchains"))
        || binary.file_stem().and_then(|stem| stem.to_str()) != Some(RUST_PRESET)
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "rust-analyzer 安装路径不可信",
        ));
    }
    Ok(binary)
}

#[derive(Deserialize)]
pub(super) struct StartRequest {
    path: String,
    session_id: Option<String>,
    chat_room_id: String,
    expected_workspace: String,
}

pub(super) async fn api_start(Json(request): Json<StartRequest>) -> ApiResult<Json<LspStatus>> {
    let _start = start_guard().lock().await;
    let _pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let root = canonical_workspace()?;
    if request.expected_workspace.trim().is_empty()
        || request.expected_workspace != display_path(&root)
    {
        return Err(api_error(
            StatusCode::CONFLICT,
            "工作区已切换，请刷新后重新启动语言服务",
        ));
    }
    let generation = workspace_generation().load(Ordering::Acquire);
    let owner = request_owner(request.session_id.as_deref(), &request.chat_room_id)?;
    let room_db = room_db_path(&root)?;
    if !read_config(&root)? {
        return Err(api_error(
            StatusCode::CONFLICT,
            "请先由用户配置 Rust 语言服务",
        ));
    }
    let existing = {
        let guard = current()
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "语言服务状态锁已损坏"))?;
        guard
            .as_ref()
            .filter(|scope| scope.root == root && is_active(scope))
            .cloned()
    };
    if let Some(scope) = existing {
        if scope.owner != owner {
            return Err(api_error(
                StatusCode::CONFLICT,
                "语言服务属于其他 Agent 或聊天室，请重新启动",
            ));
        }
        if let Err(error) = authorize_running(&scope) {
            invalidate_if_current(&scope);
            return Err(error);
        }
        if scope.manager.is_running().await {
            authorize_running(&scope)?;
            let path = document(&scope, &request.path)?;
            scope
                .manager
                .sync_document_from_disk(&path)
                .await
                .map_err(server_error)?;
            if !is_active(&scope) {
                return Err(api_error(StatusCode::CONFLICT, "工作区或聊天室已改变"));
            }
            authorize_running(&scope)?;
            return api_status().await;
        }
        let removed = {
            let mut guard = current().lock().map_err(|_| {
                api_error(StatusCode::INTERNAL_SERVER_ERROR, "语言服务状态锁已损坏")
            })?;
            if guard
                .as_ref()
                .is_some_and(|active| active.handle == scope.handle)
            {
                guard.take()
            } else {
                None
            }
        };
        if let Some(old) = removed {
            old.revoked.store(true, Ordering::Release);
            let _ = old.manager.shutdown().await;
        }
    }
    let path = resolve_project_existing_path(&root, Some(&request.path))?;
    if !path.is_file() || path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "当前仅支持工作区内的 Rust 文件",
        ));
    }
    let (home, rustup) = trusted_rustup(&root)?;
    // 手动按钮点击是本次调用的用户授权事实；仍完整走与 PowerShell 相同的权限档位和保护规则。
    check_launch_gate(&root, &room_db, &owner, &rustup)?;
    let binary = trusted_rust_analyzer(&root, &home, &rustup).await?;
    if workspace_generation().load(Ordering::Acquire) != generation || !owner_is_active(&owner) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "启动期间工作区或聊天室已改变",
        ));
    }
    check_launch_gate(&root, &room_db, &owner, &binary)?;
    let handle = fresh_handle()?;
    let manager = Arc::new(
        LspManager::new(vec![LspServerConfig {
            name: RUST_PRESET.to_string(),
            command: binary.to_string_lossy().into_owned(),
            args: Vec::new(),
            env: BTreeMap::new(),
            workspace_root: root.clone(),
            initialization_options: None,
            extension_to_language: BTreeMap::from([(".rs".to_string(), "rust".to_string())]),
        }])
        .map_err(server_error)?,
    );
    if workspace_generation().load(Ordering::Acquire) != generation || !owner_is_active(&owner) {
        return Err(api_error(
            StatusCode::CONFLICT,
            "启动期间工作区或聊天室已改变",
        ));
    }
    if let Err(error) = manager.sync_document_from_disk(&path).await {
        let _ = manager.shutdown().await;
        return Err(server_error(error));
    }
    let scope = Arc::new(LspWorkspace {
        handle,
        root,
        room_db,
        owner: owner.clone(),
        rustup,
        binary,
        manager,
        revoked: AtomicBool::new(false),
    });
    if request_owner(request.session_id.as_deref(), &request.chat_room_id)
        .ok()
        .as_ref()
        != Some(&owner)
        || authorize_running(&scope).is_err()
    {
        scope.revoked.store(true, Ordering::Release);
        let _ = scope.manager.shutdown().await;
        return Err(api_error(
            StatusCode::CONFLICT,
            "启动期间聊天室或执行权限已改变",
        ));
    }
    let installed = {
        let mut guard = current()
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "语言服务状态锁已损坏"))?;
        if workspace_generation().load(Ordering::Acquire) != generation
            || canonical_workspace().map_or(true, |current_root| current_root != scope.root)
            || !owner_is_active(&scope.owner)
        {
            None
        } else {
            Some(guard.replace(scope.clone()))
        }
    };
    let Some(old) = installed else {
        let _ = scope.manager.shutdown().await;
        return Err(api_error(StatusCode::CONFLICT, "启动期间工作区已改变"));
    };
    if let Some(old) = old {
        old.revoked.store(true, Ordering::Release);
        tokio::spawn(async move {
            let _ = old.manager.shutdown().await;
        });
    }
    api_status().await
}

#[derive(Deserialize)]
pub(super) struct HandleRequest {
    handle: String,
    path: String,
}

pub(super) async fn api_diagnostics(
    Json(request): Json<HandleRequest>,
) -> ApiResult<Json<JsonValue>> {
    let _pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let scope = resolve_handle(&request.handle)?;
    let path = document(&scope, &request.path)?;
    scope
        .manager
        .sync_document_from_disk(&path)
        .await
        .map_err(server_error)?;
    if !is_active(&scope) {
        return Err(api_error(StatusCode::CONFLICT, "工作区或聊天室已改变"));
    }
    authorize_running(&scope)?;
    let diagnostics = scope
        .manager
        .collect_workspace_diagnostics()
        .await
        .map_err(server_error)?;
    if !is_active(&scope) {
        return Err(api_error(StatusCode::CONFLICT, "工作区或聊天室已改变"));
    }
    authorize_running(&scope)?;
    let items = diagnostics
        .files
        .iter()
        .filter(|file| file.path == path)
        .flat_map(|file| {
            file.diagnostics
                .iter()
                .take(100)
                .map(|diagnostic| {
                    json!({
                        "line": diagnostic.range.start.line + 1,
                        "character": diagnostic.range.start.character + 1,
                        "severity": diagnostic.severity.map(|severity| format!("{severity:?}")),
                        "message": diagnostic.message,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    Ok(Json(
        json!({"handle": scope.handle, "path": request.path, "diagnostics": items}),
    ))
}

#[derive(Deserialize)]
pub(super) struct NavigationRequest {
    handle: String,
    path: String,
    line: u32,
    character: u32,
    kind: String,
}

pub(super) async fn api_navigation(
    Json(request): Json<NavigationRequest>,
) -> ApiResult<Json<JsonValue>> {
    let _pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    if request.line == 0
        || request.character == 0
        || request.line > 1_000_000
        || request.character > 1_000_000
    {
        return Err(api_error(StatusCode::BAD_REQUEST, "行列位置无效"));
    }
    let scope = resolve_handle(&request.handle)?;
    let path = document(&scope, &request.path)?;
    scope
        .manager
        .sync_document_from_disk(&path)
        .await
        .map_err(server_error)?;
    if !is_active(&scope) {
        return Err(api_error(StatusCode::CONFLICT, "工作区或聊天室已改变"));
    }
    authorize_running(&scope)?;
    let position = Position::new(request.line - 1, request.character - 1);
    let locations = match request.kind.as_str() {
        "definition" => scope.manager.go_to_definition(&path, position).await,
        "references" => scope.manager.find_references(&path, position, true).await,
        _ => return Err(api_error(StatusCode::BAD_REQUEST, "未知的语言服务跳转类型")),
    }
    .map_err(server_error)?;
    if !is_active(&scope) {
        return Err(api_error(StatusCode::CONFLICT, "工作区或聊天室已改变"));
    }
    authorize_running(&scope)?;
    let items = locations
        .iter()
        .take(100)
        .filter_map(|location| {
            Some(json!({
                "path": relative_path(&scope.root, &location.path)?,
                "line": location.start_line(),
                "character": location.start_character(),
            }))
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({"handle": scope.handle, "locations": items})))
}

#[derive(Deserialize)]
pub(super) struct CloseRequest {
    handle: String,
}

pub(super) async fn api_close(Json(request): Json<CloseRequest>) -> ApiResult<Json<JsonValue>> {
    let _pin = workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))?;
    let scope = resolve_handle(&request.handle)?;
    let removed = {
        let mut guard = current()
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "语言服务状态锁已损坏"))?;
        if guard
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &scope) && is_active(active))
        {
            guard.take()
        } else {
            None
        }
    }
    .ok_or_else(|| api_error(StatusCode::CONFLICT, "语言服务句柄已失效"))?;
    removed.revoked.store(true, Ordering::Release);
    removed.manager.shutdown().await.map_err(server_error)?;
    Ok(Json(json!({"stopped": true})))
}
