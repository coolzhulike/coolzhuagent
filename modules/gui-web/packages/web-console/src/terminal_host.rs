//! 右栏手动终端的宿主归属、权限与生命周期。ConPTY/Job 句柄留在进程监督库。
use super::*;
use windows_process_guard::{ConPtyOutput, ManagedConPty};

#[derive(Clone, PartialEq, Eq)]
struct TerminalScope {
    workspace: PathBuf,
    workspace_id: String,
    session_id: String,
    room_id: String,
}

struct TerminalRecord {
    handle: String,
    scope: TerminalScope,
    revoked: AtomicBool,
    pty: Mutex<ManagedConPty>,
}

fn current() -> &'static Mutex<Option<Arc<TerminalRecord>>> {
    static CURRENT: OnceLock<Mutex<Option<Arc<TerminalRecord>>>> = OnceLock::new();
    CURRENT.get_or_init(|| Mutex::new(None))
}

fn start_guard() -> &'static tokio::sync::Mutex<()> {
    static START: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    START.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn generation() -> &'static AtomicU64 {
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    &GENERATION
}

fn pin_request() -> ApiResult<workspace_activity::WorkspacePin> {
    workspace_activity::pin_workspace()
        .map_err(|message| api_error(StatusCode::CONFLICT, &message))
}

fn scope_for(session_id: &str, room_id: &str) -> ApiResult<TerminalScope> {
    let session_id = session_id.trim();
    let room_id = room_id.trim();
    if session_id.is_empty() || room_id.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "请先选择当前 Agent 和聊天室"));
    }
    let workspace = active_workspace_path().canonicalize()
        .map_err(|_| api_error(StatusCode::CONFLICT, "当前工作区不可访问"))?;
    let store = session_store().lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "会话状态不可用"))?;
    if !store.is_active(session_id) || !store.is_active_chat_room(room_id) {
        return Err(api_error(StatusCode::CONFLICT, "当前 Agent 或聊天室已切换，请刷新终端"));
    }
    Ok(TerminalScope {
        workspace_id: workspace_identity(&workspace), workspace,
        session_id: session_id.to_string(), room_id: room_id.to_string(),
    })
}

fn assert_write_permission(scope: &TerminalScope, command: &str) -> ApiResult<()> {
    if llm_tool_permission_for_room(Some(&scope.room_id)) == PermissionMode::ReadOnly {
        return Err(api_error(StatusCode::FORBIDDEN,
            "当前聊天室为只读权限；请到聊天室权限设置中开启完整访问后再使用终端"));
    }
    let room_grant = room_permission_grant_view_for_path(
        &default_session_sqlite_path(), Some(&scope.room_id));
    let grant = if room_grant.session_authorized { room_grant } else {
        session_grant_view_for(&scope.workspace_id, Some(&scope.session_id), "PowerShell")
    };
    if !grant.session_authorized || !grant.session_confirmed_twice {
        return Err(api_error(StatusCode::FORBIDDEN,
            "终端需要完整访问权限；请到当前聊天室的权限设置中开启完整访问"));
    }
    // 手动按钮只授权这次动作，不伪造二次确认；模型工具不会进入此入口。
    let gate = preview_tool_permission(
        "PowerShell",
        &json!({"command": command, "cwd": scope.workspace.to_string_lossy()}),
        ToolCaller::WebUi, &scope.workspace, grant, true, false,
    );
    if !gate.decision.is_allowed() {
        return Err(api_error(StatusCode::FORBIDDEN,
            "终端需要完整访问权限；请到当前聊天室的权限设置中开启完整访问"));
    }
    Ok(())
}

fn fresh_handle() -> ApiResult<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes)
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建终端句柄"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn resolve(handle: &str, scope: &TerminalScope) -> ApiResult<Arc<TerminalRecord>> {
    current().lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态不可用"))?
        .as_ref()
        .filter(|record| record.handle == handle && record.scope == *scope
            && !record.revoked.load(Ordering::Acquire))
        .cloned()
        .ok_or_else(|| api_error(StatusCode::CONFLICT, "终端已关闭或归属已变，请重新启动"))
}

fn revoke_current() -> Option<Arc<TerminalRecord>> {
    let record = current().lock().ok()?.take();
    if let Some(record) = &record { record.revoked.store(true, Ordering::Release); }
    record
}

fn revoke_if_matches(handle: &str, scope: &TerminalScope) -> ApiResult<Arc<TerminalRecord>> {
    let mut guard = current().lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态不可用"))?;
    if !guard.as_ref().is_some_and(|record| record.handle == handle && record.scope == *scope
        && !record.revoked.load(Ordering::Acquire)) {
        return Err(api_error(StatusCode::CONFLICT, "终端已关闭或归属已变，请重新启动"));
    }
    let record = guard.take().expect("已验证终端存在");
    record.revoked.store(true, Ordering::Release);
    Ok(record)
}

fn close_blocking(record: Arc<TerminalRecord>) -> std::io::Result<()> {
    let mut pty = record.pty.lock().map_err(|_| std::io::Error::other("终端状态锁已损坏"))?;
    pty.close()
}

async fn close_record(record: Arc<TerminalRecord>) -> ApiResult<()> {
    tokio::task::spawn_blocking(move || close_blocking(record)).await
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端清理任务失败"))?
        .map_err(io_api_error)
}

/// 工程或会话切换时同步撤销旧句柄，实际进程清理转交阻塞线程。
pub(super) fn invalidate_scope() {
    generation().fetch_add(1, Ordering::AcqRel);
    if let Some(record) = revoke_current() {
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn_blocking(move || { let _ = close_blocking(record); });
        } else {
            std::thread::spawn(move || { let _ = close_blocking(record); });
        }
    }
}

#[derive(Deserialize)]
struct ScopeQuery { session_id: String, room_id: String }

#[derive(Deserialize)]
struct StartRequest { session_id: String, room_id: String, cols: u16, rows: u16 }

#[derive(Deserialize)]
struct InputRequest { session_id: String, room_id: String, text: String }

#[derive(Deserialize)]
struct ResizeRequest { session_id: String, room_id: String, cols: u16, rows: u16 }

#[derive(Deserialize)]
struct ScopeBody { session_id: String, room_id: String }

#[derive(Deserialize)]
struct OutputQuery { session_id: String, room_id: String, cursor: Option<u64> }

#[derive(Serialize)]
struct TerminalStatus {
    active: bool,
    handle: Option<String>,
    text: String,
    next_cursor: u64,
    truncated: bool,
    closed: bool,
    message: String,
}

impl TerminalStatus {
    fn absent() -> Self {
        Self { active: false, handle: None, text: String::new(), next_cursor: 0,
            truncated: false, closed: true, message: "终端尚未启动".to_string() }
    }
    fn from_output(handle: &str, output: ConPtyOutput) -> Self {
        Self { active: true, handle: Some(handle.to_string()), text: output.text,
            next_cursor: output.next_cursor, truncated: output.truncated,
            closed: output.closed, message: if output.closed { "终端已退出" } else { "终端正在运行" }.to_string() }
    }
}

async fn output_for(record: Arc<TerminalRecord>, cursor: u64) -> ApiResult<TerminalStatus> {
    if record.revoked.load(Ordering::Acquire) {
        return Err(api_error(StatusCode::CONFLICT, "终端已失效"));
    }
    let for_read = Arc::clone(&record);
    let output = tokio::task::spawn_blocking(move || {
        let pty = for_read.pty.lock().map_err(|_| std::io::Error::other("终端状态锁已损坏"))?;
        let mut status = TerminalStatus::from_output(&for_read.handle,
            pty.read_since(cursor, 64 * 1024));
        status.closed |= pty.exit_code()?.is_some();
        if status.closed { status.message = "终端已退出".to_string(); }
        Ok::<_, std::io::Error>(status)
    }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端输出读取失败"))?
        .map_err(io_api_error)?;
    if record.revoked.load(Ordering::Acquire) {
        return Err(api_error(StatusCode::CONFLICT, "终端已失效"));
    }
    Ok(output)
}

async fn authorized_output_for(record: Arc<TerminalRecord>, cursor: u64,
    scope: &TerminalScope) -> ApiResult<TerminalStatus> {
    assert_write_permission(scope, "读取交互式终端输出")?;
    let output = output_for(record, cursor).await?;
    assert_write_permission(scope, "读取交互式终端输出")?;
    Ok(output)
}

fn hide_ungranted_output(status: &mut TerminalStatus, scope: &TerminalScope) {
    if assert_write_permission(scope, "查看交互式终端状态").is_err() {
        status.text.clear();
        status.next_cursor = 0;
        status.message = if status.closed { "终端已退出，可关闭" }
            else { "终端权限已降低；可中断或关闭终端" }.to_string();
    }
}

async fn api_status(Query(query): Query<ScopeQuery>) -> ApiResult<Json<TerminalStatus>> {
    let _pin = pin_request()?;
    let scope = scope_for(&query.session_id, &query.room_id)?;
    let record = current().lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态不可用"))?
        .as_ref().filter(|record| record.scope == scope
            && !record.revoked.load(Ordering::Acquire)).cloned();
    let Some(record) = record else { return Ok(Json(TerminalStatus::absent())); };
    let mut status = output_for(record, 0).await?;
    hide_ungranted_output(&mut status, &scope);
    Ok(Json(status))
}

async fn api_start(Json(request): Json<StartRequest>) -> ApiResult<Json<TerminalStatus>> {
    let _guard = start_guard().lock().await;
    let _pin = pin_request()?;
    let scope = scope_for(&request.session_id, &request.room_id)?;
    assert_write_permission(&scope, "启动交互式 PowerShell")?;
    let accepted_generation = generation().load(Ordering::Acquire);
    let existing = current().lock()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态不可用"))?
        .as_ref().filter(|record| record.scope == scope
            && !record.revoked.load(Ordering::Acquire)).cloned();
    if let Some(record) = existing {
        let for_check = Arc::clone(&record);
        let running = tokio::task::spawn_blocking(move || {
            let pty = for_check.pty.lock().map_err(|_| std::io::Error::other("终端状态锁已损坏"))?;
            Ok::<_, std::io::Error>(pty.exit_code()?.is_none())
        }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态检查失败"))?
            .map_err(io_api_error)?;
        if running { return authorized_output_for(record, 0, &scope).await.map(Json); }
        if let Ok(old) = revoke_if_matches(&record.handle, &scope) { close_record(old).await?; }
    }
    if let Some(old) = revoke_current() { close_record(old).await?; }
    let workspace = scope.workspace.clone();
    let pty = tokio::task::spawn_blocking(move ||
        ManagedConPty::spawn_powershell(&workspace, request.cols, request.rows))
        .await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端启动任务失败"))?
        .map_err(io_api_error)?;
    let record = Arc::new(TerminalRecord { handle: fresh_handle()?, scope: scope.clone(),
        revoked: AtomicBool::new(false), pty: Mutex::new(pty) });
    if generation().load(Ordering::Acquire) != accepted_generation
        || scope_for(&request.session_id, &request.room_id).ok().as_ref() != Some(&scope)
        || assert_write_permission(&scope, "启动交互式 PowerShell").is_err() {
        record.revoked.store(true, Ordering::Release);
        close_record(record).await?;
        return Err(api_error(StatusCode::CONFLICT, "启动期间工作区或聊天室已切换"));
    }
    let installed = {
        let mut guard = current().lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态不可用"))?;
        if generation().load(Ordering::Acquire) != accepted_generation { false }
        else { *guard = Some(Arc::clone(&record)); true }
    };
    if !installed {
        record.revoked.store(true, Ordering::Release);
        close_record(record).await?;
        return Err(api_error(StatusCode::CONFLICT, "启动期间工作区或聊天室已切换"));
    }
    authorized_output_for(record, 0, &scope).await.map(Json)
}

async fn api_input(AxumPath(handle): AxumPath<String>,
    Json(request): Json<InputRequest>) -> ApiResult<Json<TerminalStatus>> {
    let _pin = pin_request()?;
    let scope = scope_for(&request.session_id, &request.room_id)?;
    let record = resolve(&handle, &scope)?;
    assert_write_permission(&scope, &request.text)?;
    let dispatch_scope = scope.clone();
    let text = request.text;
    tokio::task::spawn_blocking(move || {
        if scope_for(&dispatch_scope.session_id, &dispatch_scope.room_id)? != dispatch_scope {
            return Err(api_error(StatusCode::CONFLICT, "终端归属已变化"));
        }
        assert_write_permission(&dispatch_scope, &text)?;
        if record.revoked.load(Ordering::Acquire) {
            return Err(api_error(StatusCode::CONFLICT, "终端已失效"));
        }
        let pty = record.pty.lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态锁已损坏"))?;
        if record.revoked.load(Ordering::Acquire) {
            return Err(api_error(StatusCode::CONFLICT, "终端已失效"));
        }
        pty.write(text.as_bytes()).map_err(io_api_error)
    }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端输入任务失败"))?
        ?;
    authorized_output_for(resolve(&handle, &scope)?, 0, &scope).await.map(Json)
}

async fn api_output(AxumPath(handle): AxumPath<String>, Query(query): Query<OutputQuery>)
    -> ApiResult<Json<TerminalStatus>> {
    let _pin = pin_request()?;
    let scope = scope_for(&query.session_id, &query.room_id)?;
    authorized_output_for(resolve(&handle, &scope)?, query.cursor.unwrap_or(0), &scope)
        .await.map(Json)
}

async fn api_resize(AxumPath(handle): AxumPath<String>, Json(request): Json<ResizeRequest>)
    -> ApiResult<Json<TerminalStatus>> {
    let _pin = pin_request()?;
    let scope = scope_for(&request.session_id, &request.room_id)?;
    assert_write_permission(&scope, "调整交互式 PowerShell 终端大小")?;
    let record = resolve(&handle, &scope)?;
    let dispatch_scope = scope.clone();
    tokio::task::spawn_blocking(move || {
        if scope_for(&dispatch_scope.session_id, &dispatch_scope.room_id)? != dispatch_scope {
            return Err(api_error(StatusCode::CONFLICT, "终端归属已变化"));
        }
        assert_write_permission(&dispatch_scope, "调整交互式 PowerShell 终端大小")?;
        if record.revoked.load(Ordering::Acquire) {
            return Err(api_error(StatusCode::CONFLICT, "终端已失效"));
        }
        let pty = record.pty.lock().map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端状态锁已损坏"))?;
        if record.revoked.load(Ordering::Acquire) {
            return Err(api_error(StatusCode::CONFLICT, "终端已失效"));
        }
        pty.resize(request.cols, request.rows).map_err(io_api_error)
    }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端大小调整失败"))?
        ?;
    authorized_output_for(resolve(&handle, &scope)?, 0, &scope).await.map(Json)
}

async fn api_interrupt(AxumPath(handle): AxumPath<String>, Json(request): Json<ScopeBody>)
    -> ApiResult<Json<TerminalStatus>> {
    let _pin = pin_request()?;
    let scope = scope_for(&request.session_id, &request.room_id)?;
    let record = resolve(&handle, &scope)?;
    tokio::task::spawn_blocking(move || {
        let pty = record.pty.lock().map_err(|_| std::io::Error::other("终端状态锁已损坏"))?;
        pty.interrupt()
    }).await.map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "终端中断失败"))?
        .map_err(io_api_error)?;
    let mut status = output_for(resolve(&handle, &scope)?, 0).await?;
    hide_ungranted_output(&mut status, &scope);
    Ok(Json(status))
}

async fn api_close(AxumPath(handle): AxumPath<String>, Json(request): Json<ScopeBody>)
    -> ApiResult<Json<TerminalStatus>> {
    let _pin = pin_request()?;
    let scope = scope_for(&request.session_id, &request.room_id)?;
    let record = revoke_if_matches(&handle, &scope)?;
    close_record(record).await?;
    Ok(Json(TerminalStatus::absent()))
}

pub(super) fn routes() -> Router {
    Router::new()
        .route("/api/terminal", get(api_status))
        .route("/api/terminal/start", post(api_start))
        .route("/api/terminal/{handle}/input", post(api_input))
        .route("/api/terminal/{handle}/output", get(api_output))
        .route("/api/terminal/{handle}/resize", post(api_resize))
        .route("/api/terminal/{handle}/interrupt", post(api_interrupt))
        .route("/api/terminal/{handle}/close", post(api_close))
}
