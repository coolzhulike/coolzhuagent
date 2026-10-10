//! 权限输入随原回合取消；Windows内核调用集中在guard安全边界。
use std::io;

pub(crate) fn read_line()->io::Result<String> {
    #[cfg(windows)] {
        let control=runtime::managed_process::current_execution_control();
        let observed=control.clone();
        let result=coolzhu_windows_process_guard::permission_input::read_permission_line(
            std::sync::Arc::new(move ||observed.as_ref().is_some_and(|c|c.interruption().is_some())));
        finish_read(control,result)
    }
    #[cfg(not(windows))] {
        let mut response=String::new();
        io::stdin().read_line(&mut response)?;
        Ok(response)
    }
}

#[cfg(any(windows,test))]
fn finish_read(control:Option<runtime::managed_process::ExecutionControl>,result:io::Result<String>)->io::Result<String> {
    if let Some(control)=control {
        if let Some(reason)=control.interruption() {
            return Err(io::Error::other(format!("权限输入已中断：{reason:?}")));
        }
        // 输入基础设施失败后停止本回合，不能继续请求审批并消费失败行的后缀。
        if result.is_err(){control.cancel();}
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::managed_process::{ExecutionControl,Interruption};
    #[test]
    fn input_failure_cancels_turn_and_preserves_original_error() {
        let control=ExecutionControl::new(None,None);
        let error=finish_read(Some(control.clone()),Err(io::Error::other("输入预算拒绝"))).unwrap_err();
        assert_eq!(error.to_string(),"输入预算拒绝");
        assert_eq!(control.interruption(),Some(Interruption::Cancelled));
    }
    #[test]
    fn late_approval_cannot_override_deadline() {
        let control=ExecutionControl::new(Some(std::time::Duration::ZERO),None);
        let error=finish_read(Some(control.clone()),Ok("yes\n".into())).unwrap_err();
        assert!(error.to_string().contains("TimedOut"));
        assert_eq!(control.interruption(),Some(Interruption::TimedOut));
    }
    #[test]
    fn normal_response_does_not_cancel_turn() {
        let control=ExecutionControl::new(None,None);
        assert_eq!(finish_read(Some(control.clone()),Ok("no\n".into())).unwrap(),"no\n");
        assert!(control.interruption().is_none());
    }
}
