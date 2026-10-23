//! 仅回收从未发布的内容写入残留。任何已发布内容均保留，覆盖未知的外部引用。
use std::{fs::{self,File,OpenOptions}, io::{self,Write}, path::{Path,PathBuf}};
use sha2::{Digest,Sha256};

const STAGING:&str=".content-staging-v1";
const PREFIX:&str="pending-v1-sha256-";

struct PendingBlob {
    temporary:tempfile::NamedTempFile,
    final_path:PathBuf,
    file_name:String,
    sha256:String,
    // 持锁直至原子发布；GC不能回收仍在写入的对象。进程结束后OS释放锁。
    _lock:File,
}

fn directory(root:&Path)->io::Result<PathBuf> {
    fs::create_dir_all(root)?;
    if !fs::symlink_metadata(root)?.file_type().is_dir(){return Err(io::Error::new(io::ErrorKind::PermissionDenied,"内容目录不能是链接"));}
    root.canonicalize()
}

fn lock_file(root:&Path)->io::Result<File> {
    let path=root.join(".content-write.lock");
    if let Ok(meta)=fs::symlink_metadata(&path){if !meta.file_type().is_file(){return Err(io::Error::new(io::ErrorKind::PermissionDenied,"内容锁不是普通文件"));}}
    let file=OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)?;
    Ok(file)
}

fn collect_locked(root:&Path)->io::Result<usize> {
    let staging=root.join(STAGING);
    if !staging.exists(){return Ok(0);}
    if !fs::symlink_metadata(&staging)?.file_type().is_dir(){return Err(io::Error::new(io::ErrorKind::PermissionDenied,"暂存目录不能是链接"));}
    let mut deleted=0;
    // 单次有界；未知文件、未完成写入、损坏对象保守保留，绝不扫描已发布目录。
    for entry in fs::read_dir(staging)?.take(128) {
        let entry=entry?;let name=entry.file_name();let name=name.to_string_lossy();
        let Some(tail)=name.strip_prefix(PREFIX).and_then(|s|s.strip_suffix(".part")) else{continue};
        let Some((digest,nonce))=tail.split_once('-') else{continue};
        if digest.len()!=64 || !digest.bytes().all(|b|b.is_ascii_hexdigit()) || nonce.len()<6 || !nonce.bytes().all(|b|b.is_ascii_alphanumeric()){continue;}
        if !entry.file_type()?.is_file(){continue;}
        if super::verify(&entry.path(),digest).is_err(){continue;}
        fs::remove_file(entry.path())?;deleted+=1;
    }
    Ok(deleted)
}

/// 不与在途写入争抢；任一调用者持锁时本轮跳过。未发布路径不会作为URL/工具原文返回。
pub(super) fn collect_unpublished(root:&Path)->io::Result<usize> {
    if !root.exists(){return Ok(0);}
    let root=directory(root)?;let lock=lock_file(&root)?;
    match lock.try_lock() {
        Ok(())=>collect_locked(&root),
        Err(std::fs::TryLockError::WouldBlock)=>Ok(0),
        Err(std::fs::TryLockError::Error(error))=>Err(error),
    }
}

fn stage(root:&Path,bytes:&[u8],extension:&str)->io::Result<PendingBlob> {
    if extension.is_empty() || extension.len()>16 || !extension.bytes().all(|c|c.is_ascii_alphanumeric()) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput,"内容对象扩展名无效"));
    }
    let root=directory(root)?;let lock=lock_file(&root)?;lock.lock()?;
    collect_locked(&root)?;
    let staging=directory(&root.join(STAGING))?;
    if staging.parent()!=Some(root.as_path()){return Err(io::Error::new(io::ErrorKind::PermissionDenied,"暂存路径越界"));}
    let sha256=format!("{:x}",Sha256::digest(bytes));
    let file_name=format!("sha256-{sha256}.{}",extension.to_ascii_lowercase());
    let mut temporary=tempfile::Builder::new().prefix(&format!("{PREFIX}{sha256}-")).suffix(".part").tempfile_in(staging)?;
    temporary.write_all(bytes)?;temporary.as_file().sync_all()?;
    super::verify(temporary.path(),&sha256)?;
    Ok(PendingBlob{temporary,final_path:root.join(&file_name),file_name,sha256,_lock:lock})
}

pub(super) fn put(root:&Path,bytes:&[u8],extension:&str)->io::Result<super::StoredBlob> {
    let pending=stage(root,bytes,extension)?;
    if let Err(error)=pending.temporary.persist_noclobber(&pending.final_path) {
        if error.error.kind()!=io::ErrorKind::AlreadyExists{return Err(error.error);}
    }
    super::verify(&pending.final_path,&pending.sha256)?;
    Ok(super::StoredBlob{path:pending.final_path,file_name:pending.file_name,sha256:pending.sha256})
}

#[cfg(test)]
mod tests {
    #[test]
    fn unpublished_orphan_removed_published_and_unknown_retained() {
        let root=tempfile::tempdir().unwrap();
        let published=super::put(root.path(),"已被会话、任务、导出或待发引用".as_bytes(),"txt").unwrap();
        let pending=super::stage(root.path(),b"unpublished orphan","txt").unwrap();
        assert!(!pending.final_path.exists(),"未发布对象不能通过正式文件URL访问");
        assert_eq!(super::collect_unpublished(root.path()).unwrap(),0,"在途锁必须阻止回收");
        let (file,path)=pending.temporary.keep().unwrap();drop(file);drop(pending._lock);
        let unknown=root.path().join(super::STAGING).join("unknown-user-file.txt");std::fs::write(&unknown,b"keep").unwrap();
        assert_eq!(super::collect_unpublished(root.path()).unwrap(),1);
        assert!(!path.exists());assert!(unknown.exists());assert!(published.path.exists());
        assert_eq!(std::fs::read(&published.path).unwrap(),"已被会话、任务、导出或待发引用".as_bytes());
        assert_eq!(super::collect_unpublished(root.path()).unwrap(),0);
    }
}
