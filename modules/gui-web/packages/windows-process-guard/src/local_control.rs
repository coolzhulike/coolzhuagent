//! 本机控制面传输：受用户 SID ACL 保护的命名管道与内核读取的对端实例。
//! 同 SID 本身不代表人工确认；可信 launcher / shell 的实例比对由宿主负责。
use crate::{capture_live_process_identity, KernelHandle, ProcessIdentity};
use std::{
    io, mem, ptr, thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{Diagnostics::ToolHelp::*, Pipes::*, Threading::*},
};

const MAX_MESSAGE: usize = 32 * 1024;
const POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Clone)]
pub struct LocalProcessPeer {
    pub process: ProcessIdentity,
    pub user_sid: String,
    pub parent_pid: u32,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn fail(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, message)
}

pub fn process_peer_identity(pid: u32) -> io::Result<LocalProcessPeer> {
    let process = capture_live_process_identity(pid).map_err(|e| fail(&e.to_string()))?;
    let handle = crate::open_process_handle(pid, PROCESS_QUERY_LIMITED_INFORMATION)
        .map_err(|e| fail(&e.to_string()))?;
    // 必须核对这一次打开的句柄仍是上面同一实例，不能只认 PID。
    let current =
        crate::identity_from_handle(handle.raw(), pid).map_err(|e| fail(&e.to_string()))?;
    if !process.is_same_instance(&current) {
        return Err(fail("读取用户身份时进程实例已改变"));
    }
    let mut token = ptr::null_mut();
    // SAFETY: 进程句柄有效，token 是可写输出。
    if unsafe { OpenProcessToken(handle.raw(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = KernelHandle(token);
    let mut needed = 0;
    // SAFETY: 首次只查询 TOKEN_USER 所需长度。
    unsafe {
        GetTokenInformation(token.raw(), TokenUser, ptr::null_mut(), 0, &mut needed);
    }
    if needed == 0 || needed > 64 * 1024 {
        return Err(fail("无法读取实际 Windows 用户 SID"));
    }
    let mut buffer = vec![0usize; (needed as usize).div_ceil(mem::size_of::<usize>())];
    // SAFETY: usize 缓冲提供 TOKEN_USER 对齐，长度覆盖 needed。
    if unsafe {
        GetTokenInformation(
            token.raw(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    let mut sid_text = ptr::null_mut();
    // SAFETY: SID 来自仍存活的 TOKEN_USER 缓冲；系统分配的字符串在下面释放。
    if unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid_text) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut length = 0;
    unsafe {
        while *sid_text.add(length) != 0 {
            length += 1;
        }
    }
    let user_sid =
        unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(sid_text, length)) };
    unsafe {
        LocalFree(sid_text.cast());
    }
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = KernelHandle(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut found = None;
    let mut valid = unsafe { Process32FirstW(snapshot.raw(), &mut entry) };
    while valid != 0 {
        if entry.th32ProcessID == pid {
            found = Some(entry.th32ParentProcessID);
            break;
        }
        valid = unsafe { Process32NextW(snapshot.raw(), &mut entry) };
    }
    let parent_pid = found.ok_or_else(|| fail("进程已退出或无法核对父实例"))?;
    let final_identity = capture_live_process_identity(pid).map_err(|e| fail(&e.to_string()))?;
    if !process.is_same_instance(&final_identity) {
        return Err(fail("核对父实例时 PID 已被复用"));
    }
    Ok(LocalProcessPeer {
        process,
        user_sid,
        parent_pid,
    })
}

pub fn recovery_pipe_name(server: &ProcessIdentity) -> String {
    format!(
        r"\\.\pipe\coolzhu-recovery-{}-{}",
        server.pid(),
        server.creation_time_filetime()
    )
}

fn read_message(handle: HANDLE, deadline: Instant) -> io::Result<Vec<u8>> {
    let mut buffer = vec![0; MAX_MESSAGE];
    loop {
        let mut read = 0;
        let ok = unsafe {
            ReadFile(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                &mut read,
                ptr::null_mut(),
            )
        };
        if ok != 0 && read > 0 {
            buffer.truncate(read as usize);
            return Ok(buffer);
        }
        let error = io::Error::last_os_error();
        if ok == 0
            && !matches!(
                error.raw_os_error().map(|v| v as u32),
                Some(ERROR_NO_DATA | ERROR_PIPE_LISTENING)
            )
        {
            return Err(error);
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "本机控制面读取超时",
            ));
        }
        thread::sleep(POLL);
    }
}

fn write_message(handle: HANDLE, message: &[u8]) -> io::Result<()> {
    if message.is_empty() || message.len() > MAX_MESSAGE {
        return Err(fail("本机控制面消息长度无效"));
    }
    let mut written = 0;
    let ok = unsafe {
        WriteFile(
            handle,
            message.as_ptr(),
            message.len() as u32,
            &mut written,
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    if written as usize != message.len() {
        return Err(io::Error::new(
            io::ErrorKind::WriteZero,
            "控制面消息未完整提交",
        ));
    }
    Ok(())
}

pub struct LocalRecoveryPipeServer {
    handle: KernelHandle,
}
impl LocalRecoveryPipeServer {
    pub fn bind(name: &str) -> io::Result<Self> {
        let own = process_peer_identity(std::process::id())?;
        let security = wide(&format!("D:P(A;;GA;;;SY)(A;;GA;;;{})", own.user_sid));
        let mut descriptor = ptr::null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                security.as_ptr(),
                1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let name = wide(name);
        // FIRST_PIPE_INSTANCE 防止同名控制面被静默接管；拒绝网络客户端。
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE
                    | PIPE_READMODE_MESSAGE
                    | PIPE_NOWAIT
                    | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_MESSAGE as u32,
                MAX_MESSAGE as u32,
                1000,
                &attributes,
            )
        };
        let error = io::Error::last_os_error();
        unsafe {
            LocalFree(descriptor);
        }
        if handle == INVALID_HANDLE_VALUE {
            return Err(error);
        }
        Ok(Self {
            handle: KernelHandle(handle),
        })
    }

    /// 一次连接最多持有 3 秒，不在管道内持有 DB/输入 lease。
    pub fn serve_one(
        &mut self,
        handler: impl FnOnce(LocalProcessPeer, &[u8]) -> Vec<u8>,
    ) -> io::Result<bool> {
        let connected = unsafe { ConnectNamedPipe(self.handle.raw(), ptr::null_mut()) };
        if connected == 0 {
            let error = io::Error::last_os_error();
            match error.raw_os_error().map(|v| v as u32) {
                Some(ERROR_PIPE_LISTENING) => return Ok(false),
                Some(ERROR_NO_DATA) => {
                    unsafe {
                        DisconnectNamedPipe(self.handle.raw());
                    }
                    return Ok(false);
                }
                Some(ERROR_PIPE_CONNECTED) => {}
                _ => return Err(error),
            }
        }
        let result = (|| {
            let mut pid = 0;
            if unsafe { GetNamedPipeClientProcessId(self.handle.raw(), &mut pid) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let peer = process_peer_identity(pid)?;
            let deadline = Instant::now() + Duration::from_secs(3);
            let request = read_message(self.handle.raw(), deadline)?;
            let response = handler(peer, &request);
            write_message(self.handle.raw(), &response)?;
            // 客户端 ACK 证明响应已读取，避免 Disconnect 丢弃尚未读取的数据；不做无界 Flush。
            if read_message(self.handle.raw(), deadline)? != b"ack" {
                return Err(fail("控制面确认回执无效"));
            }
            Ok(true)
        })();
        unsafe {
            DisconnectNamedPipe(self.handle.raw());
        }
        result
    }
}

pub fn local_recovery_pipe_request(
    name: &str,
    expected_server: &ProcessIdentity,
    request: &[u8],
    timeout: Duration,
) -> io::Result<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let name = wide(name);
    let handle = loop {
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle != INVALID_HANDLE_VALUE {
            break KernelHandle(handle);
        }
        let error = io::Error::last_os_error();
        if Instant::now() >= deadline
            || !matches!(
                error.raw_os_error().map(|v| v as u32),
                Some(ERROR_PIPE_BUSY | ERROR_FILE_NOT_FOUND)
            )
        {
            return Err(error);
        }
        thread::sleep(POLL);
    };
    let mut server_pid = 0;
    if unsafe { GetNamedPipeServerProcessId(handle.raw(), &mut server_pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let actual = capture_live_process_identity(server_pid).map_err(|e| fail(&e.to_string()))?;
    if !expected_server.is_same_instance(&actual) {
        return Err(fail("命名管道实际服务端与本次后台实例不符"));
    }
    let mode = PIPE_READMODE_MESSAGE | PIPE_NOWAIT;
    if unsafe { SetNamedPipeHandleState(handle.raw(), &mode, ptr::null(), ptr::null()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    write_message(handle.raw(), request)?;
    let response = read_message(handle.raw(), deadline)?;
    write_message(handle.raw(), b"ack")?;
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_pipe_uses_actual_peer_and_rejects_wrong_server_generation() {
        let own = process_peer_identity(std::process::id()).expect("读取真实进程/SID");
        assert!(own.user_sid.starts_with("S-1-"));
        let name = format!(
            "{}-test-{}",
            recovery_pipe_name(&own.process),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let mut server = LocalRecoveryPipeServer::bind(&name).expect("带ACL真实管道");
        assert!(
            LocalRecoveryPipeServer::bind(&name).is_err(),
            "同名控制面不能被第二实例接管"
        );
        let expected = own.clone();
        let worker = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(6);
            while Instant::now() < deadline {
                if server
                    .serve_one(|peer, request| {
                        assert!(peer.process.is_same_instance(&expected.process));
                        assert_eq!(peer.user_sid, expected.user_sid);
                        assert_eq!(request, b"inspect");
                        b"verified-peer".to_vec()
                    })
                    .unwrap_or(false)
                {
                    return;
                }
                thread::sleep(POLL);
            }
            panic!("管道未完成真实请求");
        });
        let wrong = crate::process_identity_for_test(
            own.process.pid(),
            own.process.creation_time_filetime() + 1,
        );
        assert!(
            local_recovery_pipe_request(&name, &wrong, b"inspect", Duration::from_secs(2)).is_err()
        );
        let response =
            local_recovery_pipe_request(&name, &own.process, b"inspect", Duration::from_secs(4))
                .expect("身份吻合才可通信");
        assert_eq!(response, b"verified-peer");
        worker.join().expect("管道线程");
    }
}
