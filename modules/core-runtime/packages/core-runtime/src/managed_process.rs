//! 同步工具的窄执行控制。调用者持有取消令牌，进程执行器负责退出与输出回收。
use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{atomic::{AtomicBool, AtomicUsize, Ordering}, Arc};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct ExecutionControl {
    cancelled: Arc<AtomicBool>,
    deadline: Option<Instant>,
    external_cancel: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    capture_budget: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interruption { Cancelled, TimedOut }

impl ExecutionControl {
    #[must_use]
    pub fn new(timeout: Option<Duration>, external_cancel: Option<Arc<dyn Fn() -> bool + Send + Sync>>) -> Self {
        Self { cancelled: Arc::new(AtomicBool::new(false)),
            deadline: timeout.and_then(|timeout| Instant::now().checked_add(timeout)), external_cancel,
            capture_budget: None }
    }

    pub fn cancel(&self) { self.cancelled.store(true, Ordering::Release); }

    /// 追加资格撤销信号，保留已有取消令牌和单调截止；不能创建新的业务预算。
    #[must_use]
    pub fn with_additional_cancel(mut self, check: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        let previous = self.external_cancel.take();
        self.external_cancel = Some(Arc::new(move || previous.as_ref().is_some_and(|old| old()) || check()));
        self
    }

    #[must_use]
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    #[must_use]
    pub fn interruption(&self) -> Option<Interruption> {
        if self.cancelled.load(Ordering::Acquire)
            || self.external_cancel.as_ref().is_some_and(|check| check()) {
            Some(Interruption::Cancelled)
        } else if self.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            Some(Interruption::TimedOut)
        } else { None }
    }

    /// 本次执行的采集预算字节上限；不传时读取 COOLZHU_CAPTURE_BUDGET_BYTES，
    /// 再回落到默认上限。按执行注入，避免并行测试共享环境变量产生竞态。
    #[cfg(test)]
    #[must_use]
    pub fn with_capture_budget(mut self, bytes: usize) -> Self {
        self.capture_budget = Some(bytes);
        self
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
    output: Output,
    interruption: Option<Interruption>,
    /// 全局采集预算记账；随本结构释放归还。字段私有使结果 Vec 不能被整体移出，
    /// 调用方经由只读引用访问；自行 clone/to_vec 的副本属消费者新分配，不计入采集预算。
    _capture: Arc<CaptureCharge>,
}

impl ManagedOutput {
    /// 采集结果只读视图；Vec 不能移出，保证返回字节在结构生命周期内始终计费。
    #[must_use]
    pub fn output(&self) -> &Output { &self.output }
    #[must_use]
    pub fn interruption(&self) -> Option<Interruption> { self.interruption }
}

/// 子进程已经启动，但未能完整回收结果；不能解释为未执行或正常协议结果。
#[derive(Debug)]
struct StartedProcessError(io::Error);

impl std::fmt::Display for StartedProcessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "进程已启动，执行或输出回收失败：{}", self.0)
    }
}

impl std::error::Error for StartedProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> { Some(&self.0) }
}

#[must_use]
pub fn process_started_before_error(error: &io::Error) -> bool {
    error.get_ref().is_some_and(|inner| inner.is::<StartedProcessError>())
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

/// 有独立协议的子进程仍复用同一 Job、根取消与截止控制；不把 stdout 当协议。
/// 调用者必须轮询 interruption，先通知协议取消再有界收尾；Drop 始终关闭子树。
pub struct ProtocolChild {
    owned: OwnedChild,
    control: ExecutionControl,
}

impl ProtocolChild {
    pub fn spawn(command: &mut Command, timeout: Duration) -> io::Result<Self> {
        let control = current_execution_control()
            .unwrap_or_else(|| ExecutionControl::new(None, None)).with_timeout(Some(timeout));
        if control.interruption().is_some() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "执行已取消或截止，未启动协议进程"));
        }
        #[cfg(windows)]
        let (child, job) = windows_process_guard::ChildProcessJob::spawn_managed(command)?;
        #[cfg(not(windows))]
        let child = command.spawn()?;
        Ok(Self { owned: OwnedChild { child, #[cfg(windows)] job: Some(job) }, control })
    }

    #[must_use]
    pub fn interruption(&self) -> Option<Interruption> { self.control.interruption() }

    pub fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
        self.owned.child.try_wait()
    }

    /// 包括仍在运行的孙进程；返回前回收直接子进程，不把发出 kill 当作已退出。
    pub fn stop(&mut self) -> io::Result<std::process::ExitStatus> { self.owned.stop() }
}

/// 单根同步结果管道的硬上限；超限是执行失败，不返回可被当作完整结果的前缀。
pub const MAX_CAPTURE_BYTES_PER_PIPE: usize = 8 * 1024 * 1024;

/// 所有并发执行共享的【已捕获载荷字节】配额上限；stdout/stderr 与已返回未释放的
/// `ManagedOutput` 共用同一额度。按每次 read 的 count 计费：分配器余量、Vec 空余
/// capacity 与消费者 clone/to_vec 副本均不属于本配额，故这不是总 RAM 硬限。
/// env `COOLZHU_CAPTURE_BUDGET_BYTES` 可调整默认上限。
const DEFAULT_CAPTURE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
static CAPTURE_BUDGET_USED: AtomicUsize = AtomicUsize::new(0);

fn capture_budget_limit() -> usize {
    std::env::var("COOLZHU_CAPTURE_BUDGET_BYTES")
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_CAPTURE_BUDGET_BYTES)
}

fn capture_budget_acquire(bytes: usize, limit: usize) -> bool {
    let mut used = CAPTURE_BUDGET_USED.load(Ordering::Acquire);
    loop {
        let Some(next) = used.checked_add(bytes).filter(|next| *next <= limit) else {
            return false;
        };
        match CAPTURE_BUDGET_USED.compare_exchange_weak(used, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return true,
            Err(observed) => used = observed,
        }
    }
}

fn capture_budget_release(bytes: usize) {
    CAPTURE_BUDGET_USED.fetch_sub(bytes, Ordering::AcqRel);
}

/// 一次执行在两管道上已捕获载荷字节数；最后一个引用（正常时是 `ManagedOutput`，
/// 出错/panic 时是本作用域）丢弃时归还全局额度。
struct CaptureCharge {
    bytes: AtomicUsize,
}

impl Drop for CaptureCharge {
    fn drop(&mut self) {
        capture_budget_release(*self.bytes.get_mut());
    }
}

fn read_pipe(mut pipe: impl Read, failed: Arc<AtomicBool>, charge: Arc<CaptureCharge>, limit: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let result = (|| {
        let mut chunk = [0_u8; 8192];
        loop {
            let count = match pipe.read(&mut chunk) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => result?,
            };
            if count == 0 { return Ok(bytes); }
            if count > MAX_CAPTURE_BYTES_PER_PIPE - bytes.len() {
                return Err(io::Error::new(io::ErrorKind::InvalidData,
                    format!("同步进程输出超限：单管道上限 {MAX_CAPTURE_BYTES_PER_PIPE} 字节，未取得完整结果")));
            }
            // 配额口径为已捕获载荷字节：本次 count 先原子申请成功后才复制入 Vec；
            // 额度随 ManagedOutput 生命周期持有，Vec 分配器余量不计费（见上限注释）。
            if !capture_budget_acquire(count, limit) {
                return Err(io::Error::new(io::ErrorKind::InvalidData,
                    format!("同步进程输出超限：全局采集预算 {limit} 字节已用尽，未取得完整结果")));
            }
            charge.bytes.fetch_add(count, Ordering::AcqRel);
            bytes.extend_from_slice(&chunk[..count]);
        }
    })();
    if result.is_err() { failed.store(true, Ordering::Release); }
    result
}

/// Windows 命令在执行首条指令前进入 Job；超时、取消或宿主退出均关闭整个子树。
/// 非 Windows 当前保证直接子进程回收，不宣称具有 Windows Job 的子树隔离能力。
pub fn output(command: &mut Command, timeout: Option<Duration>) -> io::Result<ManagedOutput> {
    output_with_input(command, timeout, None)
}

/// 插件协议同时通过环境变量和 stdin 交付 JSON；写入由独立线程持有，取消时随受控子树关闭。
pub fn output_with_input(command: &mut Command, timeout: Option<Duration>, input: Option<Vec<u8>>) -> io::Result<ManagedOutput> {
    output_with_input_and_integrity(command, timeout, input, false)
}

/// 显式 Windows MIC 写边界；不支持的平台及令牌设置失败均拒绝，不自动降级。
pub fn output_low_integrity(command: &mut Command, timeout: Option<Duration>) -> io::Result<ManagedOutput> {
    output_with_input_and_integrity(command, timeout, None, true)
}

fn output_with_input_and_integrity(command: &mut Command, timeout: Option<Duration>, input: Option<Vec<u8>>, low_integrity: bool) -> io::Result<ManagedOutput> {
    #[cfg(not(windows))]
    if low_integrity { return Err(io::Error::new(io::ErrorKind::Unsupported, "低完整性写隔离仅支持 Windows")); }
    let control = CURRENT.with(|slot| slot.borrow().clone())
        .unwrap_or_else(|| ExecutionControl::new(None, None)).with_timeout(timeout);
    if control.interruption().is_some() {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "执行已取消或截止，未启动进程"));
    }
    command.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    let (child, job) = if low_integrity {
        windows_process_guard::ChildProcessJob::spawn_managed_low_integrity(command)?
    } else { windows_process_guard::ChildProcessJob::spawn_managed(command)? };
    #[cfg(not(windows))]
    let child = command.spawn()?;
    let mut owned = OwnedChild { child, #[cfg(windows)] job: Some(job) };
    let result = (|| {
    let stdin_writer = input.map(|bytes| {
        let mut stdin = owned.child.stdin.take().expect("stdin 配置为管道");
        std::thread::spawn(move || stdin.write_all(&bytes))
    });
    // 必须在等待进程退出前同时读取两个 pipe，否则较大输出会阻塞子进程。
    let stdout = owned.child.stdout.take().expect("stdout 配置为管道");
    let stderr = owned.child.stderr.take().expect("stderr 配置为管道");
    let capture_failed = Arc::new(AtomicBool::new(false));
    let capture_charge = Arc::new(CaptureCharge { bytes: AtomicUsize::new(0) });
    let stdout_failed = capture_failed.clone();
    let stderr_failed = capture_failed.clone();
    let stdout_charge = capture_charge.clone();
    let stderr_charge = capture_charge.clone();
    let limit = control.capture_budget.unwrap_or_else(capture_budget_limit);
    let stdout_reader = std::thread::spawn(move || read_pipe(stdout, stdout_failed, stdout_charge, limit));
    let stderr_reader = std::thread::spawn(move || read_pipe(stderr, stderr_failed, stderr_charge, limit));
    let wait_result: io::Result<_> = (|| { loop {
        if capture_failed.load(Ordering::Acquire) {
            break Ok((owned.stop()?, None));
        }
        if let Some(status) = owned.child.try_wait()? {
            // 主进程结束后仍关闭所属子树，使继承了 pipe 的子进程不能拖住读线程。
            let _ = owned.stop()?;
            break Ok((status, None));
        }
        if let Some(reason) = control.interruption() {
            break Ok((owned.stop()?, Some(reason)));
        }
        std::thread::sleep(Duration::from_millis(10));
    } })();
    if wait_result.is_err() {
        // 等待失败也要先关闭所属子树，再回收所有管道线程。
        let _ = owned.stop();
    }
    let read_result = join_output_readers(stdout_reader, stderr_reader);
    let write_result = stdin_writer.map(|writer| writer.join()
        .map_err(|_| io::Error::other("stdin 写入线程异常")).and_then(|result| result));
    // 全部 join 完成后才传播错误，不能让另一根管道的采集线程脱离本次执行。
    let (status, interruption) = wait_result?;
    let (stdout, stderr) = read_result?;
    if let Some(write_result) = write_result {
        // 受控中止会关闭 pipe；预期 BrokenPipe 不得抹掉已确认的取消/超时事实。
        if interruption.is_none() { write_result?; }
    }
    Ok(ManagedOutput { output: Output { status, stdout, stderr }, interruption, _capture: capture_charge })
    })();
    result.map_err(|error: io::Error| io::Error::new(error.kind(), StartedProcessError(error)))
}

fn join_output_readers(
    stdout: std::thread::JoinHandle<io::Result<Vec<u8>>>,
    stderr: std::thread::JoinHandle<io::Result<Vec<u8>>>,
) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let stdout = stdout.join().map_err(|_| io::Error::other("stdout 读取线程异常"));
    let stderr = stderr.join().map_err(|_| io::Error::other("stderr 读取线程异常"));
    Ok((stdout??, stderr??))
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

    #[test]
    fn additional_revocation_preserves_parent_cancel_and_deadline() {
        let parent_cancel = Arc::new(AtomicBool::new(false));
        let observed = parent_cancel.clone();
        let parent = ExecutionControl::new(Some(Duration::from_secs(20)),
            Some(Arc::new(move || observed.load(Ordering::Acquire))));
        let derived = parent.clone().with_additional_cancel(Arc::new(|| false));
        assert_eq!(derived.deadline, parent.deadline, "追加停用检查不能重置根截止");
        parent_cancel.store(true, Ordering::Release);
        assert_eq!(derived.interruption(), Some(Interruption::Cancelled));
        parent_cancel.store(false, Ordering::Release);
        parent.cancel();
        assert_eq!(derived.interruption(), Some(Interruption::Cancelled), "原显式取消令牌必须共享");
        let revoked = ExecutionControl::new(None, None).with_additional_cancel(Arc::new(|| true));
        assert_eq!(revoked.interruption(), Some(Interruption::Cancelled));
        let expired = ExecutionControl::new(Some(Duration::ZERO), None)
            .with_additional_cancel(Arc::new(|| false));
        assert_eq!(expired.interruption(), Some(Interruption::TimedOut));
        assert_eq!(expired.remaining(), Some(Duration::ZERO));
    }

    fn fixture(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::var_os("COOLZHU_PROCESS_TEST_ROOT").map_or_else(
            || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp/2026-09-26-completion/process-tests"),
            PathBuf::from,
        );
        let path = root.join(format!("{name}-{}-{unique}", std::process::id()));
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
    fn capture_limit_preserves_exact_boundary_and_rejects_excess() {
        let flag = Arc::new(AtomicBool::new(false));
        let bytes = vec![b'a'; MAX_CAPTURE_BYTES_PER_PIPE];
        let charge = Arc::new(CaptureCharge { bytes: AtomicUsize::new(0) });
        assert_eq!(read_pipe(std::io::Cursor::new(bytes.clone()), flag.clone(), charge.clone(), capture_budget_limit()).unwrap(), bytes);
        assert!(!flag.load(Ordering::Acquire));
        let error = read_pipe(std::io::Cursor::new(vec![b'b'; MAX_CAPTURE_BYTES_PER_PIPE + 1]), flag.clone(), charge.clone(), capture_budget_limit()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(flag.load(Ordering::Acquire));
    }

    /// 分段返回的 reader：每次 read 只交付一段，覆盖 Vec 已有空余 capacity 的
    /// 场景；配额按载荷字节计费，与 Vec 扩容策略无关。
    struct StagedReader { chunks: std::collections::VecDeque<Vec<u8>> }
    impl Read for StagedReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let Some(chunk) = self.chunks.pop_front() else { return Ok(0) };
            let count = chunk.len().min(buffer.len());
            buffer[..count].copy_from_slice(&chunk[..count]);
            if count < chunk.len() { self.chunks.push_front(chunk[count..].to_vec()); }
            Ok(count)
        }
    }

    #[test]
    fn payload_budget_charges_each_read_before_copy_and_releases_on_error() {
        // 4096 + 5000 两段：预算 9000 恰好容纳第一段；第二段在分配/复制前被拒，
        // 且第一段计费由 charge 归还——计数器最终归零。
        let staged = StagedReader { chunks: [vec![b'a'; 4096], vec![b'b'; 5000]].into() };
        let charge = Arc::new(CaptureCharge { bytes: AtomicUsize::new(0) });
        let failed = Arc::new(AtomicBool::new(false));
        let error = read_pipe(staged, failed.clone(), charge.clone(), 9000).unwrap_err();
        assert!(error.to_string().contains("输出超限"));
        assert!(failed.load(Ordering::Acquire));
        assert_eq!(charge.bytes.load(Ordering::Acquire), 4096, "仅第一段计费且未归还");
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 4096);
        drop(charge);
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 0, "charge 丢弃必须归还全局额度");
        // 同预算下完整交付可复用；charge 持有者（生产路径为 ManagedOutput）在，
        // 额度就持续计费——即使 Vec 先释放，也按所有者生命周期归还。
        let charge2 = Arc::new(CaptureCharge { bytes: AtomicUsize::new(0) });
        let ok = read_pipe(StagedReader { chunks: [vec![b'c'; 9000]].into() },
            Arc::new(AtomicBool::new(false)), charge2.clone(), 9000).unwrap();
        assert_eq!(ok.len(), 9000);
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 9000);
        drop(ok);
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 9000, "所有者未释放则额度不还");
        drop(charge2);
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 0);
    }

    #[test]
    fn excessive_output_stops_both_stdout_and_stderr_producers() {
        for stream in ["Out", "Error"] {
            let directory = fixture("output-limit");
            let late = directory.join("late.txt");
            let script = format!("[Console]::{stream}.Write(('x' * {})); Start-Sleep -Seconds 4; Set-Content -LiteralPath {} late",
                MAX_CAPTURE_BYTES_PER_PIPE + 8192, quoted(&late));
            let started = Instant::now();
            let error = output(&mut powershell(&script), Some(Duration::from_secs(15))).err().unwrap();
            assert!(process_started_before_error(&error));
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains("输出超限"));
            assert!(started.elapsed() < Duration::from_secs(4), "不能等整个命令自然结束才发现超限");
            std::thread::sleep(Duration::from_secs(4));
            assert!(!late.exists(), "超限后命令仍继续产生副作用");
        }
    }

    #[test]
    fn reader_failure_still_joins_the_other_pipe() {
        for panic_reader in [false, true] {
            let completed = Arc::new(AtomicBool::new(false));
            let observed = completed.clone();
            let stdout = std::thread::spawn(move || {
                if panic_reader { panic!("模拟采集线程异常"); }
                Err(io::Error::other("模拟 stdout 读取失败"))
            });
            let stderr = std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                observed.store(true, Ordering::Release);
                Ok(vec![b'b'])
            });
            assert!(join_output_readers(stdout, stderr).is_err());
            assert!(completed.load(Ordering::Acquire), "返回错误前必须收完另一根管道线程");
        }
    }

    #[test]
    fn failed_stdin_delivery_distinguishes_started_process_from_spawn_failure() {
        let mut command = Command::new("cmd.exe");
        command.args(["/C", "exit 0"]);
        let error = output_with_input(&mut command, Some(Duration::from_secs(10)),
            Some(vec![b'x'; 8 * 1024 * 1024])).err().expect("子进程未读入完整请求就退出，必须返回错误");
        assert!(process_started_before_error(&error));
        assert!(error.to_string().contains("进程已启动"));
        let directory = fixture("missing-executable");
        let mut missing = Command::new(directory.join("absent.exe"));
        let not_started = output(&mut missing, Some(Duration::from_secs(10))).err().unwrap();
        assert!(!process_started_before_error(&not_started));
    }

    #[test]
    fn timeout_terminates_a_started_descendant_before_its_delayed_write() {
        // COOLZHU_PROCESS_TEST_ROOT 重定向夹具根；目录名含空格，验证含空格路径下的孙进程写文件。
        let directory = fixture("descendant space");
        let started = directory.join("started.txt");
        let late = directory.join("late.txt");
        // 孙进程用 cmd 而非第二层 powershell，缩短启动链路以降低超时先于孙进程启动的概率；
        // 路径经环境变量传值且变量值自带引号，命令行不含字面引号，含空格目录不受转义影响。
        let quoted_path = |path: &std::path::Path| format!("\"{}\"", path.to_string_lossy().trim_start_matches(r"\\?\").replace('\'', "''"));
        let script = format!("$env:CZ_STARTED_Q = '{}'; $env:CZ_LATE_Q = '{}'; Start-Process cmd.exe -ArgumentList @('/C','echo ready>%CZ_STARTED_Q% & ping -n 11 127.0.0.1 >nul & echo late>%CZ_LATE_Q%') -WindowStyle Hidden; Start-Sleep -Seconds 30", quoted_path(&started), quoted_path(&late));
        let actual = output(&mut powershell(&script), Some(Duration::from_secs(5))).unwrap();
        assert_eq!(actual.interruption, Some(Interruption::TimedOut));
        assert!(started.is_file(), "必须确认孙进程真实运行过，不能把未启动当成功");
        // 若孙进程未被杀死，late 约在 started 时刻+10s 出现；11s 睡眠使检查点晚于 超时5s+延迟10s。
        std::thread::sleep(Duration::from_secs(11));
        assert!(!late.exists(), "超时后孙进程仍执行了延迟写入");
    }

    #[test]
    fn global_capture_budget_is_shared_by_both_pipes_and_released_for_reuse() {
        // 两管道合计 120000 字节均远低于单管道 8MiB，但超过全局预算 100000 必须被拒；
        // 失败后额度归还，同预算下 60000 字节单管道可再次成功。
        let budget = ExecutionControl::new(None, None).with_capture_budget(100_000);
        let error = with_execution_control(budget.clone(), || output(
            &mut powershell("[Console]::Out.Write('a' * 60000); [Console]::Error.Write('b' * 60000)"),
            Some(Duration::from_secs(15)))).err().expect("全局预算超限必须报错");
        assert!(process_started_before_error(&error), "预算拒绝不能冒充未启动");
        assert!(error.to_string().contains("输出超限"));
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 0, "出错路径必须归还额度");
        let ok = with_execution_control(budget, || output(
            &mut powershell("[Console]::Out.Write('c' * 60000)"), Some(Duration::from_secs(15)))).unwrap();
        assert_eq!(ok.output().stdout.len(), 60000);
        drop(ok);
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 0, "返回结果释放后额度归零");
    }

    #[test]
    fn global_capture_budget_denies_only_one_of_two_concurrent_executions() {
        // 预算 100000 恰好容纳一路 60000 字节管道记账；两路并发时恰有一路被拒，
        // 证明申请不超量竞态；另一路受 Start-Sleep 保持，保证额度确实并发占用。
        let budget = || ExecutionControl::new(None, None).with_capture_budget(100_000);
        let first = std::thread::spawn(move || with_execution_control(budget(), || output(
            &mut powershell("[Console]::Out.Write('x' * 60000); Start-Sleep -Seconds 4"),
            Some(Duration::from_secs(15)))));
        std::thread::sleep(Duration::from_millis(300));
        let second = std::thread::spawn(move || with_execution_control(budget(), || output(
            &mut powershell("[Console]::Out.Write('x' * 60000); Start-Sleep -Seconds 4"),
            Some(Duration::from_secs(15)))));
        let results = [first.join().unwrap(), second.join().unwrap()];
        let passed = results.iter().filter(|r| r.is_ok()).count();
        let denied = results.iter().filter(|r| r.is_err()).count();
        assert_eq!((passed, denied), (1, 1), "预算只允许一路并发执行打满记账");
        let error = results.into_iter().find_map(|r| r.err()).unwrap();
        assert!(process_started_before_error(&error));
        assert!(error.to_string().contains("输出超限"));
        assert_eq!(CAPTURE_BUDGET_USED.load(Ordering::Acquire), 0);
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
