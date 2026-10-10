//! 权限输入随原回合取消；Windows内核调用集中在guard安全边界。
use std::io;

pub(crate) fn read_line()->io::Result<String> {
    #[cfg(windows)] {
        let control=runtime::managed_process::current_execution_control();
        let observed=control.clone();
        let result=coolzhu_windows_process_guard::permission_input::read_permission_line(
            std::sync::Arc::new(move ||observed.as_ref().is_some_and(|c|c.interruption().is_some())));
        if let Some(reason)=control.and_then(|c|c.interruption()) {
            return Err(io::Error::other(format!("权限输入已中断：{reason:?}")));
        }
        result
    }
    #[cfg(not(windows))] {
        let mut response=String::new();
        io::stdin().read_line(&mut response)?;
        Ok(response)
    }
}