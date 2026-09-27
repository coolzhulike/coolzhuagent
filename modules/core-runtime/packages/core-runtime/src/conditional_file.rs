//! 条件文件发布：版本是原始字节摘要；产品调用共享文件锁，外部编辑器仍可能竞争。
use std::{fs::{self, File, OpenOptions}, io::{self, Write}, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileContentVersion {
    pub sha256: String,
    pub byte_length: u64,
}

pub fn file_content_version(bytes: &[u8]) -> FileContentVersion {
    FileContentVersion { sha256: format!("{:x}", Sha256::digest(bytes)), byte_length: bytes.len() as u64 }
}

/// 持有到发布完成；锁文件保留，不能删除后重建造成两个不同 inode 的锁。
pub(crate) fn lock_file_mutation(path: &Path) -> io::Result<File> {
    let root = std::env::temp_dir().join("coolzhu-file-locks-v1");
    fs::create_dir_all(&root)?;
    if fs::symlink_metadata(&root)?.file_type().is_symlink() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "文件锁目录不能为符号链接"));
    }
    let identity = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    let identity = identity.to_lowercase();
    let lock_path = root.join(format!("{}.lock", file_content_version(identity.as_bytes()).sha256));
    if fs::symlink_metadata(&lock_path).is_ok_and(|meta| !meta.file_type().is_file()) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "文件锁不是普通文件"));
    }
    let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(lock_path)?;
    lock.try_lock().map_err(|error| io::Error::new(io::ErrorKind::WouldBlock,
        format!("FileBusy: 同一文件正在修改，未写入：{error}")))?;
    Ok(lock)
}

pub(crate) fn read_expected(path: &Path, expected: Option<&str>) -> io::Result<Option<Vec<u8>>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    match (bytes.as_deref(), expected) {
        (Some(_), None) => return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "ExpectedVersionRequired: 文件已存在；请先 read_file，再传 expected_version=version.sha256")),
        (Some(bytes), Some(expected)) if file_content_version(bytes).sha256 != expected =>
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "VersionMismatch: 文件已变化；请重新 read_file 后决定修改内容")),
        (None, Some(_)) => return Err(io::Error::new(io::ErrorKind::NotFound,
            "VersionMismatch: 原文件已不存在，未把修改请求转换成新建")),
        _ => {},
    }
    Ok(bytes)
}

/// 调用方持有 lock_file_mutation；发布前复核，缺版本只允许原子新建。
pub(crate) fn publish_locked(path: &Path, bytes: &[u8], expected: Option<&str>) -> io::Result<()> {
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "文件缺少父目录"))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    if let Ok(metadata) = fs::metadata(path) {
        if metadata.permissions().readonly() { return Err(io::Error::new(io::ErrorKind::PermissionDenied, "文件为只读")); }
        staged.as_file().set_permissions(metadata.permissions())?;
    }
    staged.as_file().sync_all()?;
    read_expected(path, expected)?;
    let published = if expected.is_some() { staged.persist(path) } else { staged.persist_noclobber(path) };
    published.map_err(|error| io::Error::new(error.error.kind(), format!("AtomicPublishFailed: {}", error.error)))?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// 前端编辑器也复用同一版本与发布边界，避免与模型工具各自覆盖。
pub fn replace_file_if_version(path: &Path, bytes: &[u8], expected: &str) -> io::Result<FileContentVersion> {
    let path: PathBuf = path.canonicalize()?;
    let _lock = lock_file_mutation(&path)?;
    read_expected(&path, Some(expected))?;
    publish_locked(&path, bytes, Some(expected))?;
    Ok(file_content_version(bytes))
}
