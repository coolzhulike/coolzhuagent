//! 固定运行资源的工程实操：真实官方工具、完整SDK、私有复制中的身份破坏与取消。
use runtime::dsh_host_process::{self, CallContext};
use runtime::dsh_runtime;
use runtime::managed_process::{self, ExecutionControl};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err("需要固定运行时、真实插件目录、来源回执与输出JSON四个参数".into());
    }
    let root = PathBuf::from(&args[0]).canonicalize()?;
    let plugin = PathBuf::from(&args[1]).canonicalize()?;
    let source: Value = serde_json::from_slice(&fs::read(&args[2])?)?;
    let receipt = source.get("source").unwrap_or(&source);
    let started = Instant::now();
    let verified = dsh_runtime::verify(&root)?;
    assert_eq!(verified.file_count, 290);
    assert_eq!(
        verified.sdk_lock_sha256,
        receipt["sdk_lock_sha256"].as_str().ok_or("SDK锁缺失")?
    );
    let paths = verified.paths(plugin);
    let context = CallContext {
        workspace_id: "fixed-runtime-engineering-probe".into(),
        room_id: "engineering".into(),
        run_id: format!("fixed-runtime-{}", std::process::id()),
        call_id: "official-calculator-96".into(),
    };
    let manifest = dsh_host_process::describe(
        &paths,
        receipt,
        &json!({}),
        &context,
        Duration::from_secs(30),
    )?;
    let result = dsh_host_process::execute(
        &paths,
        receipt,
        &json!({}),
        &context,
        &manifest,
        "calculator",
        &json!({"expression":"15 + 27 * sqrt(9)"}),
        Duration::from_secs(30),
    )?;
    assert_eq!(result["isError"], false);
    assert_eq!(result["value"], 96);

    // 只修改本探针自己的TempDir完整副本，不改正式资源、配置或实际插件来源。
    let private = tempfile::Builder::new()
        .prefix("fixed-dsh-probe-")
        .tempdir()?;
    for entry in walkdir::WalkDir::new(&root).follow_links(false) {
        let entry = entry?;
        let target = private.path().join(entry.path().strip_prefix(&root)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    dsh_runtime::verify(private.path())?;
    let sdk = private
        .path()
        .join("host/node_modules/@deepseek-ai/dsh-tools/lib/index.js");
    let original = fs::read(&sdk)?;
    let mut modified = original.clone();
    modified[0] ^= 1;
    fs::write(&sdk, &modified)?;
    let changed = dsh_runtime::verify(private.path()).expect_err("真实SDK字节变化不能启动宿主");
    fs::write(&sdk, &original)?;
    let extra = private.path().join("host/undeclared.mjs");
    fs::write(&extra, b"export const unexpected = true;\n")?;
    let added = dsh_runtime::verify(private.path()).expect_err("回执外真实文件不能通过");
    fs::remove_file(&extra)?;
    fs::remove_file(&sdk)?;
    let missing = dsh_runtime::verify(private.path()).expect_err("SDK真实文件缺失不能通过");
    fs::write(&sdk, &original)?;
    dsh_runtime::verify(private.path())?;
    let control = ExecutionControl::new(None, None);
    control.cancel();
    let cancelled =
        managed_process::with_execution_control(control, || dsh_runtime::verify(private.path()))
            .expect_err("已取消父运行不能继续核验和启动宿主");
    assert_eq!(cancelled.code, "dsh_runtime_interrupted");
    let copy = private.path().to_owned();
    private.close()?;
    assert!(!copy.exists());
    let report = json!({"passed":true,"notice":"固定运行资源与生产Rust/Node宿主的工程结果；不是正式MSI、市场按钮或真实Qwen验收",
        "runtime":verified,"manifest":manifest,"result":result,"changed_sdk":changed,"added_file":added,
        "missing_sdk":missing,"pre_cancelled":cancelled,"private_copy_cleanup_confirmed":true,
        "elapsed_ms":started.elapsed().as_millis()});
    fs::write(&args[3], serde_json::to_vec_pretty(&report)?)?;
    println!("固定290文件、真实官方计算96、SDK变更/新增/缺失拒绝、预取消与私有副本清理均通过");
    Ok(())
}
