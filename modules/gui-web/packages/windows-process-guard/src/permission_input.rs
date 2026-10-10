//! 读取调用线程的Windows标准输入；取消后回收自有监听线程。
use std::{io,sync::Arc};
use windows_sys::Win32::System::Console::{GetStdHandle,STD_INPUT_HANDLE};

/// 借用标准输入；取消谓词必须快速返回且保持取消状态，不执行阻塞I/O。
/// 读取最多8192个字节或UTF-16单位，返回前回收自身取消监听线程。
pub fn read_permission_line(cancel:Arc<dyn Fn()->bool+Send+Sync>)->io::Result<String> {
    unsafe { windows::read_line(GetStdHandle(STD_INPUT_HANDLE),cancel) }
}
#[cfg(windows)]
mod windows {
    use std::io;
    use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::Duration};
    use std::sync::Arc as Shared;
    use windows_sys::Win32::{Foundation::{HANDLE,CloseHandle,DuplicateHandle,DUPLICATE_SAME_ACCESS,ERROR_BROKEN_PIPE},
        System::{Threading::{GetCurrentProcess,GetCurrentThread},IO::CancelSynchronousIo,
            Console::{GetConsoleMode,ReadConsoleW}},Storage::FileSystem::ReadFile};

    struct ThreadHandle(HANDLE);
    // 独占重复句柄只用于取消指定读取线程；最终在监听线程退出后关闭。
    unsafe impl Send for ThreadHandle {}
    impl Drop for ThreadHandle {fn drop(&mut self){unsafe {CloseHandle(self.0);}}}
    impl ThreadHandle {fn cancel(&self){unsafe {CancelSynchronousIo(self.0);}}}
    struct Watch {stop:Arc<AtomicBool>,worker:Option<std::thread::JoinHandle<()>>}
    impl Drop for Watch {
        fn drop(&mut self){
            self.stop.store(true,Ordering::Release);
            if let Some(worker)=self.worker.take(){let _=worker.join();}
        }
    }
    fn interrupted(cancel:&(dyn Fn()->bool+Send+Sync))->io::Result<()> {
        if cancel() {
            return Err(io::Error::other("权限输入已中断"));
        }
        Ok(())
    }
    // 调用方保证输入句柄在整个读取期间有效；读取始终在原线程进行。
    pub(super) unsafe fn read_line(input:HANDLE,cancel:Shared<dyn Fn()->bool+Send+Sync>)->io::Result<String> {
        interrupted(cancel.as_ref())?;
        let _watch={
            let mut handle=std::ptr::null_mut();
            if unsafe {DuplicateHandle(GetCurrentProcess(),GetCurrentThread(),GetCurrentProcess(),&mut handle,0,0,DUPLICATE_SAME_ACCESS)}==0 {
                return Err(io::Error::last_os_error());
            }
            let handle=ThreadHandle(handle);
            let stop=Arc::new(AtomicBool::new(false));
            let stopped=stop.clone();
            let control=cancel.clone();
            let worker=std::thread::Builder::new().name("cli-permission-input".into()).spawn(move || {
                while !stopped.load(Ordering::Acquire) {
                    // 重复取消覆盖检查后、内核读取前的竞态；不假定一次取消已使I/O终止。
                    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(||control())).unwrap_or(true){handle.cancel();}
                    std::thread::sleep(Duration::from_millis(20));
                }
            })?;
            Some(Watch{stop,worker:Some(worker)})
        };
        let mut mode=0;
        let console=unsafe {GetConsoleMode(input,&mut mode)}!=0;
        let mut wide=Vec::new();
        let mut bytes=Vec::new();
        loop {
            interrupted(cancel.as_ref())?;
            let mut count=0;
            let mut unit=0u16;
            let mut byte=0u8;
            let success=if console {
                unsafe {ReadConsoleW(input,(&mut unit as *mut u16).cast(),1,&mut count,std::ptr::null())}
            } else {
                unsafe {ReadFile(input,&mut byte,1,&mut count,std::ptr::null_mut())}
            };
            interrupted(cancel.as_ref())?;
            if success==0 {
                let error=io::Error::last_os_error();
                if !console && error.raw_os_error()==Some(ERROR_BROKEN_PIPE as i32){break;}
                return Err(error);
            }
            if count==0 {break;}
            if console {wide.push(unit);} else {bytes.push(byte);}
            if (console && unit==10) || (!console && byte==10){break;}
            if wide.len()+bytes.len()>8192 {return Err(io::Error::other("权限输入超过8192单位上限"));}
        }
        if console {String::from_utf16(&wide).map_err(|e|io::Error::new(io::ErrorKind::InvalidData,e))}
        else {String::from_utf8(bytes).map_err(|e|io::Error::new(io::ErrorKind::InvalidData,e))}
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows_sys::Win32::{System::Pipes::CreatePipe,Storage::FileSystem::WriteFile};
        struct PipeHandle(HANDLE);
        unsafe impl Send for PipeHandle {}
        impl Drop for PipeHandle {fn drop(&mut self){unsafe {CloseHandle(self.0);}}}
        impl PipeHandle {
            fn write(&self,bytes:&[u8]) {let mut n=0;assert_ne!(unsafe {WriteFile(self.0,bytes.as_ptr(),bytes.len() as u32,&mut n,std::ptr::null_mut())},0);}
            fn hold(&self){}
        }
        fn pipe()->(PipeHandle,PipeHandle) {
            let(mut read,mut write)=(std::ptr::null_mut(),std::ptr::null_mut());
            assert_ne!(unsafe {CreatePipe(&mut read,&mut write,std::ptr::null(),0)},0);
            (PipeHandle(read),PipeHandle(write))
        }
        #[test]
        fn blocked_pipe_is_cancelled_before_writer_closes() {
            let(read,write)=pipe();
            let flag=Arc::new(AtomicBool::new(false));
            let observed=flag.clone();
            let writer=std::thread::spawn(move || {
                write.hold();std::thread::sleep(Duration::from_millis(100));flag.store(true,Ordering::Release);
                // 失败时也能结束测试；自然关闭不能充当取消成功。
                std::thread::sleep(Duration::from_secs(2));drop(write);
            });
            let start=std::time::Instant::now();
            let result=unsafe {read_line(read.0,Arc::new(move ||observed.load(Ordering::Acquire)))};
            let elapsed=start.elapsed();writer.join().unwrap();
            assert!(result.unwrap_err().to_string().contains("已中断"));
            assert!(elapsed<Duration::from_secs(1),"必须在写端关闭前取消：{elapsed:?}");
        }
        #[test]
        fn approval_does_not_consume_next_line() {
            let(read,write)=pipe();write.write(b"yes\nno\n");
            assert_eq!(unsafe {read_line(read.0,Arc::new(||false))}.unwrap(),"yes\n");
            assert_eq!(unsafe {read_line(read.0,Arc::new(||false))}.unwrap(),"no\n");
        }
        #[test]
        fn cancelled_input_is_not_read() {
            let(read,write)=pipe();write.write(b"yes\n");
            assert!(unsafe {read_line(read.0,Arc::new(||true))}.is_err());
            assert_eq!(unsafe {read_line(read.0,Arc::new(||false))}.unwrap(),"yes\n");
        }
        #[test]
        fn final_line_without_newline_is_preserved_at_pipe_eof() {
            let(read,write)=pipe();write.write(b"yes");drop(write);
            assert_eq!(unsafe {read_line(read.0,Arc::new(||false))}.unwrap(),"yes");
        }
    }
}
