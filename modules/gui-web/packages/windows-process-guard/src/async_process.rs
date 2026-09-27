//! Tokio 进程使用与同步工具相同的“先挂 Job、后执行”边界。
use std::io;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
use super::{ChildProcessJob, resume_initial_thread};

impl ChildProcessJob {
    pub fn spawn_managed_async(command: &mut tokio::process::Command) -> io::Result<(tokio::process::Child, Self)> {
        let job = Self::new_kill_on_close()?;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED).kill_on_drop(true);
        let mut child = command.spawn()?;
        let bind = || -> io::Result<()> {
            let handle = child.raw_handle().ok_or_else(|| io::Error::other("受管异步进程句柄不存在"))?;
            let process_id = child.id().ok_or_else(|| io::Error::other("受管异步进程身份不存在"))?;
            if unsafe { AssignProcessToJobObject(job.handle as HANDLE, handle as HANDLE) } == 0 {
                return Err(io::Error::last_os_error());
            }
            resume_initial_thread(process_id)
        };
        if let Err(error) = bind() {
            let _ = child.start_kill();
            return Err(error); // Job Drop + Tokio kill_on_drop 负责子树与直接 Child 退出。
        }
        Ok((child, job))
    }
}
