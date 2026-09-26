//! 启动失败的本地可见提示。只解释错误，不选择工作区或修改业务数据库。
use app_launcher::{LaunchError, LauncherConfig};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(super) struct FailureContext {
    config_path: PathBuf,
    log_dir: Option<PathBuf>,
    selfcheck_file: Option<PathBuf>,
    web_attempt_started: Option<SystemTime>,
}

impl FailureContext {
    pub(super) fn new(config_path: &Path) -> Self {
        Self {
            config_path: config_path.to_path_buf(),
            log_dir: None,
            selfcheck_file: None,
            web_attempt_started: None,
        }
    }

    pub(super) fn configure(&mut self, config: &LauncherConfig) {
        self.log_dir = Some(config.log_dir.clone());
        self.selfcheck_file = Some(config.selfcheck_file.clone());
    }

    pub(super) fn web_attempt_started(&mut self) {
        self.web_attempt_started = Some(SystemTime::now());
    }

    pub(super) fn message(&self, error: &LaunchError) -> String {
        let fresh_log = if matches!(error, LaunchError::HealthTimeout { .. }) {
            self.web_attempt_started.and_then(|started| {
                read_current_attempt_tail(
                    &self.log_dir.as_ref()?.join("web-console.stderr.log"),
                    started,
                )
            })
        } else {
            None
        };
        let reason = failure_reason(error, fresh_log.as_deref());
        let mut message = format!(
            "Coolzhu Agent 未能启动。\n\n{reason}\n\n启动配置：\n{}",
            self.config_path.display()
        );
        if let Some(log_dir) = &self.log_dir {
            message.push_str(&format!("\n\n日志目录：\n{}", log_dir.display()));
        }
        if let Some(selfcheck) = &self.selfcheck_file {
            message.push_str(&format!("\n\n启动自检：\n{}", selfcheck.display()));
        }
        message.push_str("\n\n请保留上述日志，以便定位本次启动失败的原因。");
        message
    }
}

/// 旧日志不参与本次失败归因；只读本次启动之后写入的末尾 64 KiB。
fn read_current_attempt_tail(path: &Path, started: SystemTime) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if metadata.modified().ok()? < started {
        return None;
    }
    file.seek(SeekFrom::Start(metadata.len().saturating_sub(64 * 1024)))
        .ok()?;
    let mut bytes = Vec::new();
    file.take(64 * 1024).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn failure_reason(error: &LaunchError, stderr: Option<&str>) -> String {
    if let Some(text) = stderr {
        // 只认宿主的固定拒绝标记。原始日志、路径和远端响应不复制到提示框。
        if text.contains("会话库 schema 比本二进制更新") {
            let versions = match (
                number_after(text, "user_version="),
                number_after(text, "本构建只支持到 "),
            ) {
                (Some(current), Some(supported)) => {
                    format!("（数据版本 {current}，当前程序支持到 {supported}）")
                }
                _ => String::new(),
            };
            return format!("现有会话数据由更新版本的程序创建{versions}。当前程序已拒绝继续打开，以保护数据。\n\n请安装支持该数据版本的程序后重新启动。请勿删除会话库，也不要手工降低数据库版本号。");
        }
    }
    match error {
        LaunchError::ConfigInvalid(_) => "启动配置无法读取或不符合当前版本要求。请检查下方配置路径，或重新安装同一版本的完整程序。",
        LaunchError::ExecutableMissing { .. } => "安装目录缺少必需的程序文件。请重新安装完整程序；无需删除工作区或聊天数据。",
        LaunchError::WebConsoleSpawn(_) => "后台服务无法启动。请检查安装文件、日志目录的访问权限及系统安全软件提示。",
        LaunchError::HealthTimeout { .. } => "后台服务未在等待时间内就绪。请查看本次后台错误日志；不要通过删除会话数据来尝试恢复。",
        LaunchError::TauriSpawn(_) => "后台启动后，桌面窗口未能打开。请检查桌面程序文件及 WebView2 环境。",
        LaunchError::Persistence(_) => "无法写入启动日志或自检文件。请检查安装配置中日志目录的访问权限。",
        LaunchError::WorkspaceUnavailable { .. } => "所选工作区目前不可访问。请确认目录或磁盘可用，然后重新选择需要使用的工作区。",
        LaunchError::WorkspaceSelectionRequired { .. } | LaunchError::WorkspaceSelectionAmbiguous { .. } => "需要明确选择工作区后才能启动。程序没有替您选择、合并或删除现有工作区；请根据启动日志中的候选路径进行选择。",
        LaunchError::SelectionStore { .. } => "已保存的工作区选择无法读取。请保留选择文件与日志，按日志提示恢复启动选择。",
        LaunchError::SelectionConflict { .. } | LaunchError::SelectionLocked { .. } => "另一个启动器正在更新工作区选择。请关闭重复启动窗口后重试，现有选择已保留。",
        LaunchError::ServiceConflict { .. } => "已有后台服务与本次选择的工作区或版本不一致。请关闭原应用后重试；程序没有连接到错误工作区。",
    }.into()
}

fn number_after(text: &str, marker: &str) -> Option<u64> {
    let value: String = text
        .split_once(marker)?
        .1
        .chars()
        .take_while(char::is_ascii_digit)
        .take(20)
        .collect();
    value.parse().ok()
}

/// 消息只作为子进程环境值传递，不拼接为 PowerShell 代码。
#[cfg(windows)]
pub(super) fn show_message(message: &str) {
    use std::process::{Command, Stdio};
    let script = "Add-Type -AssemblyName System.Windows.Forms; [void][System.Windows.Forms.MessageBox]::Show($env:COOLZHU_STARTUP_FAILURE_MESSAGE, 'Coolzhu Agent - 启动失败', [System.Windows.Forms.MessageBoxButtons]::OK, [System.Windows.Forms.MessageBoxIcon]::Error)";
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .env("COOLZHU_STARTUP_FAILURE_MESSAGE", message)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    super::apply_hidden_window(&mut command);
    match command.status() {
        Ok(status) if status.success() => {}
        _ => eprintln!("package-launcher: 系统无法显示启动错误窗口；请查看上述终端诊断。"),
    }
}

#[cfg(not(windows))]
pub(super) fn show_message(_message: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_hint_uses_fixed_marker_and_never_copies_raw_log() {
        let error = LaunchError::HealthTimeout {
            url: "http://localhost/health".into(),
            waited_secs: 2,
        };
        let message = failure_reason(&error, Some("unrelated secret=never-show\n会话库 schema 比本二进制更新：user_version=26，本构建只支持到 19；拒绝"));
        assert!(message.contains("数据版本 26，当前程序支持到 19"));
        assert!(message.contains("请勿删除会话库"));
        assert!(!message.contains("never-show"));
        assert!(!failure_reason(&error, None).contains("数据版本"));
    }

    #[test]
    fn stale_log_is_not_used_for_a_new_attempt() {
        let path = std::env::temp_dir().join(format!(
            "coolzhu-launch-diagnostic-{}.log",
            std::process::id()
        ));
        std::fs::write(&path, "会话库 schema 比本二进制更新").unwrap();
        let future = SystemTime::now() + std::time::Duration::from_secs(60);
        assert!(read_current_attempt_tail(&path, future).is_none());
        std::fs::remove_file(path).unwrap();
    }
}
