//! 真实固定Node与生产Rust桥的预算回收测试；受控协议夹具不冒充官方SDK工具。
#![cfg(windows)]
use runtime::dsh_host_process::{self, CallContext, HostManifest, HostPaths};
use runtime::managed_process::{with_execution_control, ExecutionControl};
use serde_json::json;
use std::{fs, path::PathBuf, time::{Duration, Instant}};

#[test]
#[ignore = "需要DSH_DEADLINE_RUNTIME指向已核验的完整固定资源，仅使用独立tmp夹具"]
fn parent_deadline_stops_dispatched_node_and_rejects_delayed_result() {
    let runtime_root = PathBuf::from(std::env::var_os("DSH_DEADLINE_RUNTIME").expect("固定资源目录"));
    let verified = runtime::dsh_runtime::verify(&runtime_root).expect("固定290文件必须真实核验");
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../tmp/dsh-deadline-tests");
    fs::create_dir_all(&output).unwrap();
    let private = tempfile::Builder::new().prefix("deadline-").tempdir_in(output).unwrap();
    let root = private.path().canonicalize().unwrap();
    let script = root.join("controlled-host.mjs");
    fs::write(&script, r#"
import fs from 'node:fs';
import path from 'node:path';
const ipc=process.argv[2];
const request=JSON.parse(fs.readFileSync(path.join(ipc,'request.json'),'utf8'));
const envelope={protocol:request.protocol,nonce:request.nonce,context:request.context};
const publish=(name,value)=>{
  const temporary=path.join(ipc,name+'.tmp');
  fs.writeFileSync(temporary,JSON.stringify({...envelope,...value}));
  fs.renameSync(temporary,path.join(ipc,name));
};
publish('manifest.json',{manifest:request.config.manifest});
const poll=setInterval(()=>{
  if (!fs.existsSync(path.join(ipc,'execute.json'))) return;
  clearInterval(poll);
  fs.writeFileSync('dispatched.json',JSON.stringify({pid:process.pid,context:request.context,deadline_ms:request.deadline_ms}));
  // 故意忽略cancel通知，验证生产桥的强制进程回收，而非夹具自行退出。
  setTimeout(()=>{
    fs.writeFileSync('late-result.json',JSON.stringify({value:96}));
    publish('result.json',{status:'executed',result:{value:96}});
  },4000);
},5);
"#).unwrap();
    let manifest: HostManifest = serde_json::from_value(json!({
        "protocol":1,"generation":"controlled-live-generation","revision":"a".repeat(64),
        "plugin":{"name":"controlled-budget-fixture","version":"1"},"services":["tools"],
        "tools":[{"name":"delayed_result","description":"预算测试夹具","input_schema":{"type":"object"}}]
    })).unwrap();
    let paths = HostPaths { node_binary: verified.paths(root.clone()).node_binary,
        entry_script: script, plugin_root: root.clone() };
    let context = CallContext {workspace_id:"isolated-deadline".into(),room_id:"engineering".into(),
        run_id:"deadline-fixture-run".into(),call_id:"deadline-fixture-call".into()};
    let control = ExecutionControl::new(Some(Duration::from_secs(2)), None);
    let started = Instant::now();
    let error = with_execution_control(control, || dsh_host_process::execute(
        &paths, &json!({}), &json!({"manifest":manifest}), &context, &manifest,
        "delayed_result", &json!({}), Duration::from_secs(30),
    )).expect_err("30秒局部时限不能覆盖2秒原父预算，迟到96不能成功");
    let elapsed = started.elapsed();
    let dispatched: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("dispatched.json")).expect("须确认Node收到实际执行消息，不能把未启动算通过")
    ).unwrap();
    assert_eq!(dispatched["context"], serde_json::to_value(&context).unwrap());
    assert_eq!(error.code, "host_interrupted");
    assert_eq!(error.interruption.as_deref(), Some("timed_out"));
    assert!(error.cleanup_confirmed, "实际子进程与Job回收须有确认");
    assert!(elapsed < Duration::from_secs(10), "不能等待较长局部预算");
    std::thread::sleep(Duration::from_secs(5).saturating_sub(started.elapsed()));
    assert!(!root.join("late-result.json").exists(), "已回收进程不得继续迟到写入");
    println!("{}", json!({"真实Node派发":dispatched,"生产桥错误":error,
        "返回耗时毫秒":elapsed.as_millis(),"迟到写入不存在":true,
        "范围":"固定Node与生产Rust桥的受控协议夹具；不是官方SDK、Qwen或GUI验收"}));
}
