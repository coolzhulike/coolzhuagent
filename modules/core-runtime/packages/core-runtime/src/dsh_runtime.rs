//! 固定DSH运行资源身份。只核验本次构建登记的资源，不从PATH或工作区寻找替代运行时。
use crate::dsh_host_process::HostPaths;
use crate::managed_process::current_execution_control;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const LOCK: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../../config/dsh-runtime-lock.json"
));
const HOST_FILES: &[(&str, &[u8])] = &[
    (
        "package.json",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tooling/packages/dsh-plugin-host/package.json"
        )),
    ),
    (
        "package-lock.json",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tooling/packages/dsh-plugin-host/package-lock.json"
        )),
    ),
    (
        "src/host.mjs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tooling/packages/dsh-plugin-host/src/host.mjs"
        )),
    ),
    (
        "src/process.mjs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tooling/packages/dsh-plugin-host/src/process.mjs"
        )),
    ),
    (
        "src/source_imports.mjs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../tooling/packages/dsh-plugin-host/src/source_imports.mjs"
        )),
    ),
];

#[derive(Debug, Deserialize)]
struct RuntimeLock {
    schema: u32,
    platform: String,
    node_version: String,
    sdk_lock_sha256: String,
    files: Vec<FileIdentity>,
}
#[derive(Debug, Deserialize)]
struct FileIdentity {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Serialize)]
pub struct RuntimeError {
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}：{}", self.code, self.message)
    }
}
impl std::error::Error for RuntimeError {}
fn invalid(message: &str) -> RuntimeError {
    RuntimeError {
        code: "dsh_runtime_unverified",
        message: message.into(),
    }
}
fn checkpoint() -> Result<(), RuntimeError> {
    if current_execution_control().is_some_and(|control| control.interruption().is_some()) {
        return Err(RuntimeError {
            code: "dsh_runtime_interrupted",
            message: "父运行已取消或到期，未启动插件宿主".into(),
        });
    }
    Ok(())
}
fn plain(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    true
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 元数据只表示已核验的文件身份，不表示第三方代码已经启用或被操作系统沙箱隔离。
#[derive(Debug, Serialize)]
pub struct VerifiedRuntime {
    #[serde(skip)]
    root: PathBuf,
    pub platform: String,
    pub node_version: String,
    pub sdk_lock_sha256: String,
    pub runtime_lock_sha256: String,
    pub file_count: usize,
    pub total_bytes: u64,
}
impl VerifiedRuntime {
    /// 调用者应在每次接纳/执行时重新核验；不可把旧核验结果当作新的执行资格。
    pub fn paths(&self, plugin_root: PathBuf) -> HostPaths {
        HostPaths {
            node_binary: self.root.join("node/node.exe"),
            entry_script: self.root.join("host/src/process.mjs"),
            plugin_root,
        }
    }
}

/// 只定位正式可执行文件旁的固定资源。无PATH、用户目录或本地SDK兜底。
pub fn installed() -> Result<VerifiedRuntime, RuntimeError> {
    let executable = std::env::current_exe().map_err(|_| invalid("无法确定正式运行时位置"))?;
    let bin = executable
        .parent()
        .ok_or_else(|| invalid("正式运行时位置无效"))?;
    verify(&bin.join("dsh-runtime"))
}

pub fn verify(root: &Path) -> Result<VerifiedRuntime, RuntimeError> {
    checkpoint()?;
    plugins::DshPackage::validate_source_root(root)
        .map_err(|_| invalid("固定运行时缺失或目录身份无效"))?;
    let lock: RuntimeLock =
        serde_json::from_str(LOCK).map_err(|_| invalid("编译期运行时锁无效"))?;
    if lock.schema != 1
        || lock.platform != "win32-x64"
        || lock.node_version != "24.15.0"
        || lock.sdk_lock_sha256 != hash(HOST_FILES[1].1)
        || lock.files.is_empty()
        || lock.files.len() > 2000
    {
        return Err(invalid("编译期运行时身份与宿主SDK锁不符"));
    }
    let mut expected = BTreeMap::new();
    let mut case_names = BTreeSet::new();
    for item in lock.files {
        // 该表来自编译期受审查来源；仍拒绝无效/重复路径，禁止锁错误造成越界。
        if item.path.is_empty()
            || item.path.starts_with('/')
            || item.path.contains(['\\', ':'])
            || item
                .path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
            || item.size > 160 * 1024 * 1024
            || item.sha256.len() != 64
            || !item
                .sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
            || !case_names.insert(item.path.to_lowercase())
        {
            return Err(invalid("固定运行时文件表无效"));
        }
        expected.insert(item.path, (item.size, item.sha256));
    }
    for (relative, bytes) in HOST_FILES {
        let name = format!("host/{relative}");
        if !case_names.insert(name.to_lowercase())
            || expected
                .insert(name, (bytes.len() as u64, hash(bytes)))
                .is_some()
        {
            return Err(invalid("运行时文件表占用宿主源码身份"));
        }
    }
    if !expected.contains_key("node/node.exe") || !expected.contains_key("node/LICENSE") {
        return Err(invalid("固定运行时缺少Node或许可证身份"));
    }
    let mut observed = BTreeSet::new();
    let mut count = 0;
    let mut total = 0_u64;
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        checkpoint()?;
        let entry = entry.map_err(|_| invalid("运行时文件枚举失败"))?;
        count += 1;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|_| invalid("运行时文件状态不可读取"))?;
        if !plain(&metadata) || count > 5000 {
            return Err(invalid("运行时包含链接或超出数量限制"));
        }
        if metadata.is_dir() {
            continue;
        }
        if !metadata.is_file() {
            return Err(invalid("运行时包含非普通文件"));
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| invalid("运行时目录越界"))?
            .to_str()
            .ok_or_else(|| invalid("运行时文件名编码无效"))?
            .replace('\\', "/");
        let (size, digest) = expected
            .get(&relative)
            .ok_or_else(|| invalid("运行时存在未登记的文件"))?;
        if !observed.insert(relative) || metadata.len() != *size {
            return Err(invalid("运行时文件大小或重复身份不符"));
        }
        let mut file = fs::File::open(entry.path()).map_err(|_| invalid("运行时文件不可读取"))?;
        let mut hasher = Sha256::new();
        let mut actual_size = 0_u64;
        let mut bytes = [0_u8; 64 * 1024];
        loop {
            checkpoint()?;
            let len = file
                .read(&mut bytes)
                .map_err(|_| invalid("运行时读取中断"))?;
            if len == 0 {
                break;
            }
            actual_size += len as u64;
            if actual_size > *size {
                return Err(invalid("运行时读取超出登记大小"));
            }
            hasher.update(&bytes[..len]);
        }
        if actual_size != *size || format!("{:x}", hasher.finalize()) != *digest {
            return Err(invalid("运行时文件摘要不符，未启动宿主"));
        }
        total += actual_size;
    }
    checkpoint()?;
    if observed.len() != expected.len() || total > 160 * 1024 * 1024 {
        return Err(invalid("固定运行时文件缺失或总大小超限"));
    }
    Ok(VerifiedRuntime {
        root: root.to_path_buf(),
        platform: lock.platform,
        node_version: lock.node_version,
        sdk_lock_sha256: lock.sdk_lock_sha256,
        runtime_lock_sha256: hash(LOCK.as_bytes()),
        file_count: observed.len(),
        total_bytes: total,
    })
}
