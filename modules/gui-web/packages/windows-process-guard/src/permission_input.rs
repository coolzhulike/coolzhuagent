//! 读取调用线程的Windows标准输入；取消后回收自有监听线程。
use std::{io,sync::Arc};

/// 首次读取时持有标准输入的独占重复句柄及行边界状态。
/// 取消谓词必须快速返回且保持取消状态，不执行阻塞I/O。
/// 读取最多8192个字节或UTF-16单位，返回前回收自身取消监听线程。
pub fn read_permission_line(cancel:Arc<dyn Fn()->bool+Send+Sync>)->io::Result<String> {
    windows::read_stdin(cancel)
}
#[cfg(windows)]
mod windows {
    use std::io;
    use std::{sync::{Arc,atomic::{AtomicBool,Ordering}},time::Duration};
    use std::sync::Arc as Shared;
    use windows_sys::Win32::{Foundation::{HANDLE,CloseHandle,DuplicateHandle,DUPLICATE_SAME_ACCESS,ERROR_BROKEN_PIPE},
        System::{Threading::{GetCurrentProcess,GetCurrentThread},IO::CancelSynchronousIo,
            Console::{GetConsoleMode,ReadConsoleW,GetStdHandle,STD_INPUT_HANDLE}},Storage::FileSystem::ReadFile};

    struct InputHandle(HANDLE);
    // 独占重复句柄保证跨回合仍指向同一内核输入对象，不向子进程继承。
    unsafe impl Send for InputHandle {}
    impl Drop for InputHandle {fn drop(&mut self){unsafe {CloseHandle(self.0);}}}
    struct Reader {input:InputHandle,incomplete:bool}
    impl Reader {
        unsafe fn duplicate(input:HANDLE)->io::Result<Self> {
            let mut handle=std::ptr::null_mut();
            if unsafe {DuplicateHandle(GetCurrentProcess(),input,GetCurrentProcess(),&mut handle,0,0,DUPLICATE_SAME_ACCESS)}==0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self{input:InputHandle(handle),incomplete:false})
        }
        fn read(&mut self,cancel:Shared<dyn Fn()->bool+Send+Sync>)->io::Result<String> {
            if self.incomplete {
                // 不收集旧行内容；即使后缀迟到，也必须先恢复到行边界。
                unsafe {read_line_state(self.input.0,cancel.clone(),&mut self.incomplete,true)}?;
            }
            unsafe {read_line_state(self.input.0,cancel,&mut self.incomplete,false)}
        }
    }
    pub(super) fn read_stdin(cancel:Shared<dyn Fn()->bool+Send+Sync>)->io::Result<String> {
        static INPUT:std::sync::Mutex<Option<Reader>>=std::sync::Mutex::new(None);
        interrupted(cancel.as_ref())?;
        let mut owner=INPUT.try_lock().map_err(|_|io::Error::other("权限输入已有读取者或状态损坏"))?;
        if owner.is_none(){*owner=Some(unsafe {Reader::duplicate(GetStdHandle(STD_INPUT_HANDLE))}?);}
        owner.as_mut().unwrap().read(cancel)
    }

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
    #[cfg(test)]
    unsafe fn read_line(input:HANDLE,cancel:Shared<dyn Fn()->bool+Send+Sync>)->io::Result<String> {
        unsafe {read_line_state(input,cancel,&mut false,false)}
    }
    unsafe fn read_line_state(input:HANDLE,cancel:Shared<dyn Fn()->bool+Send+Sync>,incomplete:&mut bool,discard:bool)->io::Result<String> {
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
            *incomplete=true;
            let success=if console {
                unsafe {ReadConsoleW(input,(&mut unit as *mut u16).cast(),1,&mut count,std::ptr::null())}
            } else {
                unsafe {ReadFile(input,&mut byte,1,&mut count,std::ptr::null_mut())}
            };
            if success==0 {
                let error=io::Error::last_os_error();
                if !console && error.raw_os_error()==Some(ERROR_BROKEN_PIPE as i32){*incomplete=false;interrupted(cancel.as_ref())?;break;}
                interrupted(cancel.as_ref())?;
                return Err(error);
            }
            let ended=count==0 || (console && unit==10) || (!console && byte==10);
            if ended {*incomplete=false;}
            interrupted(cancel.as_ref())?;
            if count==0 {break;}
            if discard {if ended {break;} else {continue;}}
            if console {wide.push(unit);} else {bytes.push(byte);}
            if wide.len()+bytes.len()>8192 {return Err(io::Error::other("权限输入超过8192单位上限"));}
            if ended {break;}
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
        fn isolated_console_ctrl_c_ends_permission_read() {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
            let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp");
            std::fs::create_dir_all(&root).unwrap();
            let directory=tempfile::Builder::new().prefix("permission-console-").tempdir_in(root).unwrap().keep();
            let proof=directory.join("console-proof.txt");
            let stderr=directory.join("stderr.txt");
            let mut child=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact","permission_input::windows::tests::isolated_console_ctrl_c_probe","--ignored"])
                .env("COOLZHU_PERMISSION_CONSOLE_PROOF",&proof)
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(std::fs::File::create(directory.join("stdout.txt")).unwrap())
                .stderr(std::fs::File::create(&stderr).unwrap())
                .spawn().unwrap();
            let limit=std::time::Instant::now()+Duration::from_secs(10);
            let status=loop {
                if let Some(status)=child.try_wait().unwrap(){break status;}
                if std::time::Instant::now()>=limit {
                    child.kill().unwrap();child.wait().unwrap();
                    panic!("独立控制台探针超时，仅回收本测试子进程");
                }
                std::thread::sleep(Duration::from_millis(20));
            };
            assert!(status.success(),"独立探针失败：{status}；日志{}",stderr.display());
            assert_eq!(std::fs::read_to_string(proof).unwrap(),"own-console;ctrl-c-observed;read-rejected;listener-joined;owned-reader");
        }
        #[test]
        #[ignore="只由父测试在无共享控制台的独立子进程中运行"]
        fn isolated_console_ctrl_c_probe() {
            use std::{future::Future,task::Poll};
            use windows_sys::Win32::{System::Console::{AllocConsole,FreeConsole,GetConsoleWindow,
                GenerateConsoleCtrlEvent,CTRL_C_EVENT,SetConsoleCtrlHandler},
                Foundation::{GENERIC_READ,INVALID_HANDLE_VALUE},
                Storage::FileSystem::{CreateFileW,FILE_SHARE_READ,FILE_SHARE_WRITE,OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL},
                UI::WindowsAndMessaging::{ShowWindow,SW_HIDE}};
            let proof=std::env::var_os("COOLZHU_PERMISSION_CONSOLE_PROOF").expect("必须由父测试启动");
            let panic_path=std::path::PathBuf::from(&proof).with_extension("panic.txt");
            std::panic::set_hook(Box::new(move |info| {let _=std::fs::write(&panic_path,info.to_string());}));
            // 仅子进程解除自身可能继承的附着，再创建独占控制台；父进程不受影响。
            unsafe {FreeConsole();}
            assert_ne!(unsafe {AllocConsole()},0,"无法分配独立控制台：{}",io::Error::last_os_error());
            unsafe {ShowWindow(GetConsoleWindow(),SW_HIDE);}
            assert_ne!(unsafe {SetConsoleCtrlHandler(None,0)},0);
            // 重定向的标准句柄不会自动变成新控制台输入，显式打开本控制台CONIN$。
            let name:Vec<u16>="CONIN$\0".encode_utf16().collect();
            let console=PipeHandle(unsafe {CreateFileW(name.as_ptr(),GENERIC_READ,
                FILE_SHARE_READ|FILE_SHARE_WRITE,std::ptr::null(),OPEN_EXISTING,FILE_ATTRIBUTE_NORMAL,std::ptr::null_mut())});
            assert_ne!(console.0,INVALID_HANDLE_VALUE);
            let input=console.0;
            assert_ne!(unsafe {windows_sys::Win32::System::Console::SetStdHandle(STD_INPUT_HANDLE,input)},0);
            let mut mode=0;
            assert_ne!(unsafe {GetConsoleMode(input,&mut mode)},0,"输入必须是真控制台");
            let flag=Arc::new(AtomicBool::new(false));
            let observed=flag.clone();
            let (ready,started)=std::sync::mpsc::sync_channel(1);
            let listener=std::thread::spawn(move || {
                let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                runtime.block_on(async {
                    let signal=tokio::signal::ctrl_c();tokio::pin!(signal);
                    std::future::poll_fn(|cx| {
                        assert!(signal.as_mut().poll(cx).is_pending());
                        Poll::Ready(())
                    }).await;
                    ready.send(()).unwrap();
                    tokio::time::timeout(Duration::from_secs(3),signal).await.unwrap().unwrap();
                    flag.store(true,Ordering::Release);
                });
            });
            started.recv_timeout(Duration::from_secs(3)).unwrap();
            let sender=std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(100));
                // group=0仅广播给刚创建的本子进程专属控制台，未附着共享控制台。
                assert_ne!(unsafe {GenerateConsoleCtrlEvent(CTRL_C_EVENT,0)},0);
            });
            let start=std::time::Instant::now();
            let cancelled=observed.clone();
            let result=read_stdin(Arc::new(move ||cancelled.load(Ordering::Acquire)));
            sender.join().unwrap();listener.join().unwrap();
            assert!(result.is_err(),"Ctrl+C不得成为允许输入");
            assert!(observed.load(Ordering::Acquire),"必须观察实际Tokio信号");
            assert!(start.elapsed()<Duration::from_secs(2));
            std::fs::write(proof,"own-console;ctrl-c-observed;read-rejected;listener-joined;owned-reader").unwrap();
            assert_ne!(unsafe {FreeConsole()},0);
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
        #[test]
        fn newline_counts_toward_input_budget() {
            for length in [8192usize,8193] {
                let(read,write)=pipe();
                let writer=std::thread::spawn(move || {
                    let mut line=vec![b'x';length-1];line.push(b'\n');write.write(&line);
                });
                let result=unsafe {read_line(read.0,Arc::new(||false))};
                writer.join().unwrap();
                if length==8192 {assert_eq!(result.unwrap().len(),length);}
                else {assert!(result.unwrap_err().to_string().contains("8192"));}
            }
        }
        #[test]
        fn known_gap_rejected_line_suffix_reaches_next_reader() {
            // 历史无状态底层反例；生产入口现使用持续持有状态的Reader。
            let(read,write)=pipe();
            let writer=std::thread::spawn(move || {
                let mut line=vec![b'x';8193];
                line.extend_from_slice(b"yes\nno\n");
                write.write(&line);
            });
            let first=unsafe {read_line(read.0,Arc::new(||false))};
            let suffix=unsafe {read_line(read.0,Arc::new(||false))};
            let following=unsafe {read_line(read.0,Arc::new(||false))};
            writer.join().unwrap();
            assert!(first.is_err());
            assert_eq!(suffix.unwrap(),"yes\n","旧行后缀被当作完整新行");
            assert_eq!(following.unwrap(),"no\n");
        }
        #[test]
        fn owned_reader_discards_delayed_rejected_suffix() {
            let(read,write)=pipe();
            let mut reader=unsafe {Reader::duplicate(read.0)}.unwrap();
            drop(read); // 原借用句柄释放后，独占重复句柄仍保证输入对象身份。
            let writer=std::thread::spawn(move || {
                write.write(&vec![b'x';8193]);
                std::thread::sleep(Duration::from_millis(100));
                write.write(b"yes\nno\n");
            });
            let first=reader.read(Arc::new(||false));
            let next=reader.read(Arc::new(||false));
            writer.join().unwrap();
            assert!(first.is_err());
            assert_eq!(next.unwrap(),"no\n","迟到yes属于旧行，不能成为审批");
            assert!(!reader.incomplete);
        }
        #[test]
        fn owned_reader_recovers_partial_cancelled_line() {
            let(read,write)=pipe();
            let mut reader=unsafe {Reader::duplicate(read.0)}.unwrap();
            let flag=Arc::new(AtomicBool::new(false));
            let observed=flag.clone();
            let writer=std::thread::spawn(move || {
                write.write(b"old");
                std::thread::sleep(Duration::from_millis(100));flag.store(true,Ordering::Release);
                std::thread::sleep(Duration::from_millis(100));write.write(b"yes\nno\n");
            });
            let first=reader.read(Arc::new(move ||observed.load(Ordering::Acquire)));
            let next=reader.read(Arc::new(||false));
            writer.join().unwrap();
            assert!(first.is_err());
            assert_eq!(next.unwrap(),"no\n");
            assert!(!reader.incomplete);
        }
    }
}
