use std::{io, path::{Path, PathBuf}};

/// 服务端 URI 会小写 Windows 盘符；先解析真实文件，再统一盘符，不能小写整个路径。
/// 保留目录字节和逐组件边界，避免把区分大小写的目录或同前缀邻居误当作工作区。
pub fn canonical_document_path(path: &Path) -> io::Result<PathBuf> {
    let canonical = path.canonicalize()?;
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        let mut parts = canonical.components();
        if let Some(Component::Prefix(prefix)) = parts.next() {
            let head = match prefix.kind() {
                Prefix::VerbatimDisk(drive) => Some(format!("\\\\?\\{}:", drive.to_ascii_uppercase() as char)),
                Prefix::Disk(drive) => Some(format!("{}:", drive.to_ascii_uppercase() as char)),
                _ => None,
            };
            if let Some(head) = head {
                let mut normalized = PathBuf::from(head);
                normalized.push(parts.as_path());
                return Ok(normalized);
            }
        }
    }
    Ok(canonical)
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn analyzer_lowercase_drive_preserves_workspace_boundary() {
        let root = std::env::current_dir().unwrap().canonicalize().unwrap();
        let root_text = root.to_string_lossy();
        let mut lower = root_text.as_bytes().to_vec();
        assert_eq!(&lower[..4], b"\\\\?\\");
        lower[4] = lower[4].to_ascii_lowercase();
        let upper = super::canonical_document_path(&root).unwrap();
        let lower = super::canonical_document_path(std::path::Path::new(std::str::from_utf8(&lower).unwrap())).unwrap();
        assert_eq!(lower, upper);
        assert!(lower.join("src").strip_prefix(&upper).is_ok());
        let sibling = upper.with_file_name(format!("{}-outside", upper.file_name().unwrap().to_string_lossy()));
        assert!(!sibling.starts_with(&upper));
    }
}
