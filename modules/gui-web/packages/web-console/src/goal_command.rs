//! Goal 验证命令复用工具进程监督；等待者消失不会让命令失去取消与回收责任。
use std::{path::Path, process::Command, time::Duration};
use runtime::managed_process::{self, ExecutionControl, Interruption};

struct CancelOnDrop(ExecutionControl);
impl Drop for CancelOnDrop {
    fn drop(&mut self) { self.0.cancel(); }
}

pub(crate) async fn run(command: &str, cwd: &Path, timeout_ms: u64) -> Result<(), String> {
    let timeout_ms = crate::root_execution_budget::limit_current_ms(timeout_ms);
    if timeout_ms == 0 {
        return Err("Goal 验证命令未启动：执行时限已到".into());
    }
    let external_cancel = crate::current_turn_trace().as_deref()
        .map(crate::tool_turn_cancellation_checker);
    let control = ExecutionControl::new(Some(Duration::from_millis(timeout_ms)), external_cancel);
    let _cancel_on_drop = CancelOnDrop(control.clone());
    let command = command.to_owned();
    let cwd = cwd.to_owned();
    let worker = tokio::task::spawn_blocking(move || {
        managed_process::with_execution_control(control, || {
            let mut process = if cfg!(windows) {
                let mut process = Command::new("powershell.exe");
                process.args(["-NoProfile", "-NonInteractive", "-Command", &command]);
                process
            } else {
                let mut process = Command::new("sh");
                process.args(["-c", &command]);
                process
            };
            process.current_dir(cwd);
            #[cfg(windows)] {
                use std::os::windows::process::CommandExt;
                process.creation_flags(0x08000000);
            }
            let result = managed_process::output(&mut process, None)
                .map_err(|error| format!("`{command}` 未能完成：{error}"))?;
            if let Some(reason) = result.interruption {
                return Err(format!("`{command}` {}；所属进程已回收", match reason {
                    Interruption::TimedOut => "超时", Interruption::Cancelled => "已取消"
                }));
            }
            if result.output.status.success() { return Ok(()); }
            let code = result.output.status.code().unwrap_or(-1);
            let combined = format!("{}\n{}",
                managed_process::decode_console_output(&result.output.stderr),
                managed_process::decode_console_output(&result.output.stdout));
            let lines = combined.lines().collect::<Vec<_>>();
            let tail = lines[lines.len().saturating_sub(20)..].join("\n");
            Err(format!("`{command}` 失败（exit {code}）：\n{tail}"))
        })
    });
    let (sender, receiver) = tokio::sync::oneshot::channel();
    // 单独持有 JoinHandle，Goal/HTTP future 被取消时仍收取真实进程退出结果。
    tokio::spawn(async move {
        let result = worker.await.unwrap_or_else(|error| Err(format!("Goal 验证 worker 异常：{error}")));
        if let Err(late) = sender.send(result) {
            tracing::info!(success = late.is_ok(), "Goal 验证命令在等待者结束后完成收尾");
        }
    });
    receiver.await.map_err(|error| format!("Goal 验证监督回执丢失，结果未知：{error}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn gate_exit_and_expired_root_use_the_same_managed_path() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        assert!(run("exit 0", cwd, 30_000).await.is_ok());
        assert!(run("exit 1", cwd, 30_000).await.unwrap_err().contains("exit 1"));
        crate::root_execution_budget::scope(
            crate::root_execution_budget::RootExecutionBudget::from_started_at(1, 1),
            async { assert!(run("exit 0", cwd, 30_000).await.unwrap_err().contains("未启动")); }
        ).await;
    }
}
