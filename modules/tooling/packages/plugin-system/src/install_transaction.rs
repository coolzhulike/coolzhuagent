// 安装与更新事务：同卷暂存、跨进程锁、路径检查、操作记录及恢复。
use super::*;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

pub(super) const OPERATION_DIR_NAME: &str = ".ops";
const OPERATION_JOURNAL_NAME: &str = "journal.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OperationKind {
    Install,
    Update,
    BundledSync,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct OperationJournal {
    version: u8,
    operation_id: String,
    kind: OperationKind,
    plugin_id: String,
    target: PathBuf,
    registry_path: PathBuf,
    settings_path: PathBuf,
    old_target_present: bool,
    old_registry: Option<Vec<u8>>,
    old_settings: Option<Vec<u8>>,
}

#[derive(Debug)]
pub(super) struct TransactionOutcome {
    pub(super) operation_id: String,
    pub(super) plugin_id: String,
    pub(super) old_version: Option<String>,
    pub(super) new_version: String,
    pub(super) install_path: PathBuf,
    pub(super) enabled_state: Option<bool>,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransactionFault {
    BeforeStageCopy,
    BeforeRegistryWrite,
    BeforeSettingsWrite,
    LeaveAfterDirectorySwap,
    LeaveAfterRegistryWrite,
    LeaveAfterSettingsWrite,
    LeaveAfterCommitBeforeCleanup,
}

impl PluginManager {
    pub(super) fn lock_and_recover(&self) -> Result<(PathBuf, File), PluginError> {
        let root = checked_install_root(&self.install_root())?;
        let lock_path = root.join(".plugin-manager.lock");
        checked_managed_path(&root, &lock_path)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)?;
        // 目录读取与工具快照复核共享此锁；短暂读竞争应等待，不能被上层当成工具消失。
        // 只等待取得锁，不重试恢复、事务或第三方执行；持续占用仍有界返回Busy。
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                Err(error) => return Err(PluginError::Busy(format!(
                    "插件管理锁不可用，本次操作未执行（{}）：{error}", lock_path.display()
                ))),
            }
        }
        self.recover_pending_operations_locked(&root)?;
        Ok((root, lock))
    }

    pub fn recover_pending_operations(&self) -> Result<(), PluginError> {
        let (_root, _lock) = self.lock_and_recover()?;
        Ok(())
    }

    fn validate_operation_journal(
        &self,
        root: &Path,
        operation_dir: &Path,
        journal: &OperationJournal,
    ) -> Result<(), PluginError> {
        let operation_id = operation_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if journal.version != 1
            || !valid_operation_id(operation_id)
            || journal.operation_id != operation_id
            || !match journal.kind {
                OperationKind::Install => journal.plugin_id.ends_with("@external"),
                OperationKind::BundledSync => journal.plugin_id.ends_with("@bundled"),
                OperationKind::Update => {
                    journal.plugin_id.ends_with("@external")
                        || journal.plugin_id.ends_with("@bundled")
                }
            }
            || journal.plugin_id.contains(['/', '\\', ':'])
        {
            return Err(PluginError::RecoveryPending(format!(
                "插件操作记录不可信，待人工恢复：{}",
                operation_dir.display()
            )));
        }
        let target = checked_install_destination(root, &journal.target)?;
        if target != root.join(sanitize_plugin_id(&journal.plugin_id))
            || journal.registry_path != checked_metadata_path(&self.registry_path())?
            || journal.settings_path != checked_metadata_path(&self.settings_path())?
        {
            return Err(PluginError::RecoveryPending(format!(
                "插件操作记录路径与当前配置不符，待使用原配置恢复：{}",
                operation_dir.display()
            )));
        }
        Ok(())
    }

    fn rollback_operation(
        &self,
        root: &Path,
        operation_dir: &Path,
        journal: &OperationJournal,
    ) -> Result<(), PluginError> {
        self.validate_operation_journal(root, operation_dir, journal)?;
        let target = checked_install_destination(root, &journal.target)?;
        let backup = checked_managed_path(root, &operation_dir.join("backup"))?;
        let stage = checked_managed_path(root, &operation_dir.join("stage"))?;
        if journal.old_target_present {
            if backup.exists() {
                remove_managed_dir(root, &target)?;
                // 恢复期间保留备份；若进程再次中断，下一次恢复仍有可信的旧版来源。
                copy_dir_all(&backup, &target)?;
                verify_staged_copy(&backup, &target)?;
            } else if !target.exists()
                || !stage.exists()
                || operation_dir.join("phase-old-moved").exists()
            {
                return Err(PluginError::RecoveryPending(format!(
                    "旧插件备份缺失，无法证明当前目录仍为旧版，待人工恢复：{}",
                    journal.plugin_id
                )));
            }
        } else {
            if target.exists() && stage.exists() {
                return Err(PluginError::RecoveryPending(format!(
                    "新插件目标与暂存目录同时存在，已拒绝删除未知目录：{}",
                    target.display()
                )));
            }
            if target.exists() {
                remove_managed_dir(root, &target)?;
            }
        }
        restore_optional_bytes(&journal.registry_path, journal.old_registry.as_deref())?;
        restore_optional_bytes(&journal.settings_path, journal.old_settings.as_deref())?;
        Ok(())
    }

    fn recover_pending_operations_locked(&self, root: &Path) -> Result<(), PluginError> {
        let operations = checked_managed_path(root, &root.join(OPERATION_DIR_NAME))?;
        if !operations.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(&operations)? {
            let entry = entry?;
            let operation_dir = checked_managed_path(root, &entry.path())?;
            let operation_id = entry.file_name().to_string_lossy().to_string();
            if !valid_operation_id(&operation_id) || !entry.file_type()?.is_dir() {
                return Err(PluginError::RecoveryPending(format!(
                    "插件操作目录不可信，已停止发现与安装：{}",
                    operation_dir.display()
                )));
            }
            let journal_path =
                checked_managed_path(root, &operation_dir.join(OPERATION_JOURNAL_NAME))?;
            if !journal_path.exists() {
                remove_managed_dir(root, &operation_dir)?;
                continue;
            }
            let journal: OperationJournal = serde_json::from_slice(&fs::read(&journal_path)?)
                .map_err(|error| {
                    PluginError::RecoveryPending(format!(
                        "插件操作记录损坏，已停止发现与安装（{}）：{error}",
                        journal_path.display()
                    ))
                })?;
            self.rollback_operation(root, &operation_dir, &journal)
                .map_err(|error| {
                    PluginError::RecoveryPending(format!(
                        "插件 {} 在{}后恢复失败，待恢复（{}）：{error}",
                        journal.plugin_id,
                        operation_phase(&operation_dir),
                        operation_dir.display()
                    ))
                })?;
            checked_managed_path(root, &journal_path)?;
            fs::remove_file(&journal_path)?;
            // 记录已清除后旧状态已经恢复；清理失败只留下可再次清理的孤儿暂存目录。
            let _ = remove_managed_dir(root, &operation_dir);
        }
        Ok(())
    }

    pub(super) fn replace_plugin_locked(
        &self,
        root: &Path,
        kind: OperationKind,
        install_source: Option<PluginInstallSource>,
        update_id: Option<&str>,
    ) -> Result<TransactionOutcome, PluginError> {
        self.replace_prepared_locked(root, kind, install_source, update_id, None)
    }

    pub(super) fn replace_prepared_locked(
        &self,
        root: &Path,
        kind: OperationKind,
        install_source: Option<PluginInstallSource>,
        update_id: Option<&str>,
        prepared_dsh: Option<(&Path, &DshPackage, &str)>,
    ) -> Result<TransactionOutcome, PluginError> {
        let mut registry = self.load_registry()?;
        let updating = if let Some(update_id) = update_id {
            Some(registry.plugins.get(update_id).cloned().ok_or_else(|| {
                PluginError::NotFound(format!("plugin `{update_id}` is not installed"))
            })?)
        } else {
            None
        };
        let source = install_source
            .or_else(|| updating.as_ref().map(|record| record.source.clone()))
            .ok_or_else(|| PluginError::CommandFailed("缺少插件安装来源".to_string()))?;
        let operation_dir = create_operation_dir(root)?;
        let journal_path = operation_dir.join(OPERATION_JOURNAL_NAME);
        let result = (|| {
            let stage = checked_managed_path(root, &operation_dir.join("stage"))?;
            #[cfg(test)]
            if self.transaction_fault == Some(TransactionFault::BeforeStageCopy) {
                return Err(PluginError::Io(std::io::Error::other(
                    "测试注入暂存复制失败",
                )));
            }
            let source_manifest = if let Some((source_root, package, description)) = prepared_dsh {
                package.stage(source_root, &stage, description)?
            } else {
                let materialized = materialize_source(&source, &operation_dir)?;
                let source_manifest = load_plugin_from_directory(&materialized)?;
                if source_manifest.dsh.is_some() {
                    return Err(PluginError::InvalidManifest("DSH 包须通过静态核验安装入口，不能混用原生安装".into()));
                }
                copy_dir_all(&materialized, &stage)?;
                verify_staged_copy(&materialized, &stage)?;
                source_manifest
            };
            let manifest = load_plugin_from_directory(&stage)?;
            if manifest != source_manifest {
                return Err(PluginError::CommandFailed(
                    "插件暂存包校验后 manifest 发生变化".to_string(),
                ));
            }

            let marketplace = if kind == OperationKind::BundledSync {
                BUNDLED_MARKETPLACE
            } else {
                updating
                    .as_ref()
                    .map_or(EXTERNAL_MARKETPLACE, |record| record.kind.marketplace())
            };
            let plugin_id = plugin_id(&manifest.name, marketplace);
            if let Some(update_id) = update_id {
                if plugin_id != update_id {
                    return Err(PluginError::CommandFailed(format!(
                        "更新来源的插件身份不匹配：预期 {update_id}，实际 {plugin_id}"
                    )));
                }
            }
            let target = checked_install_destination(
                root,
                &updating.as_ref().map_or_else(
                    || root.join(sanitize_plugin_id(&plugin_id)),
                    |record| record.install_path.clone(),
                ),
            )?;
            if target != root.join(sanitize_plugin_id(&plugin_id)) {
                return Err(PluginError::CommandFailed(format!(
                    "registry 中的安装目录与插件身份不符，已拒绝移动：{}",
                    target.display()
                )));
            }
            let old_record = registry.plugins.get(&plugin_id).cloned();
            if let Some(record) = &old_record {
                if let Some((_, package, _)) = prepared_dsh {
                    if !matches!(&record.source, PluginInstallSource::DshBundle { repository, .. }
                        if repository == &package.repository) {
                        return Err(PluginError::InvalidManifest("登记 ID 已属于其它运行类型或来源，未覆盖".into()));
                    }
                }
                if checked_install_destination(root, &record.install_path)? != target {
                    return Err(PluginError::CommandFailed(format!(
                        "registry 中的旧安装路径与目标不符，已拒绝覆盖：{}",
                        record.install_path.display()
                    )));
                }
            } else if target.exists() {
                return Err(PluginError::CommandFailed(format!(
                    "安装目标已存在但不属于 registry，已拒绝覆盖：{}",
                    target.display()
                )));
            }
            if registry.plugins.iter().any(|(id, record)| {
                id != &plugin_id
                    && absolute_path(&record.install_path).ok().as_deref() == Some(target.as_path())
            }) {
                return Err(PluginError::CommandFailed(format!(
                    "安装目标已属于其他插件，已拒绝覆盖：{}",
                    target.display()
                )));
            }

            let old_registry = read_optional_bytes(&self.registry_path())?;
            let old_settings = read_optional_bytes(&self.settings_path())?;
            let enabled = if kind == OperationKind::BundledSync {
                None
            } else if kind == OperationKind::Install {
                Some(false)
            } else {
                let old_default = if old_record
                    .as_ref()
                    .is_some_and(|record| record.kind == PluginKind::Bundled)
                {
                    load_plugin_from_directory(&target)?.default_enabled
                } else {
                    false
                };
                Some(
                    enabled_from_settings_bytes(old_settings.as_deref(), &plugin_id)?
                        .or_else(|| self.config.enabled_plugins.get(&plugin_id).copied())
                        .unwrap_or(old_default),
                )
            };
            let now = unix_time_ms();
            let old_version = old_record.as_ref().map(|record| record.version.clone());
            let operation_id = operation_dir.file_name().and_then(|name| name.to_str())
                .expect("new operation directory should have an ID").to_string();
            let new_record = match kind {
                OperationKind::Install | OperationKind::BundledSync => InstalledPluginRecord {
                    kind: if kind == OperationKind::BundledSync {
                        PluginKind::Bundled
                    } else {
                        PluginKind::External
                    },
                    id: plugin_id.clone(),
                    name: manifest.name.clone(),
                    version: manifest.version.clone(),
                    description: manifest.description.clone(),
                    install_path: target.clone(),
                    source,
                    installed_at_unix_ms: old_record
                        .as_ref()
                        .map_or(now, |record| record.installed_at_unix_ms),
                    updated_at_unix_ms: now,
                    installation_id: operation_id.clone(),
                },
                OperationKind::Update => InstalledPluginRecord {
                    installation_id: operation_id.clone(),
                    version: manifest.version.clone(),
                    description: manifest.description.clone(),
                    updated_at_unix_ms: now,
                    ..updating.expect("update record should exist")
                },
            };
            registry.plugins.insert(plugin_id.clone(), new_record);
            let journal = OperationJournal {
                version: 1,
                operation_id,
                kind,
                plugin_id: plugin_id.clone(),
                target: target.clone(),
                registry_path: checked_metadata_path(&self.registry_path())?,
                settings_path: checked_metadata_path(&self.settings_path())?,
                old_target_present: target.exists(),
                old_registry,
                old_settings,
            };
            write_operation_journal(root, &operation_dir, &journal)?;
            self.commit_staged_locked(root, &operation_dir, &journal, &registry, enabled)?;
            Ok(TransactionOutcome {
                operation_id: journal.operation_id,
                plugin_id,
                old_version,
                new_version: manifest.version,
                install_path: target,
                enabled_state: enabled,
            })
        })();
        if result.is_err() && !journal_path.exists() {
            let _ = remove_managed_dir(root, &operation_dir);
        }
        result
    }

    fn commit_staged_locked(
        &self,
        root: &Path,
        operation_dir: &Path,
        journal: &OperationJournal,
        registry: &InstalledPluginRegistry,
        enabled: Option<bool>,
    ) -> Result<(), PluginError> {
        let target = &journal.target;
        let backup = operation_dir.join("backup");
        let stage = operation_dir.join("stage");
        let commit = (|| {
            if journal.old_target_present {
                rename_managed_dir(root, target, &backup)?;
                mark_operation_phase(root, operation_dir, "old-moved")?;
            }
            rename_managed_dir(root, &stage, target)?;
            mark_operation_phase(root, operation_dir, "new-moved")?;
            #[cfg(test)]
            if self.transaction_fault == Some(TransactionFault::LeaveAfterDirectorySwap) {
                return Err(PluginError::RecoveryPending(
                    "测试模拟目录切换后的进程中断".to_string(),
                ));
            }
            #[cfg(test)]
            if self.transaction_fault == Some(TransactionFault::BeforeRegistryWrite) {
                return Err(PluginError::Io(std::io::Error::other(
                    "测试注入 registry 写入失败",
                )));
            }
            self.store_registry(registry)?;
            mark_operation_phase(root, operation_dir, "registry-written")?;
            #[cfg(test)]
            if self.transaction_fault == Some(TransactionFault::LeaveAfterRegistryWrite) {
                return Err(PluginError::RecoveryPending("测试模拟登记写入后中断".into()));
            }
            #[cfg(test)]
            if self.transaction_fault == Some(TransactionFault::BeforeSettingsWrite) {
                return Err(PluginError::Io(std::io::Error::other(
                    "测试注入 settings 写入失败",
                )));
            }
            if let Some(enabled) = enabled {
                self.write_enabled_state(&journal.plugin_id, Some(enabled))?;
                mark_operation_phase(root, operation_dir, "settings-written")?;
            }
            #[cfg(test)]
            if self.transaction_fault == Some(TransactionFault::LeaveAfterSettingsWrite) {
                return Err(PluginError::RecoveryPending("测试模拟设置写入后中断".into()));
            }
            Ok(())
        })();
        #[cfg(test)]
        if matches!(self.transaction_fault, Some(TransactionFault::LeaveAfterDirectorySwap
            | TransactionFault::LeaveAfterRegistryWrite | TransactionFault::LeaveAfterSettingsWrite)) {
            return commit;
        }
        if let Err(error) = commit {
            if let Err(restore_error) = self.rollback_operation(root, operation_dir, journal) {
                return Err(PluginError::RecoveryPending(format!(
                    "插件 {} 提交失败（{error}），自动恢复失败，待恢复（{}）：{restore_error}",
                    journal.plugin_id,
                    operation_dir.display()
                )));
            }
            let journal_path =
                checked_managed_path(root, &operation_dir.join(OPERATION_JOURNAL_NAME))?;
            fs::remove_file(journal_path)?;
            let _ = remove_managed_dir(root, operation_dir);
            return Err(error);
        }
        let journal_path = checked_managed_path(root, &operation_dir.join(OPERATION_JOURNAL_NAME))?;
        if let Err(error) = fs::remove_file(&journal_path) {
            if let Err(restore_error) = self.rollback_operation(root, operation_dir, journal) {
                return Err(PluginError::RecoveryPending(format!(
                    "插件 {} 完成记录清除失败（{error}），自动恢复失败，待恢复：{restore_error}",
                    journal.plugin_id
                )));
            }
            return Err(PluginError::Io(error));
        }
        #[cfg(test)]
        if self.transaction_fault == Some(TransactionFault::LeaveAfterCommitBeforeCleanup) {
            return Ok(());
        }
        let _ = remove_managed_dir(root, operation_dir);
        Ok(())
    }
}

pub(super) fn absolute_path(path: &Path) -> Result<PathBuf, PluginError> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(PluginError::CommandFailed(format!(
            "路径包含上级跳转，已拒绝：{}",
            path.display()
        )));
    }
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    })
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(super) fn reject_reparse_chain(path: &Path) -> Result<(), PluginError> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if is_reparse_point(&metadata) => {
                return Err(PluginError::CommandFailed(format!(
                    "路径经过符号链接或重解析点，已拒绝：{}",
                    ancestor.display()
                )));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(PluginError::Io(error)),
        }
    }
    Ok(())
}

pub(super) fn checked_install_root(root: &Path) -> Result<PathBuf, PluginError> {
    let root = absolute_path(root)?;
    reject_reparse_chain(&root)?;
    fs::create_dir_all(&root)?;
    reject_reparse_chain(&root)?;
    Ok(root)
}

pub(super) fn checked_managed_path(root: &Path, path: &Path) -> Result<PathBuf, PluginError> {
    let path = absolute_path(path)?;
    if path == root || !path.starts_with(root) {
        return Err(PluginError::CommandFailed(format!(
            "路径不在插件管理根内，已拒绝：{}",
            path.display()
        )));
    }
    reject_reparse_chain(&path)?;
    Ok(path)
}

pub(super) fn checked_install_destination(
    root: &Path,
    path: &Path,
) -> Result<PathBuf, PluginError> {
    let path = checked_managed_path(root, path)?;
    if path.parent() != Some(root)
        || path.file_name().is_some_and(|name| {
            name == OPERATION_DIR_NAME || name == ".tmp" || name == ".plugin-manager.lock"
        })
    {
        return Err(PluginError::CommandFailed(format!(
            "安装目标不是插件管理根的直属目录，已拒绝：{}",
            path.display()
        )));
    }
    Ok(path)
}

pub(super) fn checked_plugin_record_destination(
    root: &Path,
    plugin_id: &str,
    path: &Path,
) -> Result<PathBuf, PluginError> {
    let path = checked_install_destination(root, path)?;
    if path != root.join(sanitize_plugin_id(plugin_id)) {
        return Err(PluginError::CommandFailed(format!(
            "registry 路径与插件身份不符，已拒绝删除：{}",
            path.display()
        )));
    }
    Ok(path)
}

pub(super) fn checked_metadata_path(path: &Path) -> Result<PathBuf, PluginError> {
    let path = absolute_path(path)?;
    reject_reparse_chain(&path)?;
    Ok(path)
}

pub(super) fn remove_managed_dir(root: &Path, path: &Path) -> Result<(), PluginError> {
    let path = checked_managed_path(root, path)?;
    if path.exists() {
        tree_inventory(&path)?;
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn rename_managed_dir(root: &Path, from: &Path, to: &Path) -> Result<(), PluginError> {
    let from = checked_managed_path(root, from)?;
    let to = checked_managed_path(root, to)?;
    if to.exists() {
        return Err(PluginError::CommandFailed(format!(
            "目录切换目标已存在，已拒绝覆盖：{}",
            to.display()
        )));
    }
    tree_inventory(&from)?;
    fs::rename(from, to)?;
    Ok(())
}

fn read_optional_bytes(path: &Path) -> Result<Option<Vec<u8>>, PluginError> {
    let path = checked_metadata_path(path)?;
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(PluginError::Io(error)),
    }
}

fn restore_optional_bytes(path: &Path, bytes: Option<&[u8]>) -> Result<(), PluginError> {
    let path = checked_metadata_path(path)?;
    match bytes {
        Some(bytes) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            reject_reparse_chain(&path)?;
            fs::write(path, bytes)?;
        }
        None if path.exists() => fs::remove_file(path)?,
        None => {}
    }
    Ok(())
}

pub(super) fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), PluginError> {
    reject_reparse_chain(source)?;
    let source_metadata = fs::symlink_metadata(source)?;
    if !source_metadata.is_dir() || is_reparse_point(&source_metadata) {
        return Err(PluginError::CommandFailed(format!(
            "插件来源不是普通目录：{}",
            source.display()
        )));
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(entry.path())?;
        if is_reparse_point(&metadata) {
            return Err(PluginError::CommandFailed(format!(
                "插件包包含符号链接或重解析点，已拒绝：{}",
                entry.path().display()
            )));
        }
        if metadata.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if metadata.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            return Err(PluginError::CommandFailed(format!(
                "插件包包含非常规文件，已拒绝：{}",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

fn tree_inventory(root: &Path) -> Result<BTreeMap<PathBuf, Option<u64>>, PluginError> {
    let mut inventory = BTreeMap::new();
    fn visit(
        root: &Path,
        current: &Path,
        inventory: &mut BTreeMap<PathBuf, Option<u64>>,
    ) -> Result<(), PluginError> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if is_reparse_point(&metadata) {
                return Err(PluginError::CommandFailed(format!(
                    "插件清单遇到重解析点，已拒绝：{}",
                    path.display()
                )));
            }
            let relative = path.strip_prefix(root).map_err(|error| {
                PluginError::CommandFailed(format!("插件清单路径错误：{error}"))
            })?;
            if metadata.is_dir() {
                inventory.insert(relative.to_path_buf(), None);
                visit(root, &path, inventory)?;
            } else if metadata.is_file() {
                inventory.insert(relative.to_path_buf(), Some(metadata.len()));
            } else {
                return Err(PluginError::CommandFailed(format!(
                    "插件清单遇到非常规文件，已拒绝：{}",
                    path.display()
                )));
            }
        }
        Ok(())
    }
    visit(root, root, &mut inventory)?;
    Ok(inventory)
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, PluginError> {
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let mut left_buffer = [0u8; 65536];
    let mut right_buffer = [0u8; 65536];
    loop {
        let left_len = left.read(&mut left_buffer)?;
        let right_len = right.read(&mut right_buffer)?;
        if left_len != right_len || left_buffer[..left_len] != right_buffer[..right_len] {
            return Ok(false);
        }
        if left_len == 0 {
            return Ok(true);
        }
    }
}

fn verify_staged_copy(source: &Path, stage: &Path) -> Result<(), PluginError> {
    let source_items = tree_inventory(source)?;
    if source_items != tree_inventory(stage)? {
        return Err(PluginError::CommandFailed(
            "插件暂存清单与来源不一致".to_string(),
        ));
    }
    for (relative, size) in source_items {
        if size.is_some() && !files_equal(&source.join(&relative), &stage.join(&relative))? {
            return Err(PluginError::CommandFailed(format!(
                "插件暂存文件与来源不一致：{}",
                relative.display()
            )));
        }
    }
    Ok(())
}

fn valid_operation_id(value: &str) -> bool {
    let Some(suffix) = value.strip_prefix("op-") else {
        return false;
    };
    value.len() <= 80
        && suffix.split('-').count() == 3
        && suffix
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn create_operation_dir(root: &Path) -> Result<PathBuf, PluginError> {
    let operations = checked_managed_path(root, &root.join(OPERATION_DIR_NAME))?;
    fs::create_dir_all(&operations)?;
    checked_managed_path(root, &operations)?;
    for attempt in 0..100u32 {
        let operation_id = format!(
            "op-{}-{}-{attempt}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time should be after epoch")
                .as_nanos(),
            std::process::id()
        );
        let path = operations.join(operation_id);
        checked_managed_path(root, &path)?;
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(PluginError::Io(error)),
        }
    }
    Err(PluginError::Busy(
        "无法创建唯一的插件暂存目录，本次操作未执行".to_string(),
    ))
}

fn write_operation_journal(
    root: &Path,
    operation_dir: &Path,
    journal: &OperationJournal,
) -> Result<(), PluginError> {
    let temporary = checked_managed_path(root, &operation_dir.join("journal.tmp"))?;
    let destination = checked_managed_path(root, &operation_dir.join(OPERATION_JOURNAL_NAME))?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(journal)?)?;
    file.sync_all()?;
    drop(file);
    fs::rename(temporary, destination)?;
    Ok(())
}

fn mark_operation_phase(
    root: &Path,
    operation_dir: &Path,
    phase: &'static str,
) -> Result<(), PluginError> {
    let marker = checked_managed_path(root, &operation_dir.join(format!("phase-{phase}")))?;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker)?;
    file.sync_all()?;
    Ok(())
}

fn operation_phase(operation_dir: &Path) -> &'static str {
    for (file, label) in [
        ("phase-settings-written", "settings 已写入"),
        ("phase-registry-written", "registry 已写入"),
        ("phase-new-moved", "新目录已切换"),
        ("phase-old-moved", "旧目录已保留"),
    ] {
        if operation_dir.join(file).exists() {
            return label;
        }
    }
    "暂存完成"
}

fn enabled_from_settings_bytes(
    bytes: Option<&[u8]>,
    plugin_id: &str,
) -> Result<Option<bool>, PluginError> {
    let Some(bytes) = bytes else { return Ok(None) };
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(None);
    }
    let value: Value = serde_json::from_slice(bytes)?;
    Ok(value
        .get("enabledPlugins")
        .and_then(Value::as_object)
        .and_then(|items| items.get(plugin_id))
        .and_then(Value::as_bool))
}

#[cfg(test)]
#[path = "dsh_recovery_tests.rs"]
mod dsh_recovery_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{
        load_enabled_plugins, temp_dir, write_bundled_plugin, write_external_plugin, write_file,
    };

    #[test]
    fn update_preserves_disabled_and_enabled_state() {
        let config_home = temp_dir("transaction-state-home");
        let source_root = temp_dir("transaction-state-source");
        write_external_plugin(&source_root, "state-demo", "1.0.0");
        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        manager
            .install(source_root.to_str().expect("source path"))
            .expect("install");
        assert_eq!(
            load_enabled_plugins(&manager.settings_path()).get("state-demo@external"),
            Some(&false)
        );

        write_external_plugin(&source_root, "state-demo", "2.0.0");
        manager
            .update("state-demo@external")
            .expect("disabled update");
        assert_eq!(
            load_enabled_plugins(&manager.settings_path()).get("state-demo@external"),
            Some(&false)
        );
        manager
            .enable("state-demo@external")
            .expect("explicit enable");
        write_external_plugin(&source_root, "state-demo", "3.0.0");
        manager
            .update("state-demo@external")
            .expect("enabled update");
        assert_eq!(
            load_enabled_plugins(&manager.settings_path()).get("state-demo@external"),
            Some(&true)
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn transaction_stage_registry_and_settings_failures_keep_old_install() {
        for fault in [
            TransactionFault::BeforeStageCopy,
            TransactionFault::BeforeRegistryWrite,
            TransactionFault::BeforeSettingsWrite,
        ] {
            let config_home = temp_dir("transaction-failure-home");
            let source_root = temp_dir("transaction-failure-source");
            write_external_plugin(&source_root, "rollback-demo", "1.0.0");
            let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
            let installed = manager
                .install(source_root.to_str().expect("source path"))
                .expect("install");
            manager.enable("rollback-demo@external").expect("enable");
            let registry_before = fs::read(manager.registry_path()).expect("registry snapshot");
            let settings_before = fs::read(manager.settings_path()).expect("settings snapshot");
            write_external_plugin(&source_root, "rollback-demo", "2.0.0");
            manager.transaction_fault = Some(fault);
            manager
                .update("rollback-demo@external")
                .expect_err("injected failure");
            manager.transaction_fault = None;

            assert_eq!(
                load_plugin_from_directory(&installed.install_path)
                    .expect("old package")
                    .version,
                "1.0.0"
            );
            assert_eq!(
                fs::read(manager.registry_path()).expect("registry restored"),
                registry_before
            );
            assert_eq!(
                fs::read(manager.settings_path()).expect("settings restored"),
                settings_before
            );
            manager
                .recover_pending_operations()
                .expect("no pending failed operation");
            let _ = fs::remove_dir_all(config_home);
            let _ = fs::remove_dir_all(source_root);
        }
    }

    #[test]
    fn bundled_refresh_failure_preserves_old_copy_and_explicit_enable_state() {
        let config_home = temp_dir("bundled-transaction-home");
        let bundled_root = temp_dir("bundled-transaction-source");
        let source = bundled_root.join("refresh-demo");
        write_bundled_plugin(&source, "refresh-demo", "1.0.0", false);
        let mut config = PluginManagerConfig::new(&config_home);
        config.bundled_root = Some(bundled_root.clone());
        let mut manager = PluginManager::new(config);
        manager
            .list_installed_plugins()
            .expect("initial bundled sync");
        manager
            .enable("refresh-demo@bundled")
            .expect("explicitly enable bundled plugin");
        let registry_before = fs::read(manager.registry_path()).expect("registry snapshot");
        let settings_before = fs::read(manager.settings_path()).expect("settings snapshot");
        let installed = manager
            .install_root()
            .join(sanitize_plugin_id("refresh-demo@bundled"));

        write_bundled_plugin(&source, "refresh-demo", "2.0.0", false);
        manager.transaction_fault = Some(TransactionFault::BeforeRegistryWrite);
        manager
            .list_installed_plugins()
            .expect_err("injected bundled refresh failure");
        manager.transaction_fault = None;
        assert_eq!(
            load_plugin_from_directory(&installed)
                .expect("old bundled copy")
                .version,
            "1.0.0"
        );
        assert_eq!(
            fs::read(manager.registry_path()).expect("registry"),
            registry_before
        );
        assert_eq!(
            fs::read(manager.settings_path()).expect("settings"),
            settings_before
        );

        let refreshed = manager
            .list_installed_plugins()
            .expect("bundled refresh after fault");
        assert!(refreshed.iter().any(|plugin| {
            plugin.metadata.id == "refresh-demo@bundled"
                && plugin.metadata.version == "2.0.0"
                && plugin.enabled
        }));
        assert_eq!(
            fs::read(manager.settings_path()).expect("settings unchanged"),
            settings_before
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(bundled_root);
    }

    #[test]
    fn interrupted_directory_swap_recovers_before_discovery() {
        let config_home = temp_dir("transaction-recovery-home");
        let source_root = temp_dir("transaction-recovery-source");
        write_external_plugin(&source_root, "recover-demo", "1.0.0");
        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let installed = manager
            .install(source_root.to_str().expect("source path"))
            .expect("install");
        manager.enable("recover-demo@external").expect("enable");
        let registry_before = fs::read(manager.registry_path()).expect("registry snapshot");
        let settings_before = fs::read(manager.settings_path()).expect("settings snapshot");
        write_external_plugin(&source_root, "recover-demo", "2.0.0");
        manager.transaction_fault = Some(TransactionFault::LeaveAfterDirectorySwap);
        manager
            .update("recover-demo@external")
            .expect_err("simulated interruption");
        assert_eq!(
            load_plugin_from_directory(&installed.install_path)
                .expect("swapped package")
                .version,
            "2.0.0"
        );

        let root = checked_install_root(&manager.install_root()).expect("managed root");
        let operation_dir = fs::read_dir(root.join(OPERATION_DIR_NAME))
            .expect("pending operation directory")
            .next()
            .expect("pending operation")
            .expect("operation entry")
            .path();
        let journal: OperationJournal = serde_json::from_slice(
            &fs::read(operation_dir.join(OPERATION_JOURNAL_NAME)).expect("pending journal"),
        )
        .expect("parse pending journal");
        manager
            .rollback_operation(&root, &operation_dir, &journal)
            .expect("first rollback");
        manager
            .rollback_operation(&root, &operation_dir, &journal)
            .expect("repeated rollback after interruption");
        assert!(operation_dir.join("backup").exists());

        let recovered = PluginManager::new(PluginManagerConfig::new(&config_home));
        recovered
            .recover_pending_operations()
            .expect("recover old package");
        assert_eq!(
            load_plugin_from_directory(&installed.install_path)
                .expect("restored package")
                .version,
            "1.0.0"
        );
        assert_eq!(
            fs::read(recovered.registry_path()).expect("registry restored"),
            registry_before
        );
        assert_eq!(
            fs::read(recovered.settings_path()).expect("settings restored"),
            settings_before
        );
        assert!(recovered
            .list_installed_plugins()
            .expect("discovery after recovery")
            .iter()
            .any(|plugin| plugin.metadata.id == "recover-demo@external"));

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn missing_backup_blocks_discovery_instead_of_exposing_half_install() {
        let config_home = temp_dir("transaction-pending-home");
        let source_root = temp_dir("transaction-pending-source");
        write_external_plugin(&source_root, "pending-demo", "1.0.0");
        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        manager
            .install(source_root.to_str().expect("source path"))
            .expect("install");
        write_external_plugin(&source_root, "pending-demo", "2.0.0");
        manager.transaction_fault = Some(TransactionFault::LeaveAfterDirectorySwap);
        manager
            .update("pending-demo@external")
            .expect_err("simulated interruption");
        let operations = manager.install_root().join(OPERATION_DIR_NAME);
        let operation = fs::read_dir(&operations)
            .expect("operation dir")
            .next()
            .expect("pending operation")
            .expect("operation entry")
            .path();
        fs::remove_dir_all(operation.join("backup")).expect("simulate lost backup in test root");

        let recovered = PluginManager::new(PluginManagerConfig::new(&config_home));
        let error = recovered
            .list_installed_plugins()
            .expect_err("discovery must stop");
        assert!(matches!(error, PluginError::RecoveryPending(_)));
        assert!(error.to_string().contains("待恢复"));

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn manager_lock_rejects_competing_operation_before_mutation() {
        let config_home = temp_dir("transaction-lock-home");
        let source_root = temp_dir("transaction-lock-source");
        write_external_plugin(&source_root, "lock-demo", "1.0.0");
        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let root = checked_install_root(&manager.install_root()).expect("managed root");
        let lock_path = root.join(".plugin-manager.lock");
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(lock_path)
            .expect("lock file");
        lock.try_lock().expect("hold external process-style lock");
        let error = manager
            .install(source_root.to_str().expect("source path"))
            .expect_err("competing operation must fail");
        assert!(matches!(error, PluginError::Busy(_)));
        assert!(!root.join("lock-demo-external").exists());
        drop(lock);
        manager
            .install(source_root.to_str().expect("source path"))
            .expect("install after lock release");

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn manager_read_waits_for_brief_lock_contention_without_losing_plugins() {
        let config_home = temp_dir("transaction-read-contention-home");
        let source_root = temp_dir("transaction-read-contention-source");
        write_external_plugin(&source_root, "read-contention", "1.0.0");
        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        manager.install(source_root.to_str().unwrap()).unwrap();
        let expected = manager.list_installed_plugins().unwrap().into_iter()
            .map(|plugin| plugin.metadata.id).collect::<Vec<_>>();
        let before = fs::read(manager.registry_path()).unwrap();
        let (_root, held) = manager.lock_and_recover().unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            drop(held);
        });
        let installed = manager.list_installed_plugins().expect("普通读取竞争应等待后成功");
        release.join().unwrap();
        assert_eq!(installed.into_iter().map(|plugin| plugin.metadata.id).collect::<Vec<_>>(), expected);
        assert_eq!(fs::read(manager.registry_path()).unwrap(), before);
        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn untrusted_journal_path_blocks_recovery_without_touching_outside_directory() {
        let config_home = temp_dir("transaction-journal-home");
        let outside = temp_dir("transaction-journal-outside");
        write_file(&outside.join("keep.txt"), "user data");
        let manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        let root = checked_install_root(&manager.install_root()).expect("managed root");
        let operation_dir = create_operation_dir(&root).expect("operation directory");
        let journal = OperationJournal {
            version: 1,
            operation_id: operation_dir
                .file_name()
                .expect("operation id")
                .to_string_lossy()
                .to_string(),
            kind: OperationKind::Install,
            plugin_id: "escape@external".to_string(),
            target: outside.clone(),
            registry_path: checked_metadata_path(&manager.registry_path()).expect("registry path"),
            settings_path: checked_metadata_path(&manager.settings_path()).expect("settings path"),
            old_target_present: false,
            old_registry: None,
            old_settings: None,
        };
        write_operation_journal(&root, &operation_dir, &journal).expect("journal");
        let error = manager
            .list_installed_plugins()
            .expect_err("untrusted journal must block discovery");
        assert!(matches!(error, PluginError::RecoveryPending(_)));
        assert_eq!(
            fs::read_to_string(outside.join("keep.txt")).expect("outside data"),
            "user data"
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(outside);
    }

    #[test]
    fn uninstall_rejects_registry_path_outside_managed_root() {
        let config_home = temp_dir("transaction-escape-home");
        let source_root = temp_dir("transaction-escape-source");
        let outside = temp_dir("transaction-escape-outside");
        write_external_plugin(&source_root, "escape-demo", "1.0.0");
        write_file(&outside.join("keep.txt"), "user data");
        let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
        manager
            .install(source_root.to_str().expect("source path"))
            .expect("install");
        let mut registry = manager.load_registry().expect("registry");
        registry
            .plugins
            .get_mut("escape-demo@external")
            .expect("record")
            .install_path = outside.clone();
        manager
            .store_registry(&registry)
            .expect("tamper registry for safety test");
        let error = manager
            .uninstall("escape-demo@external")
            .expect_err("outside path must be refused");
        assert!(error.to_string().contains("插件管理根"));
        assert_eq!(
            fs::read_to_string(outside.join("keep.txt")).expect("outside data"),
            "user data"
        );

        let _ = fs::remove_dir_all(config_home);
        let _ = fs::remove_dir_all(source_root);
        let _ = fs::remove_dir_all(outside);
    }
}
