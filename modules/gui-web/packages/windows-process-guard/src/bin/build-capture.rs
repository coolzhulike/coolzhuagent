//! 构建输出原始字节采集；固定缓冲，不以截断日志冒充成功。
use std::{fs::{File, OpenOptions}, io::{self, Read, Write}, process::{Command, Stdio}, sync::mpsc, thread, time::Duration};
use coolzhu_windows_process_guard::ChildProcessJob;

fn pump(mut source: impl Read, mut target: File) -> io::Result<()> {
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = source.read(&mut buffer)?;
        if count == 0 { break; }
        target.write_all(&buffer[..count])?;
    }
    target.sync_all()
}

fn run() -> io::Result<i32> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 5 || args[3] != "--" {
        return Err(io::Error::other("用法：build-capture stdout文件 stderr文件 工作目录 -- 命令 参数…"));
    }
    // 两个日志均成功独占创建之后才启动构建，既有日志绝不覆盖。
    let stdout = OpenOptions::new().write(true).create_new(true).open(&args[0])?;
    let stderr = OpenOptions::new().write(true).create_new(true).open(&args[1])?;
    let mut command = Command::new(&args[4]);
    command.args(&args[5..]).current_dir(&args[2]).stdin(Stdio::null())
        .stdout(Stdio::piped()).stderr(Stdio::piped());
    let (mut child, job) = ChildProcessJob::spawn_managed(&mut command)?;
    let out = child.stdout.take().ok_or_else(|| io::Error::other("缺少 stdout 管道"))?;
    let err = child.stderr.take().ok_or_else(|| io::Error::other("缺少 stderr 管道"))?;
    let (sender, receiver) = mpsc::sync_channel(2);
    let first_sender = sender.clone();
    let first = thread::spawn(move || { let _ = first_sender.send(pump(out, stdout)); });
    let second = thread::spawn(move || { let _ = sender.send(pump(err, stderr)); });
    let mut completed = 0;
    let mut failure = None;
    let mut status = None;
    let mut root_done = false;
    let mut terminated = false;
    while completed < 2 || !root_done {
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
                match job.active_process_count() {
                    Ok(0) => {},
                    Ok(_) => { failure = Some(io::Error::other("根命令退出后受管后代仍活跃，输出完整性未确认")); },
                    Err(error) => { failure = Some(error); },
                }
            }
            // 根命令结束也回收仍持管道的后代；辅助程序不提供后台构建语义。
            if let Err(error) = job.terminate_and_wait(Duration::from_secs(10)) { failure.get_or_insert(error); }
            terminated = true;
        }
    }
    for result in [first.join(), second.join()] {
        if result.is_err() { failure.get_or_insert(io::Error::other("输出采集线程崩溃")); }
    }
    if let Some(error) = failure { return Err(error); }
    Ok(status.and_then(|status| status.code()).unwrap_or(1))
}

fn main() {
    match run() {
        Ok(code) => std::process::exit(code),
        Err(error) => { eprintln!("BUILD-CAPTURE-FAILED: {error}"); std::process::exit(125); }
    }
}
