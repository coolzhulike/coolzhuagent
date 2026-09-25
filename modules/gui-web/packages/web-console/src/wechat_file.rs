use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const WECHAT_FILE_READ_LIMIT_BYTES: u64 = 3_000;
pub const WECHAT_FILE_ATTACHMENT_LIMIT_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WechatFileError {
    pub code: &'static str,
    pub message: String,
}

impl WechatFileError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WechatFileInfo {
    pub relative_path: String,
    pub size_bytes: u64,
    pub is_dir: bool,
    pub modified_at_ms: Option<u64>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WechatFileAttachment {
    pub relative_path: String,
    pub local_path: String,
    pub display_name: String,
    pub mime: Option<String>,
    pub size_bytes: u64,
    pub checksum: String,
}

pub fn execute_file_command(
    workspace_root: &Path,
    command_id: &str,
    arguments: &str,
) -> Result<String, WechatFileError> {
    match command_id {
        "file.list" => file_list(workspace_root, arguments.trim()),
        "file.info" => file_info_text(workspace_root, required_path(arguments, "file.info")?),
        "file.get" => file_get_text(workspace_root, required_path(arguments, "file.get")?),
        "file.write" => {
            let (path, content) = arguments.split_once('\n').ok_or_else(|| {
                WechatFileError::new(
                    "file_write_content_required",
                    "用法：/file write <路径> 后换行填写要写入的文本。",
                )
            })?;
            if content.trim().is_empty() {
                return Err(WechatFileError::new(
                    "file_write_content_required",
                    "写入内容不能为空。",
                ));
            }
            if content.len() as u64 > WECHAT_FILE_READ_LIMIT_BYTES {
                return Err(WechatFileError::new(
                    "file_write_too_large_for_text_reply",
                    format!(
                        "写入内容超过微信文本回传上限：{} bytes > {} bytes；请等待附件发送能力接通后再写入大文件。",
                        content.len(),
                        WECHAT_FILE_READ_LIMIT_BYTES
                    ),
                ));
            }
            let info = write_text_file(workspace_root, path.trim(), content)?;
            Ok(format!(
                "文件已写入：{}\n大小：{} bytes\n校验：{}\n---\n{}",
                info.relative_path,
                info.size_bytes,
                info.checksum.as_deref().unwrap_or("n/a"),
                content
            ))
        }
        _ => Err(WechatFileError::new(
            "file_command_unsupported",
            format!("文件命令尚未接通：{command_id}"),
        )),
    }
}

pub fn file_list(workspace_root: &Path, relative_path: &str) -> Result<String, WechatFileError> {
    let relative_path = if relative_path.trim().is_empty() {
        "."
    } else {
        relative_path.trim()
    };
    let directory = resolve_existing_path(workspace_root, relative_path)?;
    let metadata = fs::metadata(&directory)
        .map_err(|error| WechatFileError::new("file_stat_failed", error.to_string()))?;
    if !metadata.is_dir() {
        return Err(WechatFileError::new(
            "file_not_directory",
            format!("{relative_path} 不是目录。"),
        ));
    }
    let mut entries = fs::read_dir(&directory)
        .map_err(|error| WechatFileError::new("file_list_failed", error.to_string()))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.is_empty() {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            let suffix = if metadata.is_dir() { "/" } else { "" };
            Some(format!("{}{} · {} bytes", name, suffix, metadata.len()))
        })
        .collect::<Vec<_>>();
    entries.sort();
    if entries.is_empty() {
        Ok(format!("目录为空：{relative_path}"))
    } else {
        Ok(format!(
            "目录：{relative_path}\n{}",
            entries.into_iter().take(80).collect::<Vec<_>>().join("\n")
        ))
    }
}

pub fn file_info_text(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<String, WechatFileError> {
    let info = file_info(workspace_root, relative_path)?;
    Ok(format!(
        "文件信息：{}\n类型：{}\n大小：{} bytes\n修改时间(ms)：{}\n校验：{}",
        info.relative_path,
        if info.is_dir { "目录" } else { "文件" },
        info.size_bytes,
        info.modified_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        info.checksum.as_deref().unwrap_or("n/a")
    ))
}

pub fn file_get_text(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<String, WechatFileError> {
    let path = resolve_existing_path(workspace_root, relative_path)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| WechatFileError::new("file_stat_failed", error.to_string()))?;
    if metadata.is_dir() {
        return Err(WechatFileError::new(
            "file_is_directory",
            format!("{relative_path} 是目录，请使用 /file list。"),
        ));
    }
    if metadata.len() > WECHAT_FILE_READ_LIMIT_BYTES {
        return Err(WechatFileError::new(
            "file_too_large",
            format!(
                "文件超过微信文本回传上限：{} bytes > {} bytes。",
                metadata.len(),
                WECHAT_FILE_READ_LIMIT_BYTES
            ),
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| WechatFileError::new("file_read_failed", error.to_string()))?;
    let text = String::from_utf8(bytes.clone()).map_err(|_| {
        WechatFileError::new(
            "file_not_utf8",
            "当前 /file get 文本回传仅支持 UTF-8 文件；二进制附件回传待 iLink 文件发送接通。",
        )
    })?;
    let checksum = checksum_hex(&bytes);
    Ok(format!(
        "文件：{relative_path}\n大小：{} bytes\n校验：{checksum}\n---\n{text}",
        metadata.len()
    ))
}

pub fn file_get_attachment(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<WechatFileAttachment, WechatFileError> {
    let path = resolve_existing_path(workspace_root, relative_path)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| WechatFileError::new("file_stat_failed", error.to_string()))?;
    if metadata.is_dir() {
        return Err(WechatFileError::new(
            "file_is_directory",
            format!("{relative_path} 是目录，请使用 /file list。"),
        ));
    }
    if metadata.len() > WECHAT_FILE_ATTACHMENT_LIMIT_BYTES {
        return Err(WechatFileError::new(
            "file_attachment_too_large",
            format!(
                "文件超过微信附件回传上限：{} bytes > {} bytes。",
                metadata.len(),
                WECHAT_FILE_ATTACHMENT_LIMIT_BYTES
            ),
        ));
    }
    let bytes = fs::read(&path)
        .map_err(|error| WechatFileError::new("file_read_failed", error.to_string()))?;
    let display_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| WechatFileError::new("file_name_missing", "附件文件名为空。"))?;
    Ok(WechatFileAttachment {
        relative_path: normalize_relative_display(relative_path),
        local_path: path.to_string_lossy().to_string(),
        mime: mime_type_for_path(&path).map(str::to_string),
        display_name,
        size_bytes: metadata.len(),
        checksum: checksum_hex(&bytes),
    })
}

pub fn file_info(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<WechatFileInfo, WechatFileError> {
    let path = resolve_existing_path(workspace_root, relative_path)?;
    let metadata = fs::metadata(&path)
        .map_err(|error| WechatFileError::new("file_stat_failed", error.to_string()))?;
    let checksum = if metadata.is_file() && metadata.len() <= WECHAT_FILE_READ_LIMIT_BYTES {
        fs::read(&path).ok().map(|bytes| checksum_hex(&bytes))
    } else {
        None
    };
    Ok(WechatFileInfo {
        relative_path: normalize_relative_display(relative_path),
        size_bytes: metadata.len(),
        is_dir: metadata.is_dir(),
        modified_at_ms: metadata.modified().ok().and_then(system_time_to_ms),
        checksum,
    })
}

pub fn write_text_file(
    workspace_root: &Path,
    relative_path: &str,
    content: &str,
) -> Result<WechatFileInfo, WechatFileError> {
    let target = resolve_write_target(workspace_root, relative_path)?;
    if let Ok(existing) = target.canonicalize() {
        ensure_under_root(&workspace_root_canonical(workspace_root)?, &existing)?;
    }
    let parent = target
        .parent()
        .ok_or_else(|| WechatFileError::new("file_parent_missing", "目标文件缺少父目录。"))?;
    fs::create_dir_all(parent)
        .map_err(|error| WechatFileError::new("file_create_parent_failed", error.to_string()))?;
    let parent_canonical = parent
        .canonicalize()
        .map_err(|error| WechatFileError::new("file_parent_resolve_failed", error.to_string()))?;
    ensure_under_root(
        &workspace_root_canonical(workspace_root)?,
        &parent_canonical,
    )?;

    let temp_name = format!(
        ".coolzhu-wechat-write-{}-{}.tmp",
        current_time_ms(),
        checksum_hex(relative_path.as_bytes()).replace(':', "-")
    );
    let temp_path = parent.join(temp_name);
    fs::write(&temp_path, content.as_bytes())
        .map_err(|error| WechatFileError::new("file_write_failed", error.to_string()))?;
    fs::rename(&temp_path, &target).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        WechatFileError::new("file_replace_failed", error.to_string())
    })?;
    file_info(workspace_root, relative_path)
}

fn required_path<'a>(
    arguments: &'a str,
    command_id: &'static str,
) -> Result<&'a str, WechatFileError> {
    let path = arguments.trim();
    if path.is_empty() {
        Err(WechatFileError::new(
            "file_path_required",
            format!("命令 {command_id} 需要提供工作区内相对路径。"),
        ))
    } else {
        Ok(path)
    }
}

fn resolve_existing_path(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, WechatFileError> {
    let root = workspace_root_canonical(workspace_root)?;
    let relative = validate_relative_path(relative_path)?;
    let target = root.join(relative);
    let canonical = target
        .canonicalize()
        .map_err(|error| WechatFileError::new("file_not_found", error.to_string()))?;
    ensure_under_root(&root, &canonical)?;
    Ok(canonical)
}

fn resolve_write_target(
    workspace_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, WechatFileError> {
    let root = workspace_root_canonical(workspace_root)?;
    let relative = validate_relative_path(relative_path)?;
    Ok(root.join(relative))
}

fn workspace_root_canonical(workspace_root: &Path) -> Result<PathBuf, WechatFileError> {
    workspace_root
        .canonicalize()
        .map_err(|error| WechatFileError::new("workspace_not_accessible", error.to_string()))
}

fn validate_relative_path(relative_path: &str) -> Result<PathBuf, WechatFileError> {
    let trimmed = relative_path.trim();
    if trimmed.is_empty() {
        return Err(WechatFileError::new("file_path_required", "路径不能为空。"));
    }
    let path = Path::new(trimmed);
    if path.is_absolute() {
        return Err(WechatFileError::new(
            "file_path_not_relative",
            "只允许工作区内相对路径。",
        ));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let part = value.to_string_lossy();
                if is_sensitive_component(&part) {
                    return Err(WechatFileError::new(
                        "file_sensitive_path",
                        format!("不允许通过微信访问敏感目录：{part}"),
                    ));
                }
                normalized.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(WechatFileError::new(
                    "file_path_escape",
                    "路径不能包含 ..、盘符或根目录。",
                ));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }
    Ok(normalized)
}

fn is_sensitive_component(component: &str) -> bool {
    matches!(
        component.to_ascii_lowercase().as_str(),
        ".git" | ".coolzhu" | ".ssh" | ".codex" | "node_modules"
    )
}

fn ensure_under_root(root: &Path, candidate: &Path) -> Result<(), WechatFileError> {
    if candidate.starts_with(root) {
        Ok(())
    } else {
        Err(WechatFileError::new(
            "file_path_escape",
            "目标路径逃逸出授权工作区。",
        ))
    }
}

fn normalize_relative_display(relative_path: &str) -> String {
    relative_path.trim().replace('\\', "/")
}

fn mime_type_for_path(path: &Path) -> Option<&'static str> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("txt" | "md" | "log" | "toml" | "yaml" | "yml") => Some("text/plain"),
        Some("json") => Some("application/json"),
        Some("csv") => Some("text/csv"),
        Some("pdf") => Some("application/pdf"),
        Some("png") => Some("image/png"),
        Some("jpg" | "jpeg") => Some("image/jpeg"),
        Some("gif") => Some("image/gif"),
        Some("zip") => Some("application/zip"),
        _ => Some("application/octet-stream"),
    }
}

fn system_time_to_ms(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
}

fn current_time_ms() -> u64 {
    system_time_to_ms(SystemTime::now()).unwrap_or(0)
}

fn checksum_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_paths_reject_escape_and_sensitive_directories() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("safe.txt"), "ok").expect("safe file");

        let escape = file_get_text(temp.path(), "../safe.txt").expect_err("escape rejected");
        assert_eq!(escape.code, "file_path_escape");

        fs::create_dir_all(temp.path().join(".coolzhu")).expect("sensitive dir");
        fs::write(temp.path().join(".coolzhu/secret.txt"), "secret").expect("secret");
        let sensitive =
            file_get_text(temp.path(), ".coolzhu/secret.txt").expect_err("sensitive rejected");
        assert_eq!(sensitive.code, "file_sensitive_path");
    }

    #[test]
    fn file_write_creates_text_file_inside_workspace() {
        let temp = tempfile::tempdir().expect("tempdir");
        let info = write_text_file(temp.path(), "notes/todo.md", "验收微信写文件")
            .expect("write inside workspace");

        assert_eq!(info.relative_path, "notes/todo.md");
        assert_eq!(
            fs::read_to_string(temp.path().join("notes/todo.md")).expect("written file"),
            "验收微信写文件"
        );
        assert!(info.checksum.as_deref().unwrap_or("").starts_with("fnv64:"));
    }

    #[test]
    fn file_get_rejects_large_text_until_file_payload_is_available() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(
            temp.path().join("large.txt"),
            vec![b'a'; WECHAT_FILE_READ_LIMIT_BYTES as usize + 1],
        )
        .expect("large file");

        let error = file_get_text(temp.path(), "large.txt").expect_err("large rejected");
        assert_eq!(error.code, "file_too_large");
    }

    #[test]
    fn file_get_attachment_returns_binary_file_metadata_without_text_preview() {
        let temp = tempfile::tempdir().expect("tempdir");
        let bytes = vec![0_u8, 0xff, 0x10, 0x80];
        fs::write(temp.path().join("sample.bin"), &bytes).expect("binary sample");

        let attachment =
            file_get_attachment(temp.path(), "sample.bin").expect("attachment metadata");

        assert_eq!(attachment.relative_path, "sample.bin");
        assert_eq!(attachment.display_name, "sample.bin");
        assert_eq!(attachment.size_bytes, bytes.len() as u64);
        assert_eq!(
            PathBuf::from(&attachment.local_path),
            temp.path()
                .join("sample.bin")
                .canonicalize()
                .expect("canonical")
        );
        assert!(attachment.checksum.starts_with("fnv64:"));
    }

    #[test]
    fn execute_file_command_supports_list_info_get_and_write() {
        let temp = tempfile::tempdir().expect("tempdir");
        fs::write(temp.path().join("README.md"), "hello").expect("readme");

        assert!(execute_file_command(temp.path(), "file.list", "")
            .expect("list")
            .contains("README.md"));
        assert!(execute_file_command(temp.path(), "file.info", "README.md")
            .expect("info")
            .contains("文件信息"));
        assert!(execute_file_command(temp.path(), "file.get", "README.md")
            .expect("get")
            .contains("hello"));
        let write_reply =
            execute_file_command(temp.path(), "file.write", "out.txt\nfrom wechat").expect("write");
        assert!(write_reply.contains("from wechat"));
        assert_eq!(
            fs::read_to_string(temp.path().join("out.txt")).expect("out"),
            "from wechat"
        );
    }
}
