//! 工程探针：输入已核验的真实远程包，不构造模型或 SDK；不代表正式页面验收。
use plugins::{DshPackage, PluginManager, PluginManagerConfig};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("需要：真实来源目录、原始来源回执JSON、独立探针配置目录、输出JSON".into());
    }
    let source = PathBuf::from(&args[0]).canonicalize()?;
    let original: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let package = DshPackage {
        repository: "https://github.com/omdsh-dev/dsh-tool-calculator".into(),
        commit: "b2007a13f06bcf75bf07b9d277ee8d434a316490".into(),
        receipt: serde_json::from_value(original["source"].clone())?,
    };
    if package.receipt.name != "@deepseek-ai/dsh-tool-calculator"
        || package.receipt.version != "0.0.1"
    {
        return Err("探针只接受本次已核验的真实首包".into());
    }
    let config = PluginManagerConfig::new(PathBuf::from(&args[2]));
    let mut manager = PluginManager::new(config);
    let installed = manager.install_dsh(&source, &package, "真实 DSH 官方计算器首包")?;
    let summary = manager
        .list_installed_plugins()?
        .into_iter()
        .find(|p| p.metadata.id == installed.plugin_id)
        .ok_or("安装后未登记")?;
    assert!(!summary.enabled);
    assert_eq!(summary.metadata.dsh.as_ref(), Some(&package));
    assert!(manager
        .aggregated_tools()?
        .iter()
        .all(|t| t.plugin_id() != installed.plugin_id));
    assert!(manager.enable(&installed.plugin_id).is_err());
    assert!(manager.update(&installed.plugin_id).is_err());
    let registry_before = fs::read(manager.registry_path())?;
    let settings_before = fs::read(manager.settings_path())?;
    let manifest_before = fs::read(installed.install_path.join("plugin.json"))?;
    let mut changed = package.clone();
    changed.receipt.files[0].sha256 = "0".repeat(64);
    assert!(manager
        .install_dsh(&source, &changed, "摘要不符应拒绝")
        .is_err());
    assert_eq!(fs::read(manager.registry_path())?, registry_before);
    assert_eq!(fs::read(manager.settings_path())?, settings_before);
    assert_eq!(
        fs::read(installed.install_path.join("plugin.json"))?,
        manifest_before
    );
    package.verify(&installed.install_path)?;
    let stage_pending = fs::read_dir(manager.install_root().join(".ops"))?.count();
    assert_eq!(stage_pending, 0);
    let report = json!({ "passed":true,
        "notice":"真实远程来源包的静态安装工程核验，不是正式市场或Qwen调用验收。",
        "repository":package.repository, "commit":package.commit, "plugin_id":installed.plugin_id,
        "install_path":installed.install_path, "source":package.receipt,
        "checks":["已安装默认停用并保留完整来源身份", "未伪造原生工具或加载状态",
            "宿主未接入前拒绝启用和普通浮动更新", "摘要不符在提交前拒绝，旧包/登记/设置保持原字节", "无残留暂存操作"] });
    fs::write(&args[3], serde_json::to_vec_pretty(&report)?)?;
    println!("真实 DSH 来源包静态安装工程核验通过，5组；输出已保存。");
    Ok(())
}
