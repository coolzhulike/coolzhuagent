//! 显式低完整性执行边界：禁止写入普通中完整性对象，保留 Job 生命周期约束。
//! 这不是读隔离或网络隔离；只给新建的专用工作目录设置低完整性，不改已有用户文件。
use std::{fs, io, os::windows::{ffi::OsStrExt, io::AsRawHandle}, path::{Path,PathBuf}, process::Child,
    sync::atomic::{AtomicU64,Ordering}};
use windows_sys::Win32::{Foundation::{CloseHandle,HANDLE,LocalFree}, Security::{
    GetLengthSid,SetTokenInformation,TokenIntegrityLevel,TOKEN_ADJUST_DEFAULT,TOKEN_QUERY,
    TOKEN_MANDATORY_LABEL,
    Authorization::{ConvertStringSidToSidW,ConvertStringSecurityDescriptorToSecurityDescriptorW,SetNamedSecurityInfoW,SE_FILE_OBJECT},
    GetSecurityDescriptorSacl,GetSecurityDescriptorDacl,LABEL_SECURITY_INFORMATION,DACL_SECURITY_INFORMATION,PROTECTED_DACL_SECURITY_INFORMATION,
}, System::Threading::OpenProcessToken};

// WinNT.h 的 SE_GROUP_INTEGRITY；windows-sys 将该常量置于未启用的 SystemServices 特性。
const SE_GROUP_INTEGRITY:u32=0x0000_0020;

struct OwnedHandle(HANDLE);
impl Drop for OwnedHandle {fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
struct LocalAllocation(*mut std::ffi::c_void);
impl Drop for LocalAllocation {fn drop(&mut self){unsafe{LocalFree(self.0);}}}
fn wide(value:&std::ffi::OsStr)->Vec<u16>{value.encode_wide().chain(Some(0)).collect()}

/// 只能用于仍挂起、尚未执行用户代码的子进程；失败由外层 Job 回收。
pub(crate) fn lower_suspended_child(child:&Child)->io::Result<()> {
    let mut raw=std::ptr::null_mut();
    if unsafe{OpenProcessToken(child.as_raw_handle() as HANDLE,TOKEN_ADJUST_DEFAULT|TOKEN_QUERY,&mut raw)}==0 {
        return Err(io::Error::last_os_error());
    }
    let token=OwnedHandle(raw);let mut sid=std::ptr::null_mut();let name=wide(std::ffi::OsStr::new("S-1-16-4096"));
    if unsafe{ConvertStringSidToSidW(name.as_ptr(),&mut sid)}==0{return Err(io::Error::last_os_error());}
    let _allocation=LocalAllocation(sid);
    let label=TOKEN_MANDATORY_LABEL{Label:windows_sys::Win32::Security::SID_AND_ATTRIBUTES{Sid:sid,Attributes:SE_GROUP_INTEGRITY}};
    let len=std::mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32+unsafe{GetLengthSid(sid)};
    if unsafe{SetTokenInformation(token.0,TokenIntegrityLevel,(&label as *const TOKEN_MANDATORY_LABEL).cast(),len)}==0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// 在调用方指定的现存父目录内新建独占目录；仅修改该目录的强制标签。
/// 返回目录供本次命令使用。保留输出，清理由上层按引用规则处理。
pub fn create_low_integrity_workspace(parent:&Path)->io::Result<PathBuf> {
    if !fs::symlink_metadata(parent)?.file_type().is_dir(){return Err(io::Error::new(io::ErrorKind::InvalidInput,"隔离目录父路径必须为普通目录"));}
    let parent=parent.canonicalize()?;
    static NEXT:AtomicU64=AtomicU64::new(0);
    let stamp=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let root=parent.join(format!("low-integrity-{}-{stamp}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    fs::create_dir(&root)?;
    // D盘父目录只继承 Modify，缺 WRITE_OWNER，不能直接设置强制标签。
    // 仅本函数刚创建的私有目录给所有者完整权限；不扩大已有目录/其他用户的权限。
    let mut descriptor=std::ptr::null_mut();let text=wide(std::ffi::OsStr::new("D:P(A;OICI;FA;;;OW)S:(ML;OICI;NW;;;LW)"));
    if unsafe{ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(),1,&mut descriptor,std::ptr::null_mut())}==0 {
        return Err(io::Error::last_os_error());
    }
    let _allocation=LocalAllocation(descriptor);
    let mut present=0;let mut defaulted=0;let mut sacl=std::ptr::null_mut();
    if unsafe{GetSecurityDescriptorSacl(descriptor,&mut present,&mut sacl,&mut defaulted)}==0 || present==0 {
        return Err(io::Error::last_os_error());
    }
    let path=wide(root.as_os_str());
    let mut dacl=std::ptr::null_mut();
    if unsafe{GetSecurityDescriptorDacl(descriptor,&mut present,&mut dacl,&mut defaulted)}==0 || present==0 {
        return Err(io::Error::last_os_error());
    }
    let code=unsafe{SetNamedSecurityInfoW(path.as_ptr(),SE_FILE_OBJECT,DACL_SECURITY_INFORMATION|PROTECTED_DACL_SECURITY_INFORMATION,
        std::ptr::null_mut(),std::ptr::null_mut(),dacl,std::ptr::null())};
    if code!=0{return Err(io::Error::from_raw_os_error(code as i32));}
    let code=unsafe{SetNamedSecurityInfoW(path.as_ptr(),SE_FILE_OBJECT,LABEL_SECURITY_INFORMATION,
        std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null(),sacl)};
    if code!=0{return Err(io::Error::from_raw_os_error(code as i32));}
    Ok(root)
}

#[cfg(test)]
mod tests {
    #[test]
    fn low_integrity_allows_scratch_and_denies_medium_write() {
        use std::{fs,process::{Command,Stdio}};
        let root=tempfile::tempdir().unwrap();
        let allowed=super::create_low_integrity_workspace(root.path()).unwrap();
        let blocked=root.path().join("protected.txt");fs::write(&blocked,b"unchanged").unwrap();
        let mut command=Command::new("cmd.exe");
        command.args(["/D","/C","(echo allowed> allowed.txt) && (echo forbidden> \"%DENIED_FILE%\")"])
            .current_dir(&allowed).env("DENIED_FILE",&blocked).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        let (mut child,job)=super::super::ChildProcessJob::spawn_managed_low_integrity(&mut command).unwrap();
        let status=child.wait().unwrap();drop(job);
        assert!(!status.success(),"拒绝写入必须反映在退出码");
        assert_eq!(fs::read(&blocked).unwrap(),b"unchanged");
        assert!(fs::read_to_string(allowed.join("allowed.txt")).unwrap().contains("allowed"));
    }
}
