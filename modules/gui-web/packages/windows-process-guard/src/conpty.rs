//! 受 Job 监督的 Windows ConPTY；工作区与调用者权限由宿主验证。
use std::collections::VecDeque;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Console::{
    COORD, HPCON, ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::{
    CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess, OpenProcess,
    InitializeProcThreadAttributeList, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};

use super::ChildProcessJob;

const OUTPUT_CAPACITY_BYTES: usize = 1024 * 1024;
const MAX_INPUT_BYTES: usize = 8 * 1024;
const INPUT_QUEUE_DEPTH: usize = 32;

#[derive(Debug, Clone)]
pub struct ConPtyOutput {
    pub text: String,
    pub next_cursor: u64,
    pub truncated: bool,
    pub closed: bool,
}

#[derive(Default)]
struct OutputRing {
    chunks: VecDeque<(u64, String)>,
    next_seq: u64,
    bytes: usize,
    closed: bool,
}

impl OutputRing {
    fn push(&mut self, text: String) {
        if text.is_empty() { return; }
        self.next_seq = self.next_seq.saturating_add(1);
        self.bytes += text.len();
        self.chunks.push_back((self.next_seq, text));
        while self.bytes > OUTPUT_CAPACITY_BYTES {
            if let Some((_, old)) = self.chunks.pop_front() { self.bytes -= old.len(); }
            else { break; }
        }
    }

    fn read_since(&self, cursor: u64, max_bytes: usize) -> ConPtyOutput {
        let oldest = self.chunks.front().map_or(self.next_seq.saturating_add(1), |(seq, _)| *seq);
        let truncated = cursor.saturating_add(1) < oldest;
        let mut text = String::new();
        let mut next_cursor = if truncated { oldest.saturating_sub(1) } else { cursor };
        let limit = max_bytes.clamp(4096, 256 * 1024);
        for (seq, chunk) in &self.chunks {
            if *seq <= next_cursor { continue; }
            if !text.is_empty() && text.len().saturating_add(chunk.len()) > limit { break; }
            text.push_str(chunk);
            next_cursor = *seq;
        }
        ConPtyOutput { text, next_cursor, truncated, closed: self.closed }
    }
}

#[derive(Default)]
struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    fn feed(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut output = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    output.push_str(text);
                    self.pending.clear();
                    break;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    let invalid = error.error_len();
                    if valid > 0 {
                        output.push_str(std::str::from_utf8(&self.pending[..valid]).unwrap_or_default());
                        self.pending.drain(..valid);
                    }
                    if let Some(length) = invalid {
                        output.push('\u{fffd}');
                        self.pending.drain(..length);
                    } else { break; }
                }
            }
        }
        output
    }

    fn finish(&mut self) -> String {
        let tail = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending.clear();
        tail
    }
}

struct PseudoConsole(HPCON);
// HPCON 是进程内句柄，ConPTY 的 resize/close 可在不同宿主线程调用；
// ManagedConPty 通过宿主 Mutex 串行访问该句柄。
unsafe impl Send for PseudoConsole {}
impl Drop for PseudoConsole {
    fn drop(&mut self) { unsafe { ClosePseudoConsole(self.0); } }
}

struct AttributeList {
    words: Vec<usize>,
    initialized: bool,
}

impl AttributeList {
    fn new(console: HPCON) -> io::Result<Self> {
        let mut bytes = 0_usize;
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut bytes); }
        if bytes == 0 { return Err(io::Error::last_os_error()); }
        let mut list = Self { words: vec![0; bytes.div_ceil(std::mem::size_of::<usize>())], initialized: false };
        let ptr = list.as_mut_ptr();
        if unsafe { InitializeProcThreadAttributeList(ptr, 1, 0, &mut bytes) } == 0 {
            return Err(io::Error::last_os_error());
        }
        list.initialized = true;
        let result = unsafe { UpdateProcThreadAttribute(
            ptr, 0, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            console as *const std::ffi::c_void, std::mem::size_of::<HPCON>(),
            std::ptr::null_mut(), std::ptr::null(),
        ) };
        if result == 0 { return Err(io::Error::last_os_error()); }
        Ok(list)
    }

    fn as_mut_ptr(&mut self) -> *mut std::ffi::c_void {
        self.words.as_mut_ptr().cast()
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()); }
        }
    }
}

fn make_pipe() -> io::Result<(OwnedHandle, OwnedHandle)> {
    let mut read: HANDLE = std::ptr::null_mut();
    let mut write: HANDLE = std::ptr::null_mut();
    if unsafe { CreatePipe(&mut read, &mut write, std::ptr::null(), 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    // CreatePipe 成功后两端由 OwnedHandle 在所有失败分支自动释放。
    Ok((unsafe { OwnedHandle::from_raw_handle(read) },
        unsafe { OwnedHandle::from_raw_handle(write) }))
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn system_directory() -> io::Result<std::path::PathBuf> {
    let mut system = vec![0_u16; 32768];
    let len = unsafe { GetSystemDirectoryW(system.as_mut_ptr(), system.len() as u32) } as usize;
    if len == 0 || len >= system.len() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "无法确定可信系统目录"));
    }
    Ok(std::path::PathBuf::from(String::from_utf16_lossy(&system[..len])))
}

fn system_powershell() -> io::Result<std::path::PathBuf> {
    let shell = system_directory()?.join("WindowsPowerShell").join("v1.0").join("powershell.exe");
    if !shell.is_file() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "系统 PowerShell 不存在"));
    }
    Ok(shell)
}

fn coord(cols: u16, rows: u16) -> io::Result<COORD> {
    if !(1..=300).contains(&cols) || !(1..=200).contains(&rows) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "终端尺寸超出允许范围"));
    }
    Ok(COORD { X: cols as i16, Y: rows as i16 })
}

fn shell_directory(workspace: &Path) -> io::Result<std::path::PathBuf> {
    let canonical = workspace.canonicalize()?;
    if !canonical.is_dir() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "终端工作区不是目录"));
    }
    // Windows canonicalize 会给普通盘符路径添加 \\?\；cmd.exe 无法把它当作 cwd，
    // PowerShell 也会落入 FileSystem::\\?\ 命名空间。验证后只移除 Win32 长路径前缀。
    let raw = canonical.to_string_lossy();
    if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        Ok(std::path::PathBuf::from(format!(r"\\{rest}")))
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        Ok(std::path::PathBuf::from(rest))
    } else {
        Ok(canonical)
    }
}

/// 一个固定 PowerShell 进程及其 ConPTY。创建时先挂起、加入 Job，再恢复初始线程。
pub struct ManagedConPty {
    process: OwnedHandle,
    process_id: u32,
    job: Option<ChildProcessJob>,
    console: Option<PseudoConsole>,
    input: Option<mpsc::SyncSender<Vec<u8>>>,
    writer: Option<thread::JoinHandle<()>>,
    reader: Option<thread::JoinHandle<()>>,
    output: Arc<Mutex<OutputRing>>,
}

impl ManagedConPty {
    /// 官方登录固定入口，直接启动原生 CLI，不经过 shell，也不接受页面参数。
    pub fn spawn_devin_browser_login(binary: &Path, workspace: &Path) -> io::Result<Self> {
        if !binary.is_absolute() || !binary.is_file()
            || !binary.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            || binary.as_os_str().to_string_lossy().contains(['"', '\r', '\n']) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "登录组件路径无效"));
        }
        let command = format!("\"{}\" auth login", binary.display());
        Self::spawn_shell(workspace, 120, 40, binary, OsStr::new(&command))
    }

    /// 登录必须确认整个 Job 已排空，不能只以主进程退出或句柄释放为证据。
    pub fn close_verified(&mut self) -> io::Result<()> {
        let drained = self.job.as_ref().map(|job| job.terminate_and_wait(std::time::Duration::from_secs(3))).transpose();
        let closed = self.close();
        drained?;
        closed
    }

    pub fn spawn_powershell(workspace: &Path, cols: u16, rows: u16) -> io::Result<Self> {
        let shell = system_powershell()?;
        // 右栏文本框提交完整命令，不依赖系统行编辑器。Windows PowerShell 的
        // PSReadLine 会吞掉 ConPTY 输入中的补充平面字符，并投影全局历史建议；
        // 只在本终端进程停用它，保留真正的交互 shell、Ctrl+C 与子程序控制台。
        Self::spawn_shell(workspace, cols, rows, &shell, OsStr::new(
            r#"powershell.exe -NoLogo -NoProfile -NoExit -Command "Remove-Module PSReadLine -ErrorAction SilentlyContinue""#,
        ))
    }

    fn spawn_shell(workspace: &Path, cols: u16, rows: u16,
        shell: &Path, command_line: &OsStr) -> io::Result<Self> {
        let workspace = shell_directory(workspace)?;
        let dimensions = coord(cols, rows)?;
        let (pseudo_input, host_input) = make_pipe()?;
        let (host_output, pseudo_output) = make_pipe()?;
        let mut console: HPCON = 0;
        let hr = unsafe { CreatePseudoConsole(
            dimensions, pseudo_input.as_raw_handle() as HANDLE,
            pseudo_output.as_raw_handle() as HANDLE, 0, &mut console,
        ) };
        if hr < 0 { return Err(io::Error::other(format!("创建 ConPTY 失败: 0x{:08x}", hr as u32))); }
        let console = PseudoConsole(console);
        let mut attributes = AttributeList::new(console.0)?;
        let mut startup = STARTUPINFOEXW::default();
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        // 在测试宿主或 GUI 宿主已重定向 stdout/stderr 时，Windows 仍可能复制父标准句柄。
        // 显式提供三个空标准句柄，确保 ConPTY 成为子进程控制台 I/O 的唯一入口。
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.lpAttributeList = attributes.as_mut_ptr();
        let app = wide_null(shell.as_os_str());
        let mut command = wide_null(command_line);
        let directory = wide_null(workspace.as_os_str());
        let mut info = PROCESS_INFORMATION::default();
        let created = unsafe { CreateProcessW(
            app.as_ptr(), command.as_mut_ptr(), std::ptr::null(), std::ptr::null(), 0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT | CREATE_SUSPENDED,
            std::ptr::null(), directory.as_ptr(), &startup.StartupInfo, &mut info,
        ) };
        if created == 0 { return Err(io::Error::last_os_error()); }
        // 官方 ConPTY 协议：子进程建立后立即释放本进程持有的伪控制台端管道副本，
        // 否则关闭时无法可靠观察断链。读/写工作线程各保留宿主端。
        drop(pseudo_input);
        drop(pseudo_output);
        let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
        let thread_handle = unsafe { OwnedHandle::from_raw_handle(info.hThread) };
        let job = match ChildProcessJob::new_kill_on_close() {
            Ok(job) => job,
            Err(error) => {
                unsafe { TerminateProcess(process.as_raw_handle() as HANDLE, 1); }
                return Err(error);
            }
        };
        if let Err(error) = job.assign_handle(process.as_raw_handle() as HANDLE) {
            unsafe { TerminateProcess(process.as_raw_handle() as HANDLE, 1); }
            return Err(error);
        }

        let output = Arc::new(Mutex::new(OutputRing::default()));
        let output_for_reader = Arc::clone(&output);
        let reader = thread::Builder::new().name("coolzhu-conpty-read".into()).spawn(move || {
            let mut source = File::from(host_output);
            let mut bytes = [0_u8; 4096];
            let mut utf8 = Utf8Stream::default();
            loop {
                match source.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(size) => {
                        let chunk = utf8.feed(&bytes[..size]);
                        if let Ok(mut ring) = output_for_reader.lock() { ring.push(chunk); }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            if let Ok(mut ring) = output_for_reader.lock() {
                ring.push(utf8.finish());
                ring.closed = true;
            }
        })?;
        let (sender, receiver) = mpsc::sync_channel::<Vec<u8>>(INPUT_QUEUE_DEPTH);
        let writer = match thread::Builder::new().name("coolzhu-conpty-write".into()).spawn(move || {
            let mut sink = File::from(host_input);
            for bytes in receiver {
                if sink.write_all(&bytes).is_err() { break; }
            }
        }) {
            Ok(writer) => writer,
            Err(error) => {
                drop(job);
                drop(console);
                let _ = reader.join();
                return Err(error);
            }
        };
        let resumed = unsafe { ResumeThread(thread_handle.as_raw_handle() as HANDLE) };
        if resumed == u32::MAX {
            let error = io::Error::last_os_error();
            drop(job);
            drop(sender);
            drop(console);
            let _ = writer.join();
            let _ = reader.join();
            return Err(error);
        }
        drop(thread_handle);
        drop(attributes);
        Ok(Self { process, process_id: info.dwProcessId, job: Some(job), console: Some(console),
            input: Some(sender), writer: Some(writer), reader: Some(reader), output })
    }

    pub fn process_id(&self) -> u32 { self.process_id }

    pub fn write(&self, input: &[u8]) -> io::Result<()> {
        if input.len() > MAX_INPUT_BYTES {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "单次终端输入过长"));
        }
        let sender = self.input.as_ref().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "终端已关闭"))?;
        sender.try_send(input.to_vec()).map_err(|error| match error {
            mpsc::TrySendError::Full(_) => io::Error::new(io::ErrorKind::WouldBlock, "终端输入队列已满"),
            mpsc::TrySendError::Disconnected(_) => io::Error::new(io::ErrorKind::BrokenPipe, "终端输入流已结束"),
        })
    }

    pub fn interrupt(&self) -> io::Result<()> { self.write(&[3]) }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let console = self.console.as_ref().ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "终端已关闭"))?;
        let hr = unsafe { ResizePseudoConsole(console.0, coord(cols, rows)?) };
        if hr < 0 { Err(io::Error::other(format!("调整终端尺寸失败: 0x{:08x}", hr as u32))) }
        else { Ok(()) }
    }

    pub fn read_since(&self, cursor: u64, max_bytes: usize) -> ConPtyOutput {
        self.output.lock().map(|ring| ring.read_since(cursor, max_bytes))
            .unwrap_or(ConPtyOutput { text: String::new(), next_cursor: cursor, truncated: false, closed: true })
    }

    pub fn exit_code(&self) -> io::Result<Option<u32>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle() as HANDLE, 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.as_raw_handle() as HANDLE, &mut code) } == 0 {
                    Err(io::Error::last_os_error())
                } else { Ok(Some(code)) }
            }
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn close(&mut self) -> io::Result<()> {
        if self.console.is_none() { return Ok(()); }
        self.input.take();
        // Job 在伪控制台关闭前停止 shell 与所有后代；reader 持续排空输出管道。
        self.job.take();
        let wait = unsafe { WaitForSingleObject(self.process.as_raw_handle() as HANDLE, 5000) };
        self.console.take();
        if let Some(writer) = self.writer.take() { let _ = writer.join(); }
        if let Some(reader) = self.reader.take() { let _ = reader.join(); }
        if wait == WAIT_OBJECT_0 { Ok(()) }
        else if wait == WAIT_TIMEOUT { Err(io::Error::new(io::ErrorKind::TimedOut, "终端进程未在期限内退出")) }
        else { Err(io::Error::last_os_error()) }
    }
}

impl Drop for ManagedConPty {
    fn drop(&mut self) { let _ = self.close(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_ring_reports_truncation_and_cursor() {
        let mut ring = OutputRing::default();
        ring.push("旧".repeat(OUTPUT_CAPACITY_BYTES / 3));
        ring.push("新".to_string());
        let read = ring.read_since(0, 8192);
        assert!(read.truncated);
        assert_eq!(read.text, "新");
        assert_eq!(read.next_cursor, 2);
    }

    #[test]
    fn utf8_stream_preserves_split_chinese_characters() {
        let mut decoder = Utf8Stream::default();
        let bytes = "中文".as_bytes();
        assert_eq!(decoder.feed(&bytes[..2]), "");
        assert_eq!(decoder.feed(&bytes[2..4]), "中");
        assert_eq!(decoder.feed(&bytes[4..]), "文");
        assert_eq!(decoder.finish(), "");
    }

    #[test]
    #[ignore = "仅诊断 ConPTY 底层输入，产品入口仍固定 PowerShell"]
    fn diagnostic_cmd_input_creates_file() {
        let workspace = tempfile::tempdir().unwrap();
        let cmd = system_directory().unwrap().join("cmd.exe");
        let mut terminal = ManagedConPty::spawn_shell(workspace.path(), 90, 30,
            &cmd, OsStr::new("cmd.exe /Q")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        terminal.write(b"echo ready> cmd-ready.txt\r\n").unwrap();
        let file = workspace.path().join("cmd-ready.txt");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let mut output = String::new();
        let mut cursor = 0;
        while std::time::Instant::now() < deadline && !file.exists() {
            let batch = terminal.read_since(cursor, 64 * 1024);
            cursor = batch.next_cursor;
            output.push_str(&batch.text);
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(file.exists(), "cmd 未消费 ConPTY 输入: {output}");
        terminal.close().unwrap();
    }

    #[test]
    #[ignore = "需真实 Windows ConPTY；定向运行以核对 PowerShell、Ctrl+C 与 Job 回收"]
    fn real_powershell_survives_interrupt_and_job_close() {
        let workspace = tempfile::tempdir().unwrap();
        let mut terminal = ManagedConPty::spawn_powershell(workspace.path(), 90, 30).unwrap();
        let mut cursor = 0;
        let wait_file = |path: &Path, terminal: &ManagedConPty, cursor: &mut u64| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
            let mut captured = String::new();
            while std::time::Instant::now() < deadline {
                let output = terminal.read_since(*cursor, 64 * 1024);
                *cursor = output.next_cursor;
                captured.push_str(&output.text);
                if path.exists() { return; }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            panic!("PowerShell 未创建预期文件 {}: {captured}", path.display());
        };
        // PowerShell 的提示符不保证按行刷新；启动后直接以文件副作用确认可交互。
        std::thread::sleep(std::time::Duration::from_millis(600));
        terminal.write("Set-Content -LiteralPath .\\unicode.txt -Encoding UTF8 -Value '竹林𠮷😀'\r\n".as_bytes()).unwrap();
        wait_file(&workspace.path().join("unicode.txt"), &terminal, &mut cursor);
        assert_eq!(std::fs::read_to_string(workspace.path().join("unicode.txt")).unwrap()
            .trim_start_matches('\u{feff}').trim(), "竹林𠮷😀",
            "补充平面输入必须经真实 PowerShell 解析后完整写入，不能只检查命令回显");
        terminal.write(b"Set-Content -LiteralPath .\\first.txt -Value done\r\n").unwrap();
        wait_file(&workspace.path().join("first.txt"), &terminal, &mut cursor);
        terminal.write(b"Set-Content -LiteralPath .\\started.txt -Value started; Start-Sleep -Seconds 15; Set-Content -LiteralPath .\\should-not.txt -Value late\r\n").unwrap();
        wait_file(&workspace.path().join("started.txt"), &terminal, &mut cursor);
        // 剩余旧提示符可能比文件写入更晚送达；中断前先排空，避免误认恢复提示符。
        std::thread::sleep(std::time::Duration::from_millis(100));
        cursor = terminal.read_since(cursor, 64 * 1024).next_cursor;
        let interrupted_at = std::time::Instant::now();
        terminal.interrupt().unwrap();
        let prompt_deadline = interrupted_at + std::time::Duration::from_secs(5);
        let mut after_interrupt = String::new();
        while std::time::Instant::now() < prompt_deadline {
            let batch = terminal.read_since(cursor, 64 * 1024);
            cursor = batch.next_cursor;
            after_interrupt.push_str(&batch.text);
            if after_interrupt.contains("PS ") && after_interrupt.contains('>') { break; }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(after_interrupt.contains("PS ") && after_interrupt.contains('>'),
            "Ctrl+C 后未恢复 PowerShell 提示符: {after_interrupt:?}");
        assert!(!workspace.path().join("should-not.txt").exists(), "长命令必须在 15 秒结束前被中断");
        terminal.write(b"Set-Content -LiteralPath .\\after.txt -Value resumed\r\n").unwrap();
        wait_file(&workspace.path().join("after.txt"), &terminal, &mut cursor);
        assert!(!workspace.path().join("should-not.txt").exists(), "Ctrl+C 必须中断前台长命令");
        assert!(terminal.exit_code().unwrap().is_none(), "Ctrl+C 不得杀掉 shell");
        terminal.write(b"$p = Start-Process -FilePath \"$PSHOME\\powershell.exe\" -ArgumentList '-NoLogo -NoProfile -Command Start-Sleep -Seconds 60' -WindowStyle Hidden -PassThru; Set-Content -LiteralPath .\\child.pid -Value $p.Id\r\n").unwrap();
        let pid_file = workspace.path().join("child.pid");
        wait_file(&pid_file, &terminal, &mut cursor);
        let child_pid: u32 = std::fs::read_to_string(pid_file).unwrap().trim().parse().unwrap();
        // Win32 标准权限 SYNCHRONIZE；保留进程 handle，可准确区分退出与 PID 复用。
        let child = unsafe { OpenProcess(0x0010_0000, 0, child_pid) };
        assert!(!child.is_null(), "没有打开由终端启动的后代进程");
        let child = unsafe { OwnedHandle::from_raw_handle(child) };
        assert_eq!(unsafe { WaitForSingleObject(child.as_raw_handle() as HANDLE, 0) }, WAIT_TIMEOUT,
            "关闭前后代进程应仍在运行");
        terminal.close().unwrap();
        assert!(terminal.exit_code().unwrap().is_some(), "关闭须回收受监督 shell");
        assert_eq!(unsafe { WaitForSingleObject(child.as_raw_handle() as HANDLE, 5000) }, WAIT_OBJECT_0,
            "关闭 Job 须连同终端后代进程一并回收");
    }
}
