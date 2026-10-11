//! DSH持久事务专项：独立包和设置；子进程退出后恢复，不执行SDK或插件入口。
use super::*;
use sha2::{Digest, Sha256};
use std::process::Command;

const WORKSPACE: &str = "isolated-dsh-recovery";
fn fixture(root: &Path, version: &str) -> DshPackage {
    fs::create_dir_all(root).unwrap();
    let metadata=serde_json::json!({"name":"@fixture/recovery","version":version,"main":"index.js"});
    fs::write(root.join("package.json"),serde_json::to_vec(&metadata).unwrap()).unwrap();
    fs::write(root.join("index.js"),format!("// 恢复夹具{version}，测试禁止执行此入口\n")).unwrap();
    DshPackage {repository:"https://github.com/fixture/recovery".into(),commit:"a".repeat(40),
        receipt:DshSourceReceipt {protocol:1,name:"@fixture/recovery".into(),version:version.into(),
            entry:"index.js".into(),sdk_lock_sha256:"b".repeat(64),files:["package.json","index.js"].into_iter()
                .map(|name| DshSourceFile {path:name.into(),sha256:format!("{:x}",Sha256::digest(fs::read(root.join(name)).unwrap()))}).collect()}}
}
fn manifest(package: &DshPackage) -> Value {
    serde_json::json!({"protocol":1,"generation":"fixture-generation","revision":"c".repeat(64),
        "plugin":{"name":package.receipt.name,"version":package.receipt.version},"services":["tools"],
        "tools":[{"name":"fixture","description":"只验证持久快照，不声称真实describe","input_schema":{"type":"object"}}]})
}
fn enable(manager: &mut PluginManager, id: &str) -> DshActivationSnapshot {
    let ticket=manager.dsh_activation_ticket(id,WORKSPACE).unwrap();
    manager.enable_dsh_snapshot(&ticket,serde_json::json!({"retained":"原配置"}),"d".repeat(64),manifest(&ticket.package)).unwrap()
}
fn new_case() -> tempfile::TempDir {
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp/dsh-recovery-tests");
    fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new().prefix("case-").tempdir_in(root).unwrap()
}
fn records(manager: &PluginManager) -> (Option<Vec<u8>>,Option<Vec<u8>>) {
    (read_optional_bytes(&manager.registry_path()).unwrap(),read_optional_bytes(&manager.settings_path()).unwrap())
}
fn operation(manager: &PluginManager) -> PathBuf {
    let dirs=fs::read_dir(manager.install_root().join(OPERATION_DIR_NAME)).unwrap()
        .map(|x|x.unwrap().path()).collect::<Vec<_>>();
    assert_eq!(dirs.len(),1,"必须保留本次唯一事务现场"); dirs[0].clone()
}
fn no_pending(manager: &PluginManager) {
    let root=manager.install_root().join(OPERATION_DIR_NAME);
    assert!(!root.exists() || fs::read_dir(root).unwrap().next().is_none());
}
fn worker(root: &Path, fault: &str) -> Value {
    let result=Command::new(std::env::current_exe().unwrap())
        .args(["--exact","install_transaction::dsh_recovery_tests::subprocess_worker","--ignored","--nocapture"])
        .env("COOLZHU_DSH_RECOVERY_CASE",root).env("COOLZHU_DSH_RECOVERY_FAULT",fault)
        .output().unwrap();
    assert!(result.status.success(),"子测试进程失败：{}{}",String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));
    serde_json::from_slice(&fs::read(root.join("worker-result.json")).unwrap()).unwrap()
}

#[test]
#[ignore = "仅由父测试以独立case目录启动；不是独立验收项"]
fn subprocess_worker() {
    let root=PathBuf::from(std::env::var_os("COOLZHU_DSH_RECOVERY_CASE").expect("独立case目录")).canonicalize().unwrap();
    let allowed=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp/dsh-recovery-tests").canonicalize().unwrap();
    assert!(root.starts_with(&allowed) && root!=allowed);
    let mut manager=PluginManager::new(PluginManagerConfig::new(root.join("home")));
    manager.transaction_fault=Some(match std::env::var("COOLZHU_DSH_RECOVERY_FAULT").unwrap().as_str() {
        "directory"=>TransactionFault::LeaveAfterDirectorySwap,
        "registry"=>TransactionFault::LeaveAfterRegistryWrite,
        "settings"=>TransactionFault::LeaveAfterSettingsWrite,
        "committed"=>TransactionFault::LeaveAfterCommitBeforeCleanup,
        _=>panic!("未知故障点"),
    });
    let package:DshPackage=serde_json::from_slice(&fs::read(root.join("package.json")).unwrap()).unwrap();
    let result=manager.install_dsh(&root.join("source"),&package,"DSH故障恢复夹具");
    let op=operation(&manager);
    let evidence=serde_json::json!({"ok":result.is_ok(),"operation_id":op.file_name().unwrap().to_string_lossy(),
        "journal_present":op.join(OPERATION_JOURNAL_NAME).is_file(),"phase":operation_phase(&op),
        "backup_present":op.join("backup").is_dir(),"error":result.err().map(|e|e.to_string())});
    fs::write(root.join("worker-result.json"),serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    // 正常结束此测试进程，但持久现场不回滚；下一进程/管理器走生产恢复入口。
}

#[test]
fn dsh_write_failures_restore_enabled_snapshot_and_exact_metadata() {
    for fault in [TransactionFault::BeforeStageCopy,TransactionFault::BeforeRegistryWrite,TransactionFault::BeforeSettingsWrite] {
        let case=new_case();let root=case.path().canonicalize().unwrap();
        let mut manager=PluginManager::new(PluginManagerConfig::new(root.join("home")));
        let old=fixture(&root.join("old"),"1.0.0");
        let installed=manager.install_dsh(&root.join("old"),&old,"旧安装").unwrap();
        let snapshot=enable(&mut manager,&installed.plugin_id);let before=records(&manager);
        let ticket=manager.dsh_activation_ticket(&installed.plugin_id,WORKSPACE).unwrap();
        let new=fixture(&root.join("source"),"2.0.0");manager.transaction_fault=Some(fault);
        assert!(manager.install_dsh(&root.join("source"),&new,"失败替换").is_err());
        assert_eq!(records(&manager),before);
        old.verify(&installed.install_path).unwrap();
        assert_eq!(manager.dsh_snapshot(&installed.plugin_id,WORKSPACE).unwrap(),Some(snapshot));
        assert_eq!(manager.dsh_activation_ticket(&installed.plugin_id,WORKSPACE).unwrap(),ticket);
        no_pending(&manager);println!("写入失败精确回滚通过：{fault:?}");
    }
}

#[test]
fn dsh_dot_relative_entry_keeps_source_bytes_and_rejects_escape() {
    let case = new_case();
    let root = case.path().canonicalize().unwrap();
    let source = root.join("source");
    let mut package = fixture(&source, "1.0.0");
    let metadata = serde_json::to_vec(&serde_json::json!({
        "name":package.receipt.name,"version":"1.0.0","main":"./index.js"
    })).unwrap();
    fs::write(source.join("package.json"), &metadata).unwrap();
    package.receipt.files.iter_mut().find(|file| file.path == "package.json").unwrap().sha256
        = format!("{:x}", Sha256::digest(&metadata));
    package.verify(&source).unwrap();
    let mut manager = PluginManager::new(PluginManagerConfig::new(root.join("home")));
    let installed = manager.install_dsh(&source, &package, "相对入口回归").unwrap();
    package.verify(&installed.install_path).unwrap();
    assert_eq!(fs::read(installed.install_path.join("package.json")).unwrap(), metadata);
    assert_eq!(package.receipt.entry, "index.js");
    for entry in ["../index.js", "./../index.js", "./C:/index.js", "./index.js:alias", "./node_modules/index.js", "././index.js"] {
        assert!(DshPackage::normalize_source_entry(entry).is_err(), "越界或歧义入口必须拒绝：{entry}");
    }
    package.receipt.entry = "other.js".into();
    assert!(package.verify(&source).is_err(), "不得将入口规范化当作放宽回执身份");
}

#[test]
fn dsh_read_lock_contention_does_not_revoke_unchanged_lifecycle() {
    let case = new_case();
    let root = case.path().canonicalize().unwrap();
    let mut manager = PluginManager::new(PluginManagerConfig::new(root.join("home")));
    let package = fixture(&root.join("source"), "1.0.0");
    let installed = manager.install_dsh(&root.join("source"), &package, "生命周期锁竞争回归").unwrap();
    let snapshot = enable(&mut manager, &installed.plugin_id);
    let ticket = manager.dsh_activation_ticket(&installed.plugin_id, WORKSPACE).unwrap();
    // 模拟目录读取持有同一把管理锁；持久身份没有变化，轮询不得将读竞争当成撤销。
    let (_root, held) = manager.lock_and_recover().unwrap();
    assert!(manager.dsh_ticket_lifecycle_current(&ticket).unwrap());
    assert!(manager.dsh_snapshot_lifecycle_current(&snapshot, &installed.install_path).unwrap());
    // 新接纳与配置发布仍必须持锁，不能借轮询获得写入或启动资格。
    assert!(matches!(manager.dsh_activation_ticket(&installed.plugin_id, WORKSPACE), Err(PluginError::Busy(_))));
    drop(held);
    let replacement = enable(&mut manager, &installed.plugin_id);
    assert_ne!(snapshot.activation_id, replacement.activation_id);
    assert!(!manager.dsh_ticket_lifecycle_current(&ticket).unwrap());
    assert!(!manager.dsh_snapshot_lifecycle_current(&snapshot, &installed.install_path).unwrap());
    manager.disable(&installed.plugin_id).unwrap();
    assert!(!manager.dsh_snapshot_lifecycle_current(&replacement, &installed.install_path).unwrap());
}

#[test]
fn dsh_uncommitted_process_exit_restores_old_snapshot_or_removes_new_install() {
    for existing in [false,true] { for fault in ["directory","registry","settings"] {
        let case=new_case();let root=case.path().canonicalize().unwrap();
        let mut manager=PluginManager::new(PluginManagerConfig::new(root.join("home")));
        let old=fixture(&root.join("old"),"1.0.0");
        let old_state=if existing {
            let installed=manager.install_dsh(&root.join("old"),&old,"旧安装").unwrap();
            let snapshot=enable(&mut manager,&installed.plugin_id);Some((installed,snapshot))
        } else {None};
        let before=records(&manager);let new=fixture(&root.join("source"),"2.0.0");
        fs::write(root.join("package.json"),serde_json::to_vec(&new).unwrap()).unwrap();
        let evidence=worker(&root,fault);assert_eq!(evidence["ok"],false);assert_eq!(evidence["journal_present"],true);
        drop(manager);
        let recovered=PluginManager::new(PluginManagerConfig::new(root.join("home")));
        recovered.recover_pending_operations().unwrap();assert_eq!(records(&recovered),before);
        if let Some((installed,snapshot))=old_state {
            old.verify(&installed.install_path).unwrap();
            assert_eq!(recovered.dsh_snapshot(&installed.plugin_id,WORKSPACE).unwrap(),Some(snapshot.clone()));
            assert!(recovered.dsh_snapshot_lifecycle_current(&snapshot,&installed.install_path).unwrap());
        } else {
            assert!(recovered.load_registry().unwrap().plugins.is_empty());
            let id=format!("{}@external",new.registration_name());
            assert!(!recovered.install_root().join(sanitize_plugin_id(&id)).exists());
            assert!(recovered.dsh_snapshot(&id,WORKSPACE).unwrap().is_none());
        }
        recovered.recover_pending_operations().unwrap();assert_eq!(records(&recovered),before);no_pending(&recovered);
        println!("子进程退出后恢复通过：existing={existing} fault={fault} {evidence}");
    }}
}

#[test]
fn dsh_committed_orphan_cleanup_preserves_new_disabled_generation() {
    for existing in [false,true] {
        let case=new_case();let root=case.path().canonicalize().unwrap();
        let mut manager=PluginManager::new(PluginManagerConfig::new(root.join("home")));
        let package=fixture(&root.join("source"),"1.0.0");
        let old_ticket=if existing {
            let installed=manager.install_dsh(&root.join("source"),&package,"旧安装").unwrap();
            enable(&mut manager,&installed.plugin_id);
            Some(manager.dsh_activation_ticket(&installed.plugin_id,WORKSPACE).unwrap())
        } else {None};
        fs::write(root.join("package.json"),serde_json::to_vec(&package).unwrap()).unwrap();
        let evidence=worker(&root,"committed");assert_eq!(evidence["ok"],true);assert_eq!(evidence["journal_present"],false);
        let committed=records(&manager);drop(manager);
        let mut recovered=PluginManager::new(PluginManagerConfig::new(root.join("home")));
        recovered.recover_pending_operations().unwrap();assert_eq!(records(&recovered),committed);
        let id=format!("{}@external",package.registration_name());
        let ticket=recovered.dsh_activation_ticket(&id,WORKSPACE).unwrap();package.verify(&ticket.root).unwrap();
        assert_eq!(ticket.installation_id,evidence["operation_id"].as_str().unwrap());
        assert!(recovered.dsh_snapshot(&id,WORKSPACE).unwrap().is_none());
        assert_eq!(recovered.load_registry().unwrap().plugins.len(),1);
        if let Some(old)=old_ticket {
            assert_ne!(old.installation_id,ticket.installation_id);
            assert!(!recovered.dsh_ticket_lifecycle_current(&old).unwrap());
            assert!(recovered.enable_dsh_snapshot(&old,serde_json::json!({}),"d".repeat(64),manifest(&package)).is_err());
        }
        recovered.recover_pending_operations().unwrap();assert_eq!(records(&recovered),committed);no_pending(&recovered);
        println!("提交后孤儿清理保留新停用世代通过：existing={existing} {evidence}");
    }
}

#[test]
fn dsh_untrusted_journal_or_missing_backup_refuses_recovery_without_state_change() {
    for damage in ["journal","backup"] {
        let case=new_case();let root=case.path().canonicalize().unwrap();
        let mut manager=PluginManager::new(PluginManagerConfig::new(root.join("home")));
        let old=fixture(&root.join("old"),"1.0.0");
        let installed=manager.install_dsh(&root.join("old"),&old,"旧安装").unwrap();enable(&mut manager,&installed.plugin_id);
        let new=fixture(&root.join("source"),"2.0.0");fs::write(root.join("package.json"),serde_json::to_vec(&new).unwrap()).unwrap();
        worker(&root,"settings");let op=operation(&manager);
        if damage=="journal" {fs::write(op.join(OPERATION_JOURNAL_NAME),b"broken").unwrap();}
        else {fs::rename(op.join("backup"),root.join("preserved-backup")).unwrap();}
        let before=records(&manager);let journal=fs::read(op.join(OPERATION_JOURNAL_NAME)).unwrap();drop(manager);
        let recovered=PluginManager::new(PluginManagerConfig::new(root.join("home")));
        for _ in 0..2 {
            assert!(matches!(recovered.recover_pending_operations(),Err(PluginError::RecoveryPending(_))));
            assert_eq!(records(&recovered),before);assert_eq!(fs::read(op.join(OPERATION_JOURNAL_NAME)).unwrap(),journal);
            new.verify(&installed.install_path).unwrap();
            assert!(recovered.dsh_snapshot(&installed.plugin_id,WORKSPACE).is_err());
        }
        println!("证据不足拒绝恢复且保留现场通过：{damage}");
    }
}

#[cfg(windows)]
#[test]
fn dsh_committed_orphan_sharing_violation_preserves_commit_until_cleanup_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    let case=new_case();let root=case.path().canonicalize().unwrap();
    let manager=PluginManager::new(PluginManagerConfig::new(root.join("home")));
    let package=fixture(&root.join("source"),"1.0.0");
    fs::write(root.join("package.json"),serde_json::to_vec(&package).unwrap()).unwrap();
    let evidence=worker(&root,"committed");assert_eq!(evidence["ok"],true);
    let committed=records(&manager);let op=operation(&manager);
    // 只锁此测试自行创建的孤儿文件，实际Windows共享冲突必须阻止删除。
    let held=OpenOptions::new().read(true).share_mode(1).open(op.join("phase-settings-written")).unwrap();
    let recovered=PluginManager::new(PluginManagerConfig::new(root.join("home")));
    assert!(recovered.recover_pending_operations().is_err());
    assert!(op.exists());assert!(!op.join(OPERATION_JOURNAL_NAME).exists());
    assert_eq!(records(&recovered),committed);
    let id=format!("{}@external",package.registration_name());
    let path=recovered.install_root().join(sanitize_plugin_id(&id));package.verify(&path).unwrap();
    drop(held);
    recovered.recover_pending_operations().unwrap();assert_eq!(records(&recovered),committed);
    assert!(recovered.dsh_snapshot(&id,WORKSPACE).unwrap().is_none());no_pending(&recovered);
    println!("真实Windows共享冲突：提交事实保留，释放测试文件句柄后仅清理孤儿目录");
}
