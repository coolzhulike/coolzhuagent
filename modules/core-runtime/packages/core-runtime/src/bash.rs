use std::env;
use std::io;
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::sandbox::{
    build_linux_sandbox_command, resolve_sandbox_status_for_request, FilesystemIsolationMode,
    SandboxConfig, SandboxStatus,
};
use crate::ConfigLoader;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BashCommandInput {
    pub command: String,
    pub cwd: Option<String>,
    pub timeout: Option<u64>,
    pub description: Option<String>,
    #[serde(rename = "run_in_background")]
    pub run_in_background: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "namespaceRestrictions")]
    pub namespace_restrictions: Option<bool>,
    #[serde(rename = "isolateNetwork")]
    pub isolate_network: Option<bool>,
    #[serde(rename = "filesystemMode")]
    pub filesystem_mode: Option<FilesystemIsolationMode>,
    #[serde(rename = "allowedMounts")]
    pub allowed_mounts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BashCommandOutput {
    pub stdout: String,
    pub stderr: String,
    #[serde(rename = "rawOutputPath")]
    pub raw_output_path: Option<String>,
    pub interrupted: bool,
    #[serde(rename = "isImage")]
    pub is_image: Option<bool>,
    #[serde(rename = "backgroundTaskId")]
    pub background_task_id: Option<String>,
    #[serde(rename = "backgroundedByUser")]
    pub backgrounded_by_user: Option<bool>,
    #[serde(rename = "assistantAutoBackgrounded")]
    pub assistant_auto_backgrounded: Option<bool>,
    #[serde(rename = "dangerouslyDisableSandbox")]
    pub dangerously_disable_sandbox: Option<bool>,
    #[serde(rename = "returnCodeInterpretation")]
    pub return_code_interpretation: Option<String>,
    #[serde(rename = "noOutputExpected")]
    pub no_output_expected: Option<bool>,
    #[serde(rename = "structuredContent")]
    pub structured_content: Option<Vec<serde_json::Value>>,
    #[serde(rename = "persistedOutputPath")]
    pub persisted_output_path: Option<String>,
    #[serde(rename = "persistedOutputSize")]
    pub persisted_output_size: Option<u64>,
    #[serde(rename = "sandboxStatus")]
    pub sandbox_status: Option<SandboxStatus>,
    /// 宿主真实进程回执；只有工具 registry 的 shell 出口会填充，模型正文不能生成。
    #[serde(rename = "executionReceipt", default, skip_serializing_if = "Option::is_none")]
    pub execution_receipt: Option<ProcessExecutionReceipt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessExecutionReceipt {
    pub id: String,
    pub shell: String,
    pub command: String,
    pub cwd: String,
    #[serde(rename = "timeoutMs")]
    pub timeout_ms: u64,
    #[serde(rename = "startedAtUnixMs")]
    pub started_at_unix_ms: u64,
    #[serde(rename = "finishedAtUnixMs")]
    pub finished_at_unix_ms: u64,
    #[serde(rename = "exitCode")]
    pub exit_code: Option<i32>,
    pub interrupted: bool,
}

pub fn execute_bash(input: BashCommandInput) -> io::Result<BashCommandOutput> {
    let cwd = input
        .cwd
        .as_ref()
        .map_or_else(env::current_dir, |cwd| Ok(std::path::PathBuf::from(cwd)))?;
    let sandbox_status = sandbox_status_for_input(&input, &cwd);

    if sandbox_status.enabled && sandbox_status.filesystem_mode == FilesystemIsolationMode::WindowsLowIntegrity {
        if !sandbox_status.active || input.run_in_background.unwrap_or(false) {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "低完整性命令必须受监督前台执行，且不能请求未实现的命名空间或网络隔离"));
        }
        #[cfg(windows)]
        {
            let scratch = windows_process_guard::create_low_integrity_workspace(&cwd)?;
            return execute_bash_managed(input, sandbox_status, scratch);
        }
        #[cfg(not(windows))]
        return Err(io::Error::new(io::ErrorKind::Unsupported, "低完整性写隔离仅支持 Windows"));
    }

    if input.run_in_background.unwrap_or(false) {
        let mut child = prepare_command(&input.command, &cwd, &sandbox_status, false);
        let child = child
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        return Ok(BashCommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            raw_output_path: None,
            interrupted: false,
            is_image: None,
            background_task_id: Some(child.id().to_string()),
            backgrounded_by_user: Some(false),
            assistant_auto_backgrounded: Some(false),
            dangerously_disable_sandbox: input.dangerously_disable_sandbox,
            return_code_interpretation: None,
            no_output_expected: Some(true),
            structured_content: None,
            persisted_output_path: None,
            persisted_output_size: None,
            sandbox_status: Some(sandbox_status),
            execution_receipt: None,
        });
    }

    execute_bash_managed(input, sandbox_status, cwd)
}

fn execute_bash_managed(
    input: BashCommandInput,
    sandbox_status: SandboxStatus,
    cwd: std::path::PathBuf,
) -> io::Result<BashCommandOutput> {
    let mut command = prepare_command(&input.command, &cwd, &sandbox_status, true);
    let low_integrity = sandbox_status.enabled && sandbox_status.filesystem_mode == FilesystemIsolationMode::WindowsLowIntegrity;
    let managed = if low_integrity {
        crate::managed_process::output_low_integrity(&mut command, input.timeout.map(Duration::from_millis))?
    } else { crate::managed_process::output(&mut command, input.timeout.map(Duration::from_millis))? };
    let output = managed.output();
    let interrupted = managed.interruption().is_some();
    let stdout = crate::managed_process::decode_console_output(&output.stdout);
    let mut stderr = crate::managed_process::decode_console_output(&output.stderr);
    if let Some(reason) = managed.interruption() {
        if !stderr.is_empty() { stderr.push('\n'); }
        stderr.push_str(&match reason {
            crate::managed_process::Interruption::TimedOut => format!("Command exceeded timeout of {} ms", input.timeout.unwrap_or(0)),
            crate::managed_process::Interruption::Cancelled => "Command cancelled; managed process exited".to_string(),
        });
    }
    let no_output_expected = Some(stdout.trim().is_empty() && stderr.trim().is_empty());
    let return_code_interpretation = managed.interruption().map(|reason| match reason {
        crate::managed_process::Interruption::TimedOut => "timeout".to_string(),
        crate::managed_process::Interruption::Cancelled => "cancelled".to_string(),
    }).or_else(|| output.status.code().and_then(|code| {
        if code == 0 {
            None
        } else {
            Some(format!("exit_code:{code}"))
        }
    }));

    Ok(BashCommandOutput {
        stdout,
        stderr,
        raw_output_path: None,
        interrupted,
        is_image: None,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        dangerously_disable_sandbox: input.dangerously_disable_sandbox,
        return_code_interpretation,
        no_output_expected,
        structured_content: low_integrity.then(|| vec![serde_json::json!({
            "isolation": "windows-low-integrity", "working_directory": cwd,
            "write_boundary": "MIC no-write-up", "read_isolation": false, "network_isolation": false,
        })]),
        persisted_output_path: None,
        persisted_output_size: None,
        sandbox_status: Some(sandbox_status),
        execution_receipt: None,
    })
}

fn sandbox_status_for_input(input: &BashCommandInput, cwd: &std::path::Path) -> SandboxStatus {
    let config = ConfigLoader::default_for(cwd).load().map_or_else(
        |_| SandboxConfig::default(),
        |runtime_config| runtime_config.sandbox().clone(),
    );
    let request = config.resolve_request(
        input.dangerously_disable_sandbox.map(|disabled| !disabled),
        input.namespace_restrictions,
        input.isolate_network,
        input.filesystem_mode,
        input.allowed_mounts.clone(),
    );
    resolve_sandbox_status_for_request(&request, cwd)
}

fn prepare_command(
    command: &str,
    cwd: &std::path::Path,
    sandbox_status: &SandboxStatus,
    create_dirs: bool,
) -> Command {
    if create_dirs {
        prepare_sandbox_dirs(cwd);
    }

    if let Some(launcher) = build_linux_sandbox_command(command, cwd, sandbox_status) {
        let mut prepared = Command::new(launcher.program);
        prepared.args(launcher.args);
        prepared.current_dir(cwd);
        crate::env_denylist::sanitize_command_env(&mut prepared);
        prepared.envs(launcher.env);
        return prepared;
    }

    #[cfg(windows)]
    let mut prepared = {
        let mut prepared = Command::new("cmd");
        prepared.arg("/C").arg(command);
        prepared
    };
    #[cfg(not(windows))]
    let mut prepared = {
        let mut prepared = Command::new("sh");
        prepared.arg("-lc").arg(command);
        prepared
    };
    prepared.current_dir(cwd);
    crate::env_denylist::sanitize_command_env(&mut prepared);
    if sandbox_status.filesystem_active {
        prepared.env("HOME", cwd.join(".sandbox-home"));
        prepared.env("TMPDIR", cwd.join(".sandbox-tmp"));
        if sandbox_status.filesystem_mode == FilesystemIsolationMode::WindowsLowIntegrity {
            prepared.env("TEMP", cwd.join(".sandbox-tmp"));
            prepared.env("TMP", cwd.join(".sandbox-tmp"));
        }
    }
    prepared
}

fn prepare_sandbox_dirs(cwd: &std::path::Path) {
    let _ = std::fs::create_dir_all(cwd.join(".sandbox-home"));
    let _ = std::fs::create_dir_all(cwd.join(".sandbox-tmp"));
}

#[cfg(test)]
mod tests {
    use super::{execute_bash, BashCommandInput};
    use crate::sandbox::FilesystemIsolationMode;

    #[cfg(windows)]
    #[test]
    fn low_integrity_runtime_write_boundary_and_background_refusal() {
        let root=tempfile::tempdir().unwrap();
        let protected=root.path().join("protected.txt");
        std::fs::write(&protected,b"unchanged").unwrap();
        let mut input=BashCommandInput {
            command:format!("(echo allowed> allowed.txt) && (echo blocked> \"{}\")",protected.display()),
            cwd:Some(root.path().display().to_string()),timeout:Some(5_000),description:None,
            run_in_background:Some(false),dangerously_disable_sandbox:Some(false),
            namespace_restrictions:Some(false),isolate_network:Some(false),
            filesystem_mode:Some(FilesystemIsolationMode::WindowsLowIntegrity),allowed_mounts:None,
        };
        let result=execute_bash(input.clone()).unwrap();
        assert!(result.sandbox_status.as_ref().unwrap().filesystem_active);
        assert!(result.return_code_interpretation.as_ref().is_some_and(|s|s.starts_with("exit_code:")));
        assert_eq!(std::fs::read(&protected).unwrap(),b"unchanged");
        let scratch=result.structured_content.as_ref().unwrap()[0]["working_directory"].as_str().unwrap();
        assert!(std::path::Path::new(scratch).starts_with(root.path().canonicalize().unwrap()));
        assert!(std::fs::read_to_string(std::path::Path::new(scratch).join("allowed.txt")).unwrap().contains("allowed"));
        input.run_in_background=Some(true);
        assert_eq!(execute_bash(input).unwrap_err().kind(),std::io::ErrorKind::Unsupported);
    }

    #[test]
    fn executes_simple_command() {
        let output = execute_bash(BashCommandInput {
            command: print_command("hello"),
            cwd: None,
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(false),
            namespace_restrictions: Some(false),
            isolate_network: Some(false),
            filesystem_mode: Some(FilesystemIsolationMode::WorkspaceOnly),
            allowed_mounts: None,
        })
        .expect("bash command should execute");

        assert_eq!(output.stdout.trim(), "hello");
        assert!(!output.interrupted);
        assert!(output.sandbox_status.is_some());
    }

    #[test]
    fn disables_sandbox_when_requested() {
        let output = execute_bash(BashCommandInput {
            command: print_command("hello"),
            cwd: None,
            timeout: Some(1_000),
            description: None,
            run_in_background: Some(false),
            dangerously_disable_sandbox: Some(true),
            namespace_restrictions: None,
            isolate_network: None,
            filesystem_mode: None,
            allowed_mounts: None,
        })
        .expect("bash command should execute");

        assert!(!output.sandbox_status.expect("sandbox status").enabled);
    }

    #[cfg(windows)]
    fn print_command(text: &str) -> String {
        format!("echo {text}")
    }

    #[cfg(not(windows))]
    fn print_command(text: &str) -> String {
        format!("printf '{text}'")
    }
}
