//! 有界文件诊断输出；不用于会话、用量或输入安全等权威审计数据。
use std::{fs::{self, File, OpenOptions}, io::{self, Write}, path::{Path, PathBuf}};

pub(crate) const FILE_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const RECORD_BYTES: usize = 64 * 1024;
const BACKUPS: usize = 3;

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn rename_if_exists(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

/// 同目录锁文件不轮转，所有合作进程每条写入前重新打开活动文件。
/// 锁竞争或磁盘错误立即返回，诊断失败不阻塞业务预算，也不递归写日志。
pub(crate) fn append(path: &Path, record: &[u8]) -> io::Result<()> {
    append_with_limit(path, record, FILE_BYTES)
}

fn append_with_limit(path: &Path, record: &[u8], limit: u64) -> io::Result<()> {
    if record.len() as u64 > limit {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "诊断记录超过文件上限"));
    }
    let lock = OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .open(sibling(path, ".lock"))?;
    lock.try_lock().map_err(|error| io::Error::other(error.to_string()))?;
    // lock文件句柄保持到函数结束；活动文件句柄只在此锁范围内存活，兼容Windows改名。
    let bytes = match fs::metadata(path) {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(error),
    };
    if bytes.saturating_add(record.len() as u64) > limit {
        remove_if_exists(&sibling(path, &format!(".{BACKUPS}")))?;
        for generation in (1..BACKUPS).rev() {
            rename_if_exists(&sibling(path, &format!(".{generation}")), &sibling(path, &format!(".{}", generation + 1)))?;
        }
        rename_if_exists(path, &sibling(path, ".1"))?;
    }
    let mut file: File = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 轮转保留整条记录且按新旧顺序淘汰最早代() {
        let root = std::env::temp_dir().join(format!("coolzhu-rotation-{}-{}", std::process::id(), crate::unix_millis()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("events.jsonl");
        for number in 0..7 {
            append_with_limit(&path, format!("{{\"序号\":{number}}}\n").as_bytes(), 18).unwrap();
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"序号\":6}\n");
        for generation in 1..=3 {
            assert_eq!(fs::read_to_string(sibling(&path, &format!(".{generation}"))).unwrap(), format!("{{\"序号\":{}}}\n", 6-generation));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn 独立锁句柄竞争立即失败且不改日志随后可继续写入() {
        let root = std::env::temp_dir().join(format!("coolzhu-lock-{}-{}", std::process::id(), crate::unix_millis()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("events.jsonl");
        append_with_limit(&path, b"before\n", 64).unwrap();
        let lock = OpenOptions::new().read(true).write(true).open(sibling(&path, ".lock")).unwrap();
        lock.lock().unwrap();
        assert!(append_with_limit(&path, b"blocked\n", 64).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"before\n");
        drop(lock);
        append_with_limit(&path, b"after\n", 64).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"before\nafter\n");
        fs::remove_dir_all(root).unwrap();
    }
}
