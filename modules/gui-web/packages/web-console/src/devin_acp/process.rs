//! 进程树监督与协议服务的所有权分离；只能在真实排空后解除确定终态的锁。
use super::{journal::Journal, protocol::ExecutionScope, session::SessionTransport};
use std::{io, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
    task::JoinHandle,
};

pub(super) struct ManagedProcess {
    child: Child,
    #[cfg(windows)]
    job: Option<windows_process_guard::ChildProcessJob>,
    stderr: JoinHandle<io::Result<()>>,
    journal: Journal,
    scope: ExecutionScope,
    drained: bool,
}

impl ManagedProcess {
    /// 经过固定版本/文件核对后的纯文本入口；不能传入任意 CLI 参数。
    pub(super) async fn spawn_text_cli(
        binary: &Path, cwd: &Path, config: &Path, model: &str,
        journal: Journal, scope: ExecutionScope,
    ) -> Result<(Self, SessionTransport<ChildStdin>), String> {
        if !config.is_absolute() || !super::chat::valid_model_id(model) {
            return Err("文本 CLI 配置或模型不受支持。".into());
        }
        let mut command = tokio::process::Command::new(binary);
        command.args(["--config", config.to_str().ok_or("配置路径无效。")?,
            "--permission-mode", "auto", "acp", "--model", model]);
        // 只保留运行/官方凭据定位所需环境，不继承模型覆盖、付费回退、Hook 或外部密钥。
        command.env_clear();
        for name in ["SystemRoot","WINDIR","USERPROFILE","APPDATA","LOCALAPPDATA","PROGRAMDATA","TEMP","TMP"] {
            if let Some(value) = std::env::var_os(name) { command.env(name,value); }
        }
        if let Some(system) = std::env::var_os("SystemRoot") {
            command.env("PATH", Path::new(&system).join("System32"));
        }
        Self::spawn_command(command,cwd,journal,scope).await
    }
    /// 正式入口不能通过环境变量开启。固定原生 CLI 的配置和旁路尚待验收。
    pub(super) async fn spawn_cli(
        _binary: &Path,
        _cwd: &Path,
        _journal: Journal,
        _scope: ExecutionScope,
    ) -> Result<(Self, SessionTransport<ChildStdin>), String> {
        Err("Devin 原生 CLI 的配置隔离与内建工具旁路尚未验收，未启动生成进程。".into())
    }

    /// 仅测试构建可用：进程 fixture 与显式授权的真实账号 smoke；正式入口没有此构造器。
    #[cfg(test)]
    pub(super) async fn spawn_fixture(
        binary: &Path,
        args: &[&str],
        cwd: &Path,
        journal: Journal,
        scope: ExecutionScope,
    ) -> Result<(Self, SessionTransport<ChildStdin>), String> {
        let mut command = tokio::process::Command::new(binary);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // 真实 smoke 不继承模型覆盖和付费回退，也不复用外部 API 密钥。
        for name in ["DEVIN_MODEL", "DEVIN_REFUSAL_FALLBACK", "WINDSURF_API_KEY", "DEVIN_PERMISSION_MODE", "DEVIN_SANDBOX"] {
            command.env_remove(name);
        }
        Self::spawn_command(command,cwd,journal,scope).await
    }

    async fn spawn_command(mut command: tokio::process::Command, cwd:&Path, journal:Journal, scope:ExecutionScope)
        -> Result<(Self,SessionTransport<ChildStdin>),String> {
        command.current_dir(cwd).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        #[cfg(windows)]
        let (mut child, job) =
            windows_process_guard::ChildProcessJob::spawn_managed_async(&mut command)
                .map_err(|_| "测试进程启动失败。")?;
        #[cfg(not(windows))]
        let mut child = command.spawn().map_err(|_| "测试进程启动失败。")?;
        let input = child.stdin.take().ok_or("进程输入不可用。")?;
        let output: ChildStdout = child.stdout.take().ok_or("进程输出不可用。")?;
        let mut errors = child.stderr.take().ok_or("进程错误流不可用。")?;
        let stderr = tokio::spawn(async move {
            // 持续排空且不保存原始错误，避免填满管道或记录登录秘密。
            let mut buffer = [0; 4096];
            let mut total = 0usize;
            loop {
                let count = errors.read(&mut buffer).await?;
                if count == 0 {
                    return Ok(());
                }
                total = total.saturating_add(count);
                if total > 4 * 1024 * 1024 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "CLI 错误流超限。",
                    ));
                }
            }
        });
        Ok((
            Self {
                child,
                #[cfg(windows)]
                job: Some(job),
                stderr,
                journal,
                scope,
                drained: false,
            },
            SessionTransport::new(BufReader::new(output), input),
        ))
    }

    pub(super) async fn drain(&mut self) -> Result<(), String> {
        if self.drained {
            return Ok(());
        }
        #[cfg(windows)]
        {
            let job = self.job.take().ok_or("ACP 进程树监督句柄缺失。")?;
            tokio::task::spawn_blocking(move || job.terminate_and_wait(Duration::from_secs(3)))
                .await
                .map_err(|_| "ACP 进程树监督任务异常。")?
                .map_err(|_| "ACP 进程树未确认排空，会话锁保留。")?;
        }
        #[cfg(not(windows))]
        {
            // 当前正式环境是原生 Windows；其它平台尚未实现树级回执，不能宣布排空。
            let _ = self.child.start_kill();
            return Err("当前平台尚无 ACP 进程树排空回执，会话锁保留。".into());
        }
        #[cfg(windows)]
        {
            tokio::time::timeout(Duration::from_secs(2), self.child.wait())
                .await
                .map_err(|_| "ACP 主进程尚未回收。")?
                .map_err(|_| "ACP 主进程回收失败。")?;
            self.stderr.abort();
            self.journal.record_drained(&self.scope)?;
            self.drained = true;
            Ok(())
        }
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        self.stderr.abort();
        if !self.drained {
            let _ = self.child.start_kill();
            // Drop 的停止意图不构成排空或取消确认，也不解除 unknown 锁。
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::super::journal::tests::{scope, setup};
    use super::*;
    #[tokio::test]
    async fn job_tree_is_really_drained_before_attempt_unlock() {
        let (dir, journal, binding) = setup();
        let claim = journal.claim(scope("p1"), &binding).unwrap();
        journal
            .transition(&claim, &["prepared"], "submitted", None, true)
            .unwrap();
        let shell = Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        // 后台子进程同样进入 Job，父进程保持存活；仅此临时测试创建这些进程。
        let child_script = "Start-Process powershell.exe -WindowStyle Hidden -ArgumentList '-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 60' -PassThru | Select-Object -ExpandProperty Id | Set-Content -LiteralPath child.pid; Start-Sleep -Seconds 60";
        let (mut process, transport) = ManagedProcess::spawn_fixture(
            shell,
            &["-NoProfile", "-NonInteractive", "-Command", child_script],
            dir.path(),
            journal.clone(),
            claim.clone(),
        )
        .await
        .unwrap();
        let parent_pid = process.child.id().unwrap();
        let pid_file = dir.path().join("child.pid");
        // 仅等待测试辅助 PowerShell 的启动确认；CI 冷启动可能超过 5 秒。
        // 不修改正式调用期限，也不放宽下面的进程树排空与解锁断言。
        tokio::time::timeout(Duration::from_secs(20), async {
            while !pid_file.exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        journal
            .transition(&claim, &["submitted"], "terminal", Some("cancelled"), false)
            .unwrap();
        assert!(!journal.status(&claim).unwrap().process_drained);
        assert!(journal.claim(scope("p2"), &binding).is_err());
        process.drain().await.unwrap();
        drop(transport);
        assert!(process.child.try_wait().unwrap().is_some());
        assert!(journal.status(&claim).unwrap().process_drained);
        assert!(journal.claim(scope("p2"), &binding).is_ok());
        // Job ActiveProcesses==0 是整树证据，主 PID wait 单独核对，不用 tasklist 文本推断。
        assert!(parent_pid > 0);
    }
    #[tokio::test]
    async fn formal_cli_gate_cannot_be_bypassed_by_fixture_configuration() {
        let (dir, journal, binding) = setup();
        let claim = journal.claim(scope("p1"), &binding).unwrap();
        assert!(
            ManagedProcess::spawn_cli(
                Path::new("missing.exe"),
                dir.path(),
                journal.clone(),
                claim.clone()
            )
            .await
            .is_err()
        );
        assert_eq!(journal.status(&claim).unwrap().state, "prepared");
    }
}
