//! 同步工具的窄执行控制。调用者持有取消令牌，进程执行器负责退出与输出回收。
use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct ExecutionControl {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
    external_cancel: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interruption { Cancelled, TimedOut }

impl ExecutionControl {
    #[must_use]
    pub fn new(timeout: Option<Duration>, external_cancel: Option<Arc<dyn Fn() -> bool + Send + Sync>>) -> Self {
        Self { cancelled: Arc::new(AtomicBool::new(false)),
            deadline: timeout.and_then(|timeout| Instant::now().checked_add(timeout)), external_cancel }
    }

    pub fn cancel(&self) { self.cancelled.store(true, Ordering::Release); }

    #[must_use]
    pub fn interruption(&self) -> Option<Interruption> {
        if self.cancelled.load(Ordering::Acquire)
            || self.external_cancel.as_ref().is_some_and(|check| check()) {
            Some(Interruption::Cancelled)
        } else if self.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            Some(Interruption::TimedOut)
        } else { None }
    }

    fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        if let Some(deadline) = timeout.and_then(|timeout| Instant::now().checked_add(timeout)) {
            self.deadline = Some(self.deadline.map_or(deadline, |outer| outer.min(deadline)));
        }
        self
    }
}

thread_local! { static CURRENT: RefCell<Option<ExecutionControl>> = const { RefCell::new(None) }; }

/// 异步协议适配器在进入请求时冻结已有根取消/时限；不创建新的权限或预算。
#[must_use]
pub fn current_execution_control() -> Option<ExecutionControl> { CURRENT.with(|slot| slot.borrow().clone()) }

/// 上下文仅在本次同步调用所在的线程生效；退出或 panic 恢复之前的上下文。
pub fn with_execution_control<T>(control: ExecutionControl, execute: impl FnOnce() -> T) -> T {
    struct Restore(Option<ExecutionControl>);
    impl Drop for Restore { fn drop(&mut self) { CURRENT.with(|slot| *slot.borrow_mut() = self.0.take()); } }
    let _restore = Restore(CURRENT.with(|slot| slot.replace(Some(control))));
    execute()
}

pub struct ManagedOutput {
    pub output: Output,
    pub interruption: Option<Interruption>,
}

struct OwnedChild {
    child: Child,
    #[cfg(windows)]
    job: Option<windows_process_guard::ChildProcessJob>,
}

impl OwnedChild {
    fn stop(&mut self) -> io::Result<std::process::ExitStatus> {
        // 先关闭 Job，子孙进程一并退出，随后回收主进程和输出；不能只杀 shell。
        #[cfg(windows)]
        self.job.take();
        let _ = self.child.kill();
        self.child.wait()
    }
}

impl Drop for OwnedChild { fn drop(&mut self) { let _ = self.stop(); } }

fn read_pipe(mut pipe: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Windows 命令在执行首条指令前进入 Job；超时、取消或宿主退出均关闭整个子树。
/// 非 Windows 当前保证直接子进程回收，不宣称具有 Windows Job 的子树隔离能力。
pub fn output(command: &mut Command, timeout: Option<Duration>) -> io::Result<ManagedOutput> {
    output_with_input(command, timeout, None)
}

/// 插件协议同时通过环境变量和 stdin 交付 JSON；写入由独立线程持有，取消时随受控子树关闭。
pub fn output_with_input(command: &mut Command, timeout: Option<Duration>, input: Option<Vec<u8>>) -> io::Result<ManagedOutput> {
    let control = CURRENT.with(|slot| slot.borrow().clone())
        .unwrap_or_else(|| ExecutionControl::new(None, None)).with_timeout(timeout);
    if control.interruption().is_some() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "执行已取消或截止，未启动进程"));
    }
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    let (child, job) = windows_process_guard::ChildProcessJob::spawn_managed(command)?;
    #[cfg(not(windows))]
    let child = command.spawn()?;
    let mut owned = OwnedChild { child, #[cfg(windows)] job: Some(job) };
    let stdin_writer = input.map(|bytes| {
        let mut stdin = owned.child.stdin.take().expect("stdin 配置为管道");
        std::thread::spawn(move || stdin.write_all(&bytes))
    });
    // 必须在等待进程退出前同时读取两个 pipe，否则较大输出会阻塞子进程。
    let stdout = owned.child.stdout.take().expect("stdout 配置为管道");
    let stderr = owned.child.stderr.take().expect("stderr 配置为管道");
    let stdout_reader = std::thread::spawn(move || read_pipe(stdout));
    let stderr_reader = std::thread::spawn(move || read_pipe(stderr));
    let (status, interruption) = loop {
        if let Some(status) = owned.child.try_wait()? {
            // 主进程结束后仍关闭所属子树，使继承了 pipe 的子进程不能拖住读线程。
            let _ = owned.stop()?;
            break (status, None);
        }
        if let Some(reason) = control.interruption() {
            break (owned.stop()?, Some(reason));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = stdout_reader.join().map_err(|_| io::Error::other("stdout 读取线程异常"))??;
    let stderr = stderr_reader.join().map_err(|_| io::Error::other("stderr 读取线程异常"))??;
    if let Some(writer) = stdin_writer {
        let write_result = writer.join().map_err(|_| io::Error::other("stdin 写入线程异常"))?;
        // 受控中止会关闭 pipe；预期 BrokenPipe 不得抹掉已确认的取消/超时事实。
        if interruption.is_none() { write_result?; }
    }
    Ok(ManagedOutput { output: Output { status, stdout, stderr }, interruption })
}

/// 工具模块不能把中文控制台输出直接按 UTF-8 有损解码。
#[must_use]
pub fn decode_console_output(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) { return text.to_owned(); }
    let (text, _, errors) = encoding_rs::GB18030.decode(bytes);
    if errors { String::from_utf8_lossy(bytes).into_owned() } else { text.into_owned() }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../tmp/2026-09-26-completion/process-tests")
            .join(format!("{name}-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path.canonicalize().unwrap()
    }

    fn powershell(script: &str) -> Command {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        command
    }

    fn quoted(path: &std::path::Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "''"))
    }

    #[test]
    fn large_stdout_and_stderr_are_drained_before_waiting_for_exit() {
        let mut command = powershell("[Console]::Out.Write(('a' * 200000)); [Console]::Error.Write(('b' * 200000))");
        let actual = output(&mut command, Some(Duration::from_secs(10))).unwrap();
        assert_eq!(actual.interruption, None);
        assert!(actual.output.status.success());
        assert_eq!(actual.output.stdout.len(), 200_000);
        assert_eq!(actual.output.stderr.len(), 200_000);
    }

    #[test]
    fn timeout_terminates_a_started_descendant_before_its_delayed_write() {
        let directory = fixture("descendant");
        let started = directory.join("started.txt");
        let late = directory.join("late.txt");
        let child = format!("Set-Content -LiteralPath {} ready; Start-Sleep -Seconds 4; Set-Content -LiteralPath {} late", quoted(&started), quoted(&late));
        let script = format!("$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes('{}')); Start-Process powershell.exe -ArgumentList @('-NoProfile','-NonInteractive','-EncodedCommand',$encoded) -WindowStyle Hidden; Start-Sleep -Seconds 30", child.replace('\'', "''"));
        let actual = output(&mut powershell(&script), Some(Duration::from_secs(3))).unwrap();
        assert_eq!(actual.interruption, Some(Interruption::TimedOut));
        assert!(started.is_file(), "必须确认孙进程真实运行过，不能把未启动当成功");
        std::thread::sleep(Duration::from_secs(3));
        assert!(!late.exists(), "超时后孙进程仍执行了延迟写入");
    }

    #[test]
    fn external_cancel_stops_an_already_started_process() {
        let directory = fixture("cancel");
        let started = directory.join("started.txt");
        let late = directory.join("late.txt");
        let control = ExecutionControl::new(Some(Duration::from_secs(20)), None);
        let captured = control.clone();
        let script = format!("Set-Content -LiteralPath {} ready; Start-Sleep -Seconds 4; Set-Content -LiteralPath {} late", quoted(&started), quoted(&late));
        let worker = std::thread::spawn(move || with_execution_control(captured, || output(&mut powershell(&script), None)));
        let wait_until = Instant::now() + Duration::from_secs(8);
        while !started.exists() && Instant::now() < wait_until { std::thread::sleep(Duration::from_millis(20)); }
        control.cancel();
        let actual = worker.join().unwrap().unwrap();
        assert!(started.exists(), "必须取消已经执行的进程");
        assert_eq!(actual.interruption, Some(Interruption::Cancelled));
        std::thread::sleep(Duration::from_secs(5));
        assert!(!late.exists(), "取消后仍执行了延迟写入");
    }
}
