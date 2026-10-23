//! 内容寻址原文存储；原子发布、拒绝覆盖已有损坏对象，只回收从未发布的完整暂存残留。
use std::{fs, io::{self, Read}, path::{Path, PathBuf}};
use sha2::{Digest, Sha256};

pub(crate) struct StoredBlob {
    pub(crate) path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) sha256: String,
}

#[path = "content_staging_gc.rs"]
mod staging_gc;

pub(crate) fn put(root: &Path, bytes: &[u8], extension: &str) -> io::Result<StoredBlob> {
    staging_gc::put(root, bytes, extension)
}

/// 只删除从未发布的完整暂存残留；已发布或未知引用对象一律保留。
pub(crate) fn collect_unpublished(root: &Path) -> io::Result<usize> {
    staging_gc::collect_unpublished(root)
}

pub(crate) fn digest_from_name(name: &str) -> Option<&str> {
    let value = name.strip_prefix("sha256-")?.split_once('.')?.0;
    (value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())).then_some(value)
}

/// 校验已读取的同一份字节，避免重开文件后另一次读取替换掉发送时证据。
/// 旧版非内容寻址附件没有摘要身份，继续由各入口的格式/快照校验处理。
pub(crate) fn verify_named_bytes(name: &str, bytes: &[u8]) -> io::Result<()> {
    if let Some(expected) = digest_from_name(name) {
        if !format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(expected) {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "附件内容对象摘要不匹配，请重新上传；未覆盖损坏文件"));
        }
    }
    Ok(())
}

pub(crate) fn verify(path: &Path, expected: &str) -> io::Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "内容对象不是普通文件"));
    }
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop { let len = file.read(&mut buffer)?; if len == 0 { break; } hasher.update(&buffer[..len]); }
    if format!("{:x}", hasher.finalize()) != expected {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "内容对象摘要不匹配，未覆盖损坏文件"));
    }
    Ok(())
}

pub(crate) fn extension(name: &str) -> &str {
    Path::new(name).extension().and_then(|s| s.to_str())
        .filter(|s| !s.is_empty() && s.len() <= 16 && s.bytes().all(|c| c.is_ascii_alphanumeric())).unwrap_or("bin")
}
