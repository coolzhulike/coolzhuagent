//! DSH 来源包的静态核验与暂存；不运行 npm、生命周期脚本或插件入口。
use super::*;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};

const MAX_FILE: u64 = 4 * 1024 * 1024;
const MAX_SOURCE: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DshSourceFile {
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DshSourceReceipt {
    pub protocol: u32,
    pub name: String,
    pub version: String,
    pub entry: String,
    pub sdk_lock_sha256: String,
    pub files: Vec<DshSourceFile>,
}

/// 来源信息由远程下载层核验后提供。静态文件核验不能证明代码可信或支持全部 DSH 服务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DshPackage {
    pub repository: String,
    pub commit: String,
    pub receipt: DshSourceReceipt,
}

fn invalid(message: &str) -> PluginError {
    PluginError::InvalidManifest(message.into())
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

impl DshPackage {
    /// 注册 ID 不使用 scoped npm 名中的斜线；完整 npm 身份始终保留在 receipt。
    #[must_use]
    pub fn registration_name(&self) -> String {
        format!("dsh-{}", &hash(self.receipt.name.as_bytes())[..24])
    }

    fn validate_identity(&self) -> Result<(), PluginError> {
        let repository = self
            .repository
            .strip_prefix("https://github.com/")
            .ok_or_else(|| invalid("首批 DSH 包仅接受固定 GitHub 来源"))?;
        let parts = repository.split('/').collect::<Vec<_>>();
        if parts.len() != 2
            || parts.iter().any(|p| {
                p.is_empty()
                    || p.starts_with('.')
                    || !p
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            })
            || !hex(&self.commit, 40)
            || self.receipt.protocol != 1
            || !hex(&self.receipt.sdk_lock_sha256, 64)
            || self.receipt.name.is_empty()
            || self.receipt.name.len() > 214
            || self.receipt.version.is_empty()
            || self.receipt.version.len() > 96
            || !(2..=1024).contains(&self.receipt.files.len())
        {
            return Err(invalid("DSH 来源、固定修订或回执身份无效"));
        }
        Ok(())
    }

    fn source_path(root: &Path, relative: &str) -> Result<PathBuf, PluginError> {
        if relative.is_empty()
            || relative.len() > 240
            || relative.chars().any(char::is_control)
            || relative.contains(['\\', ':'])
            || relative
                .split('/')
                .any(|p| p.is_empty() || p == "." || p == ".." || p.ends_with(['.', ' ']))
            || Path::new(relative).is_absolute()
            || relative.eq_ignore_ascii_case(MANIFEST_FILE_NAME)
            || relative
                .split('/')
                .any(|p| matches!(p.to_ascii_lowercase().as_str(), "node_modules" | ".git"))
        {
            return Err(invalid("DSH 来源文件路径无效或占用宿主登记文件"));
        }
        let result = root.join(relative);
        reject_reparse_chain(&result)?;
        let metadata = fs::symlink_metadata(&result)?;
        if !metadata.is_file() || metadata.len() > MAX_FILE {
            return Err(invalid("DSH 来源文件无效或超出大小限制"));
        }
        Ok(result)
    }

    fn read_file(root: &Path, item: &DshSourceFile) -> Result<Vec<u8>, PluginError> {
        let path = Self::source_path(root, &item.path)?;
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_FILE || !hex(&item.sha256, 64) || hash(&bytes) != item.sha256 {
            return Err(invalid("DSH 来源文件摘要不符，未安装或运行"));
        }
        Ok(bytes)
    }

    pub fn verify(&self, root: &Path) -> Result<(), PluginError> {
        self.validate_identity()?;
        if !root.is_absolute() || !root.is_dir() {
            return Err(invalid("DSH 来源目录须为绝对路径"));
        }
        reject_reparse_chain(root)?;
        let mut seen = BTreeSet::new();
        let mut total = 0_u64;
        let mut metadata = None;
        for item in &self.receipt.files {
            if !seen.insert(item.path.to_ascii_lowercase()) {
                return Err(invalid("DSH 文件身份重复"));
            }
            let bytes = Self::read_file(root, item)?;
            total += bytes.len() as u64;
            if total > MAX_SOURCE {
                return Err(invalid("DSH 来源包超出总大小限制"));
            }
            if item.path == "package.json" {
                metadata = Some(serde_json::from_slice::<Value>(&bytes)?);
            }
        }
        let metadata = metadata.ok_or_else(|| invalid("DSH 来源缺少 package.json"))?;
        if !self
            .receipt
            .files
            .iter()
            .any(|f| f.path == self.receipt.entry)
            || metadata["name"].as_str() != Some(&self.receipt.name)
            || metadata["version"].as_str() != Some(&self.receipt.version)
            || metadata["main"].as_str() != Some(&self.receipt.entry)
        {
            return Err(invalid("DSH 包名、版本或入口与静态回执不符"));
        }
        Ok(())
    }

    pub(super) fn wrapper(&self, description: &str) -> PluginManifest {
        PluginManifest {
            name: self.registration_name(),
            version: self.receipt.version.clone(),
            description: description.into(),
            permissions: vec![PluginPermission::Execute],
            default_enabled: false,
            hooks: PluginHooks::default(),
            lifecycle: PluginLifecycle::default(),
            tools: Vec::new(),
            commands: Vec::new(),
            dsh: Some(self.clone()),
        }
    }

    pub(super) fn stage(
        &self,
        source: &Path,
        stage: &Path,
        description: &str,
    ) -> Result<PluginManifest, PluginError> {
        self.verify(source)?;
        fs::create_dir(stage)?;
        // 仅复制回执文件，排除环境中的 node_modules、.git、未核验文件及安装脚本产物。
        for item in &self.receipt.files {
            let bytes = Self::read_file(source, item)?;
            let destination = stage.join(&item.path);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            reject_reparse_chain(&destination)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
        }
        self.verify(stage)?;
        let manifest = self.wrapper(description);
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        if bytes.len() > 256 * 1024 {
            return Err(invalid("DSH 登记清单超出大小限制"));
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.join(MANIFEST_FILE_NAME))?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(manifest)
    }
}

impl PluginManager {
    /// 使用同一写锁、目录交换、登记/设置日志及恢复机制安装静态 DSH 包，始终默认停用。
    pub fn install_dsh(
        &mut self,
        source_root: &Path,
        package: &DshPackage,
        description: &str,
    ) -> Result<InstallOutcome, PluginError> {
        if description.trim().is_empty() || description.len() > 4096 {
            return Err(invalid("DSH 描述为空或超出大小限制"));
        }
        let (root, _lock) = self.lock_and_recover()?;
        let source = PluginInstallSource::DshBundle {
            repository: package.repository.clone(),
            commit: package.commit.clone(),
        };
        let outcome = self.replace_prepared_locked(
            &root,
            OperationKind::Install,
            Some(source),
            None,
            Some((source_root, package, description)),
        )?;
        self.config
            .enabled_plugins
            .insert(outcome.plugin_id.clone(), false);
        Ok(InstallOutcome {
            plugin_id: outcome.plugin_id,
            version: outcome.new_version,
            install_path: outcome.install_path,
        })
    }
}
