//! 官方固定源码、真实固定Node/SDK的工程验收；只写独立临时目录，不代表模型或正式UI验收。
#[path = "../src/dsh_activation_host.rs"]
mod dsh_activation_host;
#[path = "../src/dsh_install_service.rs"]
mod dsh_install_service;
#[path = "../src/dsh_source_download.rs"]
mod dsh_source_download;
#[path = "../src/dsh_source_resolve.rs"]
mod dsh_source_resolve;
use plugins::{PluginManager, PluginManagerConfig};
use runtime::dsh_host_process::{self, CallContext, HostManifest};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::{Duration, Instant};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        return Err("需要：空临时目录、已核验资源目录、输出JSON".into());
    }
    let root = PathBuf::from(&args[0]).canonicalize()?;
    if std::fs::read_dir(&root)?.count() != 0 {
        return Err("只能使用独立空临时目录".into());
    }
    let started = Instant::now();
    let runtime = runtime::dsh_runtime::verify(&PathBuf::from(&args[1]).canonicalize()?)?;
    let workspace = "engineering-dsh-activation-probe";
    let context = CallContext {
        workspace_id: workspace.into(),
        room_id: "engineering-probe".into(),
        run_id: "engineering-install-snapshot".into(),
        call_id: "engineering-calculate".into(),
    };
    let control = dsh_source_download::DownloadControl::new(
        Instant::now() + Duration::from_secs(120),
        Arc::new(AtomicBool::new(false)),
    )?;
    let candidate = dsh_source_resolve::SourceCandidate::new(
        "omdsh-dev",
        "https://github.com/omdsh-dev/dsh-tool-calculator",
        None,
    )?;
    let prepared = dsh_source_resolve::prepare(
        &candidate,
        Some("b2007a13f06bcf75bf07b9d277ee8d434a316490"),
        &runtime.sdk_lock_sha256,
        &root,
        &control,
    )
    .await?;
    let package = prepared.package.clone();
    let fingerprint = package.source_fingerprint()?;
    let mut reversed = package.clone();
    reversed.receipt.files.reverse();
    assert_eq!(fingerprint, reversed.source_fingerprint()?);
    let config_home = root.join("plugin-settings");
    let mut manager = PluginManager::new(PluginManagerConfig::new(&config_home));
    let installation = dsh_install_service::install_confirmed(
        &mut manager,
        prepared,
        &fingerprint,
        workspace,
        "官方计算器工程验证",
    )?;
    assert!(installation.installed && !installation.enabled && installation.cleanup_confirmed);
    let plugin = &installation.plugin_id;
    assert!(manager.dsh_snapshot(plugin, workspace)?.is_none());
    let ticket = manager.dsh_activation_ticket(plugin, workspace)?;
    assert_eq!(ticket.installation_id, installation.operation_id);
    let source_copy = root.join("fixed-source-copy");
    std::fs::create_dir(&source_copy)?;
    for file in &package.receipt.files {
        let destination = source_copy.join(&file.path);
        std::fs::create_dir_all(destination.parent().unwrap())?;
        std::fs::copy(ticket.root.join(&file.path), destination)?;
    }
    package.verify(&source_copy)?;
    let mut checks = serde_json::Map::new();
    checks.insert("default_disabled".into(), json!(true));
    let next_control = dsh_source_download::DownloadControl::new(
        Instant::now() + Duration::from_secs(120),
        Arc::new(AtomicBool::new(false)),
    )?;
    let mismatch_source = dsh_source_resolve::prepare(
        &candidate,
        Some(&package.commit),
        &runtime.sdk_lock_sha256,
        &root,
        &next_control,
    )
    .await?;
    let mismatch_path = mismatch_source.path().to_owned();
    let settings_before = std::fs::read(manager.settings_path())?;
    let registry_before = std::fs::read(manager.registry_path())?;
    let mismatch = dsh_install_service::install_confirmed(
        &mut manager,
        mismatch_source,
        &"0".repeat(64),
        workspace,
        "不得替换已确认来源",
    )
    .unwrap_err();
    assert!(!mismatch_path.exists());
    assert_eq!(settings_before, std::fs::read(manager.settings_path())?);
    assert_eq!(registry_before, std::fs::read(manager.registry_path())?);
    checks.insert(
        "different_confirmation_rejected_and_discarded_without_state_write".into(),
        json!(mismatch),
    );
    let before = std::fs::read(manager.settings_path())?;
    let ordinary_enable = manager.enable(plugin).unwrap_err().to_string();
    assert_eq!(before, std::fs::read(manager.settings_path())?);
    checks.insert(
        "ordinary_enable_rejected_without_write".into(),
        json!(ordinary_enable),
    );
    let before = std::fs::read(manager.settings_path())?;
    let bad_config =
        dsh_activation_host::enable_verified(&mut manager, plugin, &context, &json!([]), &runtime)
            .unwrap_err();
    assert_eq!(before, std::fs::read(manager.settings_path())?);
    checks.insert(
        "invalid_config_rejected_before_describe_without_settings_write".into(),
        json!(bad_config),
    );
    let snapshot =
        dsh_activation_host::enable_verified(&mut manager, plugin, &context, &json!({}), &runtime)?;
    assert!(manager.dsh_snapshot(plugin, workspace)?.is_some());
    let reloaded = PluginManager::new(PluginManagerConfig::new(&config_home));
    assert_eq!(
        reloaded.dsh_snapshot(plugin, workspace)?,
        Some(snapshot.clone())
    );
    checks.insert("real_describe_persisted_and_reloaded".into(), json!(true));
    let manifest: HostManifest = serde_json::from_value(snapshot.host_manifest.clone())?;
    let expected_receipt = serde_json::to_value(&snapshot.package.receipt)?;
    let result = dsh_host_process::execute(
        &runtime.reverify()?.paths(ticket.root.clone()),
        &expected_receipt,
        &snapshot.config,
        &context,
        &manifest,
        "calculator",
        &json!({"expression":"12 * (3 + 5)"}),
        Duration::from_secs(30),
    )?;
    assert_eq!(result["isError"], false);
    assert_eq!(result["value"], 96);
    checks.insert("production_node_execute".into(), result);
    let wrong_workspace = reloaded
        .dsh_snapshot(plugin, "another-workspace")
        .unwrap_err()
        .to_string();
    checks.insert("cross_workspace_rejected".into(), json!(wrong_workspace));
    let current = manager.dsh_activation_ticket(plugin, workspace)?;
    let mut invalid_manifest = snapshot.host_manifest.clone();
    invalid_manifest["tools"][0]["input_schema"]["type"] = json!("string");
    let before = std::fs::read(manager.settings_path())?;
    let invalid = manager
        .enable_dsh_snapshot(
            &current,
            json!({}),
            runtime.runtime_lock_sha256.clone(),
            invalid_manifest,
        )
        .unwrap_err()
        .to_string();
    assert_eq!(before, std::fs::read(manager.settings_path())?);
    checks.insert("invalid_manifest_no_settings_write".into(), json!(invalid));
    manager.disable(plugin)?;
    assert!(manager.dsh_snapshot(plugin, workspace)?.is_none());
    let before = std::fs::read(manager.settings_path())?;
    let late_disable = manager
        .enable_dsh_snapshot(
            &current,
            json!({}),
            runtime.runtime_lock_sha256.clone(),
            snapshot.host_manifest.clone(),
        )
        .unwrap_err()
        .to_string();
    assert_eq!(before, std::fs::read(manager.settings_path())?);
    checks.insert(
        "late_enable_after_disable_rejected".into(),
        json!(late_disable),
    );
    let snapshot2 =
        dsh_activation_host::enable_verified(&mut manager, plugin, &context, &json!({}), &runtime)?;
    let old_ticket = manager.dsh_activation_ticket(plugin, workspace)?;
    let reinstall = manager.install_dsh(&source_copy, &package, "同版本重装真实来源")?;
    assert_ne!(installation.operation_id, reinstall.operation_id);
    assert!(manager.dsh_snapshot(plugin, workspace)?.is_none());
    let before = std::fs::read(manager.settings_path())?;
    let late_reinstall = manager
        .enable_dsh_snapshot(
            &old_ticket,
            json!({}),
            runtime.runtime_lock_sha256.clone(),
            snapshot2.host_manifest.clone(),
        )
        .unwrap_err()
        .to_string();
    assert_eq!(before, std::fs::read(manager.settings_path())?);
    checks.insert(
        "same_version_reinstall_changes_generation_and_rejects_late_enable".into(),
        json!(late_reinstall),
    );
    let live_ticket = manager.dsh_activation_ticket(plugin, workspace)?;
    let entry = live_ticket.root.join(&package.receipt.entry);
    let saved = std::fs::read(&entry)?;
    let mut tampered = saved.clone();
    tampered.push(b' ');
    std::fs::write(&entry, tampered)?;
    let before = std::fs::read(manager.settings_path())?;
    let tamper_error =
        dsh_activation_host::enable_verified(&mut manager, plugin, &context, &json!({}), &runtime)
            .unwrap_err();
    assert_eq!(before, std::fs::read(manager.settings_path())?);
    std::fs::write(&entry, saved)?;
    package.verify(&live_ticket.root)?;
    checks.insert(
        "source_tamper_rejected_before_describe_without_settings_write".into(),
        json!(tamper_error),
    );
    let snapshot3 =
        dsh_activation_host::enable_verified(&mut manager, plugin, &context, &json!({}), &runtime)?;
    let before = std::fs::read(manager.settings_path())?;
    let sdk_error = manager
        .enable_dsh_snapshot(
            &manager.dsh_activation_ticket(plugin, workspace)?,
            json!({}),
            "bad-lock".into(),
            snapshot3.host_manifest.clone(),
        )
        .unwrap_err()
        .to_string();
    assert_eq!(before, std::fs::read(manager.settings_path())?);
    checks.insert(
        "invalid_runtime_lock_no_settings_write".into(),
        json!(sdk_error),
    );
    manager.uninstall(plugin)?;
    assert!(manager.dsh_snapshot(plugin, workspace)?.is_none());
    assert!(!live_ticket.root.exists());
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manager.settings_path())?)?;
    assert!(persisted["dshSnapshots"].get(plugin).is_none());
    assert!(persisted["enabledPlugins"].get(plugin).is_none());
    checks.insert(
        "uninstall_revokes_snapshot_and_enabled_state".into(),
        json!(true),
    );
    let settings_hash = format!(
        "{:x}",
        Sha256::digest(std::fs::read(manager.settings_path())?)
    );
    let output = json!({"kind":"独立工程验收，不是正式UI或真实模型验收", "elapsed_ms":started.elapsed().as_millis(),
        "installation":installation, "reinstall_operation_id":reinstall.operation_id, "runtime":runtime,
        "source_fingerprint":fingerprint, "snapshot":snapshot, "checks":checks, "final_settings_sha256":settings_hash,
        "production_installation_changed":false, "input_safety_changed":false, "model_requested":false});
    std::fs::write(&args[2], serde_json::to_vec_pretty(&output)?)?;
    println!("官方源码默认停用安装、真实describe/执行96、持久快照及撤资格失败路径通过");
    Ok(())
}
