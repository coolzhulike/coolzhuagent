//! 每回合同步宿主的信号范围；启动前完成注册，结束后回收自有监听线程。
use std::{future::Future,io,sync::{Arc,atomic::{AtomicBool,Ordering}},task::Poll,time::Duration};
use runtime::managed_process::{current_execution_control,with_execution_control,ExecutionControl};

pub(crate) fn run<T>(execute: impl FnOnce()->T)->io::Result<T> {
    with_signal(tokio::signal::ctrl_c,execute)
}

fn with_signal<T,F,S>(signal:F,execute:impl FnOnce()->T)->io::Result<T>
where F:FnOnce()->S+Send+'static,S:Future<Output=io::Result<()>> {
    let control=current_execution_control().unwrap_or_else(||ExecutionControl::new(None,None));
    let cancel=control.clone();
    let stopped=Arc::new(AtomicBool::new(false));
    let stop=stopped.clone();
    let (ready,started)=std::sync::mpsc::sync_channel(1);
    let worker=std::thread::Builder::new().name("cli-turn-signal".into()).spawn(move || {
        let runtime=match tokio::runtime::Builder::new_current_thread().enable_all().build(){
            Ok(runtime)=>runtime,
            Err(error)=>{let _=ready.send(Err(error.to_string()));return;}
        };
        runtime.block_on(async move {
            let signal=signal();tokio::pin!(signal);
            // 首次轮询使真实Ctrl+C注册先于执行；注册失败不能继续派发。
            let initial=std::future::poll_fn(|cx|Poll::Ready(match signal.as_mut().poll(cx){
                Poll::Ready(value)=>Some(value),Poll::Pending=>None,
            })).await;
            if let Some(result)=initial {
                match result {Ok(())=>{cancel.cancel();let _=ready.send(Ok(()));},Err(error)=>{let _=ready.send(Err(error.to_string()));}}
                return;
            }
            if ready.send(Ok(())).is_err(){return;}
            loop {
                if stop.load(Ordering::Acquire){break;}
                tokio::select! {
                    result=&mut signal=>{
                        if let Err(error)=result {eprintln!("CLI取消监听失败：{error}");}
                        cancel.cancel();break;
                    }
                    _=tokio::time::sleep(Duration::from_millis(20))=>{}
                }
            }
        });
    })?;
    struct Guard {stop:Arc<AtomicBool>,worker:Option<std::thread::JoinHandle<()>>}
    impl Drop for Guard {fn drop(&mut self){self.stop.store(true,Ordering::Release);if let Some(worker)=self.worker.take(){let _=worker.join();}}}
    let _guard=Guard{stop:stopped,worker:Some(worker)};
    started.recv().map_err(|_|io::Error::other("CLI取消监听未确认启动"))?
        .map_err(io::Error::other)?;
    Ok(with_execution_control(control,execute))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_signal_registration_installs_and_restores_root_control() {
        run(||assert!(current_execution_control().is_some())).unwrap();
        assert!(current_execution_control().is_none());
    }
    #[test]
    fn startup_failure_does_not_execute_and_scope_restores_parent() {
        let result=with_signal(||async {Err(io::Error::other("register failed"))},||panic!("注册失败不得派发"));
        assert!(result.is_err());assert!(current_execution_control().is_none());
    }
    #[test]
    fn signal_cancels_only_this_turn_and_preserves_parent_deadline() {
        with_signal(||async {Ok(())},||assert!(current_execution_control().unwrap().interruption().is_some())).unwrap();
        with_signal(||std::future::pending::<io::Result<()>>(),||assert!(current_execution_control().unwrap().interruption().is_none())).unwrap();
        let parent=ExecutionControl::new(Some(Duration::ZERO),None);
        with_execution_control(parent,||with_signal(||std::future::pending::<io::Result<()>>(),||assert_eq!(current_execution_control().unwrap().interruption(),Some(runtime::managed_process::Interruption::TimedOut))).unwrap());
    }
}
