//! OAuth 凭据文件的窄存储边界：跨进程串行、版本比较、同目录原子发布。
use std::{fs::{self, File, OpenOptions}, io::{self, Read, Write}, path::Path, time::{Duration, Instant}};
use serde_json::{Map, Value};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
pub(super) struct Snapshot { pub data: Option<Value>, pub revision: u64, pub needs_migration: bool }

pub(super) fn lock(path: &Path, refresh: bool) -> io::Result<File> {
    let parent = path.parent().ok_or_else(|| io::Error::other("凭据路径缺少父目录"))?;
    fs::create_dir_all(parent)?;
    let path = path.with_extension(if refresh { "refresh.lock" } else { "write.lock" });
    if fs::symlink_metadata(&path).is_ok_and(|metadata| !metadata.file_type().is_file()) {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "凭据锁必须为普通文件"));
    }
    let mut options = OpenOptions::new(); options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt; options.mode(0o600); }
    let file = options.open(path)?;
    let started = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) if started.elapsed() < Duration::from_secs(if refresh { 35 } else { 10 }) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(io::Error::new(io::ErrorKind::WouldBlock, format!("凭据操作正在进行：{error}"))),
        }
    }
}

fn read_root(path: &Path) -> io::Result<Map<String, Value>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(error),
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_FILE_BYTES { return Err(io::Error::new(io::ErrorKind::InvalidData, "凭据文件类型或大小无效")); }
    let mut bytes = Vec::new(); File::open(path)?.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES { return Err(io::Error::new(io::ErrorKind::InvalidData, "凭据文件超过上限")); }
    if bytes.iter().all(u8::is_ascii_whitespace) { return Ok(Map::new()); }
    let value = serde_json::from_slice::<Value>(&bytes).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "凭据 JSON 损坏"))?;
    value.as_object().cloned().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "凭据必须为 JSON 对象"))
}

fn revision(root: &Map<String, Value>) -> io::Result<u64> {
    match root.get("oauth_revision") {
        None => Ok(0), Some(value) => value.as_u64().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "OAuth 凭据版本无效")),
    }
}

pub(super) fn read(path: &Path) -> io::Result<Snapshot> {
    let _lock = lock(path, false)?;
    let root = read_root(path)?; let revision = revision(&root)?;
    let Some(value) = root.get("oauth").filter(|value| !value.is_null()) else { return Ok(Snapshot { data: None, revision, needs_migration: false }); };
    match value.get("format").and_then(Value::as_str) {
        None => Ok(Snapshot { data: Some(value.clone()), revision, needs_migration: true }),
        Some("permission_only_v1") => Ok(Snapshot { data: Some(value.get("data").cloned().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "OAuth 凭据数据缺失"))?), revision, needs_migration: cfg!(windows) }),
        Some("windows_dpapi_v1") => {
            #[cfg(windows)] {
                let encoded = value.get("ciphertext_hex").and_then(Value::as_str).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "OAuth 加密数据缺失"))?;
                let bytes = decode_hex(encoded)?;
                let mut clear = windows_process_guard::unprotect_user_secret(&bytes).map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "无法以当前 Windows 用户解密 OAuth 凭据；请重新登录或使用受控备份恢复"))?;
                let parsed = serde_json::from_slice(&clear).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "OAuth 解密后数据格式无效"));
                clear.fill(0);
                Ok(Snapshot { data: Some(parsed?), revision, needs_migration: false })
            }
            #[cfg(not(windows))] { Err(io::Error::new(io::ErrorKind::Unsupported, "Windows DPAPI 凭据不能在当前平台解密，请重新登录")) }
        }
        Some(_) => Err(io::Error::new(io::ErrorKind::Unsupported, "OAuth 凭据格式高于当前支持版本，未覆盖")),
    }
}

pub(super) fn save(path: &Path, data: Option<&Value>, expected_revision: Option<u64>) -> io::Result<bool> {
    let _lock = lock(path, false)?;
    let mut root = read_root(path)?; let current = revision(&root)?;
    if expected_revision.is_some_and(|expected| expected != current) { return Ok(false); }
    let next = current.checked_add(1).ok_or_else(|| io::Error::other("OAuth 凭据版本已耗尽"))?;
    if let Some(data) = data {
        #[cfg(windows)] let protected = {
            let mut clear = serde_json::to_vec(data).map_err(io::Error::other)?;
            let encrypted = windows_process_guard::protect_user_secret(&clear); clear.fill(0);
            let encrypted = encrypted?;
            serde_json::json!({"format":"windows_dpapi_v1","ciphertext_hex":encrypted.iter().map(|byte| format!("{byte:02x}")).collect::<String>()})
        };
        #[cfg(not(windows))] let protected = serde_json::json!({"format":"permission_only_v1","data":data});
        root.insert("oauth".into(), protected);
    } else { root.remove("oauth"); }
    // 注销也推进版本，使在途旧刷新不能复活已删除的凭据。
    root.insert("oauth_revision".into(), Value::from(next));
    let parent = path.parent().ok_or_else(|| io::Error::other("凭据目录缺失"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)] { use std::os::unix::fs::PermissionsExt; temporary.as_file().set_permissions(fs::Permissions::from_mode(0o600))?; }
    serde_json::to_writer_pretty(&mut temporary, &root).map_err(io::Error::other)?;
    temporary.write_all(b"\n")?; temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    #[cfg(unix)] File::open(parent)?.sync_all()?;
    Ok(true)
}

#[cfg(windows)]
fn decode_hex(value: &str) -> io::Result<Vec<u8>> {
    if value.len() % 2 != 0 || value.len() > MAX_FILE_BYTES as usize { return Err(io::Error::new(io::ErrorKind::InvalidData, "OAuth 密文编码无效")); }
    value.as_bytes().chunks_exact(2).map(|pair| {
        let digit = |byte: u8| (byte as char).to_digit(16).map(|value| value as u8);
        digit(pair[0]).zip(digit(pair[1])).map(|(a,b)| a * 16 + b)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "OAuth 密文编码无效"))
    }).collect()
}
