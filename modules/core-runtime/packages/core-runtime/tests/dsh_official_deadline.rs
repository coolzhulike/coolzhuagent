//! 官方固定SDK/计算器的执行中截止。调试断点仅注入调度延迟，生产来源文件不变。
#![cfg(windows)]
use plugins::DshPackage;
use runtime::{
    dsh_host_process::{self, CallContext},
    dsh_runtime,
    managed_process::{ExecutionControl, with_execution_control},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use windows_process_guard::{ProcessIdentityError, capture_live_process_identity};
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn hash(path: impl AsRef<Path>) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}
fn node_path(path: &Path) -> String {
    path.to_string_lossy()
        .strip_prefix(r"\\?\")
        .unwrap_or(&path.to_string_lossy())
        .to_owned()
}
fn env_path(key: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(key).expect(key))
        .canonicalize()
        .unwrap()
}

#[test]
#[ignore = "需要已有官方固定资源/来源/回执与独立空输出目录；不触碰正式实例"]
fn official_function_entered_parent_deadline_reclaims_and_rejects_late_result() {
    let runtime_root = env_path("DSH_OFFICIAL_RUNTIME");
    let source = env_path("DSH_OFFICIAL_SOURCE");
    let output = env_path("DSH_OFFICIAL_OUTPUT");
    assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
    let package: DshPackage = serde_json::from_value(
        read(env_path("DSH_OFFICIAL_RECEIPT"))["snapshot"]["package"].clone(),
    )
    .unwrap();
    assert_eq!(package.receipt.name, "@deepseek-ai/dsh-tool-calculator");
    assert_eq!(package.commit, "b2007a13f06bcf75bf07b9d277ee8d434a316490");
    package.verify(&source).unwrap();
    let verified = dsh_runtime::verify(&runtime_root).unwrap();
    assert_eq!(package.receipt.sdk_lock_sha256, verified.sdk_lock_sha256);
    let receipt = serde_json::to_value(&package.receipt).unwrap();
    let original = verified.paths(source.clone());
    let context = CallContext {
        workspace_id: "isolated-official-deadline".into(),
        room_id: "engineering".into(),
        run_id: "official-deadline-run".into(),
        call_id: "official-describe".into(),
    };
    let manifest = dsh_host_process::describe(
        &original,
        &receipt,
        &json!({}),
        &context,
        Duration::from_secs(30),
    )
    .unwrap();
    let mut evidence = Vec::new();
    for mode in ["normal", "late", "forced"] {
        let case = output.join(mode);
        fs::create_dir(&case).unwrap();
        let wrapper = case.join("entry-probe.mjs");
        fs::write(&wrapper, include_str!("fixtures/dsh-official-entry.mjs")).unwrap();
        fs::write(case.join("probe-config.json"),serde_json::to_vec_pretty(&json!({"mode":mode,
            "production_entry":node_path(&original.entry_script),"evaluate_sha256":hash(source.join("lib/evaluate.js"))})).unwrap()).unwrap();
        let mut paths = original.clone();
        paths.entry_script = wrapper;
        let mut call = context.clone();
        call.call_id = format!("official-{mode}");
        let expected_call = call.clone();
        let receipt = receipt.clone();
        let manifest = manifest.clone();
        let budget = Duration::from_secs(8);
        let control = ExecutionControl::new(Some(budget), None);
        let started = Instant::now();
        let child = std::thread::spawn(move || {
            with_execution_control(control, || {
                dsh_host_process::execute(
                    &paths,
                    &receipt,
                    &json!({}),
                    &call,
                    &manifest,
                    "calculator",
                    &json!({"expression":"12 * (3 + 5)"}),
                    Duration::from_secs(30),
                )
            })
        });
        let marker = case.join("entered.json");
        while !marker.exists()
            && !child.is_finished()
            && started.elapsed() < Duration::from_secs(12)
        {
            std::thread::sleep(Duration::from_millis(2));
        }
        let observation = marker.is_file().then(|| read(&marker));
        let identity = observation.as_ref().map(|event| {
            capture_live_process_identity(event["pid"].as_u64().unwrap() as u32)
                .expect("须在真实函数暂停期间记录存活的Node实例身份")
        });
        let result = child.join().unwrap();
        let elapsed = started.elapsed();
        let outcome = match &result {
            Ok(value) => json!({"ok":true,"result":value}),
            Err(error) => json!({"ok":false,"error":error}),
        };
        fs::write(
            case.join("rust-outcome.json"),
            serde_json::to_vec_pretty(&json!({"elapsed_ms":elapsed.as_millis(),"outcome":outcome}))
                .unwrap(),
        )
        .unwrap();
        let observation =
            observation.expect("必须命中官方函数内部，握手不足以通过；查看rust-outcome.json");
        assert_eq!(
            observation["context"],
            serde_json::to_value(&expected_call).unwrap()
        );
        assert!(
            observation["entered_at_ms"].as_u64().unwrap()
                < observation["deadline_ms"].as_u64().unwrap()
        );
        let frames = observation["frames"].as_array().unwrap();
        assert!(frames.iter().any(|f| {
            f["function"] == "parse"
                && f["url"]
                    .as_str()
                    .unwrap_or("")
                    .ends_with("/lib/evaluate.js")
        }));
        assert!(frames.iter().any(|f| f["function"] == "evaluate"));
        assert!(
            frames.iter().any(
                |f| f["url"].as_str().unwrap_or("").ends_with("/lib/index.js")
                    && f["url"].as_str().unwrap_or("").contains("dsh-tools")
            ),
            "调用栈须包含真实官方SDK"
        );
        let identity = identity.unwrap();
        let exit_evidence = match capture_live_process_identity(identity.pid()) {
            Err(ProcessIdentityError::NotFound { .. }) => "原PID不存在或进程句柄已退出",
            Ok(actual) if !identity.is_same_instance(&actual) => "PID已复用，原创建时间实例已退出",
            other => panic!("未证实原Node实例退出，拒绝将访问失败当作退出: {other:?}"),
        };
        let terminal = case
            .join("terminal-copy.json")
            .is_file()
            .then(|| read(case.join("terminal-copy.json")));
        if mode == "normal" {
            assert_eq!(result.unwrap()["value"], 96);
            assert!(terminal.is_some());
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.interruption.as_deref(), Some("timed_out"));
            assert!(error.cleanup_confirmed);
            assert!(elapsed >= budget && elapsed < budget + Duration::from_secs(3));
            if mode == "late" {
                let resumed = read(case.join("resumed.json"));
                assert!(
                    resumed["at_ms"].as_u64().unwrap() > resumed["deadline_ms"].as_u64().unwrap()
                );
                assert_eq!(resumed["cancellation_file_present"], true);
                assert!(terminal.is_some(), "须保留实际生产终态，不能只证明杀进程");
                let late = terminal.as_ref().unwrap();
                assert_eq!(late["terminal"]["status"], "executed");
                assert_eq!(late["terminal"]["result"]["value"], 96);
                assert!(
                    late["at_ms"].as_u64().unwrap() > observation["deadline_ms"].as_u64().unwrap()
                );
            } else {
                assert!(!case.join("resumed.json").exists());
                assert!(terminal.is_none());
                // 超过预定恢复时刻后，已回收进程也不能继续写入。
                std::thread::sleep(
                    (budget + Duration::from_millis(4500)).saturating_sub(started.elapsed()),
                );
                assert!(
                    !case.join("resumed.json").exists()
                        && !case.join("terminal-copy.json").exists()
                );
            }
        }
        package.verify(&source).unwrap();
        verified.reverify().unwrap();
        let row = json!({"mode":mode,"entered":observation,"elapsed_ms":elapsed.as_millis(),"outcome":outcome,
            "process_identity":{"pid":identity.pid(),"creation_time_filetime":identity.creation_time_filetime(),"image_path":identity.image_path()},
            "exit_evidence":exit_evidence,"terminal":terminal,"official_sources_unchanged":true});
        println!("模式{mode}通过，生产桥返回{}ms", elapsed.as_millis());
        evidence.push(row);
    }
    fs::write(output.join("result.json"),serde_json::to_vec_pretty(&json!({"passed":true,"package":package,
        "manifest":manifest,"cases":evidence,"scope":"真实官方SDK/函数，inspector有界暂停注入；非自然慢函数或GUI/模型验收"})).unwrap()).unwrap();
}
