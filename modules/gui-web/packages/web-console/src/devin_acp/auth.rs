//! 本机官方浏览器授权入口；页面只接收状态，不接收账号、口令或令牌。
use axum::{Json, http::{HeaderMap, StatusCode}};
use serde::{Deserialize, Serialize};
use std::{path::{Path, PathBuf}, process::Stdio, sync::{Arc, Mutex, OnceLock}, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt};
type CancellationToken = Arc<crate::ChatTurnCancellation>;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_OUTPUT: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Authentication { Unknown, Authenticated, Unauthenticated, Unavailable, CheckFailed }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Phase { Starting, Waiting, Checking, Cancelling, Completed, Failed, Cancelled, TimedOut, CleanupFailed }

impl Phase {
    fn message(self) -> &'static str {
        match self {
            Self::Starting => "正在准备 Devin 登录…",
            Self::Waiting => "请在 Devin 官方网页完成授权，完成后这里会自动更新。若网页未打开，请检查默认浏览器设置。",
            Self::Checking => "正在核对授权结果…",
            Self::Cancelling => "正在停止登录流程…",
            Self::Completed => "已确认 Devin 登录。可以获取账号模型。",
            Self::Failed => "未确认 Devin 登录，可刷新状态或重新登录。",
            Self::Cancelled => "登录流程已停止。取消不会注销已完成授权的账号。",
            Self::TimedOut => "等待授权已超时，可重新登录。",
            Self::CleanupFailed => "登录进程未确认退出，请重启应用后重试。",
        }
    }
}

struct Attempt { id: String, phase: Phase, active: bool, cancel: CancellationToken }
struct State {
    attempt: Option<Attempt>,
    authentication: Authentication,
    checked_at: Option<u64>,
    cli: Option<PathBuf>,
}
impl Default for State {
    fn default() -> Self {
        Self { attempt: None, authentication: Authentication::Unknown, checked_at: None, cli: None }
    }
}
type Manager = Arc<Mutex<State>>;
fn manager() -> Manager {
    static MANAGER: OnceLock<Manager> = OnceLock::new();
    MANAGER.get_or_init(|| Arc::new(Mutex::new(State::default()))).clone()
}

/// 健康诊断只读取既有认证结果，不在同步诊断中启动登录或远端请求。
/// CLI 路径改变、尚未核对或结果过期时显示未知；不会阻止实际会话接纳。
pub(super) fn cached_authentication() -> Authentication {
    let Ok(binary) = super::discovery::binary() else { return Authentication::Unavailable; };
    let manager = manager();
    let state = manager.lock().unwrap_or_else(|p| p.into_inner());
    if state.cli.as_ref() != Some(&binary)
        || state.attempt.as_ref().is_some_and(|attempt| attempt.active)
        || !state.checked_at.is_some_and(|checked| crate::unix_timestamp_millis().saturating_sub(checked) <= 60_000) {
        return Authentication::Unknown;
    }
    state.authentication
}

#[derive(Serialize)]
pub(crate) struct AuthView {
    cli_available: bool,
    authentication: Authentication,
    checked_at: Option<u64>,
    login: Option<LoginView>,
    message: &'static str,
}
#[derive(Serialize)]
struct LoginView { attempt_id: String, phase: Phase, active: bool, message: &'static str }

fn view(state: &State, available: bool) -> AuthView {
    let authentication = if available { state.authentication } else { Authentication::Unavailable };
    AuthView {
        cli_available: available, authentication, checked_at: state.checked_at,
        login: state.attempt.as_ref().map(|attempt| LoginView {
            attempt_id: attempt.id.clone(), phase: attempt.phase, active: attempt.active, message: attempt.phase.message(),
        }),
        message: match authentication {
            Authentication::Authenticated => "Devin 已登录。",
            Authentication::Unauthenticated => "Devin 尚未登录。",
            Authentication::Unavailable => "尚未找到 Devin 登录组件，请检查 Devin 安装。",
            Authentication::Unknown => "Devin 登录状态尚未确认。",
            Authentication::CheckFailed => "暂时无法核对 Devin 登录状态，请重试。",
        },
    }
}

fn classify_status(bytes: &[u8]) -> Authentication {
    let Ok(text) = std::str::from_utf8(bytes) else { return Authentication::Unknown; };
    let text = text.to_ascii_lowercase();
    // 未登录判断必须先于正向判断，不能把 "not logged in" 误报为已登录。
    if text.contains("not logged in") || text.contains("not authenticated") {
        Authentication::Unauthenticated
    } else if text.lines().any(|line| line.trim_start().starts_with("logged in") || line.trim_start().starts_with("✓ logged in"))
        || text.contains("logged in as") || text.contains("logged in with")
        || text.contains("you are logged in") || text.contains("authenticated as") {
        Authentication::Authenticated
    } else {
        Authentication::Unknown
    }
}

async fn check_auth(binary: &Path, cwd: &Path) -> Authentication {
    // 已登录状态会核对远端账号，实测耗时超过五秒；仍保留有限等待。
    match super::transport::run_readonly(binary, &["auth", "status"], cwd, Duration::from_secs(20)).await {
        Ok(bytes) => classify_status(&bytes),
        Err(message) if message.contains("尚未登录") => Authentication::Unauthenticated,
        Err(_) => Authentication::CheckFailed,
    }
}

/// 状态检查与任务接纳分开：CLI 认证不代表 ACP 工具与任务能力已经验收。
pub(crate) async fn status() -> Json<AuthView> {
    let manager = manager();
    let binary = match super::discovery::binary() {
        Ok(binary) => binary,
        Err(_) => return Json(view(&manager.lock().unwrap_or_else(|p| p.into_inner()), false)),
    };
    {
        let mut state = manager.lock().unwrap_or_else(|p| p.into_inner());
        if state.attempt.as_ref().is_some_and(|attempt| attempt.active) {
            return Json(view(&state, true));
        }
        if state.cli.as_ref() != Some(&binary) {
            state.authentication = Authentication::Unknown;
            state.checked_at = None;
            state.cli = Some(binary.clone());
        }
    }
    // 限制多页面同时查询；不在用户工程里加载登录命令。
    static CHECKS: OnceLock<tokio::sync::Semaphore> = OnceLock::new();
    let Ok(_permit) = CHECKS.get_or_init(|| tokio::sync::Semaphore::new(2)).try_acquire() else {
        return Json(view(&manager.lock().unwrap_or_else(|p| p.into_inner()), true));
    };
    let observed_attempt = manager.lock().unwrap_or_else(|p| p.into_inner())
        .attempt.as_ref().map(|a| a.id.clone());
    let observed = match tempfile::Builder::new().prefix("coolzhu-devin-auth-status-").tempdir() {
        Ok(cwd) => check_auth(&binary, cwd.path()).await,
        Err(_) => Authentication::CheckFailed,
    };
    let mut state = manager.lock().unwrap_or_else(|p| p.into_inner());
    if !state.attempt.as_ref().is_some_and(|a| a.active)
        && state.attempt.as_ref().map(|a| a.id.clone()) == observed_attempt
        && state.cli.as_ref() == Some(&binary) {
        state.authentication = observed;
        state.checked_at = Some(super::super::unix_timestamp_millis());
    }
    Json(view(&state, true))
}

pub(super) fn require_local_action(headers: &HeaderMap) -> super::super::ApiResult<()> {
    let denied = || super::super::api_error(StatusCode::FORBIDDEN, "请从本机应用界面操作 Devin。");
    let host = headers.get("host").and_then(|v| v.to_str().ok()).ok_or_else(denied)?;
    let origin = headers.get("origin").and_then(|v| v.to_str().ok()).ok_or_else(denied)?;
    let url = reqwest::Url::parse(&format!("http://{host}")).map_err(|_| denied())?;
    if !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
        || url.path() != "/" || !url.username().is_empty() || url.password().is_some()
        || url.query().is_some() || url.fragment().is_some()
        || origin != format!("http://{host}") {
        return Err(denied());
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StartRequest {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CancelRequest { attempt_id: String }

/// GET 只核查状态；只有本机同源 POST 才会启动官方授权。
pub(crate) async fn start(headers: HeaderMap, Json(_): Json<StartRequest>) -> super::super::ApiResult<Json<AuthView>> {
    require_local_action(&headers)?;
    let binary = super::discovery::binary()
        .map_err(|message| super::super::api_error(StatusCode::SERVICE_UNAVAILABLE, message))?;
    let manager = manager();
    let (id, cancel) = {
        let mut state = manager.lock().unwrap_or_else(|p| p.into_inner());
        if state.attempt.as_ref().is_some_and(|a| a.active) {
            return Ok(Json(view(&state, true)));
        }
        let id = crate::random_hex_identifier(32, "Devin 登录流程")
            .map_err(|_| crate::api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建登录流程身份。"))?;
        let cancel = Arc::new(crate::ChatTurnCancellation::new());
        state.authentication = Authentication::Unknown;
        state.checked_at = None;
        state.cli = Some(binary.clone());
        state.attempt = Some(Attempt { id: id.clone(), phase: Phase::Starting, active: true, cancel: cancel.clone() });
        (id, cancel)
    };
    let owner = manager.clone();
    tokio::spawn(async move { login(owner, id, binary, cancel).await; });
    let snapshot = view(&manager.lock().unwrap_or_else(|p| p.into_inner()), true);
    Ok(Json(snapshot))
}

pub(crate) async fn cancel(headers: HeaderMap, Json(request): Json<CancelRequest>) -> super::super::ApiResult<Json<AuthView>> {
    require_local_action(&headers)?;
    let manager = manager();
    let mut state = manager.lock().unwrap_or_else(|p| p.into_inner());
    let attempt = state.attempt.as_mut().filter(|a| a.id == request.attempt_id)
        .ok_or_else(|| super::super::api_error(StatusCode::CONFLICT, "登录流程已变化，请刷新后重试。"))?;
    if attempt.active {
        attempt.cancel.request();
        if attempt.phase != Phase::CleanupFailed { attempt.phase = Phase::Cancelling; }
    }
    Ok(Json(view(&state, state.cli.is_some())))
}

fn phase(manager: &Manager, id: &str, phase: Phase) {
    let mut state = manager.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(attempt) = state.attempt.as_mut().filter(|a| a.id == id && a.active && !a.cancel.is_requested()) {
        attempt.phase = phase;
    }
}

fn finish(manager: &Manager, id: &str, phase: Phase, authentication: Authentication) {
    let mut state = manager.lock().unwrap_or_else(|p| p.into_inner());
    let Some(attempt) = state.attempt.as_mut().filter(|a| a.id == id) else { return; };
    attempt.phase = if attempt.cancel.is_requested() && phase != Phase::CleanupFailed { Phase::Cancelled } else { phase };
    // 没有排空证明时不允许重复创建登录进程。
    attempt.active = phase == Phase::CleanupFailed;
    state.authentication = authentication;
    state.checked_at = Some(super::super::unix_timestamp_millis());
}

async fn discard_output(mut reader: impl AsyncRead + Unpin) -> std::io::Result<()> {
    let mut buffer = [0u8; 4096];
    let mut total = 0usize;
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 { return Ok(()); }
        total = total.saturating_add(count);
        if total > MAX_OUTPUT { return Err(std::io::Error::other("登录输出超过限制。")); }
    }
}

/// 只在官方命令固定入口调用；测试参数接缝不暴露为 HTTP 参数。
async fn browser_login(binary: &Path, args: &[&str], cwd: &Path, cancel: &CancellationToken,
                       timeout: Duration, started: impl FnOnce()) -> Phase {
    if cancel.is_requested() { return Phase::Cancelled; }
    let mut command = tokio::process::Command::new(binary);
    command.args(args).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    let Ok((mut child, job)) = windows_process_guard::ChildProcessJob::spawn_managed_async(&mut command) else { return Phase::Failed; };
    #[cfg(not(windows))]
    let Ok(mut child) = command.spawn() else { return Phase::Failed; };
    started();
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else { return Phase::CleanupFailed; };
    let outcome = tokio::select! {
        biased;
        _ = cancel.cancelled() => Phase::Cancelled,
        _ = tokio::time::sleep(timeout) => Phase::TimedOut,
        result = async { tokio::try_join!(discard_output(stdout), discard_output(stderr), child.wait()) } => {
            match result { Ok(((), (), exit)) if exit.success() => Phase::Completed, _ => Phase::Failed }
        }
    };
    #[cfg(windows)]
    {
        let drained = tokio::task::spawn_blocking(move || job.terminate_and_wait(Duration::from_secs(3))).await;
        if !matches!(drained, Ok(Ok(()))) { return Phase::CleanupFailed; }
    }
    #[cfg(not(windows))]
    { let _ = child.start_kill(); }
    if !matches!(tokio::time::timeout(Duration::from_secs(2), child.wait()).await, Ok(Ok(_))) {
        return Phase::CleanupFailed;
    }
    outcome
}

async fn login(manager: Manager, id: String, binary: PathBuf, cancel: CancellationToken) {
    let Ok(cwd) = tempfile::Builder::new().prefix("coolzhu-devin-login-").tempdir() else {
        finish(&manager, &id, Phase::Failed, Authentication::CheckFailed); return;
    };
    let version = super::transport::run_readonly(&binary, &["--version"], cwd.path(), Duration::from_secs(5)).await;
    let valid_version = version.as_ref().ok().and_then(|v| std::str::from_utf8(v).ok())
        .map(str::trim).is_some_and(|version| !version.is_empty() && version.len() <= 256 && !version.chars().any(char::is_control)
            && std::env::var("COOLZHU_DEVIN_CLI_VERSION").ok().filter(|p| !p.trim().is_empty()).is_none_or(|p| p.trim() == version));
    if !valid_version {
        finish(&manager, &id, Phase::Failed, Authentication::CheckFailed); return;
    }
    let before = check_auth(&binary, cwd.path()).await;
    if before == Authentication::Authenticated {
        finish(&manager, &id, Phase::Completed, before); return;
    }
    #[cfg(windows)]
    let result = windows_browser_login(&binary, cwd.path(), &cancel, LOGIN_TIMEOUT,
        || phase(&manager, &id, Phase::Waiting)).await;
    #[cfg(not(windows))]
    let result = browser_login(&binary, &["auth", "login"], cwd.path(), &cancel, LOGIN_TIMEOUT,
        || phase(&manager, &id, Phase::Waiting)).await;
    if result == Phase::CleanupFailed {
        finish(&manager, &id, result, Authentication::Unknown); return;
    }
    phase(&manager, &id, Phase::Checking);
    let authentication = check_auth(&binary, cwd.path()).await;
    let result = if result == Phase::Completed && authentication != Authentication::Authenticated { Phase::Failed } else { result };
    finish(&manager, &id, result, authentication);
}

/// 只识别固定版本的默认浏览器选项；不把终端原文、授权 URL 或凭据交给页面。
fn default_browser_prompt(text: &str) -> bool {
    let mut plain = String::new();
    let mut escape = false;
    for ch in text.chars() {
        if ch == '\u{1b}' { escape = true; continue; }
        if escape {
            if ch.is_ascii_alphabetic() || ch == '~' { escape = false; }
            continue;
        }
        plain.push(ch);
    }
    // ConPTY 用光标定位绘制菜单，各行不一定有换行符。
    plain.contains("❭ 1 Log in with browser")
        && plain.contains("select") && plain.contains("confirm")
}

#[cfg(windows)]
async fn windows_browser_login(binary: &Path, cwd: &Path, cancel: &CancellationToken,
                               timeout: Duration, started: impl FnOnce()) -> Phase {
    if cancel.is_requested() { return Phase::Cancelled; }
    let binary = binary.to_owned(); let cwd = cwd.to_owned();
    let Ok(Ok(mut terminal)) = tokio::task::spawn_blocking(move ||
        windows_process_guard::ManagedConPty::spawn_devin_browser_login(&binary, &cwd)).await else { return Phase::Failed; };
    started();
    let deadline = tokio::time::Instant::now() + timeout;
    let mut cursor = 0; let mut total = 0usize; let mut prompt = String::new(); let mut selected = false;
    let outcome = loop {
        if cancel.is_requested() { break Phase::Cancelled; }
        if tokio::time::Instant::now() >= deadline { break Phase::TimedOut; }
        let output = terminal.read_since(cursor, 16 * 1024);
        cursor = output.next_cursor; total = total.saturating_add(output.text.len());
        if output.truncated || total > MAX_OUTPUT { break Phase::Failed; }
        if !selected {
            prompt.push_str(&output.text);
            if default_browser_prompt(&prompt) {
                if terminal.write(b"\r").is_err() { break Phase::Failed; }
                selected = true; prompt.clear();
            } else if prompt.len() > 16 * 1024 { break Phase::Failed; }
        }
        match terminal.exit_code() {
            Ok(Some(0)) => break Phase::Completed,
            Ok(Some(_)) | Err(_) => break Phase::Failed,
            Ok(None) => {},
        }
        tokio::select! { _ = cancel.cancelled() => {}, _ = tokio::time::sleep(Duration::from_millis(100)) => {} }
    };
    if !matches!(tokio::task::spawn_blocking(move || terminal.close_verified()).await, Ok(Ok(()))) {
        return Phase::CleanupFailed;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "仅用户授权后诊断真实 CLI 浏览器登录，不生成模型请求"]
    async fn real_browser_login_prompt_probe() {
        assert_eq!(std::env::var("COOLZHU_DEVIN_REAL_AUTH").as_deref(), Ok("1"), "必须显式开启真实授权诊断");
        let binary = super::super::discovery::binary().unwrap();
        let cwd = tempfile::tempdir().unwrap();
        let mut terminal = windows_process_guard::ManagedConPty::spawn_devin_browser_login(&binary, cwd.path()).unwrap();
        let mut cursor = 0; let mut text = String::new(); let mut selected = false;
        for _ in 0..100 {
            let batch = terminal.read_since(cursor, 16 * 1024); cursor = batch.next_cursor;
            if batch.text.contains("\x1b[6n") { terminal.write(b"\x1b[1;1R").unwrap(); }
            if !selected {
                text.push_str(&batch.text);
                if default_browser_prompt(&text) { terminal.write(b"\r").unwrap(); selected = true; }
            }
            if terminal.exit_code().unwrap().is_some() { break; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        println!("诊断：bytes={} browser={} select={} confirm={} selected={} DSR={} chooser276d={} chooser276f={}",
            text.len(), text.contains("Log in with browser"), text.contains("select"), text.contains("confirm"), selected,
            text.contains("\x1b[6n"), text.contains('❭'), text.contains('❯'));
        terminal.close_verified().unwrap();
        assert!(selected, "未确认官方浏览器默认选项；未打印授权原文");
    }
    #[test]
    fn browser_choice_requires_exact_selected_default_and_complete_prompt() {
        assert!(default_browser_prompt("\x1b[32m❭ 1 Log in with browser\x1b[0m\n↑↓ select · ↵ confirm · esc cancel"));
        assert!(default_browser_prompt("Choose login\x1b[4;1H❭ 1 Log in with browser\x1b[8;1H↑↓ select · ↵ confirm"));
        assert!(!default_browser_prompt("· 1 Log in with browser\n❭ 3 Log in with Windsurf for Enterprise\nselect confirm"));
        assert!(!default_browser_prompt("❭ 1 Log in with browser"));
        assert!(!default_browser_prompt("Log in with browser select confirm"));
    }
    #[test]
    fn login_status_never_leaks_credentials_and_unknown_is_not_success() {
        assert_eq!(classify_status(b"Not logged in. Run devin auth login."), Authentication::Unauthenticated);
        assert_eq!(classify_status(b"Logged in as private@example.com token=secret"), Authentication::Authenticated);
        assert_eq!(classify_status(b"Logged in (Devin)\nEmail: private@example.com"), Authentication::Authenticated);
        assert_eq!(classify_status(b"command finished"), Authentication::Unknown);
        let state = State { authentication: Authentication::Authenticated, ..State::default() };
        let json = serde_json::to_string(&view(&state, true)).unwrap();
        assert!(!json.contains("private@example.com") && !json.contains("secret"));
        assert_eq!(view(&state, false).authentication, Authentication::Unavailable);
    }
    #[test]
    fn auth_mutations_require_exact_loopback_origin_and_do_not_accept_secrets() {
        for (host, origin, allowed) in [
            ("127.0.0.1:8765", "http://127.0.0.1:8765", true),
            ("localhost:8765", "http://localhost:8765", true),
            ("127.0.0.1:8765", "https://evil.test", false),
            ("evil.test:8765", "http://evil.test:8765", false),
            ("127.0.0.1:8765", "null", false),
            ("127.0.0.1:8765", "http://localhost:8765", false),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("host", host.parse().unwrap()); headers.insert("origin", origin.parse().unwrap());
            assert_eq!(require_local_action(&headers).is_ok(), allowed);
        }
        assert!(require_local_action(&HeaderMap::new()).is_err());
        assert!(serde_json::from_str::<StartRequest>(r#"{"token":"secret"}"#).is_err());
    }
    #[test]
    fn stale_attempt_cannot_finish_new_login_and_cancel_does_not_fake_logout() {
        let cancel = Arc::new(crate::ChatTurnCancellation::new()); cancel.request();
        let manager = Arc::new(Mutex::new(State {
            attempt: Some(Attempt {id:"new".into(),phase:Phase::Waiting,active:true,cancel}), ..State::default()
        }));
        finish(&manager, "old", Phase::Completed, Authentication::Authenticated);
        assert!(manager.lock().unwrap().attempt.as_ref().unwrap().active);
        finish(&manager, "new", Phase::Completed, Authentication::Authenticated);
        let state = manager.lock().unwrap();
        assert_eq!(state.attempt.as_ref().unwrap().phase, Phase::Cancelled);
        assert!(!state.attempt.as_ref().unwrap().active);
        assert_eq!(state.authentication, Authentication::Authenticated);
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn browser_auth_process_has_bounded_timeout_cancellation_and_exit() {
        let cwd = tempfile::tempdir().unwrap();
        let binary = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        let token = Arc::new(crate::ChatTurnCancellation::new());
        // 完成分支只验证子进程退出，避免把 PowerShell 冷启动耗时当登录失败。
        let immediate_exit = Path::new(r"C:\Windows\System32\cmd.exe");
        assert_eq!(browser_login(immediate_exit, &["/D", "/C", "echo done"], cwd.path(), &token, Duration::from_secs(10), || {}).await, Phase::Completed);
        assert_eq!(browser_login(binary, &["-NoProfile", "-Command", "Start-Sleep -Seconds 30"], cwd.path(), &token, Duration::from_millis(200), || {}).await, Phase::TimedOut);
        let cancel = token.clone();
        tokio::spawn(async move { tokio::time::sleep(Duration::from_millis(200)).await; cancel.request(); });
        assert_eq!(browser_login(binary, &["-NoProfile", "-Command", "Start-Sleep -Seconds 30"], cwd.path(), &token, Duration::from_secs(10), || {}).await, Phase::Cancelled);
    }
}
