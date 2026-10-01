//! 工程探针：下载固定真实远程包，通过现有事务和生产宿主执行；不构造模型。
#[path = "../src/dsh_source_download.rs"]
mod dsh_source_download;
use dsh_source_download::{download, DownloadControl};
use plugins::{PluginManager, PluginManagerConfig};
use runtime::dsh_host_process::{self, CallContext, HostPaths};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

const REPOSITORY: &str = "https://github.com/omdsh-dev/dsh-tool-calculator";
const COMMIT: &str = "b2007a13f06bcf75bf07b9d277ee8d434a316490";
const NAME: &str = "@deepseek-ai/dsh-tool-calculator";
fn control(cancel: Arc<AtomicBool>) -> DownloadControl {
    DownloadControl::new(Instant::now() + Duration::from_secs(120), cancel).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 6 {
        return Err(
            "需要：Node、process.mjs、SDK锁文件、独立下载目录、独立配置目录、输出JSON".into(),
        );
    }
    let node = PathBuf::from(&args[0]).canonicalize()?;
    let host = PathBuf::from(&args[1]).canonicalize()?;
    let sdk_hash = format!("{:x}", Sha256::digest(fs::read(&args[2])?));
    let temporary = PathBuf::from(&args[3]).canonicalize()?;
    let config = PathBuf::from(&args[4]).canonicalize()?;
    let started = Instant::now();
    let prepared = download(
        REPOSITORY,
        COMMIT,
        NAME,
        &sdk_hash,
        &temporary,
        &control(Arc::new(AtomicBool::new(false))),
    )
    .await?;
    let package = prepared.package.clone();
    let objects = prepared.objects.clone();
    let tree = prepared.tree_sha.clone();
    let license = prepared.license.clone();
    let scripts = prepared.declared_scripts.clone();
    let mut manager = PluginManager::new(PluginManagerConfig::new(config));
    let installed = manager.install_dsh(prepared.path(), &package, "真实远程 DSH 官方计算器")?;
    let source_path = prepared.path().to_owned();
    prepared.discard()?;
    assert!(!source_path.exists());
    package.verify(&installed.install_path)?;
    let summary = manager
        .list_installed_plugins()?
        .into_iter()
        .find(|p| p.metadata.id == installed.plugin_id)
        .ok_or("未登记安装结果")?;
    assert!(!summary.enabled);
    let paths = HostPaths {
        node_binary: node,
        entry_script: host,
        plugin_root: installed.install_path.clone(),
    };
    // 探针身份只用于隔离工程运行；没有声称它是正式聊天父轮。
    let context = CallContext {
        workspace_id: "dsh-remote-engineering-workspace".into(),
        room_id: "dsh-remote-engineering-room".into(),
        run_id: "dsh-remote-engineering-run".into(),
        call_id: "dsh-remote-engineering-calculator-96".into(),
    };
    let receipt = serde_json::to_value(&package.receipt)?;
    let manifest = dsh_host_process::describe(
        &paths,
        &receipt,
        &json!({}),
        &context,
        Duration::from_secs(30),
    )?;
    let result = dsh_host_process::execute(
        &paths,
        &receipt,
        &json!({}),
        &context,
        &manifest,
        "calculator",
        &json!({"expression":"(12 + 4) * 6"}),
        Duration::from_secs(30),
    )?;
    assert_eq!(result["isError"], false);
    assert_eq!(result["value"], 96);
    let registry_before = fs::read(manager.registry_path())?;
    let settings_before = fs::read(manager.settings_path())?;
    let manifest_before = fs::read(installed.install_path.join("plugin.json"))?;
    let failure = match download(
        REPOSITORY,
        &"0".repeat(40),
        NAME,
        &sdk_hash,
        &temporary,
        &control(Arc::new(AtomicBool::new(false))),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => return Err("不存在修订不应下载成功".into()),
    };
    assert_eq!(failure.code, "download_failed");
    assert!(failure.cleanup_confirmed);
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_copy = Arc::clone(&cancelled);
    let cancel_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        cancel_copy.store(true, Ordering::SeqCst);
    });
    let cancel_started = Instant::now();
    let cancellation = match download(
        REPOSITORY,
        COMMIT,
        NAME,
        &sdk_hash,
        &temporary,
        &control(cancelled),
    )
    .await
    {
        Err(e) => e,
        Ok(_) => return Err("运行中的取消没有拒绝下载".into()),
    };
    cancel_task.await?;
    assert_eq!(cancellation.code, "download_cancelled");
    assert!(cancellation.cleanup_confirmed);
    assert_eq!(fs::read(manager.registry_path())?, registry_before);
    assert_eq!(fs::read(manager.settings_path())?, settings_before);
    assert_eq!(
        fs::read(installed.install_path.join("plugin.json"))?,
        manifest_before
    );
    assert_eq!(fs::read_dir(&temporary)?.count(), 0);
    let report: Value = json!({"passed":true, "scope":"真实固定远程下载、静态安装与生产进程桥工程验证；不代表正式页面或Qwen调用通过",
        "repository":REPOSITORY,"commit":COMMIT,"tree_sha":tree,"source":package.receipt,
        "git_objects":objects,"license":license,"declared_scripts":scripts,"scripts_executed":false,
        "installed_id":installed.plugin_id,"installed_path":installed.install_path,"enabled":summary.enabled,
        "host_manifest":manifest,"calculator_result":result,"network_failure":failure,
        "cancellation":cancellation,"cancellation_elapsed_ms":cancel_started.elapsed().as_millis(),
        "elapsed_ms":started.elapsed().as_millis(),"temporary_remaining":0,
        "checks":["全仓库文件及所有层级Git树身份核验", "默认停用安装和独立宿主真实96", "不存在commit网络失败不改旧包", "下载future运行中取消且确认清理；不推断收到远端字节", "原登记、设置及已安装清单原字节保持不变"]});
    fs::write(&args[5], serde_json::to_vec_pretty(&report)?)?;
    println!("真实固定远程 DSH 下载/安装/进程执行及失败取消工程核验通过，5组。");
    Ok(())
}
