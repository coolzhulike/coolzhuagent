//! 本地诊断包导出：仅纳入调用方显式允许的附件，逐个读取、经 `redaction::text`
//! 脱敏后写入调用方指定包目录，并生成可核对的 manifest（相对路径、字节数、SHA256）。
//! 不自动上传、不默认外发；密钥库、数据库、原始附件不会默认加入。
//! 写入经 `*.part` 暂存目录原子递交；任一前置校验失败时清除暂存，不留半成品。

use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::redaction;

/// 单附件默认上限；与单管道采集上限一致。
pub const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// 整包默认上限。
pub const DEFAULT_MAX_TOTAL_BYTES: u64 = 32 * 1024 * 1024;
/// 单附件硬上限：超过视为无效配置而非隐式截断；保证 max_file_bytes+1 不溢出、有界读不近无界。
pub const HARD_MAX_FILE_BYTES: u64 = 1024 * 1024 * 1024;
/// 整包硬上限。
pub const HARD_MAX_TOTAL_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// 调用方显式允许的一个导出附件。
#[derive(Debug, Clone)]
pub struct ExportAttachment {
    /// 本地源文件路径；符号链接、目录、不可读或非 UTF-8 文件不进包，仅在 manifest 记状态。
    pub source: PathBuf,
    /// 包内相对路径；拒绝空、绝对路径、驱动器前缀与 `..` 父级穿越。
    pub bundle_path: String,
}

#[derive(Debug, Clone, Copy)]
pub struct ExportLimits {
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
}

impl Default for ExportLimits {
    fn default() -> Self {
        Self { max_file_bytes: DEFAULT_MAX_FILE_BYTES, max_total_bytes: DEFAULT_MAX_TOTAL_BYTES }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryStatus {
    Exported,
    Missing,
    ReadError,
    SymlinkRejected,
    NotUtf8,
    SkippedFileLimit,
    SkippedTotalLimit,
}

impl EntryStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Exported => "exported",
            Self::Missing => "missing",
            Self::ReadError => "read_error",
            Self::SymlinkRejected => "symlink_rejected",
            Self::NotUtf8 => "not_utf8",
            Self::SkippedFileLimit => "skipped_file_limit",
            Self::SkippedTotalLimit => "skipped_total_limit",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub bundle_path: String,
    pub status: EntryStatus,
    /// 已写出（脱敏后）字节数；未导出为 None。
    pub bytes: Option<u64>,
    /// 已写出文件 SHA256（hex）；未导出为 None。
    pub sha256: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug)]
pub struct ExportReport {
    pub bundle_dir: PathBuf,
    pub manifest_path: PathBuf,
    /// manifest.json 本身的 SHA256（hex），同时写入 manifest.sha256。
    pub manifest_sha256: String,
    pub entries: Vec<ManifestEntry>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// 只允许由普通段组成的相对路径；`..`、根、盘符前缀一律拒绝。
fn bundle_relative_path(name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || name.len() > 480 {
        return Err(invalid(format!("包内路径为空或过长: {name:?}")));
    }
    let rel = Path::new(name);
    if rel.is_absolute() {
        return Err(invalid(format!("包内路径必须相对: {name}")));
    }
    let mut clean = PathBuf::new();
    for component in rel.components() {
        match component {
            Component::Normal(part) => {
                validate_path_component(&part.to_string_lossy(), name)?;
                clean.push(part);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(invalid(format!("包内路径含越界成分: {name}")));
            }
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(invalid(format!("包内路径解析后为空: {name:?}")));
    }
    Ok(clean)
}

/// Windows 语义校验单个路径段（跨平台统一拒绝以保证确定性）：
/// NTFS ADS 冒号、通配/管道等非法字符、末尾点/空格（Windows 会静默剥离导致名字漂移）、
/// 以及保留设备名（CON/PRN/AUX/NUL/COM1-9/LPT1-9，忽略大小写与扩展名）。
fn validate_path_component(part: &str, whole: &str) -> io::Result<()> {
    if part.is_empty() {
        return Err(invalid(format!("包内路径含空段: {whole:?}")));
    }
    if part.chars().any(|c| matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*' | '\u{0}')) {
        return Err(invalid(format!("包内路径含非法字符（ADS/通配/管道）: {whole:?}")));
    }
    if part.ends_with('.') || part.ends_with(' ') {
        return Err(invalid(format!("包内路径段以点/空格结尾（Windows 会剥离）: {whole:?}")));
    }
    let stem = part.split('.').next().unwrap_or_default().to_ascii_lowercase();
    const RESERVED: [&str; 24] = [
        "con", "prn", "aux", "nul",
        "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
        "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
        "com0", "lpt0",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return Err(invalid(format!("包内路径使用 Windows 保留设备名: {whole:?}")));
    }
    Ok(())
}

/// 大小写折叠+统一分隔符：NTFS 上 \A.TXT\ 与 a.txt\ 是同一物理文件；
/// 全平台一致按折叠结果判重，保证 manifest 与落盘文件一一对应。
fn folded_key(rel: &Path) -> String {
    rel.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest { hex.push_str(&format!("{byte:02x}")); }
    hex
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn json_opt_str(value: Option<&str>) -> String {
    value.map_or_else(|| "null".to_string(), |v| format!("\"{}\"", json_escape(v)))
}

/// 把 `attachments` 逐个读取、脱敏、写入 `bundle_dir`，生成 manifest.json 与 manifest.sha256。
/// `bundle_dir` 必须尚不存在：先全部校验包内路径，再写入 `<bundle_dir>.part` 暂存，
/// 成功才 rename 递交；中途任何错误删除暂存目录。返回的导出内容与 manifest 一致可核对。
pub fn export_bundle(
    bundle_dir: &Path,
    attachments: &[ExportAttachment],
    limits: ExportLimits,
) -> io::Result<ExportReport> {
    export_bundle_inner(bundle_dir, attachments, limits, |_| Ok(()), false)
}

/// before_claim / fail_after_claim 为测试注入点：before_claim 在原子抢占暂存目录前执行
///（可确定性注入竞态创建的他人 .part）；fail_after_claim 在抢占成功后注入失败，
/// 验证失败清理只删除本调用创建的暂存目录。
fn export_bundle_inner(
    bundle_dir: &Path,
    attachments: &[ExportAttachment],
    limits: ExportLimits,
    before_claim: impl FnOnce(&Path) -> io::Result<()>,
    fail_after_claim: bool,
) -> io::Result<ExportReport> {
    if bundle_dir.exists() {
        return Err(invalid(format!("包目录已存在，不覆盖: {}", bundle_dir.display())));
    }
    // 配额硬上限校验：checked_add 防 +1 溢出，硬顶值防有界读退化为准无界内存占用。
    if limits.max_file_bytes.checked_add(1).is_none()
        || limits.max_file_bytes > HARD_MAX_FILE_BYTES
        || limits.max_total_bytes > HARD_MAX_TOTAL_BYTES
    {
        return Err(invalid("导出配额超出允许硬上限或数值溢出"));
    }
    // 全部路径先行校验，任何越界都在写盘前失败。
    let mut planned: Vec<(PathBuf, &ExportAttachment)> = Vec::with_capacity(attachments.len());
    for attachment in attachments {
        let rel = bundle_relative_path(&attachment.bundle_path)?;
        // 元文件名保留：拒绝任何大小写等价的顶层 manifest.json / manifest.sha256。
        if rel.components().count() == 1
            && matches!(rel.to_string_lossy().to_ascii_lowercase().as_str(),
                "manifest.json" | "manifest.sha256")
        {
            return Err(invalid(format!("包内路径与元文件冲突: {}", attachment.bundle_path)));
        }
        let folded = folded_key(&rel);
        for (seen, _) in &planned {
            let seen_folded = folded_key(seen);
            if seen_folded == folded {
                return Err(invalid(format!("包内路径重复（含大小写等价）: {}", attachment.bundle_path)));
            }
            // 文件/目录前缀碰撞：一方是另一方的前缀段，任何文件系统都无法共存
            if folded.starts_with(&format!("{seen_folded}/"))
                || seen_folded.starts_with(&format!("{folded}/"))
            {
                return Err(invalid(format!("包内路径与目录前缀冲突: {}", attachment.bundle_path)));
            }
        }
        planned.push((rel, attachment));
    }
    if planned.is_empty() {
        return Err(invalid("诊断包至少需要一个显式附件"));
    }
    let Some(file_name) = bundle_dir.file_name() else {
        return Err(invalid(format!("包目录名无效: {}", bundle_dir.display())));
    };
    let staging = bundle_dir.with_file_name(format!("{}.part", file_name.to_string_lossy()));
    before_claim(&staging)?;
    // 原子抢占：create_dir 已存在即失败——此时目录非本调用创建，绝不清理；
    // 成功则所有权属于本调用，之后任何失败才可清理，消除 exists→create 竞态窗口。
    claim_staging(&staging)?;
    let result = if fail_after_claim {
        Err(invalid("注入失败：暂存抢占后"))
    } else {
        run_export(bundle_dir, &staging, &planned, limits)
    };
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// 原子创建暂存目录；仅当返回 Ok 时调用方才拥有 staging 并可安全失败清理。
fn claim_staging(staging: &Path) -> io::Result<()> {
    if let Some(parent) = staging.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::create_dir(staging) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Err(invalid(format!("暂存目录已被占用: {}", staging.display())))
        }
        Err(error) => Err(error),
    }
}

fn run_export(
    bundle_dir: &Path,
    staging: &Path,
    planned: &[(PathBuf, &ExportAttachment)],
    limits: ExportLimits,
) -> io::Result<ExportReport> {
    // 暂存目录由 export_bundle_inner::claim_staging 原子创建并拥有；此处直接使用。
    let mut entries = Vec::with_capacity(planned.len());
    let mut total_bytes: u64 = 0;
    for (rel, attachment) in planned {
        let mut entry = ManifestEntry {
            bundle_path: attachment.bundle_path.clone(),
            status: EntryStatus::Exported,
            bytes: None,
            sha256: None,
            note: None,
        };
        let meta = match fs::symlink_metadata(&attachment.source) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                entry.status = EntryStatus::Missing;
                entry.note = Some("源文件不存在".to_string());
                entries.push(entry);
                continue;
            }
            Err(error) => {
                entry.status = EntryStatus::ReadError;
                // 备注只留稳定错误类名；OS 错误串与源路径可能含敏感片段，不入 manifest。
                entry.note = Some(format!("元数据读取失败: {:?}", error.kind()));
                entries.push(entry);
                continue;
            }
        };
        if meta.file_type().is_symlink() {
            entry.status = EntryStatus::SymlinkRejected;
            entry.note = Some("拒绝跟随符号链接".to_string());
            entries.push(entry);
            continue;
        }
        if !meta.is_file() {
            entry.status = EntryStatus::ReadError;
            entry.note = Some("不是常规文件".to_string());
            entries.push(entry);
            continue;
        }
        // 有界读：最多读 单附件上限+1 字节；文件并发增长不导致无界内存。
        let raw = match fs::File::open(&attachment.source) {
            Ok(file) => {
                let mut raw = Vec::new();
                match file.take((limits.max_file_bytes + 1) as u64).read_to_end(&mut raw) {
                    Ok(_) => raw,
                    Err(error) => {
                        entry.status = EntryStatus::ReadError;
                        entry.note = Some(format!("读取失败: {:?}", error.kind()));
                        entries.push(entry);
                        continue;
                    }
                }
            }
            Err(error) => {
                entry.status = EntryStatus::ReadError;
                entry.note = Some(format!("打开失败: {:?}", error.kind()));
                entries.push(entry);
                continue;
            }
        };
        if raw.len() as u64 > limits.max_file_bytes {
            entry.status = EntryStatus::SkippedFileLimit;
            entry.note = Some(format!("> 单附件上限 {} 字节", limits.max_file_bytes));
            entries.push(entry);
            continue;
        }
        let Ok(text) = String::from_utf8(raw) else {
            entry.status = EntryStatus::NotUtf8;
            entry.note = Some("非 UTF-8 内容不打包".to_string());
            entries.push(entry);
            continue;
        };
        let redacted = redaction::text(&text);
        let written = redacted.as_bytes();
        if total_bytes.saturating_add(written.len() as u64) > limits.max_total_bytes {
            entry.status = EntryStatus::SkippedTotalLimit;
            entry.note = Some(format!("累计超出总上限 {}", limits.max_total_bytes));
            entries.push(entry);
            continue;
        }
        let target = staging.join(rel);
        if let Some(parent) = target.parent() { fs::create_dir_all(parent)?; }
        fs::write(&target, written)?;
        total_bytes += written.len() as u64;
        entry.bytes = Some(written.len() as u64);
        entry.sha256 = Some(sha256_hex(written));
        entries.push(entry);
    }
    let mut manifest = String::from("{\n  \"version\": 1,\n  \"kind\": \"coolzhu-diagnostics-bundle\",\n  \"redaction\": \"applied\",\n  \"uploaded\": false,\n  \"limits\": {\"max_file_bytes\": ");
    manifest.push_str(&limits.max_file_bytes.to_string());
    manifest.push_str(", \"max_total_bytes\": ");
    manifest.push_str(&limits.max_total_bytes.to_string());
    manifest.push_str("},\n  \"entries\": [\n");
    for (index, entry) in entries.iter().enumerate() {
        if index > 0 { manifest.push_str(",\n"); }
        manifest.push_str("    {\"path\": ");
        manifest.push_str(&json_opt_str(Some(&entry.bundle_path)));
        manifest.push_str(", \"status\": \"");
        manifest.push_str(entry.status.as_str());
        manifest.push_str("\", \"bytes\": ");
        manifest.push_str(&entry.bytes.map_or_else(|| "null".into(), |b| b.to_string()));
        manifest.push_str(", \"sha256\": ");
        manifest.push_str(&json_opt_str(entry.sha256.as_deref()));
        manifest.push_str(", \"note\": ");
        manifest.push_str(&json_opt_str(entry.note.as_deref()));
        manifest.push('}');
    }
    manifest.push_str("\n  ]\n}\n");
    let manifest_path = staging.join("manifest.json");
    fs::write(&manifest_path, manifest.as_bytes())?;
    let manifest_sha256 = sha256_hex(manifest.as_bytes());
    fs::write(staging.join("manifest.sha256"), format!("{manifest_sha256}  manifest.json\n"))?;
    fs::rename(staging, bundle_dir)?;
    Ok(ExportReport {
        bundle_dir: bundle_dir.to_path_buf(),
        manifest_path: bundle_dir.join("manifest.json"),
        manifest_sha256,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn case_dir(name: &str) -> PathBuf {
        let seq = SEQ.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::var("CARGO_TARGET_TMPDIR").map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir().join("coolzhu-diag-bundle-tests"))
            .join(format!("bundle-test-{name}-{}-{seq}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn attachment(source: &Path, name: &str) -> ExportAttachment {
        ExportAttachment { source: source.to_path_buf(), bundle_path: name.to_string() }
    }

    fn tiny_limits() -> ExportLimits {
        ExportLimits { max_file_bytes: 1024, max_total_bytes: 4096 }
    }

    #[test]
    fn export_redacts_nested_secrets_and_writes_verifiable_manifest() {
        let root = case_dir("basic");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let log = src.join("run.log");
        fs::write(&log, "INFO 启动\n请求 api_key=\"sk-264secret-marker\" 于 https://user:pw264@example.test\n")
            .unwrap();
        fs::write(src.join("note.txt"), "正常文本 D:/x/y.log 超时行号42").unwrap();
        let bundle = root.join("out-bundle");
        let report = export_bundle(&bundle, &[
            attachment(&log, "logs/run.log"),
            attachment(&src.join("note.txt"), "notes/note.txt"),
            attachment(&src.join("absent.txt"), "notes/absent.txt"),
        ], tiny_limits()).unwrap();
        assert_eq!(report.entries.len(), 3);
        let content = fs::read_to_string(bundle.join("logs/run.log")).unwrap();
        for marker in ["sk-264secret-marker", "user:pw264"] {
            assert!(!content.contains(marker), "泄漏: {marker}");
        }
        assert!(content.contains("[REDACTED]") && content.contains("INFO 启动"));
        assert_eq!(report.entries[0].status, EntryStatus::Exported);
        let written = fs::read(bundle.join("logs/run.log")).unwrap();
        assert_eq!(report.entries[0].bytes, Some(written.len() as u64));
        assert_eq!(report.entries[0].sha256.as_deref(), Some(sha256_hex(&written).as_str()));
        assert_eq!(report.entries[2].status, EntryStatus::Missing);
        assert!(report.entries[2].sha256.is_none());
        let manifest = fs::read_to_string(&report.manifest_path).unwrap();
        assert!(manifest.contains("\"status\": \"missing\"") && manifest.contains("logs/run.log"));
        let sums = fs::read_to_string(bundle.join("manifest.sha256")).unwrap();
        assert!(sums.starts_with(&report.manifest_sha256));
        assert_eq!(report.manifest_sha256, sha256_hex(manifest.as_bytes()));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn export_enforces_file_and_total_limits_without_losing_records() {
        let root = case_dir("limits");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        fs::write(src.join("big.txt"), "x".repeat(2000)).unwrap();
        fs::write(src.join("a.txt"), "a".repeat(600)).unwrap();
        fs::write(src.join("b.txt"), "b".repeat(600)).unwrap();
        let limits = ExportLimits { max_file_bytes: 1024, max_total_bytes: 900 };
        let report = export_bundle(&root.join("bundle"), &[
            attachment(&src.join("big.txt"), "big.txt"),
            attachment(&src.join("a.txt"), "a.txt"),
            attachment(&src.join("b.txt"), "b.txt"),
        ], limits).unwrap();
        assert_eq!(report.entries[0].status, EntryStatus::SkippedFileLimit);
        assert_eq!(report.entries[1].status, EntryStatus::Exported);
        assert_eq!(report.entries[2].status, EntryStatus::SkippedTotalLimit);
        assert!(root.join("bundle/a.txt").is_file() && !root.join("bundle/big.txt").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn export_rejects_traversal_and_keeps_no_partial_bundle() {
        let root = case_dir("paths");
        let src = root.join("ok.txt"); fs::write(&src, "safe").unwrap();
        for bad in ["../evil.txt", "..\\evil.txt", "C:\\evil.txt", "/etc/x", "a/../../b.txt"] {
            let error = export_bundle(&root.join("bundle"),
                &[attachment(&src, bad)], tiny_limits()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "未拒绝 {bad}");
            assert!(!root.join("bundle").exists());
        }
        // 中途越界同样整体失败且清除暂存。
        let err = export_bundle(&root.join("bundle"), &[
            attachment(&src, "ok.txt"), attachment(&src, "../x")], tiny_limits()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!root.join("bundle").exists() && !root.join("bundle.part").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn export_marks_binary_and_never_overwrites_existing_bundle() {
        let root = case_dir("misc");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        fs::write(src.join("bin.dat"), [0xffu8, 0xfe, 0x00, 0x41]).unwrap();
        let bundle = root.join("bundle");
        let report = export_bundle(&bundle, &[attachment(&src.join("bin.dat"), "bin.dat")],
            tiny_limits()).unwrap();
        assert_eq!(report.entries[0].status, EntryStatus::NotUtf8);
        let err = export_bundle(&bundle, &[attachment(&src.join("bin.dat"), "x.txt")],
            tiny_limits()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn export_rejects_reserved_manifest_names_case_insensitive() {
        let root = case_dir("reserved");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let log = src.join("a.log"); fs::write(&log, b"x").unwrap();
        for reserved in ["manifest.json", "MANIFEST.JSON", "Manifest.Sha256", "manifest.SHA256"] {
            let dir = root.join(format!("bundle-{reserved}"));
            let err = export_bundle(&dir, &[attachment(&log, reserved)], tiny_limits())
                .expect_err("保留名应在写盘前拒绝");
            assert!(err.to_string().contains("冲突"), "{err}");
            assert!(!dir.exists());
            let staging = dir.with_file_name(format!("{}.part", dir.file_name().unwrap().to_string_lossy()));
            assert!(!staging.exists(), "失败不得留暂存目录");
        }
        // 子目录内同名不冲突（元文件只在包根生成）
        let ok_dir = root.join("bundle-ok-subdir");
        let report = export_bundle(&ok_dir, &[attachment(&log, "sub/manifest.json")], tiny_limits())
            .expect("子目录内同名应允许");
        assert_eq!(report.entries[0].status, EntryStatus::Exported);
        assert!(ok_dir.join("manifest.json").is_file(), "根级元文件仍生成");
    }

    #[test]
    fn export_manifest_notes_do_not_leak_source_paths_or_errors() {
        let root = case_dir("notesleak").join("sk-marker-dir-a1b2c3");
        fs::create_dir_all(&root).unwrap();
        let missing = attachment(&root.join("gone.log"), "logs/gone.log");
        let dir_as_file = root.join("inner-etc");
        fs::create_dir_all(&dir_as_file).unwrap();
        let dir_src = attachment(&dir_as_file, "logs/dir-as-file.txt");
        let bundle = root.join("bundle");
        let report = export_bundle(&bundle, &[missing, dir_src], tiny_limits()).unwrap();
        let manifest = fs::read_to_string(bundle.join("manifest.json")).unwrap();
        assert_eq!(report.entries[0].status, EntryStatus::Missing);
        assert_eq!(report.entries[1].status, EntryStatus::ReadError);
        assert!(report.entries[0].note.is_some() && report.entries[1].note.is_some());
        // 源路径标记片段与敏感路径信息不得出现在 manifest
        assert!(!manifest.contains("sk-marker-dir-a1b2c3"), "{manifest}");
        assert!(!manifest.contains(root.to_string_lossy().as_ref()), "{manifest}");
    }

    #[test]
    fn export_file_limit_checked_on_actual_bounded_read_bytes() {
        let root = case_dir("bounded");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let big = src.join("big.log"); fs::write(&big, vec![b'x'; 64]).unwrap();
        let bundle = root.join("bundle");
        let report = export_bundle(
            &bundle,
            &[attachment(&big, "big.log")],
            ExportLimits { max_file_bytes: 16, max_total_bytes: 1024 },
        ).unwrap();
        assert_eq!(report.entries[0].status, EntryStatus::SkippedFileLimit);
        assert_eq!(report.entries[0].sha256, None);
        assert!(!bundle.join("big.log").exists());
        // 实际字节边界：恰好等于上限的文件仍导出
        let exact = src.join("exact.log"); fs::write(&exact, vec![b'y'; 16]).unwrap();
        let bundle2 = root.join("bundle2");
        let report2 = export_bundle(
            &bundle2,
            &[attachment(&exact, "exact.log")],
            ExportLimits { max_file_bytes: 16, max_total_bytes: 1024 },
        ).unwrap();
        assert_eq!(report2.entries[0].status, EntryStatus::Exported);
        assert_eq!(fs::read(bundle2.join("exact.log")).unwrap().len(), 16);
    }

    #[test]
    fn export_rejects_overflowing_or_extreme_limits() {
        let root = case_dir("limits");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let log = src.join("a.log"); fs::write(&log, b"x").unwrap();
        // u64::MAX 的 max_file_bytes：+1 会溢出，必须写盘前拒绝
        let err = export_bundle(
            &root.join("b1"),
            &[attachment(&log, "a.log")],
            ExportLimits { max_file_bytes: u64::MAX, max_total_bytes: 1024 },
        ).expect_err("溢出配额应拒绝");
        assert!(err.to_string().contains("硬上限") || err.to_string().contains("溢出"));
        assert!(!root.join("b1").exists());
        // 超硬上限的总量同样拒绝
        assert!(export_bundle(
            &root.join("b2"),
            &[attachment(&log, "a.log")],
            ExportLimits { max_file_bytes: 1024, max_total_bytes: HARD_MAX_TOTAL_BYTES + 1 },
        ).is_err());
        assert!(!root.join("b2").exists());
        // 合法配额正常通过
        assert!(export_bundle(
            &root.join("b3"),
            &[attachment(&log, "a.log")],
            tiny_limits(),
        ).is_ok());
    }

    #[test]
    fn export_rejects_case_equivalent_duplicate_bundle_paths() {
        // Windows 专属语义：NTFS 大小写不敏感，"logs/A.LOG" 与 "LOGS/a.log" 指向同一物理文件；
        // 实现跨平台统一按折叠名判重，保证 manifest 条目与落盘文件一一对应。
        let root = case_dir("case-fold");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let a = src.join("a.log"); fs::write(&a, b"one").unwrap();
        let b = src.join("b.log"); fs::write(&b, b"two").unwrap();
        let err = export_bundle(
            &root.join("bundle"),
            &[attachment(&a, "logs/A.LOG"), attachment(&b, "LOGS/a.log")],
            tiny_limits(),
        ).expect_err("大小写等价重名应拒绝");
        assert!(err.to_string().contains("重复"));
        assert!(!root.join("bundle").exists());
        assert!(!root.join("bundle.part").exists());
        // 不同大小写但不等价的合法路径仍然放行
        assert!(export_bundle(
            &root.join("bundle-ok"),
            &[attachment(&a, "logs/one.log"), attachment(&b, "logs/two.log")],
            tiny_limits(),
        ).is_ok());
    }

    #[test]
    fn export_rejects_reserved_device_names_and_ads() {
        // Windows 专属：CON/PRN/AUX/NUL/COM0-9/LPT0-9 为保留设备名（忽略大小写与扩展名），
        // "name:stream" 为 NTFS ADS，末尾点/空格会被 Windows 静默剥离；统一拒绝防止 manifest 谎报。
        let root = case_dir("reserved");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let log = src.join("a.log"); fs::write(&log, b"x").unwrap();
        for (i, bad) in [
            "CON.txt", "con", "NUL", "aux.log", "COM1.log", "lpt9",
            "logs/con.txt",
            "evil.txt:hidden-stream",
            "dir/file:name.txt",
            "traildot.",
        ].iter().enumerate() {
            let dir = root.join(format!("b{i}"));
            assert!(export_bundle(&dir, &[attachment(&log, bad)], tiny_limits()).is_err(),
                "{bad} 应被拒绝");
            assert!(!dir.exists());
            assert!(!dir.with_file_name(format!("{}.part", dir.file_name().unwrap().to_string_lossy())).exists());
        }
    }

    #[test]
    fn export_rejects_file_dir_prefix_collisions() {
        // "a" 作为文件与 "a/b.txt" 在任何文件系统都无法共存；两个顺序都须写盘前拒绝。
        let root = case_dir("prefix");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let a = src.join("a.log"); fs::write(&a, b"1").unwrap();
        let b = src.join("b.log"); fs::write(&b, b"2").unwrap();
        for (i, paths) in [vec!["a", "a/b.txt"], vec!["a/b.txt", "a"], vec!["logs", "logs/x.txt"]].iter().enumerate() {
            let dir = root.join(format!("d{i}"));
            let atts: Vec<ExportAttachment> = paths.iter().map(|p| attachment(&a, p)).collect();
            assert!(export_bundle(&dir, &atts, tiny_limits()).is_err(), "{paths:?} 应拒绝");
            assert!(!dir.exists());
        }
        // 同目录下多个文件不冲突（共享前缀目录是合法的）
        assert!(export_bundle(
            &root.join("ok"),
            &[attachment(&a, "logs/x.txt"), attachment(&b, "logs/y.txt")],
            tiny_limits(),
        ).is_ok());
    }

    #[test]
    fn export_refuses_foreign_staging_and_existing_destinations() {
        // 所有权：预置他人 .part 目录/文件或已存在目标文件时，导出失败且逐字节保留对方内容。
        let root = case_dir("staging-foreign");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let log = src.join("a.log"); fs::write(&log, b"x").unwrap();
        // 他人暂存目录不被写入或删除
        let dir = root.join("bundle");
        let staging = dir.with_file_name(format!("{}.part", dir.file_name().unwrap().to_string_lossy()));
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("other-owner.txt"), b"FOREIGN-KEEP-269").unwrap();
        assert!(export_bundle(&dir, &[attachment(&log, "a.log")], tiny_limits()).is_err());
        assert_eq!(fs::read(staging.join("other-owner.txt")).unwrap(), b"FOREIGN-KEEP-269");
        assert!(!dir.exists());
        // .part 位置是普通文件同样拒绝且不删
        let dir2 = root.join("bundle2");
        let staging2 = dir2.with_file_name(format!("{}.part", dir2.file_name().unwrap().to_string_lossy()));
        fs::write(&staging2, b"FOREIGN-FILE-269").unwrap();
        assert!(export_bundle(&dir2, &[attachment(&log, "a.log")], tiny_limits()).is_err());
        assert_eq!(fs::read(&staging2).unwrap(), b"FOREIGN-FILE-269");
        // 目标已是文件：拒绝且字节不变
        let dest_file = root.join("as-file");
        fs::write(&dest_file, b"DEST-KEEP-269").unwrap();
        assert!(export_bundle(&dest_file, &[attachment(&log, "a.log")], tiny_limits()).is_err());
        assert_eq!(fs::read(&dest_file).unwrap(), b"DEST-KEEP-269");
    }

    #[test]
    fn export_race_injected_foreign_staging_is_preserved() {
        // 确定性注入竞态：before_claim 在原子抢占前创建他人同名 .part；
        // 旧实现（exists 预检+无条件 remove_dir_all）会误删——现在 claim 失败即返回，不清理。
        let root = case_dir("race-inject");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let log = src.join("a.log"); fs::write(&log, b"x").unwrap();
        let dir = root.join("bundle");
        let err = export_bundle_inner(&dir, &[attachment(&log, "a.log")], tiny_limits(),
            |staging| {
                fs::create_dir_all(staging)?;
                fs::write(staging.join("winner.txt"), b"RACER-OWNED-270")
            }, false)
            .expect_err("竞态抢入的 .part 应拒绝");
        assert!(err.to_string().contains("占用") || err.to_string().contains("存在"));
        let staging = dir.with_file_name(format!("{}.part", dir.file_name().unwrap().to_string_lossy()));
        assert_eq!(fs::read(staging.join("winner.txt")).unwrap(), b"RACER-OWNED-270");
        assert!(!dir.exists());
    }

    #[test]
    fn export_injected_failure_after_claim_removes_own_staging() {
        // 本调用成功创建 .part 后中途失败：自己的暂存必须被清掉，不冒半成品。
        let root = case_dir("own-staging");
        let src = root.join("src"); fs::create_dir_all(&src).unwrap();
        let log = src.join("a.log"); fs::write(&log, b"x").unwrap();
        let dir = root.join("bundle");
        let err = export_bundle_inner(&dir, &[attachment(&log, "a.log")], tiny_limits(),
            |_| Ok(()), true)
            .expect_err("注入失败");
        assert!(err.to_string().contains("注入"));
        let staging = dir.with_file_name(format!("{}.part", dir.file_name().unwrap().to_string_lossy()));
        assert!(!staging.exists(), "本调用创建的暂存应被清理");
        assert!(!dir.exists());
        // 正常导出仍通过
        assert!(export_bundle(&dir, &[attachment(&log, "a.log")], tiny_limits()).is_ok());
    }
}
