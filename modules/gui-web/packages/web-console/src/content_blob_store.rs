//! 内容寻址原文存储；原子发布、拒绝覆盖已有损坏对象，不负责权限与自动回收。
use std::{fs, io::{self, Read, Write}, path::{Path, PathBuf}};
use sha2::{Digest, Sha256};

pub(crate) struct StoredBlob {
    pub(crate) path: PathBuf,
    pub(crate) file_name: String,
    pub(crate) sha256: String,
}

pub(crate) fn put(root: &Path, bytes: &[u8], extension: &str) -> io::Result<StoredBlob> {
    if extension.is_empty() || extension.len() > 16 || !extension.bytes().all(|c| c.is_ascii_alphanumeric()) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "内容对象扩展名无效"));
    }
    fs::create_dir_all(root)?;
    if !fs::symlink_metadata(root)?.file_type().is_dir() {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "内容目录不能是链接"));
    }
    let root = root.canonicalize()?;
    let sha256 = format!("{:x}", Sha256::digest(bytes));
    let file_name = format!("sha256-{sha256}.{}", extension.to_ascii_lowercase());
    let path = root.join(&file_name);
    let mut temporary = tempfile::NamedTempFile::new_in(&root)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    verify(temporary.path(), &sha256)?;
    if let Err(error) = temporary.persist_noclobber(&path) {
        if error.error.kind() != io::ErrorKind::AlreadyExists { return Err(error.error); }
    }
    verify(&path, &sha256)?;
    Ok(StoredBlob { path, file_name, sha256 })
}

pub(crate) fn digest_from_name(name: &str) -> Option<&str> {
    let value = name.strip_prefix("sha256-")?.split_once('.')?.0;
    (value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit())).then_some(value)
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
