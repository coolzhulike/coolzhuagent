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

pub mod launch_paths;
pub mod service_identity;

pub use launch_paths::{
    ActionOutcome, CandidateSource, ConfigSnapshot, DataPathBinding, ExplicitSelection,
    LauncherUserSelection, LauncherUserState, LaunchPathDecision, LaunchPathInputs,
    ObservedBackground, ProcessLogPath, ResolutionAction, ResolutionNote, ResolutionSource,
    ResolvedLaunchPaths, WorkspaceAccess, WorkspaceAccessProbe, WorkspaceCandidate,
    WorkspaceDataEvidence,
};
pub use service_identity::{ReuseDecision, ReuseVerification, ServiceIdentity};

/// 工作区内的业务数据目录名（与 web-console 的 `DATA_DIR_NAME` 同名同义）。
pub(crate) const DATA_DIR_NAME: &str = ".coolzhu";
/// 工作区配置文件（与 web-console 的 `CONFIG_FILE_NAME` 同名同义）。
pub(crate) const CONFIG_FILE_NAME: &str = "coolzhu.toml";

pub const WEB_CONSOLE_MANAGED_ENV: &str = "COOLZHU_DESKTOP_SHELL_MANAGED";
pub const WEB_CONSOLE_RUNTIME_ENV: &str = "COOLZHU_RUNTIME_DIR";
pub const WEB_CONSOLE_SESSION_DB_ENV: &str = "COOLZHU_WEB_SESSION_DB";
pub const WEB_CONSOLE_SESSION_STORE_ENV: &str = "COOLZHU_WEB_SESSION_STORE";
pub const WEB_CONSOLE_ATTACHMENT_STORE_ENV: &str = "COOLZHU_WEB_ATTACHMENT_STORE";
/// **输入安全存储根**（第八轮 §2 批准新增）。
///
/// 生产路径**必须注入**；子进程缺失该变量时按 `root_not_injected` **直接 fail closed**，
/// **不得**回退到 `%USERPROFILE%\.coolzhu` 之类的自造路径——否则测试环境会污染生产路径。
pub const INPUT_SAFETY_STATE_ROOT_ENV: &str = "COOLZHU_INPUT_SAFETY_STATE_ROOT";

/// 本次启动与所有子进程共享的启动标识（"同一份已解析结果"）。
pub const LAUNCH_ID_ENV: &str = "COOLZHU_LAUNCH_ID";
/// 用户级启动选择根（子进程只读使用，不得据此重算业务路径）。
pub const USER_STATE_ROOT_ENV: &str = "COOLZHU_USER_STATE_ROOT";
/// 本次工作区解析来源（自检/诊断可见）。
pub const RESOLUTION_SOURCE_ENV: &str = "COOLZHU_LAUNCH_RESOLUTION_SOURCE";

pub fn web_console_environment() -> [(&'static str, &'static str); 1] {
    [(WEB_CONSOLE_MANAGED_ENV, "1")]
}

/// 子进程共享的启动环境。
///
/// **只固定工作区根**（`COOLZHU_RUNTIME_DIR`）：业务数据根（会话库/附件/兼容 JSON）一律由
/// 工作区配置解析，launcher 不再注入 `COOLZHU_WEB_SESSION_DB` 之类的覆盖——否则会静默忽略
/// `paths.data_dir` 覆盖，而 UI/诊断仍显示"已按配置打开"（P08）。
///
/// `COOLZHU_WEB_SESSION_DB` / `_STORE` / `_ATTACHMENT_STORE` 若从父环境继承进来会被**显式清除**
/// 并记录（`scrubbed_inherited_overrides`），不静默忽略。
pub fn launch_environment(resolved: &ResolvedLaunchPaths) -> Vec<(OsString, OsString)> {
    vec![
        (
            OsString::from(WEB_CONSOLE_RUNTIME_ENV),
            resolved.workspace_root.as_os_str().to_os_string(),
        ),
        (
            OsString::from(LAUNCH_ID_ENV),
            OsString::from(resolved.launch_id.as_str()),
        ),
        (
            OsString::from(USER_STATE_ROOT_ENV),
            resolved.user_state_root.as_os_str().to_os_string(),
        ),
        (
            OsString::from(RESOLUTION_SOURCE_ENV),
            OsString::from(resolved.resolution_source.as_str()),
        ),
        // 输入安全存储根（用户级、跨工作区）：取自 ResolvedLaunchPaths 的既有契约字段，
        // **不**在此另行推导目录名。web-console 侧缺失即 fail-closed（第八轮 §2）。
        (
            OsString::from(INPUT_SAFETY_STATE_ROOT_ENV),
            resolved.input_safety_state_root.as_os_str().to_os_string(),
        ),
    ]
}

/// 必须从子进程环境里清除的、会越过工作区配置的继承覆盖。
pub fn overridden_data_path_envs() -> [&'static str; 3] {
    [
        WEB_CONSOLE_SESSION_DB_ENV,
        WEB_CONSOLE_SESSION_STORE_ENV,
        WEB_CONSOLE_ATTACHMENT_STORE_ENV,
    ]
}

/// 检查父环境里是否残留会越过工作区配置的覆盖（启动时记录，不静默忽略）。
pub fn inherited_data_path_overrides<F>(mut env_lookup: F) -> Vec<String>
where
    F: FnMut(&str) -> Option<OsString>,
{
    overridden_data_path_envs()
        .into_iter()
        .filter(|name| env_lookup(name).is_some_and(|value| !value.is_empty()))
        .map(str::to_string)
        .collect()
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
    /// 已声明/已保存/候选的工作区不可用：**明确阻断，不静默回退到其它目录**。
    WorkspaceUnavailable {
        role: String,
        path: PathBuf,
        source: String,
        detail: String,
        remedy: String,
    },
    /// 需要用户明确一次工作区选择（恢复入口）。
    WorkspaceSelectionRequired {
        reason: String,
        candidates: Vec<WorkspaceCandidate>,
        remedy: String,
    },
    /// 多个可信候选都有数据：**禁止自动选择/合并/删除**。
    WorkspaceSelectionAmbiguous {
        candidates: Vec<WorkspaceCandidate>,
        remedy: String,
    },
    /// 用户级选择文件不可读/损坏/schema 不受支持：不静默忽略。
    SelectionStore {
        path: PathBuf,
        detail: String,
        remedy: String,
    },
    /// 选择更新冲突（revision 不匹配）：**保持旧选择**，不在内存里先宣布成功。
    SelectionConflict {
        path: PathBuf,
        expected_revision: u64,
        actual_revision: u64,
    },
    /// 另一个启动器正在改选择（锁超时）：报冲突，不覆盖别人的写。
    SelectionLocked {
        path: PathBuf,
        lock: PathBuf,
        waited_ms: u64,
    },
    /// 复用既有后台服务时身份/绑定不匹配：**报告冲突、不静默连接、不按端口杀进程**。
    ServiceConflict {
        port: u16,
        reason: String,
        details: Vec<String>,
    },
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
            Self::WorkspaceUnavailable {
                role,
                path,
                source,
                detail,
                remedy,
            } => write!(
                f,
                "工作区不可用（{role}）：{} [来源 {source}]：{detail}。已阻断启动，不会创建替代工作区。\n修正入口：{remedy}",
                path.display()
            ),
            Self::WorkspaceSelectionRequired {
                reason,
                candidates,
                remedy,
            } => write!(
                f,
                "需要一次明确的工作区选择：{reason}\n候选：\n{}\n修正入口：{remedy}",
                launch_paths::render_candidates(candidates)
            ),
            Self::WorkspaceSelectionAmbiguous {
                candidates,
                remedy,
            } => write!(
                f,
                "发现多个含数据的工作区候选，拒绝自动选择（不按时间/容量/数量挑选，不合并，不删除）：\n{}\n修正入口：{remedy}",
                launch_paths::render_candidates(candidates)
            ),
            Self::SelectionStore {
                path,
                detail,
                remedy,
            } => write!(
                f,
                "用户级启动选择不可用：{}：{detail}\n修正入口：{remedy}",
                path.display()
            ),
            Self::SelectionConflict {
                path,
                expected_revision,
                actual_revision,
            } => write!(
                f,
                "用户级启动选择更新冲突：{}（期望 revision={expected_revision}，实际 revision={actual_revision}）。已保持旧选择，未写入任何变更",
                path.display()
            ),
            Self::SelectionLocked {
                path,
                lock,
                waited_ms,
            } => write!(
                f,
                "用户级启动选择被另一个启动器占用（等待 {waited_ms}ms 超时）：{}（锁 {}）。已保持旧选择，未覆盖对方的写",
                path.display(),
                lock.display()
            ),
            Self::ServiceConflict {
                port,
                reason,
                details,
            } => write!(
                f,
                "端口 {port} 上的服务不可复用：{reason}\n{}（不会静默连接，也不会按端口杀进程）",
                if details.is_empty() {
                    String::new()
                } else {
                    format!("依据：{}", details.join("；"))
                }
            ),
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
///
/// # `runtime_dir` 的现行含义（本轮不改名）
///
/// - `config_schema_version >= 2`：`runtime_dir` = **工作区默认建议值**，
///   只用于"尚未建立有效选择"的首次初始化。**缺失/空值/未解析变量/非法路径 = 配置错误**，
///   **不会**自动从 `log_dir` 推导。
/// - `config_schema_version == 1`（或根本没有该键的旧包配置）：保留可识别的兼容解析；
///   缺键时从 `log_dir` 推导的结果只作为**迁移信息/候选**（`legacy_log_dir_derived_workspace`），
///   不再是新版本常规的隐式数据源。
///
/// **不得**通过删除 `runtime_dir` 来"统一目录"：删除只改变选源，不会迁移已有数据。
#[derive(Debug, Clone)]
pub struct LauncherConfig {
    pub web_console: ExecutableSpec,
    pub tauri: ExecutableSpec,
    pub health_url: String,
    pub health_timeout: Duration,
    pub health_poll_interval: Duration,
    /// 随包配置 schema 版本（缺键 = 旧版配置，按 v1 兼容处理）。
    pub config_schema_version: u64,
    /// 随包配置声明的默认工作区（仅首次初始化用；v1 存在该键时也在此）。
    pub packaged_default_workspace: Option<PathBuf>,
    /// v1 兼容：旧配置缺 `runtime_dir` 时从 `log_dir` 推导的结果（**仅迁移候选**）。
    pub legacy_log_dir_derived_workspace: Option<PathBuf>,
    /// v1 兼容：`runtime_dir` 键存在但取值未通过校验时的原始文本（诊断用）。
    pub declared_runtime_dir_text: Option<String>,
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
        let config_schema_version = value
            .get("launcher_config_version")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        if config_schema_version > launch_paths::LAUNCHER_CONFIG_SCHEMA_VERSION {
            return Err(LaunchError::ConfigInvalid(format!(
                "launcher_config_version={config_schema_version} 高于本启动器支持的 {}：请使用匹配版本的启动器",
                launch_paths::LAUNCHER_CONFIG_SCHEMA_VERSION
            )));
        }

        // 缺键语义按配置版本分流（**不混用**）：
        // - v2：`runtime_dir` 必须明确；缺失/空值/未解析变量/非法路径 = 配置错误，不推导。
        // - v1：保留可识别的兼容解析；缺键才推导，且推导结果只作迁移候选。
        let (packaged_default_workspace, declared_runtime_dir_text, legacy_log_dir_derived_workspace) =
            if config_schema_version >= launch_paths::LAUNCHER_CONFIG_SCHEMA_VERSION {
                let raw = value
                    .get(launch_paths::RUNTIME_DIR_KEY)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|raw| !raw.is_empty())
                    .ok_or_else(|| {
                        LaunchError::ConfigInvalid(format!(
                            "新版打包配置（launcher_config_version={config_schema_version}）缺少 {} 或取值为空：这是配置错误，不会从 log_dir 推导工作区",
                            launch_paths::RUNTIME_DIR_KEY
                        ))
                    })?;
                let resolved = resolve_config_path_text(raw, config_dir, &mut env_lookup)?;
                (Some(resolved), Some(raw.to_string()), None)
            } else {
                match value.get(launch_paths::RUNTIME_DIR_KEY).and_then(Value::as_str) {
                    Some(raw) if !raw.trim().is_empty() => {
                        let resolved = resolve_config_path_text(raw, config_dir, &mut env_lookup)?;
                        (Some(resolved), Some(raw.trim().to_string()), None)
                    }
                    _ => (
                        None,
                        None,
                        // ⑤ 兼容支路：只识别旧行为与迁移候选，不再作为常规数据源。
                        Some(default_runtime_dir_from_log_dir(&log_dir)),
                    ),
                }
            };
        let selfcheck_file = log_dir.join("package-selfcheck-last.json");

        Ok(Self {
            web_console,
            tauri,
            health_url,
            health_timeout: Duration::from_secs(health_timeout_secs),
            health_poll_interval: Duration::from_millis(health_poll_interval_ms),
            config_schema_version,
            packaged_default_workspace,
            legacy_log_dir_derived_workspace,
            declared_runtime_dir_text,
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

/// ⑤ 兼容支路：**仅服务旧版（`launcher_config_version < 2`）且 `runtime_dir` 键缺失的配置**。
///
/// 从 `log_dir` 反推工作区（`%LOCALAPPDATA%\CoolzhuAgent\logs\package-launcher` ⇒ `%LOCALAPPDATA%\CoolzhuAgent`）。
///
/// 边界（必须保持）：
/// - **只在**旧配置缺 `runtime_dir` 时被调用；新版本（v2）缺键是**配置错误**，不走这里。
/// - 结果只作为**迁移候选/迁移信息**输出（见 `LaunchPathInputs::legacy_log_dir_derived_workspace`），
///   **不再作为新版本常规的隐式数据源切换**，也**不因为修改 `log_dir` 就改变已建立的工作区**。
/// - "删掉 `runtime_dir` 键就会回到 LocalAppData"**不是正式修复建议**：删除只改变选源，不迁移已有数据。
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

pub(crate) fn expand_os_env_placeholders(
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
///
/// 自检**同时记录**"请求的路径"与"后台实际使用的路径"（`resolved`）——只打印配置值不足以证明生效。
#[derive(Debug, Clone)]
pub struct SelfcheckPayload {
    pub ok: bool,
    pub web_console_pid: Option<u32>,
    pub tauri_pid: Option<u32>,
    pub health_url: String,
    pub error: Option<String>,
    pub timestamp_ms: u64,
    /// 本次已解析结果（含 requested / observed 两段路径）。
    pub resolved: Option<ResolvedLaunchPaths>,
    /// 复用判定证据（若发生）。
    pub reuse: Option<ReuseVerification>,
    /// 本次解析/落库过程的诊断（迁移信息、被忽略的候选、动作结果）。
    pub notes: Vec<String>,
}

impl SelfcheckPayload {
    /// 只带基本字段的自检（兼容既有调用点）。
    #[must_use]
    pub fn basic(
        ok: bool,
        web_console_pid: Option<u32>,
        tauri_pid: Option<u32>,
        health_url: String,
        error: Option<String>,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            ok,
            web_console_pid,
            tauri_pid,
            health_url,
            error,
            timestamp_ms,
            resolved: None,
            reuse: None,
            notes: Vec::new(),
        }
    }
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
        "resolved_launch_paths": payload.resolved.as_ref().map(ResolvedLaunchPaths::to_json),
        "reuse_verification": payload.reuse.as_ref().map(ReuseVerification::to_json),
        "resolution_notes": payload.notes,
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
///
/// 子进程使用**同一份** [`ResolvedLaunchPaths`] 快照（不能各自根据 cwd/日志目录重新猜）。
pub trait LaunchSpawner {
    /// Spawn the Web Console hidden with stdout/stderr redirected to `log_file`.
    /// Returns the spawned child PID.
    fn spawn_web_console(
        &mut self,
        spec: &ExecutableSpec,
        resolved: &ResolvedLaunchPaths,
        log_file: &Path,
    ) -> Result<u32, LaunchError>;
    /// Spawn the Tauri shell with the given (already pid-injected) args.
    /// Returns the spawned child PID.
    fn spawn_tauri(
        &mut self,
        spec: &ExecutableSpec,
        args: &[String],
        resolved: &ResolvedLaunchPaths,
        log_file: &Path,
    ) -> Result<u32, LaunchError>;
    /// 回收本次 launcher 刚拉起的进程；失败路径必须调用，避免隐藏进程占用端口。
    fn terminate_process(&mut self, pid: u32) -> io::Result<()>;
}

/// Result of a successful bring-up.
#[derive(Debug, Clone)]
pub struct LaunchOutcome {
    pub web_console_pid: u32,
    pub tauri_pid: u32,
    /// 启动完成后的最终快照（含**后台实际使用**的路径回读结果）。
    pub resolved: ResolvedLaunchPaths,
}

/// Run the full bring-up sequence using injected collaborators.
///
/// Verifies both executables exist, spawns the Web Console (hidden, stdio
/// redirected), polls health within the configured budget, spawns the Tauri
/// shell with `--web-console-pid=<pid>`, and persists a self-check record. On
/// any failure a failed self-check is still written when possible and the error
/// is surfaced.
///
/// `resolved` 是**本次启动唯一的路径权威**（解析只做一次，子进程共享）；
/// `observe` 在健康就绪后回读"后台实际使用的路径"，写进自检的 observed 段。
#[allow(clippy::too_many_arguments)]
pub fn launch<L, N, S>(
    config: &LauncherConfig,
    resolved: &ResolvedLaunchPaths,
    spawner: &mut L,
    probe: &mut dyn FnMut() -> ProbeOutcome,
    now_fn: N,
    sleep_fn: S,
    timestamp_ms: u64,
    observe: &mut dyn FnMut() -> Option<ObservedBackground>,
    notes: &[String],
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

    // 工作区目录在解析阶段已被证明可用（或已显式初始化）；这里只补日志目录。
    fs::create_dir_all(&config.log_dir).map_err(LaunchError::Persistence)?;
    let log_file = config.log_dir.join("web-console.stdout.log");

    let web_pid = spawner.spawn_web_console(&config.web_console, resolved, &log_file)?;

    let failed_payload = |error_text: String,
                              web_console_pid: Option<u32>,
                              tauri_pid: Option<u32>| SelfcheckPayload {
        ok: false,
        web_console_pid,
        tauri_pid,
        health_url: config.health_url.clone(),
        error: Some(error_text),
        timestamp_ms,
        resolved: None,
        reuse: None,
        notes: notes.to_vec(),
    };

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
        let payload = failed_payload(error_text, Some(web_pid), None);
        let _ = write_selfcheck(&config.selfcheck_file, &config.log_dir, &payload);
        return Err(e);
    }

    // 健康就绪：回读后台实际使用的路径（只打印配置值不足以证明生效）。
    let observed = observe();
    let resolved = match &observed {
        Some(observed) => resolved.with_observations(observed),
        None => resolved.clone(),
    };
    eprintln!("package-launcher: 解析结果快照");
    for line in resolved.display_lines() {
        eprintln!("  {line}");
    }

    let tauri_args = build_tauri_args(web_pid, &config.tauri.args);
    let tauri_log_file = config.log_dir.join("tauri.stdout.log");
    let tauri_pid = match spawner.spawn_tauri(&config.tauri, &tauri_args, &resolved, &tauri_log_file) {
        Ok(pid) => pid,
        Err(e) => {
            let cleanup_error = spawner.terminate_process(web_pid).err();
            let error_text = match cleanup_error {
                Some(cleanup) => format!("{e}; web console cleanup failed: {cleanup}"),
                None => e.to_string(),
            };
            let payload = failed_payload(error_text, Some(web_pid), None);
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
        resolved: Some(resolved.clone()),
        reuse: None,
        notes: notes.to_vec(),
    };
    if let Err(error) = write_selfcheck(&config.selfcheck_file, &config.log_dir, &payload) {
        let _ = spawner.terminate_process(tauri_pid);
        let _ = spawner.terminate_process(web_pid);
        return Err(error);
    }

    Ok(LaunchOutcome {
        web_console_pid: web_pid,
        tauri_pid,
        resolved,
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
pub(crate) fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
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
        // v1 配置（无 launcher_config_version）缺 runtime_dir ⇒ 兼容支路：只作迁移候选，不是工作区权威。
        assert!(cfg.packaged_default_workspace.is_none());
        assert_eq!(
            cfg.legacy_log_dir_derived_workspace,
            Some(config_dir.join("../tmp"))
        );
        assert_eq!(cfg.config_schema_version, 1);
        assert_eq!(cfg.log_dir, config_dir.join("../tmp/logs/package-launcher"));
        assert_eq!(
            cfg.selfcheck_file,
            config_dir
                .join("../tmp/logs/package-launcher")
                .join("package-selfcheck-last.json")
        );
    }

    // P07：新版配置缺 `runtime_dir` = 配置错误（不自动从 log_dir 推导），并拒绝未知的高版本 schema。
    #[test]
    fn v2_config_requires_explicit_runtime_dir_and_rejects_newer_schema() {
        let config_dir = Path::new("C:/repo/config");
        let missing_key = r#"{
            "launcher_config_version": 2,
            "web_console": {"executable": "C:/abs/web.exe", "args": []},
            "tauri": {"executable": "C:/abs/tauri.exe", "args": []},
            "health_url": "http://127.0.0.1:1/health",
            "health_timeout_secs": 1,
            "health_poll_interval_ms": 1,
            "log_dir": "C:/abs/logs/package-launcher"
        }"#;
        let error = LauncherConfig::from_json(missing_key, config_dir).unwrap_err();
        let text = error.to_string();
        assert!(text.contains("runtime_dir"), "{text}");
        assert!(text.contains("不会从 log_dir 推导"), "{text}");

        let empty_value = missing_key.replace(
            "\"log_dir\"",
            "\"runtime_dir\": \"   \",\n            \"log_dir\"",
        );
        assert!(LauncherConfig::from_json(&empty_value, config_dir).is_err());

        let unresolved = missing_key.replace(
            "\"log_dir\"",
            "\"runtime_dir\": \"%NOPE%\\\\ws\",\n            \"log_dir\"",
        );
        assert!(LauncherConfig::from_json(&unresolved, config_dir).is_err());

        let newer_schema = missing_key.replace(
            "\"launcher_config_version\": 2",
            "\"launcher_config_version\": 99",
        );
        assert!(LauncherConfig::from_json(&newer_schema, config_dir).is_err());
    }

    // 旧版配置存在 `runtime_dir`：仍作为旧有效选择被识别（升级后继续打开原选择）。
    #[test]
    fn v1_config_with_runtime_dir_keeps_declared_workspace_as_legacy_origin() {
        let config_dir = Path::new("C:/repo/package/config");
        let cfg_text = r#"{
            "web_console": {"executable": "../bin/coolzhu-web-console.exe", "args": []},
            "tauri": {"executable": "../bin/coolzhu-tauri-shell.exe", "args": []},
            "health_url": "http://127.0.0.1:1/health",
            "health_timeout_secs": 1,
            "health_poll_interval_ms": 1,
            "log_dir": "%LOCALAPPDATA%/CoolzhuAgent/logs/package-launcher",
            "runtime_dir": "%USERPROFILE%/coolzhuagent"
        }"#;

        let cfg = LauncherConfig::from_json_with_env(cfg_text, config_dir, |name| match name {
            "LOCALAPPDATA" => Some(std::ffi::OsString::from("C:/Users/me/AppData/Local")),
            "USERPROFILE" => Some(std::ffi::OsString::from("C:/Users/me")),
            _ => None,
        })
        .unwrap();

        assert_eq!(cfg.config_schema_version, 1);
        assert_eq!(
            cfg.log_dir,
            PathBuf::from("C:/Users/me/AppData/Local/CoolzhuAgent/logs/package-launcher")
        );
        assert_eq!(
            cfg.packaged_default_workspace,
            Some(PathBuf::from("C:/Users/me/coolzhuagent"))
        );
        assert_eq!(
            cfg.legacy_log_dir_derived_workspace, None,
            "存在 runtime_dir 时不得再走 log_dir 推导"
        );
        assert_eq!(
            cfg.selfcheck_file,
            cfg.log_dir.join("package-selfcheck-last.json")
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
        assert_eq!(
            cfg.legacy_log_dir_derived_workspace,
            Some(PathBuf::from("C:/abs"))
        );
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
            cfg.legacy_log_dir_derived_workspace,
            Some(PathBuf::from("C:/Users/me/AppData/Local/CoolzhuAgent"))
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
    fn launch_environment_pins_only_the_workspace_and_shares_the_snapshot() {
        let resolved = test_resolved_paths(PathBuf::from(r"C:\Users\me\coolzhuagent"));
        let vars = launch_environment(&resolved)
            .into_iter()
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(
            vars.get(&OsString::from(WEB_CONSOLE_RUNTIME_ENV)),
            Some(
                &PathBuf::from(r"C:\Users\me\coolzhuagent")
                    .into_os_string()
            )
        );
        // 业务数据根由工作区配置决定：launcher 不再注入会越过它的覆盖（P08）。
        for name in [
            WEB_CONSOLE_SESSION_DB_ENV,
            WEB_CONSOLE_SESSION_STORE_ENV,
            WEB_CONSOLE_ATTACHMENT_STORE_ENV,
        ] {
            assert!(
                vars.get(&OsString::from(name)).is_none(),
                "{name} 不得由 launcher 固定"
            );
        }
        assert_eq!(
            vars.get(&OsString::from(LAUNCH_ID_ENV)),
            Some(&OsString::from("launch-test"))
        );
        assert_eq!(
            vars.get(&OsString::from(RESOLUTION_SOURCE_ENV)),
            Some(&OsString::from("explicit_this_launch"))
        );
        assert_eq!(
            vars.get(&OsString::from(USER_STATE_ROOT_ENV)),
            Some(&OsString::from(r"C:\Users\me\AppData\Local\CoolzhuAgent"))
        );
        // 第八轮 §2：输入安全存储根必须注入，且与 resolved 的契约字段逐字一致。
        assert_eq!(
            vars.get(&OsString::from(INPUT_SAFETY_STATE_ROOT_ENV)),
            Some(&resolved.input_safety_state_root.as_os_str().to_os_string()),
            "输入安全存储根必须由 launcher 注入（生产路径不得缺失）"
        );
    }

    #[test]
    fn inherited_data_path_overrides_are_detected_for_scrubbing() {
        let found = inherited_data_path_overrides(|name| {
            (name == WEB_CONSOLE_SESSION_DB_ENV).then(|| OsString::from("D:/elsewhere/db.sqlite3"))
        });
        assert_eq!(found, vec![WEB_CONSOLE_SESSION_DB_ENV.to_string()]);
        assert!(inherited_data_path_overrides(|_| None).is_empty());
        assert_eq!(
            overridden_data_path_envs(),
            [
                WEB_CONSOLE_SESSION_DB_ENV,
                WEB_CONSOLE_SESSION_STORE_ENV,
                WEB_CONSOLE_ATTACHMENT_STORE_ENV
            ]
        );
    }

    fn test_resolved_paths(workspace_root: PathBuf) -> ResolvedLaunchPaths {
        ResolvedLaunchPaths {
            launch_id: "launch-test".into(),
            package_identity: "coolzhu-app-launcher/0.2.0+test".into(),
            config_schema_version: 2,
            selection_revision: 0,
            workspace_id: None,
            workspace_root,
            data_dir_binding: launch_paths::DataPathBinding::WorkspaceConfigAuthority,
            requested_session_db: None,
            observed_session_db: None,
            observed_workspace: None,
            observed_build_version: None,
            user_state_root: PathBuf::from(r"C:\Users\me\AppData\Local\CoolzhuAgent"),
            input_safety_state_root: PathBuf::from(
                r"C:\Users\me\AppData\Local\CoolzhuAgent\input-safety",
            ),
            per_process_log_paths: Vec::new(),
            resolution_source: launch_paths::ResolutionSource::ExplicitThisLaunch,
        }
    }

    #[test]
    fn writes_selfcheck_file_with_payload() {
        let tmp = temp_dir_unique("selfcheck");
        let log_dir = tmp.join("logs");
        let selfcheck = log_dir.join("package-selfcheck-last.json");
        let resolved = test_resolved_paths(PathBuf::from(r"C:\ws"));
        let payload = SelfcheckPayload {
            resolved: Some(resolved),
            notes: vec!["[selection] persisted".into()],
            ..SelfcheckPayload::basic(
                true,
                Some(42),
                Some(7),
                "http://127.0.0.1:1/health".into(),
                None,
                1234,
            )
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
        // 自检必须同时可读"请求的路径"与"后台实际使用的路径"。
        assert_eq!(
            v["resolved_launch_paths"]["requested"]["workspace_root"],
            Value::String(r"C:\ws".to_string())
        );
        assert_eq!(v["resolved_launch_paths"]["observed"]["session_db"], Value::Null);
        assert_eq!(v["resolution_notes"][0], "[selection] persisted");
    }

    #[test]
    fn writes_failed_selfcheck_payload() {
        let tmp = temp_dir_unique("selfcheck-fail");
        let log_dir = tmp.join("logs");
        let selfcheck = log_dir.join("package-selfcheck-last.json");
        let payload = SelfcheckPayload::basic(
            false,
            Some(42),
            None,
            "http://127.0.0.1:1/health".into(),
            Some("boom".into()),
            9,
        );
        write_selfcheck(&selfcheck, &log_dir, &payload).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&selfcheck).unwrap()).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["tauri_pid"], Value::Null);
        assert_eq!(v["error"], "boom");
        assert!(v["resolved_launch_paths"].is_null());
    }

    struct FakeSpawner {
        web_pid: u32,
        tauri_pid: u32,
        web_calls: u32,
        tauri_calls: u32,
        last_tauri_args: Vec<String>,
        last_web_workspace: Option<PathBuf>,
        last_tauri_workspace: Option<PathBuf>,
        last_tauri_log: Option<PathBuf>,
        last_web_launch_id: Option<String>,
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
                last_web_workspace: None,
                last_tauri_workspace: None,
                last_tauri_log: None,
                last_web_launch_id: None,
                terminated_pids: Vec::new(),
                fail_tauri_spawn: false,
            }
        }
    }

    impl LaunchSpawner for FakeSpawner {
        fn spawn_web_console(
            &mut self,
            _spec: &ExecutableSpec,
            resolved: &ResolvedLaunchPaths,
            _log_file: &Path,
        ) -> Result<u32, LaunchError> {
            self.web_calls += 1;
            self.last_web_workspace = Some(resolved.workspace_root.clone());
            self.last_web_launch_id = Some(resolved.launch_id.clone());
            Ok(self.web_pid)
        }
        fn spawn_tauri(
            &mut self,
            _spec: &ExecutableSpec,
            args: &[String],
            resolved: &ResolvedLaunchPaths,
            log_file: &Path,
        ) -> Result<u32, LaunchError> {
            self.tauri_calls += 1;
            self.last_tauri_args = args.to_vec();
            self.last_tauri_workspace = Some(resolved.workspace_root.clone());
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
            config_schema_version: 2,
            packaged_default_workspace: Some(
                log_dir
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| log_dir.clone()),
            ),
            legacy_log_dir_derived_workspace: None,
            declared_runtime_dir_text: Some("%TEST%\\workspace".into()),
            log_dir,
            selfcheck_file,
        }
    }

    /// 在临时目录里造一个"已解析、已存在的工作区"，供 `launch` 测试使用。
    fn workspace_for(label: &str) -> ResolvedLaunchPaths {
        let dir = temp_dir_unique(label);
        fs::create_dir_all(&dir).unwrap();
        test_resolved_paths(dir)
    }

    fn no_observation() -> Option<ObservedBackground> {
        None
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
        let resolved = workspace_for("ws-missing-web");
        let mut spawner = FakeSpawner::new(11, 22);
        let mut probe = || ProbeOutcome::Ready;
        let res = launch(
            &cfg,
            &resolved,
            &mut spawner,
            &mut probe,
            || Duration::ZERO,
            |_| {},
            0,
            &mut no_observation,
            &[],
        );
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
        let resolved = workspace_for("ws-missing-tauri");
        let mut spawner = FakeSpawner::new(11, 22);
        let mut probe = || ProbeOutcome::Ready;
        let res = launch(
            &cfg,
            &resolved,
            &mut spawner,
            &mut probe,
            || Duration::ZERO,
            |_| {},
            0,
            &mut no_observation,
            &[],
        );
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
        let resolved = workspace_for("ws-ok");
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
        let mut observe = || {
            Some(ObservedBackground {
                workspace: Some(PathBuf::from(r"C:\observed\ws")),
                session_db: Some(PathBuf::from(r"C:\observed\ws\agent-data\web-sessions.sqlite3")),
                build_version: Some("abc1234 · 2026-09-25".into()),
                health_status: Some("ok".into()),
                active_sessions: Some(2),
                port: Some(8765),
            })
        };
        let res = launch(
            &cfg,
            &resolved,
            &mut spawner,
            &mut probe,
            || clock.get(),
            |d: Duration| clock.set(clock.get() + d),
            999,
            &mut observe,
            &["[test] note".to_string()],
        );

        let outcome = res.expect("launch should succeed");
        assert_eq!(outcome.web_console_pid, 111);
        assert_eq!(outcome.tauri_pid, 222);
        assert_eq!(spawner.web_calls, 1);
        assert_eq!(spawner.tauri_calls, 1);
        // 子进程共享同一份快照（工作区根与 launch_id 一致）。
        assert_eq!(
            spawner.last_web_workspace.as_deref(),
            Some(resolved.workspace_root.as_path())
        );
        assert_eq!(
            spawner.last_tauri_workspace.as_deref(),
            Some(resolved.workspace_root.as_path())
        );
        assert_eq!(
            spawner.last_web_launch_id.as_deref(),
            Some(resolved.launch_id.as_str())
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
        // 回读结果进入返回值（后台实际使用的路径，而不是配置值）。
        assert_eq!(
            outcome.resolved.observed_workspace.as_deref(),
            Some(Path::new(r"C:\observed\ws"))
        );
        assert!(outcome.resolved.data_dir_override_observed());

        let v: Value =
            serde_json::from_str(&fs::read_to_string(&cfg.selfcheck_file).unwrap()).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["web_console_pid"], 111);
        assert_eq!(v["tauri_pid"], 222);
        assert_eq!(v["timestamp_ms"], 999);
        assert_eq!(
            v["resolved_launch_paths"]["observed"]["build_version"],
            "abc1234 · 2026-09-25"
        );
        assert_eq!(v["resolution_notes"][0], "[test] note");
    }

    #[test]
    fn launch_writes_failed_selfcheck_on_health_timeout() {
        let web = touch_executable("web-timeout");
        let tauri = touch_executable("tauri-timeout");
        let log_dir = temp_dir_unique("log-timeout");
        let cfg = config_with_paths(web, tauri, log_dir.clone());
        let resolved = workspace_for("ws-timeout");
        let mut spawner = FakeSpawner::new(555, 666);

        let clock = std::cell::Cell::new(Duration::ZERO);
        let mut probe = || ProbeOutcome::NotReady;
        let res = launch(
            &cfg,
            &resolved,
            &mut spawner,
            &mut probe,
            || clock.get(),
            |d: Duration| clock.set(clock.get() + d),
            7,
            &mut no_observation,
            &[],
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
        let resolved = workspace_for("ws-tauri-fail");
        let mut spawner = FakeSpawner::new(701, 702);
        spawner.fail_tauri_spawn = true;
        let mut probe = || ProbeOutcome::Ready;

        let result = launch(
            &cfg,
            &resolved,
            &mut spawner,
            &mut probe,
            || Duration::ZERO,
            |_| {},
            12,
            &mut no_observation,
            &[],
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
        let resolved = workspace_for("ws-selfcheck-fail");
        let mut spawner = FakeSpawner::new(801, 802);
        let mut probe = || ProbeOutcome::Ready;

        let result = launch(
            &cfg,
            &resolved,
            &mut spawner,
            &mut probe,
            || Duration::ZERO,
            |_| {},
            13,
            &mut no_observation,
            &[],
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
