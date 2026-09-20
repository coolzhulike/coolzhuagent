//! Non-visual package launcher core.
//!
//! Reads `package-launcher.json`, resolves config-relative paths, starts the
//! Web Console hidden with stdout/stderr redirected, polls its health endpoint
//! within a bounded budget, then starts the Tauri shell forwarding
//! `--web-console-pid=<pid>`. Startup outcomes are persisted to
//! `package-selfcheck-last.json` and failures are surfaced via [`LaunchError`].
//!
//! The process-spawning and time/sleep collaborators are injectable through
//! [`LaunchSpawner`] and the closures passed to [`launch`], so the full bring-up
//! sequence is unit-testable without spawning real processes.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

pub const WEB_CONSOLE_MANAGED_ENV: &str = "COOLZHU_DESKTOP_SHELL_MANAGED";
pub const WEB_CONSOLE_RUNTIME_ENV: &str = "COOLZHU_RUNTIME_DIR";
pub const WEB_CONSOLE_SESSION_DB_ENV: &str = "COOLZHU_WEB_SESSION_DB";
pub const WEB_CONSOLE_SESSION_STORE_ENV: &str = "COOLZHU_WEB_SESSION_STORE";
pub const WEB_CONSOLE_ATTACHMENT_STORE_ENV: &str = "COOLZHU_WEB_ATTACHMENT_STORE";

pub fn web_console_environment() -> [(&'static str, &'static str); 1] {
    [(WEB_CONSOLE_MANAGED_ENV, "1")]
}

/// 安装启动器显式指定运行态数据目录，避免会话、附件和兼容 JSON
/// 随安装目录/开发工作区变化。直接 `cargo run` 不经过该函数，因此开发态仍沿用工作区。
pub fn web_console_runtime_environment(runtime_dir: &Path) -> [(OsString, OsString); 4] {
    let state_dir = runtime_dir.join(".coolzhu");
    [
        (
            OsString::from(WEB_CONSOLE_RUNTIME_ENV),
            runtime_dir.as_os_str().to_os_string(),
        ),
        (
            OsString::from(WEB_CONSOLE_SESSION_DB_ENV),
            state_dir.join("web-sessions.sqlite3").into_os_string(),
        ),
        (
            OsString::from(WEB_CONSOLE_SESSION_STORE_ENV),
            state_dir.join("web-sessions.json").into_os_string(),
        ),
        (
            OsString::from(WEB_CONSOLE_ATTACHMENT_STORE_ENV),
            state_dir.join("attachments").into_os_string(),
        ),
    ]
}

/// Outcome of a single health probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Endpoint responded healthy; stop polling.
    Ready,
    /// Endpoint responded but not healthy yet; keep polling.
    NotReady,
    /// Probe failed transiently (e.g. connection refused); keep polling.
    Transient,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListenerOwner {
    pub pid: u32,
    pub alive: bool,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListenerRecoveryAction {
    Free,
    CleanupOrphan {
        parent_pid: u32,
    },
    CleanupKnownHolder {
        pid: u32,
    },
    Block {
        pid: u32,
        process_name: Option<String>,
    },
}

pub fn classify_listener_recovery(
    health_ready: bool,
    owner: Option<&ListenerOwner>,
) -> ListenerRecoveryAction {
    let Some(owner) = owner else {
        return ListenerRecoveryAction::Free;
    };
    if !owner.alive {
        return ListenerRecoveryAction::CleanupOrphan {
            parent_pid: owner.pid,
        };
    }
    let known_holder = owner
        .process_name
        .as_deref()
        .map(|name| {
            let normalized = name.to_ascii_lowercase();
            normalized == "llama-server.exe" || normalized == "conhost.exe"
        })
        .unwrap_or(false);
    if !health_ready && known_holder {
        ListenerRecoveryAction::CleanupKnownHolder { pid: owner.pid }
    } else {
        ListenerRecoveryAction::Block {
            pid: owner.pid,
            process_name: owner.process_name.clone(),
        }
    }
}

/// 判断健康的本地 Web Console 是否属于可复用的既有实例。
///
/// 启动器重复点击时只把 `--show-console` 转发给 Tauri shell，让
/// `tauri-plugin-single-instance` 唤起已有控制台；不对未知监听者或未健康的服务放行。
pub fn is_live_web_console_instance(health_ready: bool, owner: Option<&ListenerOwner>) -> bool {
    if !health_ready {
        return false;
    }
    let Some(owner) = owner else {
        return false;
    };
    if !owner.alive {
        return false;
    }
    owner.process_name.as_deref().is_some_and(|name| {
        Path::new(name)
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| {
                value.eq_ignore_ascii_case("coolzhu-web-console.exe")
                    || value.eq_ignore_ascii_case("coolzhu-web-console")
            })
    })
}

/// Errors surfaced when the launcher cannot bring the package online.
#[derive(Debug)]
pub enum LaunchError {
    /// `package-launcher.json` could not be read or parsed.
    ConfigInvalid(String),
    /// A configured executable path does not exist on disk.
    ExecutableMissing { role: &'static str, path: PathBuf },
    /// The Web Console process could not be spawned.
    WebConsoleSpawn(std::io::Error),
    /// The health endpoint did not become ready within the configured budget.
    HealthTimeout { url: String, waited_secs: u64 },
    /// The Tauri shell process could not be spawned.
    TauriSpawn(std::io::Error),
    /// The log directory or self-check file could not be written.
    Persistence(std::io::Error),
}

impl std::fmt::Display for LaunchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConfigInvalid(msg) => {
                write!(f, "package-launcher config invalid: {msg}")
            }
            Self::ExecutableMissing { role, path } => {
                write!(f, "{role} executable not found at {}", path.display())
            }
            Self::WebConsoleSpawn(e) => write!(f, "failed to spawn web console: {e}"),
            Self::HealthTimeout { url, waited_secs } => write!(
                f,
                "web console health check timed out after {waited_secs}s at {url}"
            ),
            Self::TauriSpawn(e) => write!(f, "failed to spawn tauri shell: {e}"),
            Self::Persistence(e) => write!(f, "failed to persist launcher self-check: {e}"),
        }
    }
}

impl std::error::Error for LaunchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WebConsoleSpawn(e) | Self::TauriSpawn(e) | Self::Persistence(e) => Some(e),
            _ => None,
        }
    }
}

/// One executable entry (path + extra args) from the launcher config.
#[derive(Debug, Clone)]
pub struct ExecutableSpec {
    /// Resolved (config-relative) path to the executable.
    pub path: PathBuf,
    /// Extra command-line arguments forwarded verbatim.
    pub args: Vec<String>,
}

/// Fully resolved launcher configuration.
#[derive(Debug, Clone)]
pub struct LauncherConfig {
    pub web_console: ExecutableSpec,
    pub tauri: ExecutableSpec,
    pub health_url: String,
    pub health_timeout: Duration,
    pub health_poll_interval: Duration,
    /// 安装态子进程的可写工作目录。默认从 log_dir 推导到 LocalAppData/CoolzhuAgent。
    pub runtime_dir: PathBuf,
    pub log_dir: PathBuf,
    /// Always `<log_dir>/package-selfcheck-last.json`.
    pub selfcheck_file: PathBuf,
}

impl LauncherConfig {
    /// Parse from raw JSON text, resolving relative paths against `config_dir`
    /// (the directory that contains `package-launcher.json`).
    pub fn from_json(text: &str, config_dir: &Path) -> Result<Self, LaunchError> {
        Self::from_json_with_env(text, config_dir, |name| std::env::var_os(name))
    }

    /// Parse from raw JSON text with an injectable OS environment lookup.
    ///
    /// Environment expansion is limited to OS-provided path placeholders such as
    /// `%LOCALAPPDATA%` so packaged installs can write logs outside
    /// `Program Files`; business switches still come from the config file.
    pub fn from_json_with_env<F>(
        text: &str,
        config_dir: &Path,
        mut env_lookup: F,
    ) -> Result<Self, LaunchError>
    where
        F: FnMut(&str) -> Option<OsString>,
    {
        let value: Value = serde_json::from_str(text)
            .map_err(|e| LaunchError::ConfigInvalid(format!("invalid JSON: {e}")))?;

        let web_console =
            Self::parse_executable(&value, "web_console", config_dir, &mut env_lookup)?;
        let tauri = Self::parse_executable(&value, "tauri", config_dir, &mut env_lookup)?;
        let health_url = value
            .get("health_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| LaunchError::ConfigInvalid("missing health_url".into()))?;
        let health_timeout_secs = value
            .get("health_timeout_secs")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| LaunchError::ConfigInvalid("missing health_timeout_secs".into()))?;
        let health_poll_interval_ms = value
            .get("health_poll_interval_ms")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| LaunchError::ConfigInvalid("missing health_poll_interval_ms".into()))?;
        let log_dir = Self::parse_path(&value, "log_dir", config_dir, &mut env_lookup)?;
        let runtime_dir = match value.get("runtime_dir").and_then(|v| v.as_str()) {
            Some(raw) => resolve_config_path_text(raw, config_dir, &mut env_lookup)?,
            None => default_runtime_dir_from_log_dir(&log_dir),
        };
        let selfcheck_file = log_dir.join("package-selfcheck-last.json");

        Ok(Self {
            web_console,
            tauri,
            health_url,
            health_timeout: Duration::from_secs(health_timeout_secs),
            health_poll_interval: Duration::from_millis(health_poll_interval_ms),
            runtime_dir,
            log_dir,
            selfcheck_file,
        })
    }

    fn parse_executable(
        value: &Value,
        role: &'static str,
        config_dir: &Path,
        env_lookup: &mut dyn FnMut(&str) -> Option<OsString>,
    ) -> Result<ExecutableSpec, LaunchError> {
        let entry = value
            .get(role)
            .ok_or_else(|| LaunchError::ConfigInvalid(format!("missing {role} section")))?;
        let exec_str = entry
            .get("executable")
            .and_then(|v| v.as_str())
            .ok_or_else(|| LaunchError::ConfigInvalid(format!("missing {role}.executable")))?;
        let path = resolve_config_path_text(exec_str, config_dir, env_lookup)?;
        let args = entry
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Ok(ExecutableSpec { path, args })
    }

    fn parse_path(
        value: &Value,
        key: &str,
        config_dir: &Path,
        env_lookup: &mut dyn FnMut(&str) -> Option<OsString>,
    ) -> Result<PathBuf, LaunchError> {
        let s = value
            .get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| LaunchError::ConfigInvalid(format!("missing {key}")))?;
        resolve_config_path_text(s, config_dir, env_lookup)
    }
}

fn default_runtime_dir_from_log_dir(log_dir: &Path) -> PathBuf {
    let is_launcher_log_dir = log_dir
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("package-launcher"));
    let logs_dir = log_dir.parent();
    let is_logs_dir = logs_dir
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("logs"));
    if is_launcher_log_dir && is_logs_dir {
        return logs_dir
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .unwrap_or_else(|| log_dir.to_path_buf());
    }
    log_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| log_dir.to_path_buf())
}

fn resolve_config_path_text(
    raw_path: &str,
    config_dir: &Path,
    env_lookup: &mut dyn FnMut(&str) -> Option<OsString>,
) -> Result<PathBuf, LaunchError> {
    let expanded = expand_os_env_placeholders(raw_path, env_lookup)?;
    Ok(resolve_config_relative(Path::new(&expanded), config_dir))
}

fn expand_os_env_placeholders(
    raw: &str,
    env_lookup: &mut dyn FnMut(&str) -> Option<OsString>,
) -> Result<String, LaunchError> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;

    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('%') else {
            out.push('%');
            out.push_str(after_start);
            return Ok(out);
        };
        let name = &after_start[..end];
        if name.is_empty() {
            out.push_str("%%");
        } else {
            let value = env_lookup(name).ok_or_else(|| {
                LaunchError::ConfigInvalid(format!(
                    "environment variable %{name}% not set for configured path"
                ))
            })?;
            out.push_str(&value.to_string_lossy());
        }
        rest = &after_start[end + 1..];
    }

    out.push_str(rest);
    Ok(out)
}

/// Resolve a path from the launcher config.
///
/// Relative paths are interpreted against `config_dir` (the directory that
/// holds `package-launcher.json`); absolute paths are kept unchanged.
pub fn resolve_config_relative(path: &Path, config_dir: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        config_dir.join(path)
    }
}

/// Poll `probe` until it reports [`ProbeOutcome::Ready`] or `timeout` is
/// consumed (measured via `now_fn`, sleeping `poll_interval` between probes via
/// `sleep_fn`).
///
/// `now_fn` / `sleep_fn` are injected so the bounded budget can be exercised
/// deterministically in tests without real sleeping; the real entrypoint wires
/// them to `Instant::now`-based elapsed and `std::thread::sleep`.
pub fn poll_health<N, S>(
    health_url: &str,
    timeout: Duration,
    poll_interval: Duration,
    probe: &mut dyn FnMut() -> ProbeOutcome,
    mut now_fn: N,
    mut sleep_fn: S,
) -> Result<(), LaunchError>
where
    N: FnMut() -> Duration,
    S: FnMut(Duration),
{
    let start = now_fn();
    loop {
        match probe() {
            ProbeOutcome::Ready => return Ok(()),
            ProbeOutcome::NotReady | ProbeOutcome::Transient => {}
        }
        sleep_fn(poll_interval);
        if now_fn().saturating_sub(start) >= timeout {
            return Err(LaunchError::HealthTimeout {
                url: health_url.to_string(),
                waited_secs: timeout.as_secs(),
            });
        }
    }
}

/// 构造传给 Tauri shell 的参数，只在配置参数前注入
/// `--web-console-pid=<pid>`。
///
/// 启动器默认不添加 `--show-console`：缺少该标志才是 Tauri 播放启动演出的信号；
/// 配置或转发参数中显式提供的 `--show-console` 会原样保留。
pub fn build_tauri_args(web_console_pid: u32, extra_args: &[String]) -> Vec<String> {
    let mut args = Vec::with_capacity(extra_args.len() + 1);
    args.push(format!("--web-console-pid={web_console_pid}"));
    args.extend(extra_args.iter().cloned());
    args
}

/// Self-check record persisted after each launch attempt.
#[derive(Debug, Clone)]
pub struct SelfcheckPayload {
    pub ok: bool,
    pub web_console_pid: Option<u32>,
    pub tauri_pid: Option<u32>,
    pub health_url: String,
    pub error: Option<String>,
    pub timestamp_ms: u64,
}

/// Write the self-check JSON file atomically, creating `log_dir` first.
pub fn write_selfcheck(
    selfcheck_file: &Path,
    log_dir: &Path,
    payload: &SelfcheckPayload,
) -> Result<(), LaunchError> {
    fs::create_dir_all(log_dir).map_err(LaunchError::Persistence)?;
    let body = json!({
        "ok": payload.ok,
        "web_console_pid": payload.web_console_pid,
        "tauri_pid": payload.tauri_pid,
        "health_url": payload.health_url,
        "error": payload.error,
        "timestamp_ms": payload.timestamp_ms,
    });
    let mut tmp = selfcheck_file.to_path_buf();
    tmp.set_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp).map_err(LaunchError::Persistence)?;
        f.write_all(body.to_string().as_bytes())
            .map_err(LaunchError::Persistence)?;
        f.sync_all().map_err(LaunchError::Persistence)?;
    }
    fs::rename(&tmp, selfcheck_file).map_err(LaunchError::Persistence)?;
    Ok(())
}

/// Abstraction over spawning the Web Console and Tauri shell processes.
///
/// Implementations: a real one backed by `std::process::Command` (in the
/// binary) and fakes in tests.
pub trait LaunchSpawner {
    /// Spawn the Web Console hidden with stdout/stderr redirected to `log_file`.
    /// Returns the spawned child PID.
    fn spawn_web_console(
        &mut self,
        spec: &ExecutableSpec,
        runtime_dir: &Path,
        log_file: &Path,
    ) -> Result<u32, LaunchError>;
    /// Spawn the Tauri shell with the given (already pid-injected) args.
    /// Returns the spawned child PID.
    fn spawn_tauri(
        &mut self,
        spec: &ExecutableSpec,
        args: &[String],
        runtime_dir: &Path,
        log_file: &Path,
    ) -> Result<u32, LaunchError>;
    /// 回收本次 launcher 刚拉起的进程；失败路径必须调用，避免隐藏进程占用端口。
    fn terminate_process(&mut self, pid: u32) -> io::Result<()>;
}

/// Result of a successful bring-up.
#[derive(Debug, Clone, Copy)]
pub struct LaunchOutcome {
    pub web_console_pid: u32,
    pub tauri_pid: u32,
}

/// Run the full bring-up sequence using injected collaborators.
///
/// Verifies both executables exist, spawns the Web Console (hidden, stdio
/// redirected), polls health within the configured budget, spawns the Tauri
/// shell with `--web-console-pid=<pid>`, and persists a self-check record. On
/// any failure a failed self-check is still written when possible and the error
/// is surfaced.
#[allow(clippy::too_many_arguments)]
pub fn launch<L, N, S>(
    config: &LauncherConfig,
    spawner: &mut L,
    probe: &mut dyn FnMut() -> ProbeOutcome,
    now_fn: N,
    sleep_fn: S,
    timestamp_ms: u64,
) -> Result<LaunchOutcome, LaunchError>
where
    L: LaunchSpawner,
    N: FnMut() -> Duration,
    S: FnMut(Duration),
{
    if !config.web_console.path.exists() {
        return Err(LaunchError::ExecutableMissing {
            role: "web_console",
            path: config.web_console.path.clone(),
        });
    }
    if !config.tauri.path.exists() {
        return Err(LaunchError::ExecutableMissing {
            role: "tauri",
            path: config.tauri.path.clone(),
        });
    }

    fs::create_dir_all(&config.runtime_dir).map_err(LaunchError::Persistence)?;
    fs::create_dir_all(&config.log_dir).map_err(LaunchError::Persistence)?;
    let log_file = config.log_dir.join("web-console.stdout.log");

    let web_pid = spawner.spawn_web_console(&config.web_console, &config.runtime_dir, &log_file)?;

    if let Err(e) = poll_health(
        &config.health_url,
        config.health_timeout,
        config.health_poll_interval,
        probe,
        now_fn,
        sleep_fn,
    ) {
        let cleanup_error = spawner.terminate_process(web_pid).err();
        let error_text = match cleanup_error {
            Some(cleanup) => format!("{e}; web console cleanup failed: {cleanup}"),
            None => e.to_string(),
        };
        let payload = SelfcheckPayload {
            ok: false,
            web_console_pid: Some(web_pid),
            tauri_pid: None,
            health_url: config.health_url.clone(),
            error: Some(error_text),
            timestamp_ms,
        };
        let _ = write_selfcheck(&config.selfcheck_file, &config.log_dir, &payload);
        return Err(e);
    }

    let tauri_args = build_tauri_args(web_pid, &config.tauri.args);
    let tauri_log_file = config.log_dir.join("tauri.stdout.log");
    let tauri_pid = match spawner.spawn_tauri(
        &config.tauri,
        &tauri_args,
        &config.runtime_dir,
        &tauri_log_file,
    ) {
        Ok(pid) => pid,
        Err(e) => {
            let cleanup_error = spawner.terminate_process(web_pid).err();
            let error_text = match cleanup_error {
                Some(cleanup) => format!("{e}; web console cleanup failed: {cleanup}"),
                None => e.to_string(),
            };
            let payload = SelfcheckPayload {
                ok: false,
                web_console_pid: Some(web_pid),
                tauri_pid: None,
                health_url: config.health_url.clone(),
                error: Some(error_text),
                timestamp_ms,
            };
            let _ = write_selfcheck(&config.selfcheck_file, &config.log_dir, &payload);
            return Err(e);
        }
    };

    let payload = SelfcheckPayload {
        ok: true,
        web_console_pid: Some(web_pid),
        tauri_pid: Some(tauri_pid),
        health_url: config.health_url.clone(),
        error: None,
        timestamp_ms,
    };
    if let Err(error) = write_selfcheck(&config.selfcheck_file, &config.log_dir, &payload) {
        let _ = spawner.terminate_process(tauri_pid);
        let _ = spawner.terminate_process(web_pid);
        return Err(error);
    }

    Ok(LaunchOutcome {
        web_console_pid: web_pid,
        tauri_pid,
    })
}

/// Best-effort real HTTP health probe over a raw `TcpStream`.
///
/// Not unit-tested (it touches the network); used by the binary entrypoint.
pub fn http_probe(url: &str) -> ProbeOutcome {
    let Some((host, port, path)) = parse_http_url(url) else {
        return ProbeOutcome::Transient;
    };
    let addr_str = format!("{host}:{port}");
    let Ok(addr) = addr_str.parse::<SocketAddr>() else {
        return ProbeOutcome::Transient;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(2)) else {
        return ProbeOutcome::Transient;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let req = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return ProbeOutcome::Transient;
    }
    let mut buf = [0u8; 256];
    let n = match stream.read(&mut buf) {
        Ok(0) => return ProbeOutcome::NotReady,
        Ok(n) => n,
        Err(_) => return ProbeOutcome::Transient,
    };
    let head = std::str::from_utf8(&buf[..n]).unwrap_or("");
    if head.contains(" 200 ") || head.contains(" 200\r") {
        ProbeOutcome::Ready
    } else {
        ProbeOutcome::NotReady
    }
}

/// Parse an `http://host:port/path` URL into its parts.
fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().ok()?),
        None => (authority.to_string(), 80),
    };
    Some((host, port, path.to_string()))
}

pub fn health_endpoint_port(url: &str) -> Option<u16> {
    parse_http_url(url).map(|(_, port, _)| port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp_dir_unique(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        p.push(format!("coolzhu-app-launcher-{label}-{nanos}-{seq}"));
        p
    }

    #[test]
    fn resolves_relative_paths_against_config_dir() {
        let tmp = temp_dir_unique("cfg");
        let config_dir = tmp.join("config");
        fs::create_dir_all(&config_dir).unwrap();

        let cfg_text = r#"{
            "web_console": {"executable": "../bin/coolzhu-web-console.exe", "args": ["--headless"]},
            "tauri": {"executable": "../bin/coolzhu-tauri-shell.exe", "args": ["--ui=foo"]},
            "health_url": "http://127.0.0.1:8765/api/diagnostics/health",
            "health_timeout_secs": 12,
            "health_poll_interval_ms": 250,
            "log_dir": "../tmp/logs/package-launcher"
        }"#;
        let cfg = LauncherConfig::from_json(cfg_text, &config_dir).unwrap();

        assert_eq!(
            cfg.web_console.path,
            config_dir.join("../bin/coolzhu-web-console.exe")
        );
        assert_eq!(cfg.web_console.args, vec!["--headless".to_string()]);
        assert_eq!(
            cfg.tauri.path,
            config_dir.join("../bin/coolzhu-tauri-shell.exe")
        );
        assert_eq!(cfg.tauri.args, vec!["--ui=foo".to_string()]);
        assert_eq!(
            cfg.health_url,
            "http://127.0.0.1:8765/api/diagnostics/health"
        );
        assert_eq!(cfg.health_timeout, Duration::from_secs(12));
        assert_eq!(cfg.health_poll_interval, Duration::from_millis(250));
        assert_eq!(cfg.runtime_dir, config_dir.join("../tmp"));
        assert_eq!(cfg.log_dir, config_dir.join("../tmp/logs/package-launcher"));
        assert_eq!(
            cfg.selfcheck_file,
            config_dir
                .join("../tmp/logs/package-launcher")
                .join("package-selfcheck-last.json")
        );
    }

    #[test]
    fn absolute_paths_are_preserved_unchanged() {
        let config_dir = Path::new("C:/repo/config");
        let cfg_text = r#"{
            "web_console": {"executable": "C:/abs/web.exe", "args": []},
            "tauri": {"executable": "C:/abs/tauri.exe", "args": []},
            "health_url": "http://127.0.0.1:1/health",
            "health_timeout_secs": 1,
            "health_poll_interval_ms": 1,
            "log_dir": "C:/abs/logs"
        }"#;
        let cfg = LauncherConfig::from_json(cfg_text, config_dir).unwrap();
        assert_eq!(cfg.web_console.path, PathBuf::from("C:/abs/web.exe"));
        assert_eq!(cfg.tauri.path, PathBuf::from("C:/abs/tauri.exe"));
        assert_eq!(cfg.runtime_dir, PathBuf::from("C:/abs"));
        assert_eq!(cfg.log_dir, PathBuf::from("C:/abs/logs"));
    }

    #[test]
    fn expands_os_env_placeholders_before_resolving_config_paths() {
        let config_dir = Path::new("C:/repo/package/config");
        let cfg_text = r#"{
            "web_console": {"executable": "../bin/coolzhu-web-console.exe", "args": []},
            "tauri": {"executable": "../bin/coolzhu-tauri-shell.exe", "args": []},
            "health_url": "http://127.0.0.1:1/health",
            "health_timeout_secs": 1,
            "health_poll_interval_ms": 1,
            "log_dir": "%LOCALAPPDATA%/CoolzhuAgent/logs/package-launcher"
        }"#;

        let cfg = LauncherConfig::from_json_with_env(cfg_text, config_dir, |name| {
            (name == "LOCALAPPDATA").then(|| std::ffi::OsString::from("C:/Users/me/AppData/Local"))
        })
        .unwrap();

        assert_eq!(
            cfg.log_dir,
            PathBuf::from("C:/Users/me/AppData/Local/CoolzhuAgent/logs/package-launcher")
        );
        assert_eq!(
            cfg.runtime_dir,
            PathBuf::from("C:/Users/me/AppData/Local/CoolzhuAgent")
        );
        assert_eq!(
            cfg.selfcheck_file,
            cfg.log_dir.join("package-selfcheck-last.json")
        );
    }

    #[test]
    fn invalid_json_is_surfaced_as_config_error() {
        let res = LauncherConfig::from_json("{ not json", Path::new("C:/c"));
        assert!(matches!(res, Err(LaunchError::ConfigInvalid(_))));
    }

    #[test]
    fn poll_health_returns_ok_when_probe_reports_ready() {
        let calls = std::cell::Cell::new(0u32);
        let mut probe = || {
            let n = calls.get();
            calls.set(n + 1);
            if n + 1 >= 2 {
                ProbeOutcome::Ready
            } else {
                ProbeOutcome::NotReady
            }
        };
        let clock = std::cell::Cell::new(Duration::ZERO);
        let now = || clock.get();
        let sleep = |d: Duration| clock.set(clock.get() + d);

        let res = poll_health(
            "http://x/health",
            Duration::from_secs(10),
            Duration::from_millis(100),
            &mut probe,
            now,
            sleep,
        );
        assert!(res.is_ok());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn poll_health_times_out_within_budget() {
        let mut probe = || ProbeOutcome::NotReady;
        let clock = std::cell::Cell::new(Duration::ZERO);
        let now = || clock.get();
        let sleep = |d: Duration| clock.set(clock.get() + d);

        let res = poll_health(
            "http://x/health",
            Duration::from_millis(500),
            Duration::from_millis(100),
            &mut probe,
            now,
            sleep,
        );
        assert!(matches!(res, Err(LaunchError::HealthTimeout { .. })));
    }

    #[test]
    fn poll_health_surfaces_transient_probes_until_timeout() {
        let mut probe = || ProbeOutcome::Transient;
        let clock = std::cell::Cell::new(Duration::ZERO);
        let res = poll_health(
            "http://x/health",
            Duration::from_millis(50),
            Duration::from_millis(10),
            &mut probe,
            || clock.get(),
            |d: Duration| clock.set(clock.get() + d),
        );
        assert!(matches!(res, Err(LaunchError::HealthTimeout { .. })));
    }

    #[test]
    fn tauri_args_forward_web_console_pid_first() {
        let extra = vec!["--ui=foo".to_string(), "bar".to_string()];
        let args = build_tauri_args(12345, &extra);
        assert_eq!(
            args,
            vec![
                "--web-console-pid=12345".to_string(),
                "--ui=foo".to_string(),
                "bar".to_string()
            ]
        );
    }

    #[test]
    fn tauri_args_with_no_extra_args() {
        let args = build_tauri_args(1, &[]);
        assert_eq!(args, vec!["--web-console-pid=1".to_string()]);
    }

    #[test]
    fn explicit_console_arg_is_forwarded_without_being_injected_by_default() {
        let args = build_tauri_args(7, &["--show-console".to_string()]);
        assert_eq!(
            args,
            vec![
                "--web-console-pid=7".to_string(),
                "--show-console".to_string()
            ]
        );
    }

    #[test]
    fn healthy_live_web_console_can_be_reused_but_unknown_or_unhealthy_listener_cannot() {
        let owner = ListenerOwner {
            pid: 8188,
            alive: true,
            process_name: Some("COOLZHU-WEB-CONSOLE.EXE".into()),
        };
        assert!(is_live_web_console_instance(true, Some(&owner)));
        assert!(!is_live_web_console_instance(false, Some(&owner)));

        let unknown = ListenerOwner {
            pid: 8189,
            alive: true,
            process_name: Some("other-service.exe".into()),
        };
        assert!(!is_live_web_console_instance(true, Some(&unknown)));
    }

    #[test]
    fn web_console_environment_marks_desktop_shell_as_launcher_managed() {
        assert_eq!(
            web_console_environment(),
            [("COOLZHU_DESKTOP_SHELL_MANAGED", "1")]
        );
    }

    #[test]
    fn web_console_runtime_environment_stays_under_runtime_dir() {
        let runtime_dir = PathBuf::from(r"C:\Users\me\AppData\Local\CoolzhuAgent");
        let vars = web_console_runtime_environment(&runtime_dir)
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(
            vars.get(&OsString::from(WEB_CONSOLE_RUNTIME_ENV)),
            Some(&runtime_dir.as_os_str().to_os_string())
        );
        assert_eq!(
            vars.get(&OsString::from(WEB_CONSOLE_SESSION_DB_ENV)),
            Some(
                &runtime_dir
                    .join(".coolzhu")
                    .join("web-sessions.sqlite3")
                    .into_os_string()
            )
        );
        assert_eq!(
            vars.get(&OsString::from(WEB_CONSOLE_SESSION_STORE_ENV)),
            Some(
                &runtime_dir
                    .join(".coolzhu")
                    .join("web-sessions.json")
                    .into_os_string()
            )
        );
        assert_eq!(
            vars.get(&OsString::from(WEB_CONSOLE_ATTACHMENT_STORE_ENV)),
            Some(
                &runtime_dir
                    .join(".coolzhu")
                    .join("attachments")
                    .into_os_string()
            )
        );
    }

    #[test]
    fn writes_selfcheck_file_with_payload() {
        let tmp = temp_dir_unique("selfcheck");
        let log_dir = tmp.join("logs");
        let selfcheck = log_dir.join("package-selfcheck-last.json");
        let payload = SelfcheckPayload {
            ok: true,
            web_console_pid: Some(42),
            tauri_pid: Some(7),
            health_url: "http://127.0.0.1:1/health".into(),
            error: None,
            timestamp_ms: 1234,
        };
        write_selfcheck(&selfcheck, &log_dir, &payload).unwrap();

        assert!(selfcheck.is_file());
        let text = fs::read_to_string(&selfcheck).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["web_console_pid"], 42);
        assert_eq!(v["tauri_pid"], 7);
        assert_eq!(v["health_url"], "http://127.0.0.1:1/health");
        assert_eq!(v["error"], Value::Null);
        assert_eq!(v["timestamp_ms"], 1234);
    }

    #[test]
    fn writes_failed_selfcheck_payload() {
        let tmp = temp_dir_unique("selfcheck-fail");
        let log_dir = tmp.join("logs");
        let selfcheck = log_dir.join("package-selfcheck-last.json");
        let payload = SelfcheckPayload {
            ok: false,
            web_console_pid: Some(42),
            tauri_pid: None,
            health_url: "http://127.0.0.1:1/health".into(),
            error: Some("boom".into()),
            timestamp_ms: 9,
        };
        write_selfcheck(&selfcheck, &log_dir, &payload).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&selfcheck).unwrap()).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["tauri_pid"], Value::Null);
        assert_eq!(v["error"], "boom");
    }

    struct FakeSpawner {
        web_pid: u32,
        tauri_pid: u32,
        web_calls: u32,
        tauri_calls: u32,
        last_tauri_args: Vec<String>,
        last_web_runtime_dir: Option<PathBuf>,
        last_tauri_runtime_dir: Option<PathBuf>,
        last_tauri_log: Option<PathBuf>,
        terminated_pids: Vec<u32>,
        fail_tauri_spawn: bool,
    }

    impl FakeSpawner {
        fn new(web_pid: u32, tauri_pid: u32) -> Self {
            Self {
                web_pid,
                tauri_pid,
                web_calls: 0,
                tauri_calls: 0,
                last_tauri_args: Vec::new(),
                last_web_runtime_dir: None,
                last_tauri_runtime_dir: None,
                last_tauri_log: None,
                terminated_pids: Vec::new(),
                fail_tauri_spawn: false,
            }
        }
    }

    impl LaunchSpawner for FakeSpawner {
        fn spawn_web_console(
            &mut self,
            _spec: &ExecutableSpec,
            runtime_dir: &Path,
            _log_file: &Path,
        ) -> Result<u32, LaunchError> {
            self.web_calls += 1;
            self.last_web_runtime_dir = Some(runtime_dir.to_path_buf());
            Ok(self.web_pid)
        }
        fn spawn_tauri(
            &mut self,
            _spec: &ExecutableSpec,
            args: &[String],
            runtime_dir: &Path,
            log_file: &Path,
        ) -> Result<u32, LaunchError> {
            self.tauri_calls += 1;
            self.last_tauri_args = args.to_vec();
            self.last_tauri_runtime_dir = Some(runtime_dir.to_path_buf());
            self.last_tauri_log = Some(log_file.to_path_buf());
            if self.fail_tauri_spawn {
                return Err(LaunchError::TauriSpawn(io::Error::other(
                    "injected tauri spawn failure",
                )));
            }
            Ok(self.tauri_pid)
        }

        fn terminate_process(&mut self, pid: u32) -> io::Result<()> {
            self.terminated_pids.push(pid);
            Ok(())
        }
    }

    fn touch_executable(label: &str) -> PathBuf {
        let tmp = temp_dir_unique(label);
        fs::create_dir_all(&tmp).unwrap();
        let exe = tmp.join("stub.exe");
        fs::write(&exe, b"").unwrap();
        exe
    }

    fn config_with_paths(web: PathBuf, tauri: PathBuf, log_dir: PathBuf) -> LauncherConfig {
        let selfcheck_file = log_dir.join("package-selfcheck-last.json");
        let runtime_dir = log_dir
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| log_dir.clone());
        LauncherConfig {
            web_console: ExecutableSpec {
                path: web,
                args: vec![],
            },
            tauri: ExecutableSpec {
                path: tauri,
                args: vec!["--ui=foo".into()],
            },
            health_url: "http://127.0.0.1:1/health".into(),
            health_timeout: Duration::from_millis(100),
            health_poll_interval: Duration::from_millis(10),
            runtime_dir,
            log_dir,
            selfcheck_file,
        }
    }

    #[test]
    fn launch_surfaces_missing_web_console_executable() {
        let tauri = touch_executable("t-exists");
        let log_dir = temp_dir_unique("log-missing-web");
        let cfg = config_with_paths(
            temp_dir_unique("no-web").join("missing.exe"),
            tauri,
            log_dir,
        );
        let mut spawner = FakeSpawner::new(11, 22);
        let mut probe = || ProbeOutcome::Ready;
        let res = launch(&cfg, &mut spawner, &mut probe, || Duration::ZERO, |_| {}, 0);
        assert!(matches!(
            res,
            Err(LaunchError::ExecutableMissing {
                role: "web_console",
                ..
            })
        ));
        assert_eq!(spawner.web_calls, 0);
        assert_eq!(spawner.tauri_calls, 0);
    }

    #[test]
    fn launch_surfaces_missing_tauri_executable() {
        let web = touch_executable("w-exists");
        let log_dir = temp_dir_unique("log-missing-tauri");
        let cfg = config_with_paths(
            web,
            temp_dir_unique("no-tauri").join("missing.exe"),
            log_dir,
        );
        let mut spawner = FakeSpawner::new(11, 22);
        let mut probe = || ProbeOutcome::Ready;
        let res = launch(&cfg, &mut spawner, &mut probe, || Duration::ZERO, |_| {}, 0);
        assert!(matches!(
            res,
            Err(LaunchError::ExecutableMissing { role: "tauri", .. })
        ));
    }

    #[test]
    fn launch_happy_path_spawns_both_and_writes_ok_selfcheck() {
        let web = touch_executable("web-ok");
        let tauri = touch_executable("tauri-ok");
        let log_dir = temp_dir_unique("log-ok");
        let cfg = config_with_paths(web, tauri, log_dir.clone());
        let mut spawner = FakeSpawner::new(111, 222);

        let probes = std::cell::Cell::new(0u32);
        let mut probe = || {
            let n = probes.get();
            probes.set(n + 1);
            if n + 1 >= 2 {
                ProbeOutcome::Ready
            } else {
                ProbeOutcome::Transient
            }
        };
        let clock = std::cell::Cell::new(Duration::ZERO);
        let res = launch(
            &cfg,
            &mut spawner,
            &mut probe,
            || clock.get(),
            |d: Duration| clock.set(clock.get() + d),
            999,
        );

        let outcome = res.expect("launch should succeed");
        assert_eq!(outcome.web_console_pid, 111);
        assert_eq!(outcome.tauri_pid, 222);
        assert_eq!(spawner.web_calls, 1);
        assert_eq!(spawner.tauri_calls, 1);
        assert_eq!(
            spawner.last_web_runtime_dir.as_deref(),
            Some(cfg.runtime_dir.as_path())
        );
        assert_eq!(
            spawner.last_tauri_runtime_dir.as_deref(),
            Some(cfg.runtime_dir.as_path())
        );
        assert!(spawner.terminated_pids.is_empty());
        assert_eq!(
            spawner.last_tauri_args,
            vec!["--web-console-pid=111".to_string(), "--ui=foo".to_string()]
        );
        assert_eq!(
            spawner.last_tauri_log,
            Some(cfg.log_dir.join("tauri.stdout.log"))
        );

        let v: Value =
            serde_json::from_str(&fs::read_to_string(&cfg.selfcheck_file).unwrap()).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["web_console_pid"], 111);
        assert_eq!(v["tauri_pid"], 222);
        assert_eq!(v["timestamp_ms"], 999);
    }

    #[test]
    fn launch_writes_failed_selfcheck_on_health_timeout() {
        let web = touch_executable("web-timeout");
        let tauri = touch_executable("tauri-timeout");
        let log_dir = temp_dir_unique("log-timeout");
        let cfg = config_with_paths(web, tauri, log_dir.clone());
        let mut spawner = FakeSpawner::new(555, 666);

        let clock = std::cell::Cell::new(Duration::ZERO);
        let mut probe = || ProbeOutcome::NotReady;
        let res = launch(
            &cfg,
            &mut spawner,
            &mut probe,
            || clock.get(),
            |d: Duration| clock.set(clock.get() + d),
            7,
        );

        assert!(matches!(res, Err(LaunchError::HealthTimeout { .. })));
        assert_eq!(spawner.web_calls, 1);
        assert_eq!(spawner.tauri_calls, 0);
        assert_eq!(spawner.terminated_pids, vec![555]);

        let v: Value =
            serde_json::from_str(&fs::read_to_string(&cfg.selfcheck_file).unwrap()).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["web_console_pid"], 555);
        assert_eq!(v["tauri_pid"], Value::Null);
        assert!(v["error"].as_str().unwrap().contains("timed out"));
        assert_eq!(v["timestamp_ms"], 7);
    }

    #[test]
    fn launch_reclaims_web_console_when_tauri_spawn_fails() {
        let web = touch_executable("web-tauri-fail");
        let tauri = touch_executable("tauri-spawn-fail");
        let log_dir = temp_dir_unique("log-tauri-spawn-fail");
        let cfg = config_with_paths(web, tauri, log_dir);
        let mut spawner = FakeSpawner::new(701, 702);
        spawner.fail_tauri_spawn = true;
        let mut probe = || ProbeOutcome::Ready;

        let result = launch(
            &cfg,
            &mut spawner,
            &mut probe,
            || Duration::ZERO,
            |_| {},
            12,
        );

        assert!(matches!(result, Err(LaunchError::TauriSpawn(_))));
        assert_eq!(spawner.web_calls, 1);
        assert_eq!(spawner.tauri_calls, 1);
        assert_eq!(spawner.terminated_pids, vec![701]);
    }

    #[test]
    fn launch_reclaims_both_processes_when_success_selfcheck_cannot_persist() {
        let web = touch_executable("web-selfcheck-fail");
        let tauri = touch_executable("tauri-selfcheck-fail");
        let log_dir = temp_dir_unique("log-selfcheck-fail");
        let mut cfg = config_with_paths(web, tauri, log_dir.clone());
        cfg.selfcheck_file = log_dir;
        let mut spawner = FakeSpawner::new(801, 802);
        let mut probe = || ProbeOutcome::Ready;

        let result = launch(
            &cfg,
            &mut spawner,
            &mut probe,
            || Duration::ZERO,
            |_| {},
            13,
        );

        assert!(matches!(result, Err(LaunchError::Persistence(_))));
        assert_eq!(spawner.terminated_pids, vec![802, 801]);
    }

    #[test]
    fn parse_http_url_extracts_host_port_path() {
        let (h, p, path) = parse_http_url("http://127.0.0.1:7860/api/diagnostics/health").unwrap();
        assert_eq!(h, "127.0.0.1");
        assert_eq!(p, 7860);
        assert_eq!(path, "/api/diagnostics/health");
    }

    #[test]
    fn parse_http_url_defaults_port_and_path() {
        let (h, p, path) = parse_http_url("http://localhost").unwrap();
        assert_eq!(h, "localhost");
        assert_eq!(p, 80);
        assert_eq!(path, "/");
    }

    #[test]
    fn parse_http_url_rejects_non_http() {
        assert!(parse_http_url("https://x").is_none());
        assert!(parse_http_url("not a url").is_none());
    }

    #[test]
    fn stale_listener_owner_requests_orphan_cleanup() {
        let owner = ListenerOwner {
            pid: 17300,
            alive: false,
            process_name: None,
        };

        assert_eq!(
            classify_listener_recovery(false, Some(&owner)),
            ListenerRecoveryAction::CleanupOrphan { parent_pid: 17300 }
        );
    }

    #[test]
    fn live_listener_owner_is_never_killed_as_a_ghost() {
        let owner = ListenerOwner {
            pid: 8188,
            alive: true,
            process_name: Some("coolzhu-web-console.exe".into()),
        };

        assert_eq!(
            classify_listener_recovery(false, Some(&owner)),
            ListenerRecoveryAction::Block {
                pid: 8188,
                process_name: Some("coolzhu-web-console.exe".into()),
            }
        );
    }
}
