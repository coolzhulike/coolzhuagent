//! Binary entrypoint for the non-visual package launcher.
//!
//! 启动顺序（PATH-01/PATH-02 之后）：
//!
//! 1. 读 `package-launcher.json`（缺键语义按 `launcher_config_version` 分流）。
//! 2. 在**真实用户上下文**里解析路径，得到唯一的 [`app_launcher::ResolvedLaunchPaths`] 快照
//!    （优先级 ①–⑤；高优先级来源无效时**明确阻断**，不静默回退）。
//! 3. 把随包配置留一份原始基线快照（升级前保留旧启动配置）。
//! 4. 必要时落库/采用选择（revision 冲突检测 + 原子发布）。
//! 5. 复用判定：不只核对端口健康，还要核对**实际用户/实例归属 + 构建与协议 + workspace 身份
//!    与规范路径 + 数据绑定 + 当前启动状态**；不匹配即报告冲突（不静默连接、不按端口杀进程）。
//! 6. 复用或拉起 Web Console（隐藏窗口、stdio 重定向）并轮询健康；健康就绪后**回读后台实际使用
//!    的路径**写入自检，再拉起 Tauri shell（`--web-console-pid=<pid>`）。

mod startup_diagnostics;
#[cfg(windows)]
mod native_recovery;

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use app_launcher::launch_paths::{
    self, apply_resolution_actions, load_user_state, read_config_snapshots, record_config_snapshot,
    record_observations, select_workspace, user_state_root_for, ActionOutcome, ExplicitSelection,
    LaunchPathInputs, RealWorkspaceAccess, ResolvedLaunchPaths,
};
use app_launcher::service_identity::{self, current_process_user};
use app_launcher::{
    self, classify_listener_recovery, health_endpoint_port, http_probe,
    inherited_data_path_overrides, launch, launch_environment, overridden_data_path_envs,
    web_console_environment, ExecutableSpec, LaunchError, LaunchSpawner, LauncherConfig,
    ListenerOwner, ListenerRecoveryAction, ObservedBackground, ReuseDecision, SelfcheckPayload,
};

const DEFAULT_CONFIG_PATH: &str = "config/package-launcher.json";
const WEB_CONSOLE_STDERR_LOG: &str = "web-console.stderr.log";
const TAURI_STDERR_LOG: &str = "tauri.stderr.log";

const USAGE: &str = "\
COOLZHU-AGENT.exe [<package-launcher.json>] [选项]

路径解析（PATH-01/PATH-02）：
  --workspace <绝对路径>        仅本次启动使用该工作区（不写用户级选择）
  --select-workspace <绝对路径> 校验并持久化为用户级选择后退出（revision + 原子发布）
  --list-candidates             只读列出工作区候选与来源（不启动）
  --print-resolved-paths        解析并打印本次启动路径快照后退出（不启动）
  --user-state-dir <绝对路径>   覆盖用户级选择根（仅隔离测试/受控运维；不得从 log_dir 推导）
  --no-error-dialog             启动失败时只输出诊断，不显示错误窗口（用于自动化）
  -h, --help                    显示本帮助

用户级选择固定落在 %LOCALAPPDATA%\\CoolzhuAgent\\launcher-user.json，不随 MSI 覆盖。
";

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let suppress_dialog = args.iter().any(|arg| {
        matches!(
            arg.to_str(),
            Some(
                "--no-error-dialog"
                    | "--list-candidates"
                    | "--print-resolved-paths"
                    | "--select-workspace"
                    | "--help"
                    | "-h"
            )
        )
    });
    let cli = match CliOptions::parse(args.into_iter()) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("package-launcher: 参数错误：{message}\n\n{USAGE}");
            if !suppress_dialog {
                startup_diagnostics::show_message(
                    "启动参数不正确。请检查快捷方式的目标与参数，或从命令行运行 --help 查看说明。",
                );
            }
            return ExitCode::FAILURE;
        }
    };
    if cli.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let config_path = match resolve_config_path(cli.config_path.clone(), env::current_exe) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("package-launcher: could not resolve current executable: {err}");
            if !suppress_dialog {
                startup_diagnostics::show_message(
                    "无法定位 Coolzhu Agent 安装目录，请检查快捷方式或重新安装程序。",
                );
            }
            return ExitCode::FAILURE;
        }
    };

    let mut failure_context = startup_diagnostics::FailureContext::new(&config_path);
    match run(&config_path, &cli, &mut failure_context) {
        Ok(RunOutcome::Started(outcome)) => {
            eprintln!(
                "package-launcher: web-console pid={} tauri pid={}",
                outcome.web_console_pid, outcome.tauri_pid
            );
            ExitCode::SUCCESS
        }
        Ok(RunOutcome::ForwardedExisting { tauri_pid }) => {
            eprintln!(
                "package-launcher: forwarded --show-console to existing instance via tauri pid={tauri_pid}"
            );
            ExitCode::SUCCESS
        }
        Ok(RunOutcome::SelectionRecorded { revision, path }) => {
            println!(
                "package-launcher: 已建立用户级启动选择 {}（revision={revision}）",
                path.display()
            );
            ExitCode::SUCCESS
        }
        Ok(RunOutcome::CandidatesListed) | Ok(RunOutcome::ResolvedPathsPrinted) => {
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("package-launcher: startup failed: {err}");
            let message = failure_context.message(&err);
            eprintln!("{message}");
            if !cli.no_error_dialog && !suppress_dialog {
                startup_diagnostics::show_message(&message);
            }
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CliOptions {
    config_path: Option<OsString>,
    workspace: Option<PathBuf>,
    select_workspace: Option<PathBuf>,
    user_state_dir: Option<PathBuf>,
    list_candidates: bool,
    print_resolved_paths: bool,
    help: bool,
    no_error_dialog: bool,
}

impl CliOptions {
    fn parse<I>(args: I) -> Result<Self, String>
    where
        I: Iterator<Item = OsString>,
    {
        let mut options = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let text = arg.to_string_lossy().to_string();
            match text.as_str() {
                "--workspace" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--workspace 需要一个绝对路径参数".to_string())?;
                    options.workspace = Some(PathBuf::from(value));
                }
                "--select-workspace" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--select-workspace 需要一个绝对路径参数".to_string())?;
                    options.select_workspace = Some(PathBuf::from(value));
                }
                "--user-state-dir" => {
                    let value = args
                        .next()
                        .ok_or_else(|| "--user-state-dir 需要一个绝对路径参数".to_string())?;
                    options.user_state_dir = Some(PathBuf::from(value));
                }
                "--list-candidates" => options.list_candidates = true,
                "--print-resolved-paths" => options.print_resolved_paths = true,
                "--no-error-dialog" => options.no_error_dialog = true,
                "-h" | "--help" => options.help = true,
                other if other.starts_with("--") => {
                    return Err(format!("未知选项 {other}"));
                }
                _ => {
                    if options.config_path.is_some() {
                        return Err(format!("多余的参数 {text}（配置路径只能给一个）"));
                    }
                    options.config_path = Some(arg);
                }
            }
        }
        Ok(options)
    }
}

fn resolve_config_path<F>(config_arg: Option<OsString>, current_exe: F) -> io::Result<PathBuf>
where
    F: FnOnce() -> io::Result<PathBuf>,
{
    if let Some(config_arg) = config_arg {
        return Ok(PathBuf::from(config_arg));
    }

    let current_exe = current_exe()?;
    Ok(current_exe
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(DEFAULT_CONFIG_PATH))
}

enum RunOutcome {
    Started(app_launcher::LaunchOutcome),
    ForwardedExisting { tauri_pid: u32 },
    SelectionRecorded { revision: u64, path: PathBuf },
    CandidatesListed,
    ResolvedPathsPrinted,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn package_identity(config: &LauncherConfig) -> String {
    format!(
        "coolzhu-app-launcher/{} config_schema={}",
        env!("CARGO_PKG_VERSION"),
        config.config_schema_version
    )
}

/// 命令行工作区路径：展开 `%VAR%` 模板；未解析/非法即明确报错（不落回当前目录）。
fn expand_cli_workspace(raw: &Path, flag: &str) -> Result<PathBuf, LaunchError> {
    let text = raw.to_string_lossy().to_string();
    let mut lookup = |name: &str| env::var_os(name);
    launch_paths::expand_user_path_text(&text, &mut lookup).map_err(|detail| {
        LaunchError::WorkspaceUnavailable {
            role: format!("{flag}_path"),
            path: raw.to_path_buf(),
            source: format!("declared_by={flag}"),
            detail,
            remedy: "检查该 %VAR% 是否已设置/路径是否合法，或直接传入绝对路径".into(),
        }
    })
}

fn run(
    config_path: &Path,
    cli: &CliOptions,
    failure_context: &mut startup_diagnostics::FailureContext,
) -> Result<RunOutcome, LaunchError> {
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let config_text = fs::read_to_string(config_path).map_err(|error| {
        LaunchError::ConfigInvalid(format!("could not read {}: {error}", config_path.display()))
    })?;
    let config = LauncherConfig::from_json(&config_text, &config_dir)?;
    failure_context.configure(&config);

    // 用户级启动选择的位置：%LOCALAPPDATA%\CoolzhuAgent（或显式隔离覆盖）。**不看 log_dir。**
    let user_state_root = user_state_root_for(
        env::var_os("LOCALAPPDATA").map(PathBuf::from).as_deref(),
        env::var_os("USERPROFILE").map(PathBuf::from).as_deref(),
        cli.user_state_dir.as_deref(),
    )?;

    // 升级前保留旧启动配置（原始发布基线），并报告"声明值变化"。
    let timestamp = now_ms();
    let declared = config.packaged_default_workspace.clone();
    let current_snapshot = record_config_snapshot(
        &user_state_root,
        config_path,
        &config_text,
        declared.as_deref(),
        config.declared_runtime_dir_text.as_deref(),
        timestamp,
    )
    .ok()
    .flatten();
    if let Some(snapshot) = &current_snapshot {
        eprintln!(
            "package-launcher: 已保留随包配置基线快照 {}",
            snapshot.display()
        );
    }

    // 命令行给的路径：按实际运行用户的上下文展开 %VAR% 模板（未解析即明确报错）。
    let explicit_workspace = cli
        .workspace
        .as_ref()
        .map(|raw| expand_cli_workspace(raw, "--workspace"))
        .transpose()?;

    // 显式建立/修订用户级选择（恢复入口）：成功即退出。
    if let Some(target) = &cli.select_workspace {
        let target = expand_cli_workspace(target, "--select-workspace")?;
        let revision =
            select_workspace(&user_state_root, &target, &RealWorkspaceAccess, timestamp)?;
        return Ok(RunOutcome::SelectionRecorded {
            revision,
            path: target,
        });
    }

    let user_state = load_user_state(&user_state_root)?;
    let inputs = LaunchPathInputs {
        launch_id: format!("launch-{timestamp}-{}", std::process::id()),
        package_identity: package_identity(&config),
        config_schema_version: config.config_schema_version,
        packaged_default_workspace: config.packaged_default_workspace.clone(),
        legacy_log_dir_derived_workspace: config.legacy_log_dir_derived_workspace.clone(),
        config_declares_runtime_dir: config.declared_runtime_dir_text.is_some(),
        log_dir: config.log_dir.clone(),
        user_state_root: user_state_root.clone(),
        user_state,
        // 本次刚写入的快照不是"旧启动记录"，必须排除（否则它会被当成历史候选）。
        config_snapshots: read_config_snapshots(&user_state_root)
            .into_iter()
            .filter(|snapshot| Some(&snapshot.file) != current_snapshot.as_ref())
            .collect(),
        explicit_selection: explicit_workspace.map(|path| ExplicitSelection {
            path,
            persist: false,
            declared_by: "--workspace".into(),
        }),
        now_ms: timestamp,
    };

    if cli.list_candidates {
        let (candidates, notes) =
            launch_paths::list_workspace_candidates(&inputs, &RealWorkspaceAccess);
        println!("工作区候选（只读发现；不使用时间/容量/数量排序）：");
        println!("{}", launch_paths::render_candidates(&candidates));
        for note in &notes {
            println!("- [{}] {}", note.code, note.message);
        }
        return Ok(RunOutcome::CandidatesListed);
    }

    let decision = match launch_paths::resolve_launch_paths(&inputs, &RealWorkspaceAccess) {
        Ok(decision) => decision,
        Err(error) => {
            // 拒绝路径：错误必须可读、可执行，并写入自检供排障（不创建替代工作区）。
            write_refusal_selfcheck(&config, &error);
            return Err(error);
        }
    };

    eprintln!("package-launcher: 启动路径已解析（本次启动唯一权威，子进程共享）");
    for line in decision.resolved.display_lines() {
        eprintln!("  {line}");
    }
    for note in &decision.notes {
        eprintln!("  [{}] {}", note.code, note.message);
    }
    let inherited = inherited_data_path_overrides(|name| env::var_os(name));
    if !inherited.is_empty() {
        eprintln!(
            "  [inherited_data_path_override] 检测到继承的覆盖 {}：子进程会清除它们，由工作区配置决定业务数据根",
            inherited.join(", ")
        );
    }

    let mut notes = decision
        .notes
        .iter()
        .map(|note| format!("[{}] {}", note.code, note.message))
        .collect::<Vec<_>>();
    notes.extend(
        inherited.iter().map(|name| {
            format!("[inherited_data_path_override] cleared {name} for child processes")
        }),
    );

    // 落库/采用选择（失败只告警：不阻断启动，也不谎称"选择已保存"）。
    for (label, outcome) in apply_resolution_actions(&user_state_root, &decision, timestamp) {
        match outcome {
            ActionOutcome::Applied { revision } => {
                eprintln!("package-launcher: {label} ⇒ revision={revision}");
                notes.push(format!("[selection] {label} ⇒ revision={revision}"));
            }
            ActionOutcome::Skipped { reason } => {
                eprintln!("package-launcher: 警告：{label} 未完成：{reason}");
                notes.push(format!("[selection_skipped] {label} ⇒ {reason}"));
            }
        }
    }

    if cli.print_resolved_paths {
        println!("{}", decision.resolved.to_json());
        return Ok(RunOutcome::ResolvedPathsPrinted);
    }

    let previous_state = load_user_state(&user_state_root).ok();
    let expected_build_version = previous_state
        .as_ref()
        .and_then(|state| state.observations.last_background_build_version.clone());
    // 只有"同一工作区"的上次观测值才可作为数据绑定期望（否则会误报冲突）。
    let expected_session_db = previous_state.as_ref().and_then(|state| {
        let last_workspace = state.observations.last_workspace_root.as_ref()?;
        let last_db = state.observations.last_observed_session_db.as_ref()?;
        launch_paths::same_canonical(last_workspace, &decision.resolved.workspace_root)
            .then(|| last_db.clone())
    });

    if preflight_web_console_port(
        &config,
        &decision.resolved,
        expected_build_version.as_deref(),
        expected_session_db.as_deref(),
    )? {
        let tauri_pid = forward_existing_console(&config, &decision.resolved)?;
        return Ok(RunOutcome::ForwardedExisting { tauri_pid });
    }

    let start = Instant::now();
    let now_fn = move || start.elapsed();
    let sleep_fn = |duration: Duration| std::thread::sleep(duration);

    let mut spawner = RealSpawner::for_health_url(&config.health_url);
    let health_url = config.health_url.clone();
    let mut probe = || http_probe(&health_url);
    let mut observe = || {
        let identity = service_identity::fetch_service_identity(&health_url);
        Some(ObservedBackground {
            workspace: identity.reported_workspace,
            session_db: identity.reported_session_db,
            build_version: identity.reported_build_version,
            health_status: identity.reported_health_status,
            active_sessions: identity.reported_active_sessions,
            port: identity.reported_port,
        })
    };
    failure_context.web_attempt_started();
    let open = launch(
        &config,
        &decision.resolved,
        &mut spawner,
        &mut probe,
        now_fn,
        sleep_fn,
        timestamp,
        &mut observe,
        &notes,
    )
    .map(RunOutcome::Started);

    if let Ok(RunOutcome::Started(outcome)) = &open {
        // 记录观测值：构建身份/数据绑定用于下次复用判定；冲突只是告警。
        if let Some(observed) = &outcome.resolved.observed_build_version {
            let _ = record_observations(&user_state_root, |observations| {
                observations.last_background_build_version = Some(observed.clone());
                observations.last_launch_id = Some(outcome.resolved.launch_id.clone());
                observations.last_workspace_root = Some(outcome.resolved.workspace_root.clone());
                observations.last_observed_session_db =
                    outcome.resolved.observed_session_db.clone();
            });
        }
    }
    open
}

fn write_refusal_selfcheck(config: &LauncherConfig, error: &LaunchError) {
    let payload = SelfcheckPayload {
        ok: false,
        web_console_pid: None,
        tauri_pid: None,
        health_url: config.health_url.clone(),
        error: Some(error.to_string()),
        timestamp_ms: now_ms(),
        resolved: None,
        reuse: None,
        notes: vec!["[refused] 路径解析未通过：已阻断启动，未创建替代工作区".to_string()],
    };
    let _ = app_launcher::write_selfcheck(&config.selfcheck_file, &config.log_dir, &payload);
}

fn forward_existing_console(
    config: &LauncherConfig,
    resolved: &ResolvedLaunchPaths,
) -> Result<u32, LaunchError> {
    if !config.tauri.path.exists() {
        return Err(LaunchError::ExecutableMissing {
            role: "tauri",
            path: config.tauri.path.clone(),
        });
    }
    fs::create_dir_all(&config.log_dir).map_err(LaunchError::Persistence)?;

    let mut args = config.tauri.args.clone();
    if !args.iter().any(|arg| arg == "--show-console") {
        args.push("--show-console".to_string());
    }
    let tauri_log_file = config.log_dir.join("tauri.stdout.log");
    let mut spawner = RealSpawner::for_health_url(&config.health_url);
    spawner.spawn_tauri(&config.tauri, &args, resolved, &tauri_log_file)
}

/// 复用判定：**不只核对端口健康**（裁决第六节）。
///
/// 返回 `true` 表示"可复用的既有实例已确认"（只转发 `--show-console`）。
#[cfg(windows)]
fn preflight_web_console_port(
    config: &LauncherConfig,
    resolved: &ResolvedLaunchPaths,
    expected_build_version: Option<&str>,
    expected_session_db: Option<&Path>,
) -> Result<bool, LaunchError> {
    let port = health_endpoint_port(&config.health_url).ok_or_else(|| {
        LaunchError::ConfigInvalid(format!("invalid health_url: {}", config.health_url))
    })?;
    let owner = query_listener_owner(port)?;
    let health_ready = matches!(
        http_probe(&config.health_url),
        app_launcher::ProbeOutcome::Ready
    );
    match classify_listener_recovery(health_ready, owner.as_ref()) {
        ListenerRecoveryAction::Free => Ok(false),
        ListenerRecoveryAction::CleanupOrphan { parent_pid } => {
            eprintln!(
                "package-launcher: recovering stale listener on port {port}, dead owner pid={parent_pid}"
            );
            cleanup_stale_listener_processes(parent_pid, None)?;
            wait_for_port_release(port, config.health_timeout, config.health_poll_interval)?;
            Ok(false)
        }
        ListenerRecoveryAction::CleanupKnownHolder { pid } => {
            eprintln!(
                "package-launcher: recovering known local-model holder on port {port}, pid={pid}"
            );
            cleanup_stale_listener_processes(pid, Some(pid))?;
            wait_for_port_release(port, config.health_timeout, config.health_poll_interval)?;
            Ok(false)
        }
        ListenerRecoveryAction::Block { pid, .. } => {
            let owner_user = query_process_owner_user(pid);
            let identity = service_identity::fetch_service_identity(&config.health_url)
                .with_listener(owner.as_ref(), owner_user.as_deref());
            let current_user = current_process_user();
            match service_identity::decide_reuse(
                &resolved.workspace_root,
                expected_session_db,
                expected_build_version,
                current_user.as_deref(),
                &identity,
            ) {
                ReuseDecision::Reuse { verification } => {
                    eprintln!(
                        "package-launcher: 既有实例身份核对通过（workspace={}，port={port}），转发 --show-console",
                        resolved.workspace_root.display()
                    );
                    for check in &verification.checks {
                        eprintln!(
                            "  核对[{}] expected={} observed={} ⇒ {}",
                            check.name, check.expected, check.observed, check.verdict
                        );
                    }
                    Ok(true)
                }
                ReuseDecision::Conflict { reason, details } => Err(LaunchError::ServiceConflict {
                    port,
                    reason,
                    details,
                }),
                ReuseDecision::StartFresh => Ok(false),
            }
        }
    }
}

#[cfg(not(windows))]
fn preflight_web_console_port(
    _config: &LauncherConfig,
    _resolved: &ResolvedLaunchPaths,
    _expected_build_version: Option<&str>,
    _expected_session_db: Option<&Path>,
) -> Result<bool, LaunchError> {
    Ok(false)
}

#[cfg(windows)]
fn query_listener_owner(port: u16) -> Result<Option<ListenerOwner>, LaunchError> {
    let script = format!(
        "$c=Get-NetTCPConnection -State Listen -LocalPort {port} -ErrorAction SilentlyContinue | Select-Object -First 1; \
         if($null -eq $c){{exit 0}}; \
         $ownerPid=[int]$c.OwningProcess; \
         $p=Get-CimInstance Win32_Process -Filter \"ProcessId=$ownerPid\" -ErrorAction SilentlyContinue; \
         [pscustomobject]@{{pid=$ownerPid;alive=($null -ne $p);process_name=if($p){{$p.Name}}else{{$null}}}} | ConvertTo-Json -Compress"
    );
    let output = hidden_powershell(&script)?;
    let body = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if body.is_empty() {
        return Ok(None);
    }
    let value: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
        LaunchError::ConfigInvalid(format!(
            "could not parse listener owner for port {port}: {error}"
        ))
    })?;
    let pid = value
        .get("pid")
        .and_then(serde_json::Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| {
            LaunchError::ConfigInvalid(format!("listener owner for port {port} has no valid pid"))
        })?;
    Ok(Some(ListenerOwner {
        pid,
        alive: value
            .get("alive")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        process_name: value
            .get("process_name")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
    }))
}

/// 监听进程的属主用户（`DOMAIN\user`）：用于核对"实际用户/实例归属"。
#[cfg(windows)]
fn query_process_owner_user(pid: u32) -> Option<String> {
    let script = format!(
        "$p=Get-CimInstance Win32_Process -Filter \"ProcessId={pid}\" -ErrorAction SilentlyContinue; \
         if($null -eq $p){{exit 0}}; \
         $o=$p | Invoke-CimMethod -MethodName GetOwner -ErrorAction SilentlyContinue; \
         if($null -eq $o){{exit 0}}; \
         Write-Output ($o.Domain + '\\' + $o.User)"
    );
    let output = hidden_powershell(&script).ok()?;
    let body = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!body.is_empty()).then_some(body)
}

#[cfg(windows)]
fn cleanup_stale_listener_processes(
    parent_pid: u32,
    direct_pid: Option<u32>,
) -> Result<(), LaunchError> {
    let direct_clause = direct_pid
        .map(|pid| format!(" -or $_.ProcessId -eq {pid}"))
        .unwrap_or_default();
    let script = format!(
        "$targets=Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | \
         Where-Object {{ (($_.ParentProcessId -eq {parent_pid}){direct_clause}) -and \
         ($_.Name -eq 'llama-server.exe' -or $_.Name -eq 'conhost.exe') }}; \
         $targets | ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue; \
         Write-Output (\"stopped=\" + $_.ProcessId + \";name=\" + $_.Name) }}"
    );
    let output = hidden_powershell(&script)?;
    let stopped = String::from_utf8_lossy(&output.stdout);
    if !stopped.trim().is_empty() {
        eprintln!("package-launcher: {}", stopped.trim().replace('\n', ", "));
    }
    Ok(())
}

#[cfg(windows)]
fn wait_for_port_release(
    port: u16,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<(), LaunchError> {
    let deadline = Instant::now() + timeout;
    loop {
        if query_listener_owner(port)?.is_none() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(LaunchError::ConfigInvalid(format!(
                "stale web console port {port} was not released within {}s",
                timeout.as_secs()
            )));
        }
        std::thread::sleep(poll_interval);
    }
}

#[cfg(windows)]
fn hidden_powershell(script: &str) -> Result<std::process::Output, LaunchError> {
    let mut command = Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_hidden_window(&mut command);
    let output = command.output().map_err(LaunchError::WebConsoleSpawn)?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(LaunchError::ConfigInvalid(format!(
            "port recovery command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// Real process spawner backed by `std::process::Command`.
#[derive(Default)]
struct RealSpawner {
    children: HashMap<u32, Child>,
    console_url: Option<String>,
    #[cfg(windows)]
    web_identity: Option<windows_process_guard::ProcessIdentity>,
}

impl RealSpawner {
    fn for_health_url(health_url: &str) -> Self {
        let console_url = health_url.split_once("://").map(|(scheme, tail)| {
            format!("{scheme}://{}", tail.split('/').next().unwrap_or(tail))
        });
        Self { console_url, ..Self::default() }
    }
}

impl LaunchSpawner for RealSpawner {
    fn spawn_web_console(
        &mut self,
        spec: &ExecutableSpec,
        resolved: &ResolvedLaunchPaths,
        log_file: &Path,
    ) -> Result<u32, LaunchError> {
        let log = fs::File::create(log_file).map_err(LaunchError::WebConsoleSpawn)?;
        let stderr_path = log_file.with_file_name(WEB_CONSOLE_STDERR_LOG);
        let stderr = fs::File::create(stderr_path).map_err(LaunchError::WebConsoleSpawn)?;
        let stdout = Stdio::from(log);

        let mut cmd = Command::new(&spec.path);
        cmd.args(&spec.args)
            .envs(web_console_environment())
            .envs(launch_environment(resolved))
            .current_dir(&resolved.workspace_root)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(Stdio::from(stderr));
        // 清掉从父环境继承的、会越过工作区配置的业务数据路径覆盖（不静默忽略）。
        for name in overridden_data_path_envs() {
            cmd.env_remove(name);
        }

        apply_hidden_window(&mut cmd);

        let child = cmd.spawn().map_err(LaunchError::WebConsoleSpawn)?;
        #[cfg(windows)]
        { self.web_identity = windows_process_guard::capture_child_process_identity(&child).ok(); }
        let pid = child.id();
        self.children.insert(pid, child);
        Ok(pid)
    }

    fn spawn_tauri(
        &mut self,
        spec: &ExecutableSpec,
        args: &[String],
        resolved: &ResolvedLaunchPaths,
        log_file: &Path,
    ) -> Result<u32, LaunchError> {
        let stdout = fs::File::create(log_file).map_err(LaunchError::TauriSpawn)?;
        let stderr_path = log_file.with_file_name(TAURI_STDERR_LOG);
        let stderr = fs::File::create(&stderr_path).map_err(LaunchError::TauriSpawn)?;
        let mut cmd = Command::new(&spec.path);
        cmd.args(args)
            .envs(launch_environment(resolved))
            .current_dir(&resolved.workspace_root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        if let Some(url) = self.console_url.as_deref() {
            // 本次已通过健康检查的后台优先于全局临时文件，避免多个实例串到错误窗口。
            cmd.env("COOLZHU_GUI_WEB_URL", url);
        }
        for name in overridden_data_path_envs() {
            cmd.env_remove(name);
        }
        let child = cmd.spawn().map_err(LaunchError::TauriSpawn)?;
        let pid = child.id();
        #[cfg(windows)]
        if let Some(server) = self.web_identity.as_ref() {
            if let Err(error) = native_recovery::register_shell(server, &child) {
                // 恢复资格登记失败不阻止普通聊天启动，也不会降级成未经验证的恢复入口。
                use std::io::Write;
                if let Ok(mut log) = fs::OpenOptions::new().append(true).open(&stderr_path) {
                    let reason = error.to_string().replace(['\r', '\n'], " ");
                    let _ = writeln!(log, "原生恢复通道未接通：{reason}；聊天可继续，恢复须从完整桌面启动器重新启动。");
                }
            }
        }
        self.children.insert(pid, child);
        Ok(pid)
    }

    fn terminate_process(&mut self, pid: u32) -> io::Result<()> {
        let Some(mut child) = self.children.remove(&pid) else {
            return Ok(());
        };
        if child.try_wait()?.is_none() {
            child.kill()?;
        }
        let _ = child.wait()?;
        Ok(())
    }
}

/// Hide the spawned process window on Windows (Web Console runs headless).
#[cfg(windows)]
fn apply_hidden_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW = 0x0800_0000
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn apply_hidden_window(_cmd: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_path_is_resolved_from_current_exe_directory() {
        let current_exe = PathBuf::from("C:/Program Files/Coolzhu/COOLZHU-AGENT.exe");
        let arbitrary_cwd = PathBuf::from("D:/unrelated-working-directory");

        let config_path = resolve_config_path(None, || Ok(current_exe.clone())).unwrap();

        assert_eq!(
            config_path,
            current_exe
                .parent()
                .unwrap()
                .join("config/package-launcher.json")
        );
        assert_ne!(config_path, arbitrary_cwd.join(DEFAULT_CONFIG_PATH));
    }

    #[test]
    fn explicit_config_path_does_not_resolve_current_exe() {
        let explicit = PathBuf::from("D:/configs/package-launcher.json");

        let config_path = resolve_config_path(Some(explicit.clone().into_os_string()), || {
            panic!("current_exe must not be called for an explicit config path")
        })
        .unwrap();

        assert_eq!(config_path, explicit);
    }

    #[test]
    fn cli_parses_path_flags_and_first_positional_config() {
        let args = vec![
            OsString::from("D:/cfg/package-launcher.json"),
            OsString::from("--workspace"),
            OsString::from("D:/ws"),
            OsString::from("--print-resolved-paths"),
            OsString::from("--user-state-dir"),
            OsString::from("D:/state"),
        ];
        let cli = CliOptions::parse(args.into_iter()).unwrap();
        assert_eq!(
            cli.config_path.as_deref(),
            Some(std::ffi::OsStr::new("D:/cfg/package-launcher.json"))
        );
        assert_eq!(cli.workspace.as_deref(), Some(Path::new("D:/ws")));
        assert_eq!(cli.user_state_dir.as_deref(), Some(Path::new("D:/state")));
        assert!(cli.print_resolved_paths);
        assert!(!cli.list_candidates);
    }

    #[test]
    fn cli_rejects_unknown_flags_and_missing_values() {
        assert!(CliOptions::parse(vec![OsString::from("--nope")].into_iter()).is_err());
        assert!(CliOptions::parse(vec![OsString::from("--workspace")].into_iter()).is_err());
        assert!(CliOptions::parse(
            vec![OsString::from("a.json"), OsString::from("b.json")].into_iter()
        )
        .is_err());
    }

    #[test]
    fn cli_selection_flag_is_parsed_separately_from_this_launch_workspace() {
        let cli = CliOptions::parse(
            vec![
                OsString::from("--select-workspace"),
                OsString::from("D:/ws"),
            ]
            .into_iter(),
        )
        .unwrap();
        assert_eq!(cli.select_workspace.as_deref(), Some(Path::new("D:/ws")));
        assert!(cli.workspace.is_none());
    }
}
