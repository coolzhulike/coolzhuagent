//! DSH真实宿主快照与启用位共用原插件设置/写锁；不生成模型资格或执行第三方代码。
use super::*;
use std::io::Read;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DshActivationTicket {
    pub plugin_id: String,
    pub workspace_id: String,
    pub installation_id: String,
    pub lifecycle_epoch: Option<String>,
    pub package: DshPackage,
    pub root: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DshActivationSnapshot {
    pub schema: u32,
    pub plugin_id: String,
    pub workspace_id: String,
    pub installation_id: String,
    pub activation_id: String,
    pub package: DshPackage,
    pub config: Value,
    pub runtime_lock_sha256: String,
    pub host_manifest: Value,
}

fn invalid(message: &str) -> PluginError {
    PluginError::CommandFailed(message.into())
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn identity(value: &str) -> bool {
    !value.is_empty() && value.len() <= 192 && !value.chars().any(char::is_control)
}

pub(super) fn new_epoch() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "dsh-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    )
}

fn settings(manager: &PluginManager) -> Result<Value, PluginError> {
    let path = checked_metadata_path(&manager.settings_path())?;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Value::Object(Map::new())),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(invalid("插件设置超出DSH快照读取上限"));
    }
    let value: Value = serde_json::from_slice(&bytes)?;
    if !value.is_object() {
        return Err(invalid("插件设置不是对象"));
    }
    Ok(value)
}

impl DshActivationSnapshot {
    fn validate(&self) -> Result<(), PluginError> {
        if self.schema != 1
            || !identity(&self.activation_id)
            || !identity(&self.workspace_id)
            || !identity(&self.installation_id)
            || !digest(&self.runtime_lock_sha256)
            || !self.config.is_object()
            || serde_json::to_vec(&self.config)?.len() > 64 * 1024
            || serde_json::to_vec(self)?.len() > 512 * 1024
        {
            return Err(invalid("DSH快照身份、配置或大小无效"));
        }
        let manifest = &self.host_manifest;
        if manifest["protocol"] != 1
            || manifest["services"] != serde_json::json!(["tools"])
            || manifest["plugin"]["name"].as_str() != Some(self.package.receipt.name.as_str())
            || manifest["plugin"]["version"].as_str() != Some(self.package.receipt.version.as_str())
            || !manifest["revision"].as_str().is_some_and(digest)
            || !manifest["generation"]
                .as_str()
                .is_some_and(|v| !v.is_empty() && v.len() <= 128)
        {
            return Err(invalid("DSH实际宿主清单与安装身份不符"));
        }
        let tools = manifest["tools"]
            .as_array()
            .filter(|tools| !tools.is_empty() && tools.len() <= 24)
            .ok_or_else(|| invalid("DSH实际工具清单为空或数量无效"))?;
        let mut names = BTreeSet::new();
        for tool in tools {
            let name = tool["name"]
                .as_str()
                .ok_or_else(|| invalid("DSH工具名缺失"))?;
            if name.is_empty()
                || name.len() > 57
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
                || !names.insert(name)
                || !tool["description"].is_string()
                || tool["input_schema"]["type"] != "object"
            {
                return Err(invalid("DSH工具名、重名或完整schema无效"));
            }
        }
        Ok(())
    }
}

impl PluginManager {
    // 轮询只读持久安装身份。设置采用原子替换，登记读取异常仍撤销，不取得管理写锁或执行恢复；
    // 否则正常目录读取的短暂锁竞争会被上层当成撤销。这里只能撤销，启动/提交仍持锁完整核验。
    fn dsh_lifecycle_record(
        &self,
        id: &str,
        installation: &str,
        package: &DshPackage,
        path: &Path,
    ) -> Result<bool, PluginError> {
        let registry = self.load_registry()?;
        let Some(record) = registry.plugins.get(id) else {
            return Ok(false);
        };
        let install_root = checked_install_root(&self.install_root())?;
        checked_plugin_record_destination(&install_root, id, &record.install_path)?;
        Ok(identity(installation)
            && record.id == id
            && record.installation_id == installation
            && record.install_path == path
            && record.name == package.registration_name()
            && record.version == package.receipt.version
            && record.source
                == (PluginInstallSource::DshBundle {
                    repository: package.repository.clone(),
                    commit: package.commit.clone(),
                }))
    }
    /// 仅供已核验ticket的在途撤销轮询；不授予启用或源码执行资格。
    pub fn dsh_ticket_lifecycle_current(
        &self,
        ticket: &DshActivationTicket,
    ) -> Result<bool, PluginError> {
        let value = settings(self)?;
        Ok(self.dsh_lifecycle_record(
            &ticket.plugin_id,
            &ticket.installation_id,
            &ticket.package,
            &ticket.root,
        )? && value["dshLifecycleEpochs"]
            .get(&ticket.plugin_id)
            .and_then(Value::as_str)
            == ticket.lifecycle_epoch.as_deref())
    }
    /// 固定快照的在途安装/停用世代检查；完整源码、受管路径与资源仍在边界直接复核。
    pub fn dsh_snapshot_lifecycle_current(
        &self,
        snapshot: &DshActivationSnapshot,
        root: &Path,
    ) -> Result<bool, PluginError> {
        let value = settings(self)?;
        if value["enabledPlugins"][&snapshot.plugin_id] != true
            || value["dshLifecycleEpochs"][&snapshot.plugin_id].as_str()
                != Some(snapshot.activation_id.as_str())
            || !self.dsh_lifecycle_record(
                &snapshot.plugin_id,
                &snapshot.installation_id,
                &snapshot.package,
                root,
            )?
        {
            return Ok(false);
        }
        let saved = value["dshSnapshots"]
            .get(&snapshot.plugin_id)
            .ok_or_else(|| invalid("DSH在途快照缺失"))?;
        let saved: DshActivationSnapshot = serde_json::from_value(saved.clone())?;
        Ok(saved == *snapshot)
    }

    fn dsh_ticket_locked(
        &self,
        plugin_id: &str,
        workspace_id: &str,
    ) -> Result<DshActivationTicket, PluginError> {
        if !identity(workspace_id) {
            return Err(invalid("DSH工程身份缺失"));
        }
        let registry = self.load_registry()?;
        let record = registry
            .plugins
            .get(plugin_id)
            .ok_or_else(|| invalid("DSH未安装或已卸载"))?;
        if !identity(&record.installation_id) {
            return Err(invalid("旧DSH安装缺少事务世代，请重新核验安装"));
        }
        let root = checked_install_root(&self.install_root())?;
        checked_plugin_record_destination(&root, plugin_id, &record.install_path)?;
        let manifest = load_plugin_from_directory(&record.install_path)?;
        let package = manifest.dsh.ok_or_else(|| invalid("该插件不是DSH来源包"))?;
        if record.id != plugin_id
            || manifest.name != package.registration_name()
            || manifest.version != package.receipt.version
            || record.name != manifest.name
            || record.version != package.receipt.version
            || record.source
                != (PluginInstallSource::DshBundle {
                    repository: package.repository.clone(),
                    commit: package.commit.clone(),
                })
        {
            return Err(invalid("DSH登记与已安装来源不一致"));
        }
        package.verify(&record.install_path)?;
        let settings = settings(self)?;
        let epoch = match settings["dshLifecycleEpochs"].get(plugin_id) {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if identity(value) => Some(value.clone()),
            _ => return Err(invalid("DSH生命周期世代无效")),
        };
        Ok(DshActivationTicket {
            plugin_id: plugin_id.into(),
            workspace_id: workspace_id.into(),
            installation_id: record.installation_id.clone(),
            lifecycle_epoch: epoch,
            package,
            root: record.install_path.clone(),
        })
    }

    /// 冻结describe之前的实际安装与生命周期；不在宿主运行期间持有插件写锁。
    pub fn dsh_activation_ticket(
        &self,
        plugin_id: &str,
        workspace_id: &str,
    ) -> Result<DshActivationTicket, PluginError> {
        let (_root, _lock) = self.lock_and_recover()?;
        self.dsh_ticket_locked(plugin_id, workspace_id)
    }

    /// 仅供受信任宿主适配器发布真实describe结果；页面不可直接提交宿主清单或快照。
    pub fn enable_dsh_snapshot(
        &mut self,
        ticket: &DshActivationTicket,
        config: Value,
        runtime_lock_sha256: String,
        host_manifest: Value,
    ) -> Result<DshActivationSnapshot, PluginError> {
        let (_root, _lock) = self.lock_and_recover()?;
        if self.dsh_ticket_locked(&ticket.plugin_id, &ticket.workspace_id)? != *ticket {
            return Err(invalid("DSH在描述期间已停用、重装或变更，迟到启用被拒绝"));
        }
        let snapshot = DshActivationSnapshot {
            schema: 1,
            plugin_id: ticket.plugin_id.clone(),
            workspace_id: ticket.workspace_id.clone(),
            installation_id: ticket.installation_id.clone(),
            activation_id: new_epoch(),
            package: ticket.package.clone(),
            config,
            runtime_lock_sha256,
            host_manifest,
        };
        snapshot.validate()?;
        let value = serde_json::to_value(&snapshot)?;
        update_settings_json(&self.settings_path(), |root| {
            ensure_object(root, "enabledPlugins")
                .insert(ticket.plugin_id.clone(), Value::Bool(true));
            ensure_object(root, "dshLifecycleEpochs").insert(
                ticket.plugin_id.clone(),
                Value::String(snapshot.activation_id.clone()),
            );
            ensure_object(root, "dshSnapshots").insert(ticket.plugin_id.clone(), value.clone());
        })?;
        self.config
            .enabled_plugins
            .insert(ticket.plugin_id.clone(), true);
        Ok(snapshot)
    }

    pub(super) fn dsh_snapshot_locked(
        &self,
        plugin_id: &str,
        workspace_id: Option<&str>,
    ) -> Result<Option<DshActivationSnapshot>, PluginError> {
        let value = settings(self)?;
        if value["enabledPlugins"][plugin_id] != true {
            return Ok(None);
        }
        let Some(saved) = value["dshSnapshots"].get(plugin_id) else {
            return Ok(None);
        };
        let snapshot: DshActivationSnapshot = serde_json::from_value(saved.clone())?;
        snapshot.validate()?;
        if workspace_id.is_some_and(|workspace| workspace != snapshot.workspace_id) {
            return Err(invalid("DSH快照属于另一工程"));
        }
        let ticket = self.dsh_ticket_locked(plugin_id, &snapshot.workspace_id)?;
        if snapshot.plugin_id != plugin_id
            || ticket.installation_id != snapshot.installation_id
            || ticket.package != snapshot.package
            || ticket.lifecycle_epoch.as_deref() != Some(snapshot.activation_id.as_str())
        {
            return Err(invalid("DSH启用快照已失效"));
        }
        Ok(Some(snapshot))
    }

    /// 当前设置与安装事实复核；返回快照仍不代表已获本轮模型执行权限。
    pub fn dsh_snapshot(
        &self,
        plugin_id: &str,
        workspace_id: &str,
    ) -> Result<Option<DshActivationSnapshot>, PluginError> {
        let (_root, _lock) = self.lock_and_recover()?;
        self.dsh_snapshot_locked(plugin_id, Some(workspace_id))
    }
}
