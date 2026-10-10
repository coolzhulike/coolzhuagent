//! 构建输出原始字节采集；固定缓冲，不以截断日志冒充成功。
use std::{fs::{File, OpenOptions}, io::{self, Read, Write}, process::{Command, Stdio}, sync::mpsc, thread, time::Duration};
use coolzhu_windows_process_guard::{ChildProcessJob, ProcessLifetimeLease};

fn pump(mut source: impl Read, mut target: File, _pipe: &str) -> io::Result<()> {
    let mut buffer = [0_u8; 64 * 1024];
    #[cfg(feature = "build-capture-test-support")]
    let mut written = 0;
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 { break; }
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
    if args.len() < 5 || args[3] != "--" {
        return Err(io::Error::other("用法：build-capture stdout文件 stderr文件 工作目录 -- 命令 参数…"));
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
    let first_sender = sender.clone();
    let first = thread::spawn(move || { let _ = first_sender.send(pump(out, stdout, "stdout")); });
    let second = thread::spawn(move || { let _ = sender.send(pump(err, stderr, "stderr")); });
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
                // 根退出与 Job 计数清空不保证同步；给自然收尾有界等待，不能以强杀代替成功。
                let settle_deadline = std::time::Instant::now() + Duration::from_millis(250);
                loop {
                    match job.active_process_count() {
                        Ok(0) => break,
                        Ok(_) if std::time::Instant::now() < settle_deadline => {
                            thread::sleep(Duration::from_millis(10));
                        },
                        Ok(_) => {
                            failure = Some(io::Error::other("根命令退出后受管后代仍活跃，输出完整性未确认"));
                            break;
                        },
                        Err(error) => { failure = Some(error); break; },
                    }
                }
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
