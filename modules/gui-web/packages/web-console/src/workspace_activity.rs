//! 工程切换与仍使用工程配置/消息投影的任务共享一把短时锁。
//! Pin 本身不持 MutexGuard；最后一个持有者收尾后才允许切换或重载工程。
use std::sync::{Mutex, OnceLock};

#[derive(Default)]
struct WorkspaceAccess { executions: usize, changing: bool }
fn access() -> &'static Mutex<WorkspaceAccess> {
    static ACCESS: OnceLock<Mutex<WorkspaceAccess>> = OnceLock::new();
    ACCESS.get_or_init(|| Mutex::new(WorkspaceAccess::default()))
}

#[derive(Debug)]
pub(crate) struct WorkspacePin;
impl Drop for WorkspacePin {
    fn drop(&mut self) {
        if let Ok(mut access) = access().lock() {
            access.executions = access.executions.saturating_sub(1);
        }
    }
}
pub(crate) fn pin_workspace() -> Result<WorkspacePin, String> {
    let mut access = access().lock().map_err(|_| "工程执行状态不可用")?;
    if access.changing { return Err("工程正在切换或重载，请稍后重试".into()); }
    access.executions = access.executions.checked_add(1).ok_or("工程活动任务数超过上限")?;
    Ok(WorkspacePin)
}

pub(crate) struct WorkspaceChange;
impl Drop for WorkspaceChange {
    fn drop(&mut self) {
        if let Ok(mut access) = access().lock() { access.changing = false; }
    }
}
pub(crate) fn begin_workspace_change() -> Result<WorkspaceChange, String> {
    let mut access = access().lock().map_err(|_| "工程执行状态不可用")?;
    if access.changing || access.executions != 0 {
        return Err("当前工程仍有聊天、视频或定时任务在执行或收尾，请完成或停止后再切换/重载工程".into());
    }
    access.changing = true;
    Ok(WorkspaceChange)
}
