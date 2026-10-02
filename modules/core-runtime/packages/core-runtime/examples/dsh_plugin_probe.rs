//! 实际插件进程桥探针；输入真实插件及安装回执，不内置插件或模型夹具。
use runtime::dsh_host_process::{self, CallContext, HostPaths};
use runtime::managed_process::{self, ExecutionControl};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 5 { return Err("需要 Node、宿主入口、真实插件目录、来源回执与输出路径五个参数".into()); }
    let paths = HostPaths { node_binary: PathBuf::from(&args[0]), entry_script: PathBuf::from(&args[1]),
        plugin_root: PathBuf::from(&args[2]) };
    let source: Value = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    let receipt = source.get("source").unwrap_or(&source);
    let context = CallContext { workspace_id: "real-plugin-process-probe".into(), room_id: "engineering-probe".into(),
        run_id: format!("process-probe-{}", std::process::id()), call_id: "real-calculator-normal".into() };
    let manifest = dsh_host_process::describe(&paths, receipt, &json!({}), &context, Duration::from_secs(30))?;
    assert_eq!(manifest.plugin.name, "@deepseek-ai/dsh-tool-calculator");
    let result = dsh_host_process::execute(&paths, receipt, &json!({}), &context, &manifest, "calculator",
        &json!({"expression":"15 + 27 * sqrt(9)"}), Duration::from_secs(30))?;
    assert_eq!(result["isError"], false);
    assert_eq!(result["value"], 96);
    let mut changed = manifest.clone();
    changed.revision = "a".repeat(64);
    let stale = dsh_host_process::execute(&paths, receipt, &json!({}), &context, &changed, "calculator",
        &json!({"expression":"1 + 1"}), Duration::from_secs(30)).expect_err("旧来源身份不能投递");
    assert_eq!(stale.code, "host_stale");
    assert!(stale.cleanup_confirmed);
    // 真实包仍完整，删去回执中的实际依赖文件；新进程必须拒绝未核验的相对导入。
    let mut incomplete_receipt = receipt.clone();
    incomplete_receipt["files"].as_array_mut().ok_or("回执文件列表缺失")?
        .retain(|item| item["path"] != "lib/evaluate.js");
    let missing_dependency = dsh_host_process::describe(&paths, &incomplete_receipt, &json!({}), &context,
        Duration::from_secs(30)).expect_err("包内实际依赖不得脱离来源回执");
    assert_eq!(missing_dependency.code, "host_failed");
    assert!(missing_dependency.message.contains("不属于已核验来源文件"));
    assert!(missing_dependency.cleanup_confirmed);
    let control = ExecutionControl::new(None, None);
    control.cancel();
    let cancelled = managed_process::with_execution_control(control, || dsh_host_process::execute(
        &paths, receipt, &json!({}), &context, &manifest, "calculator", &json!({"expression":"1 + 1"}),
        Duration::from_secs(30))).expect_err("已取消父运行不能启动进程");
    assert_eq!(cancelled.code, "host_io_failed");
    let report = json!({"passed":true, "notice":"真实官方计算器与生产Rust/Node进程桥的工程核验，不是正式市场或Qwen会话验收",
        "source":receipt, "manifest":manifest, "context":context, "result":result,
        "stale":stale, "missing_dependency":missing_dependency, "pre_cancelled":cancelled});
    std::fs::write(&args[4], serde_json::to_vec_pretty(&report)?)?;
    println!("真实插件进程加载、执行96、旧修订/未核验导入拒绝、根取消拒绝均通过");
    Ok(())
}
