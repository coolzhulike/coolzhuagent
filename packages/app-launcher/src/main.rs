//! Binary entrypoint for the non-visual package launcher.
//!
//! Wires the [`app_launcher`] library to real OS collaborators: a
//! [`RealSpawner`] backed by `std::process::Command`, the in-process
//! [`app_launcher::http_probe`], and `Instant`/`thread::sleep` for the
//! bounded health-poll budget. The Web Console is spawned hidden
//! (`CREATE_NO_WINDOW` on Windows) with stdout/stderr redirected to the log
//! 文件；Tauri shell 使用 `--web-console-pid=<pid>` 启动。对已健康 Web Console 的重复点击
//! 会把 `--show-console` 转发给 Tauri shell，由单实例处理器显示已有控制台。

use std::collections::HashMap;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use app_launcher::{
    self, classify_listener_recovery, health_endpoint_port, http_probe,
    is_live_web_console_instance, launch, web_console_environment, web_console_runtime_environment,
    ExecutableSpec, LaunchError, LaunchSpawner, LauncherConfig, ListenerOwner,
    ListenerRecoveryAction,
};

const DEFAULT_CONFIG_PATH: &str = "config/package-launcher.json";
const WEB_CONSOLE_STDERR_LOG: &str = "web-console.stderr.log";
const TAURI_STDERR_LOG: &str = "tauri.stderr.log";

fn main() -> ExitCode {
    let config_path = match resolve_config_path(env::args_os().nth(1), env::current_exe) {
        Ok(path) => path,
        Err(err) => {
            eprintln!("package-launcher: could not resolve current executable: {err}");
            return ExitCode::FAILURE;
        }
    };

    match run(&config_path) {
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
        Err(err) => {
            eprintln!("package-launcher: startup failed: {err}");
            ExitCode::FAILURE
        }
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
}

fn run(config_path: &Path) -> Result<RunOutcome, LaunchError> {
    let config_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let text = fs::read_to_string(config_path).map_err(|e| {
        LaunchError::ConfigInvalid(format!("could not read {}: {e}", config_path.display()))
    })?;
    let config = LauncherConfig::from_json(&text, &config_dir)?;
    if preflight_web_console_port(&config)? {
        let tauri_pid = forward_existing_console(&config)?;
        return Ok(RunOutcome::ForwardedExisting { tauri_pid });
    }

    let start = Instant::now();
    let now_fn = move || start.elapsed();
    let sleep_fn = |d: Duration| std::thread::sleep(d);
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mut spawner = RealSpawner::default();
    let health_url = config.health_url.clone();
    let mut probe = || http_probe(&health_url);
    launch(
        &config,
        &mut spawner,
        &mut probe,
        now_fn,
        sleep_fn,
        timestamp_ms,
    )
    .map(RunOutcome::Started)
}

fn forward_existing_console(config: &LauncherConfig) -> Result<u32, LaunchError> {
    if !config.tauri.path.exists() {
        return Err(LaunchError::ExecutableMissing {
            role: "tauri",
            path: config.tauri.path.clone(),
        });
    }
    fs::create_dir_all(&config.runtime_dir).map_err(LaunchError::Persistence)?;
    fs::create_dir_all(&config.log_dir).map_err(LaunchError::Persistence)?;

    let mut args = config.tauri.args.clone();
    if !args.iter().any(|arg| arg == "--show-console") {
        args.push("--show-console".to_string());
    }
    let tauri_log_file = config.log_dir.join("tauri.stdout.log");
    let mut spawner = RealSpawner::default();
    spawner.spawn_tauri(&config.tauri, &args, &config.runtime_dir, &tauri_log_file)
}

#[cfg(windows)]
fn preflight_web_console_port(config: &LauncherConfig) -> Result<bool, LaunchError> {
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
        ListenerRecoveryAction::Block { pid, process_name } => {
            if is_live_web_console_instance(health_ready, owner.as_ref()) {
                eprintln!(
                    "package-launcher: healthy Web Console already exists at port {port}; forwarding --show-console"
                );
                return Ok(true);
            }
            Err(LaunchError::ConfigInvalid(format!(
                "web console port {port} is already owned by live process pid={pid} name={}",
                process_name.as_deref().unwrap_or("unknown")
            )))
        }
    }
}

#[cfg(not(windows))]
fn preflight_web_console_port(_config: &LauncherConfig) -> Result<bool, LaunchError> {
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
}

impl LaunchSpawner for RealSpawner {
    fn spawn_web_console(
        &mut self,
        spec: &ExecutableSpec,
        runtime_dir: &Path,
        log_file: &Path,
    ) -> Result<u32, LaunchError> {
        let log = fs::File::create(log_file).map_err(LaunchError::WebConsoleSpawn)?;
        let stderr_path = log_file.with_file_name(WEB_CONSOLE_STDERR_LOG);
        let stderr = fs::File::create(stderr_path).map_err(LaunchError::WebConsoleSpawn)?;
        let stdout = Stdio::from(log);

        let mut cmd = Command::new(&spec.path);
        cmd.args(&spec.args)
            .envs(web_console_environment())
            .envs(web_console_runtime_environment(runtime_dir))
            .current_dir(runtime_dir)
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(Stdio::from(stderr));

        apply_hidden_window(&mut cmd);

        let child = cmd.spawn().map_err(LaunchError::WebConsoleSpawn)?;
        let pid = child.id();
        self.children.insert(pid, child);
        Ok(pid)
    }

    fn spawn_tauri(
        &mut self,
        spec: &ExecutableSpec,
        args: &[String],
        runtime_dir: &Path,
        log_file: &Path,
    ) -> Result<u32, LaunchError> {
        let stdout = fs::File::create(log_file).map_err(LaunchError::TauriSpawn)?;
        let stderr_path = log_file.with_file_name(TAURI_STDERR_LOG);
        let stderr = fs::File::create(stderr_path).map_err(LaunchError::TauriSpawn)?;
        let mut cmd = Command::new(&spec.path);
        cmd.args(args)
            .env("COOLZHU_RUNTIME_DIR", runtime_dir)
            .current_dir(runtime_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        let child = cmd.spawn().map_err(LaunchError::TauriSpawn)?;
        let pid = child.id();
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
}
