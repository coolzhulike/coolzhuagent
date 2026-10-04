//! 固定来源确认后复用原静态安装事务；不执行插件、不修改输入安全或模型配置。
use crate::dsh_source_download::PreparedPackage;
use plugins::PluginManager;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct InstallReceipt {
    pub workspace_id: String,
    pub plugin_id: String,
    pub operation_id: String,
    pub name: String,
    pub version: String,
    pub commit: String,
    pub source_sha256: String,
    pub installed: bool,
    pub enabled: bool,
    pub cleanup_confirmed: bool,
    pub cleanup_error: Option<String>,
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use plugins::{DshPackage, DshSourceFile, DshSourceReceipt, PluginManagerConfig};
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf, os::windows::fs::OpenOptionsExt};

    #[test]
    fn committed_install_reports_real_source_cleanup_failure_without_rollback() {
        let parent=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp/dsh-source-cleanup-tests");
        fs::create_dir_all(&parent).unwrap();
        let parent=parent.canonicalize().unwrap();
        let case=tempfile::Builder::new().prefix("case-").tempdir_in(parent).unwrap();
        let source=tempfile::Builder::new().prefix("source-").tempdir_in(case.path()).unwrap();
        let source_path=source.path().to_owned();
        fs::write(source.path().join("package.json"),br#"{"name":"@fixture/cleanup","version":"1.0.0","main":"index.js"}"#).unwrap();
        fs::write(source.path().join("index.js"),"// 仅静态安装测试，不执行插件").unwrap();
        let package=DshPackage {repository:"https://github.com/fixture/cleanup".into(),commit:"a".repeat(40),
            receipt:DshSourceReceipt {protocol:1,name:"@fixture/cleanup".into(),version:"1.0.0".into(),entry:"index.js".into(),
                sdk_lock_sha256:"b".repeat(64),files:["package.json","index.js"].into_iter().map(|name|
                    DshSourceFile {path:name.into(),sha256:format!("{:x}",Sha256::digest(fs::read(source.path().join(name)).unwrap()))}).collect()}};
        let fingerprint=package.source_fingerprint().unwrap();
        // 允许生产静态复制读取，仅拒绝删除本测试的来源文件。
        let held=fs::OpenOptions::new().read(true).share_mode(1).open(source.path().join("index.js")).unwrap();
        let home=case.path().join("home");
        let mut config=PluginManagerConfig::new(&home);
        let bundled=case.path().join("empty-bundled");fs::create_dir(&bundled).unwrap();
        config.bundled_root=Some(bundled);
        let mut manager=PluginManager::new(config.clone());
        let receipt=install_confirmed(&mut manager,PreparedPackage::cleanup_fixture(source,package.clone()),
            &fingerprint,"isolated-cleanup-workspace","来源清理故障夹具").unwrap();
        assert!(receipt.installed && !receipt.enabled && !receipt.cleanup_confirmed);
        assert!(receipt.cleanup_error.as_ref().is_some_and(|text| text.contains("download_cleanup_failed")));
        assert!(source_path.join("index.js").exists(),"实际被锁文件应保留，不能假称已清理");
        let before=(fs::read(manager.registry_path()).unwrap(),fs::read(manager.settings_path()).unwrap());drop(manager);
        let recovered=PluginManager::new(config);recovered.recover_pending_operations().unwrap();
        let ticket=recovered.dsh_activation_ticket(&receipt.plugin_id,"isolated-cleanup-workspace").unwrap();
        assert_eq!(ticket.installation_id,receipt.operation_id);package.verify(&ticket.root).unwrap();
        assert!(recovered.dsh_snapshot(&receipt.plugin_id,"isolated-cleanup-workspace").unwrap().is_none());
        // 发现接口还包含既有内置插件，只检查本次DSH身份没有重复登记。
        assert_eq!(recovered.list_installed_plugins().unwrap().iter()
            .filter(|plugin| plugin.metadata.id==receipt.plugin_id).count(),1);
        assert_eq!((fs::read(recovered.registry_path()).unwrap(),fs::read(recovered.settings_path()).unwrap()),before);
        drop(held);
        println!("{}",serde_json::to_string(&receipt).unwrap());
        // case拥有全部目录，测试结束只清理其私有夹具；正式暂存/事故完全不参与。
    }
}

/// 调用者先冻结工程，且不得因HTTP关闭而重发。进入提交后以持久登记为准。
pub fn install_confirmed(
    manager: &mut PluginManager,
    prepared: PreparedPackage,
    expected_source: &str,
    workspace_id: &str,
    description: &str,
) -> Result<InstallReceipt, String> {
    let fingerprint = prepared
        .package
        .source_fingerprint()
        .map_err(|e| e.to_string())?;
    if fingerprint != expected_source {
        let cleanup = prepared.discard();
        return Err(format!(
            "固定来源与刚才确认的版本不一致，未安装；暂存清理确认：{}",
            cleanup.is_ok()
        ));
    }
    let package = prepared.package.clone();
    let outcome = match manager.install_dsh(prepared.path(), &package, description) {
        Ok(outcome) => outcome,
        Err(error) => {
            let cleanup = prepared.discard();
            return Err(format!(
                "安装事务返回失败，请核对持久登记：{error}；暂存清理确认：{}",
                cleanup.is_ok()
            ));
        }
    };
    // 安装已提交：清理失败须保留真实安装成功事实，不能冒称事务回滚或自动重装。
    let cleanup = prepared.discard();
    Ok(InstallReceipt {
        workspace_id: workspace_id.into(),
        plugin_id: outcome.plugin_id,
        operation_id: outcome.operation_id,
        name: package.receipt.name,
        version: package.receipt.version,
        commit: package.commit,
        source_sha256: fingerprint,
        installed: true,
        enabled: false,
        cleanup_confirmed: cleanup.is_ok(),
        cleanup_error: cleanup.err().map(|e| e.to_string()),
    })
}
