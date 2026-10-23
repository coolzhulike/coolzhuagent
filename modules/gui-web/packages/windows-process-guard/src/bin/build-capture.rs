//! 构建输出原始字节采集；固定缓冲，不以截断日志冒充成功。
use std::{fs::{File, OpenOptions}, io::{self, Read, Write}, process::{Command, Stdio}, sync::mpsc, thread, time::{Duration, Instant}};
use coolzhu_windows_process_guard::{ChildProcessJob, ProcessLifetimeLease};

/// stdout+stderr 合并落盘的单次受管采集硬字节预算（两管道共享一个计数）。
/// 取值理由：产品内磁盘型日志最大额度是 diagnostics 单包总量 32MiB，构建采集允许
/// 其 2 倍（64MiB）应对真实构建日志；conpty 终端 1MiB / pipe 保留 256KiB 属内存型不适用。
/// 绝不无界；调用方可用 --output-budget <bytes> 显式收紧。
const DEFAULT_OUTPUT_BUDGET_BYTES: u64 = 64 * 1024 * 1024;
/// --output-budget 可指定值的绝对上限：再大即等同无界，fail-closed 拒绝。
const HARD_MAX_OUTPUT_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

fn pump(mut source: impl Read, mut target: File, _pipe: &str, spent: &std::sync::atomic::AtomicU64, limit: u64) -> io::Result<()> {
    let mut buffer = [0_u8; 64 * 1024];
    #[cfg(feature = "build-capture-test-support")]
    let mut written = 0;
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 { break; }
        // 两管道共享硬预算：CAS+checked_add 原子预留，越界或 u64 溢出都 fail-closed；
        // 只有预留成功的块才写盘，任意线程交错下写入总字节绝不超预算。
        // 超限块不落盘、立即失败——调用方据此回收 Job 根及后代并如实保留已写字节。
        let mut used = spent.load(std::sync::atomic::Ordering::Relaxed);
        let granted = loop {
            match used.checked_add(count as u64) {
                Some(total) if total <= limit => match spent.compare_exchange_weak(
                    used, total,
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                ) {
                    Ok(_) => break true,
                    Err(actual) => used = actual,
                },
                _ => break false,
            }
        };
        if !granted {
            return Err(io::Error::other(format!("{_pipe} 输出累计超出共享字节预算 {limit} 字节")));
        }
        #[cfg(feature = "build-capture-test-support")]
        if written > 0 && std::env::var("COOLZHU_CAPTURE_TEST_FAIL_PIPE").as_deref() == Ok(_pipe) {
            return Err(io::Error::other("测试注入日志写入失败"));
        }
        target.write_all(&buffer[..count])?;
        #[cfg(feature = "build-capture-test-support")]
        { written += count; }
    }
    target.sync_all()
}

// cargo 根进程退出与 Windows Job 计数归零并不同时发生。只等待自然清空；
// 超时或查询失败仍拒绝成功，调用方负责回收整个受管子树。
fn wait_for_job_drain(mut active_count: impl FnMut() -> io::Result<u32>, mut diagnose: impl FnMut() -> String, limit: Duration, stalled: Duration, poll: Duration) -> io::Result<()> {
    // 收尾宽限：构建器（如 MSVC 工具链）在根进程退出后常留下极少量短命工具后代
    //（例如 vctip.exe），它们延迟数秒才自然退出、期间 Job 计数不变属正常收尾。
    // 计数不超过 TAIL_GRACE_MAX 时允许用满 limit 自然排空；数量较多且停滞不动仍按
    // stalled 提前拒绝；limit 始终是硬上限，超限或查询失败依旧 fail-closed。
    const TAIL_GRACE_MAX: u32 = 4;
    let deadline = Instant::now() + limit;
    let mut last_count = u32::MAX;
    let mut last_progress = Instant::now();
    loop {
        let count = active_count()?;
        if count == 0 { return Ok(()); }
        let now = Instant::now();
        if count < last_count {
            last_count = count;
            last_progress = now;
        }
        // 数量较大的停滞组按 stalled 提前拒绝；尾部小组只受 limit 硬上限约束。
        let stalled_out = count > TAIL_GRACE_MAX && now.duration_since(last_progress) >= stalled;
        if now >= deadline || stalled_out {
            return Err(io::Error::other(format!("根命令退出后仍有 {count} 个受管进程活跃，输出完整性未确认；剩余受管进程：{}", diagnose())));
        }
        thread::sleep(poll.min(deadline.saturating_duration_since(now)));
    }
}

// 失败诊断专用：点名 Job 中尚未退出的受管进程（PID=映像名），供下一次失败
// 定位真实身份；查询失败如实标注，不据此放行。
fn describe_remaining(job: &ChildProcessJob) -> String {
    match job.active_process_ids() {
        Ok(ids) if !ids.is_empty() => ids.iter()
            .map(|pid| format!("{pid}={}", process_image_name(*pid)))
            .collect::<Vec<_>>().join(","),
        Ok(_) => "（PID 列表查询为空）".to_string(),
        Err(error) => format!("（PID 列表查询失败：{error}）"),
    }
}

fn process_image_name(pid: u32) -> String {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() { return "<无法读取>".to_string(); }
    let mut buffer = [0_u16; 1024];
    let mut length = buffer.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(handle); }
    if ok == 0 { return "<无法读取>".to_string(); }
    let path = String::from_utf16_lossy(&buffer[..length as usize]);
    path.rsplit(['\\', '/']).next().unwrap_or(path.as_str()).to_string()
}

fn run() -> io::Result<i32> {
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    let owner = if args.first().is_some_and(|value| value == "--owner") {
        if args.len() < 3 { return Err(io::Error::other("缺少宿主PID或创建时间")); }
        let pid = args[1].to_str().and_then(|value| value.parse::<u32>().ok()).ok_or_else(|| io::Error::other("宿主PID无效"))?;
        let created = args[2].to_str().and_then(|value| value.parse::<u64>().ok()).ok_or_else(|| io::Error::other("宿主创建时间无效"))?;
        let lease = ProcessLifetimeLease::open(pid, created)?;
        args.drain(..3);
        Some(lease)
    } else { return Err(io::Error::other("缺少 --owner 宿主身份，拒绝启动构建")); };
    let output_budget = if args.first().is_some_and(|value| value == "--output-budget") {
        if args.len() < 2 { return Err(io::Error::other("缺少 --output-budget 字节数")); }
        let budget = args[1].to_str().and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0 && *value <= HARD_MAX_OUTPUT_BUDGET_BYTES)
            .ok_or_else(|| io::Error::other(format!("--output-budget 必须为正整数且不超过 {} 字节", HARD_MAX_OUTPUT_BUDGET_BYTES)))?;
        args.drain(..2);
        budget
    } else { DEFAULT_OUTPUT_BUDGET_BYTES };
    if args.len() < 5 || args[3] != "--" {
        return Err(io::Error::other("用法：build-capture [--output-budget 字节] stdout文件 stderr文件 工作目录 -- 命令 参数…"));
    }
    // 两个日志均成功独占创建之后才启动构建，既有日志绝不覆盖。
    let stdout = OpenOptions::new().write(true).create_new(true).open(&args[0])?;
    let stderr = OpenOptions::new().write(true).create_new(true).open(&args[1])?;
    let mut command = Command::new(&args[4]);
    command.args(&args[5..]).current_dir(&args[2]).stdin(Stdio::null())
        .stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(owner) = &owner { owner.require_alive()?; }
    let (mut child, job) = ChildProcessJob::spawn_managed_checked(&mut command, |_| {
        if let Some(owner) = &owner { owner.require_alive()?; }
        Ok(())
    })?;
    let out = child.stdout.take().ok_or_else(|| io::Error::other("缺少 stdout 管道"))?;
    let err = child.stderr.take().ok_or_else(|| io::Error::other("缺少 stderr 管道"))?;
    let (sender, receiver) = mpsc::sync_channel(2);
    let spent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
    let first_spent = spent.clone();
    let first_sender = sender.clone();
    let first = thread::spawn(move || { let _ = first_sender.send(pump(out, stdout, "stdout", &first_spent, output_budget)); });
    let second = thread::spawn(move || { let _ = sender.send(pump(err, stderr, "stderr", &spent, output_budget)); });
    let mut completed = 0;
    let mut failure = None;
    let mut status = None;
    let mut root_done = false;
    let mut terminated = false;
    let mut drain_deadline = None;
    while completed < 2 || !root_done {
        if let Some(owner) = &owner {
            if let Err(error) = owner.require_alive() { failure.get_or_insert(error); }
        }
        if drain_deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "构建子树或输出收尾未确认，拒绝成功"));
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => { completed += 1; if let Err(error) = result { failure.get_or_insert(error); } }
            Err(mpsc::RecvTimeoutError::Timeout) => {},
            Err(mpsc::RecvTimeoutError::Disconnected) if completed < 2 => {
                failure.get_or_insert(io::Error::other("输出采集线程未回报完整终态"));
                completed = 2;
            },
            Err(_) => thread::sleep(Duration::from_millis(25)),
        }
        if !root_done {
            match child.try_wait() {
                Ok(value) => { root_done = value.is_some(); status = value; }
                Err(error) => { failure.get_or_insert(error); root_done = true; }
            }
        }
        if !terminated && (failure.is_some() || root_done) {
            if root_done && failure.is_none() {
                // 根退出后给系统和构建子进程自然收尾的时间；只有 Job 清空才可继续。
                // 双管道 EOF、写盘完成、根退出码仍在下方分别核对。
                if let Err(error) = wait_for_job_drain(
                    || {
                        if let Some(owner) = &owner { owner.require_alive()?; }
                        job.active_process_count()
                    },
                    || describe_remaining(&job),
                    Duration::from_secs(30),
                    Duration::from_secs(5),
                    Duration::from_millis(50),
                ) { failure = Some(error); }
            }
            // 根命令结束也回收仍持管道的后代；辅助程序不提供后台构建语义。
            if let Err(error) = job.terminate_and_wait(Duration::from_secs(10)) { failure.get_or_insert(error); }
            terminated = true;
            drain_deadline = Some(std::time::Instant::now() + Duration::from_secs(20));
        }
    }
    while !first.is_finished() || !second.is_finished() {
        if drain_deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "输出线程结束未确认，拒绝成功"));
        }
        thread::sleep(Duration::from_millis(10));
    }
    for result in [first.join(), second.join()] {
        if result.is_err() { failure.get_or_insert(io::Error::other("输出采集线程崩溃")); }
    }
    if let Some(error) = failure { return Err(error); }
    if let Some(owner) = &owner { owner.require_alive()?; }
    Ok(status.and_then(|status| status.code()).unwrap_or(1))
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => { eprintln!("BUILD-CAPTURE-FAILED: {error}"); std::process::exit(125); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delayed_job_accounting_can_settle_without_false_failure() {
        let mut counts = [2, 1, 1, 0].into_iter();
        assert!(wait_for_job_drain(
            || Ok(counts.next().unwrap_or(0)),
            || "stub".to_string(),
            Duration::from_secs(1), Duration::from_millis(20), Duration::from_millis(1),
        ).is_ok());
    }

    #[test]
    fn persistent_descendant_fails_closed() {
        let error = wait_for_job_drain(
            || Ok(1), || "pid=?".to_string(),
            Duration::from_millis(15), Duration::from_millis(5), Duration::from_millis(1),
        ).unwrap_err();
        assert!(error.to_string().contains("受管进程活跃"));
        assert!(error.to_string().contains("pid=?"));
    }

    #[test]
    fn job_query_error_fails_closed() {
        assert!(wait_for_job_drain(
            || Err(io::Error::other("查询失败")),
            || "stub".to_string(),
            Duration::from_secs(1), Duration::from_millis(20), Duration::from_millis(1),
        ).is_err());
    }

    #[test]
    fn tail_descendant_delayed_exit_drains_naturally() {
        // 回归（166 失败现场）：单个尾部受管后代在 stalled 之后、limit 之前
        // 自然退出必须放行，不能因计数短暂不变就误判失败。
        let mut calls = 0_u32;
        assert!(wait_for_job_drain(
            || { calls += 1; Ok(if calls < 40 { 1 } else { 0 }) },
            || "stub".to_string(),
            Duration::from_secs(2), Duration::from_millis(5), Duration::from_millis(1),
        ).is_ok());
        assert!(calls >= 40);
    }

    #[test]
    fn persistent_tail_descendant_fails_at_limit() {
        // 尾部后代持续存活仍 fail-closed：尾部宽限只用满 limit 硬上限，
        // 不放大 stalled，也不改失败文本与诊断。
        let start = Instant::now();
        let error = wait_for_job_drain(
            || Ok(1), || "pid=?".to_string(),
            Duration::from_millis(60), Duration::from_millis(5), Duration::from_millis(1),
        ).unwrap_err();
        assert!(error.to_string().contains("受管进程活跃"), "{error}");
        assert!(error.to_string().contains("pid=?"), "{error}");
        assert!(start.elapsed() >= Duration::from_millis(60));
    }

    #[test]
    fn stalled_large_group_still_fails_fast() {
        // 数量较多的停滞组仍按 stalled 门槛提前拒绝，不占用整条 limit。
        let start = Instant::now();
        let error = wait_for_job_drain(
            || Ok(9), || "many".to_string(),
            Duration::from_secs(2), Duration::from_millis(10), Duration::from_millis(1),
        ).unwrap_err();
        assert!(error.to_string().contains("受管进程活跃"), "{error}");
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn real_job_delayed_descendant_exit_drains_within_limit() {
        // 真实 Windows Job 回归（166 失败现场 vctip.exe 形态）：根 cmd 立即退出，
        // Job 内 ping 后代存活超过 stalled（ping -n 3 ≈ 2s > 1s）、在 limit 前自然退出；
        // 排空必须成功——不按进程名豁免，只认 Job 计数归零；随后核对 stdout 管道真正 EOF。
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "start /b ping -n 3 127.0.0.1 >nul"])
            .stdout(std::process::Stdio::piped());
        let (mut child, job) = ChildProcessJob::spawn_managed(&mut command).expect("managed spawn");
        let mut stdout_pipe = child.stdout.take().expect("stdout pipe");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut sink = Vec::new();
            let result = std::io::Read::read_to_end(&mut stdout_pipe, &mut sink).map(|_| sink.len());
            let _ = tx.send(result);
        });
        let _ = child.wait();
        let start = Instant::now();
        wait_for_job_drain(
            || job.active_process_count(),
            || describe_remaining(&job),
            Duration::from_secs(8), Duration::from_secs(1), Duration::from_millis(50),
        ).expect("真实 Job 中延迟退出的后代必须在 limit 内自然排空");
        // 排空横跨了 stalled 门槛之后才成功，证明不是提前放行。
        assert!(start.elapsed() >= Duration::from_secs(1), "drain ended before stalled window");
        // Job 清空 ⇒ 不再有任何进程持有管道写端，EOF 必须真实到达。
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(_)) => {}
            other => panic!("stdout pipe must reach EOF after job drain: {other:?}"),
        }
        let _ = job.terminate_and_wait(Duration::from_secs(5));
    }

    #[test]
    fn drain_failure_reports_lingering_descendant_identity() {
        // 真实 Job 行为：根进程退出后，Job 内仍活跃的后代必须在失败文本中被点名
        //（136 诊断：受管进程身份从"未知"变为可验证事实）。
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "start /b ping -n 60 127.0.0.1 >nul"]);
        let (mut child, job) = ChildProcessJob::spawn_managed(&mut command).expect("managed spawn");
        let _ = child.wait();
        let error = wait_for_job_drain(
            || job.active_process_count(),
            || describe_remaining(&job),
            Duration::from_secs(5), Duration::from_secs(3), Duration::from_millis(10),
        ).expect_err("lingering descendant must fail closed");
        let _ = job.terminate_and_wait(Duration::from_secs(5));
        let text = error.to_string();
        assert!(text.contains("受管进程活跃"), "{text}");
        assert!(text.contains("剩余受管进程："), "{text}");
        assert!(text.to_lowercase().contains("ping"), "{text}");
    }

    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicU64 as SeqCounter;
    static SEQ: SeqCounter = SeqCounter::new(0);

    fn case_dir(name: &str) -> PathBuf {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::var("CARGO_TARGET_TMPDIR").map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"D:/coolzhuagent/tmp/goal-32wbs-acceptance-20261013/capture-271"))
            .join(format!("bc-{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 限制每次 read 返回字节数的测试源，避免一次 read 吞掉全部输入。
    struct Chunk<'a> { data: &'a [u8], per: usize }
    impl<'a> Read for Chunk<'a> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let n = self.data.len().min(self.per).min(buf.len());
            buf[..n].copy_from_slice(&self.data[..n]);
            self.data = &self.data[n..];
            Ok(n)
        }
    }

    fn new_log(dir: &PathBuf, name: &str) -> (PathBuf, File) {
        let path = dir.join(name);
        let file = OpenOptions::new().write(true).create_new(true).open(&path).unwrap();
        (path, file)
    }

    #[test]
    fn pump_shared_budget_fails_on_overflow_and_preserves_written_bytes() {
        // 单管道超限：超限块不落盘，已写字节保留；第二管道共用计数立即失败。
        let dir = case_dir("budget-unit");
        let spent = AtomicU64::new(0);
        let data = vec![b'x'; 10 * 1024];
        let (out_path, out_file) = new_log(&dir, "out.log");
        let err = pump(Chunk { data: &data, per: 1024 }, out_file, "stdout", &spent, 8192).unwrap_err();
        assert!(err.to_string().contains("预算"), "{err}");
        assert_eq!(fs::metadata(&out_path).unwrap().len(), 8192);
        assert!(fs::read(&out_path).unwrap().iter().all(|b| *b == b'x'));
        // 失败预留不写账：spent 保持已授权总量；后续管道立即失败、不写盘
        assert_eq!(spent.load(Ordering::Relaxed), 8192);
        let (err_path, err_file) = new_log(&dir, "err.log");
        let err2 = pump(Chunk { data: &data, per: 1024 }, err_file, "stderr", &spent, 8192).unwrap_err();
        assert!(err2.to_string().contains("stderr"), "{err2}");
        assert_eq!(fs::metadata(&err_path).unwrap().len(), 0);
    }

    #[test]
    fn pump_within_budget_writes_exact_content() {
        let dir = case_dir("budget-ok");
        let spent = AtomicU64::new(0);
        let data = b"line-one\nline-two\n".repeat(64);
        let (path, file) = new_log(&dir, "out.log");
        pump(Chunk { data: &data, per: 512 }, file, "stdout", &spent, 1 << 20).unwrap();
        assert_eq!(fs::read(&path).unwrap(), data);
        assert_eq!(spent.load(Ordering::Relaxed), data.len() as u64);
    }

    #[test]
    fn pump_budget_near_u64_max_fails_closed_without_wrap() {
        // 共享计数被推到接近 u64::MAX：checked_add 溢出即 fail-closed，
        // debug 不 panic、release 不环绕、零字节落盘。
        let dir = case_dir("budget-max");
        let spent = AtomicU64::new(u64::MAX - 8);
        let data = vec![b'x'; 2048];
        let (path, file) = new_log(&dir, "out.log");
        let err = pump(Chunk { data: &data, per: 1024 }, file, "stdout", &spent, HARD_MAX_OUTPUT_BUDGET_BYTES)
            .unwrap_err();
        assert!(err.to_string().contains("预算"), "{err}");
        assert_eq!(fs::metadata(&path).unwrap().len(), 0);
        assert_eq!(spent.load(Ordering::Relaxed), u64::MAX - 8, "失败预留不回改计数");
    }

    #[test]
    fn managed_child_dual_pipe_budget_overflow_reclaims_job() {
        // 真实受管子进程 + 双管道合计超限：pump 失败、Job 根及后代回收、
        // 已写日志字节如实保留、合计不超预算，绝不报成功。
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "@echo off & for /L %i in (1,1,80) do (echo OUT-%i-0123456789ABCDEF0123456789ABCDEF & echo ERR-%i-0123456789ABCDEF0123456789ABCDEF 1>&2)"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let (mut child, job) = ChildProcessJob::spawn_managed(&mut command).expect("managed spawn");
        let out_pipe = child.stdout.take().unwrap();
        let err_pipe = child.stderr.take().unwrap();
        let dir = case_dir("budget-dual");
        let spent = Arc::new(AtomicU64::new(0));
        let budget: u64 = 2048;
        let (out_path, out_file) = new_log(&dir, "out.log");
        let (err_path, err_file) = new_log(&dir, "err.log");
        let spent2 = spent.clone();
        let t1 = std::thread::spawn(move || pump(out_pipe, out_file, "stdout", &spent2, budget));
        let spent3 = spent.clone();
        let t2 = std::thread::spawn(move || pump(err_pipe, err_file, "stderr", &spent3, budget));
        let r1 = t1.join().expect("pump thread");
        let r2 = t2.join().expect("pump thread");
        let _ = child.wait();
        job.terminate_and_wait(Duration::from_secs(10)).expect("reclaim job");
        assert!(r1.is_err() || r2.is_err(), "两管道合计超限必须至少一路失败");
        let msg = format!("{r1:?} {r2:?}");
        assert!(msg.contains("预算"), "{msg}");
        let total = fs::metadata(&out_path).unwrap().len() + fs::metadata(&err_path).unwrap().len();
        assert!(total <= budget, "共享预算必须约束两管道合计写入: {total}");
        assert_eq!(job.active_process_count().expect("count"), 0, "Job 必须已清空");
    }

    #[test]
    fn managed_child_within_budget_keeps_exact_output_and_success() {
        // 限内：双管道内容精确、子进程真实退出码 0、无半成品。
        let mut command = std::process::Command::new("cmd");
        command.args(["/c", "echo hello-out-271 & echo hello-err-271 1>&2"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let (mut child, job) = ChildProcessJob::spawn_managed(&mut command).expect("managed spawn");
        let out_pipe = child.stdout.take().unwrap();
        let err_pipe = child.stderr.take().unwrap();
        let dir = case_dir("budget-pass");
        let spent = Arc::new(AtomicU64::new(0));
        let (out_path, out_file) = new_log(&dir, "out.log");
        let (err_path, err_file) = new_log(&dir, "err.log");
        let spent2 = spent.clone();
        let t1 = std::thread::spawn(move || pump(out_pipe, out_file, "stdout", &spent2, 1 << 20));
        let spent3 = spent.clone();
        let t2 = std::thread::spawn(move || pump(err_pipe, err_file, "stderr", &spent3, 1 << 20));
        let status = child.wait().expect("wait");
        t1.join().expect("t1").expect("stdout pump");
        t2.join().expect("t2").expect("stderr pump");
        assert!(status.success());
        assert!(fs::read_to_string(&out_path).unwrap().contains("hello-out-271"));
        assert!(fs::read_to_string(&err_path).unwrap().contains("hello-err-271"));
        let _ = job.terminate_and_wait(Duration::from_secs(5));
    }
}
