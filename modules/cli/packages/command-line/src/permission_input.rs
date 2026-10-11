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
    #[cfg(windows)]
    #[test]
    fn redirected_prompt_and_approval_keep_their_lines() {
        use std::{io::Write,os::windows::process::CommandExt,process::{Command,Stdio}};
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp");
        std::fs::create_dir_all(&root).unwrap();
        let directory=root.join(format!("repl-input-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir(&directory).unwrap();
        let mut child=Command::new(std::env::current_exe().unwrap())
            .args(["--exact","permission_input::tests::redirected_prompt_and_approval_probe","--ignored"])
            .env("COOLZHU_REDIRECTED_INPUT_PROBE","1")
            .creation_flags(0x08000000).stdin(Stdio::piped())
            .stdout(std::fs::File::create(directory.join("stdout.txt")).unwrap())
            .stderr(std::fs::File::create(directory.join("stderr.txt")).unwrap())
            .spawn().unwrap();
        let mut stdin=child.stdin.take().unwrap();
        let writer=std::thread::spawn(move || {
            let mut input=vec![b'x';69632];input.extend_from_slice(b"\nyes\n/quit\n");
            stdin.write_all(&input)
        });
        let limit=std::time::Instant::now()+std::time::Duration::from_secs(5);
        let status=loop {
            if let Some(status)=child.try_wait().unwrap(){break status;}
            if std::time::Instant::now()>=limit {child.kill().unwrap();child.wait().unwrap();let _=writer.join();panic!("输入探针超时，仅回收自身子进程");}
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        writer.join().unwrap().unwrap();
        assert!(status.success(),"输入交接探针失败：{status}；日志{}",directory.display());
    }
    #[cfg(windows)]
    #[test]
    #[ignore="仅由父测试在独立重定向输入进程运行"]
    fn redirected_prompt_and_approval_probe() {
        assert_eq!(std::env::var("COOLZHU_REDIRECTED_INPUT_PROBE").unwrap(),"1");
        let mut editor=crate::input::LineEditor::new("> ",Vec::new());
        let crate::input::ReadOutcome::Submit(prompt)=editor.read_line().unwrap() else {panic!("应提交普通提示");};
        assert_eq!(prompt.len(),69632,"普通提示不得套用审批行预算");
        assert!(prompt.bytes().all(|b|b==b'x'));
        assert_eq!(super::read_line().unwrap(),"yes\n");
        assert_eq!(editor.read_line().unwrap(),crate::input::ReadOutcome::Submit("/quit".into()));
        assert_eq!(editor.read_line().unwrap(),crate::input::ReadOutcome::Exit);
    }
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
