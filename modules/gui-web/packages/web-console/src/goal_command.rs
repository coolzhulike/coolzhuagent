//! Goal 命令的真实宿主回执；等待者消失仍保留取消及进程回收责任。
use std::{collections::BTreeMap, path::Path, process::Command, sync::{Mutex, OnceLock}, time::Duration};
use runtime::managed_process::{self, ExecutionControl, Interruption};
use runtime::ProcessExecutionReceipt;
use serde::{Deserialize, Serialize};

static DIAGNOSTIC_RECEIPTS: OnceLock<Mutex<BTreeMap<String, ProcessExecutionReceipt>>> = OnceLock::new();

fn diagnostic_receipts() -> &'static Mutex<BTreeMap<String, ProcessExecutionReceipt>> {
    DIAGNOSTIC_RECEIPTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

/// 只由工具 registry 的真实 shell 出口写入；调用 ID 属于当前模型回合。
pub(crate) fn record_diagnostic_receipt(call_id: &str, receipt: ProcessExecutionReceipt) {
    if call_id.trim().is_empty() { return; }
    let Ok(mut pending) = diagnostic_receipts().lock() else { return; };
    if pending.len() >= 128 {
        if let Some(oldest) = pending.keys().next().cloned() { pending.remove(&oldest); }
    }
    pending.insert(call_id.to_string(), receipt);
}

/// 消费一次即移除；未匹配或旧回执不能留给下一阶段。
pub(crate) fn take_diagnostic_receipt(call_id: &str) -> Option<ProcessExecutionReceipt> {
    diagnostic_receipts().lock().ok()?.remove(call_id)
}

/// Goal Command 与 PowerShell 工具共用同一解释器选择，回执才能按实际执行器匹配。
pub(crate) fn shell_path() -> String {
    if cfg!(windows) {
        tools::resolve_powershell_shell().unwrap_or_else(|_| "powershell.exe".to_string())
    } else {
        "sh".to_string()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub(crate) enum Status {Passed, Failed, TimedOut, Cancelled, Unable}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Receipt {
    pub id:String,
    pub command:String,
    pub cwd:String,
    #[serde(default)]
    pub shell:String,
    pub timeout_ms:u64,
    pub started_at_unix_ms:u64,
    pub finished_at_unix_ms:u64,
    pub status:Status,
    pub exit_code:Option<i32>,
    pub reason:String,
    /// 验证输入快照（相关源码/配置/锁文件内容）；旧记录为空，不能据此复用。
    #[serde(default)]
    pub input_digest:String,
}
impl Receipt {
    fn finish(mut self,status:Status,code:Option<i32>,reason:String)->Self {
        self.status=status;self.exit_code=code;self.reason=reason;
        self.finished_at_unix_ms=crate::unix_timestamp_millis();self
    }
}
struct CancelOnDrop(ExecutionControl);
impl Drop for CancelOnDrop {fn drop(&mut self){self.0.cancel();}}

/// 只哈希会影响构建/测试的相关输入，排除 target/.git/.coolzhu/node_modules。
pub(crate) fn verification_digest(root: &Path) -> String {
    use sha2::Digest;
    let root = &std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    fn relevant(path: &Path) -> bool {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("");
        let ext = path.extension().and_then(|ext| ext.to_str()).unwrap_or("").to_ascii_lowercase();
        matches!(name, "Cargo.toml" | "Cargo.lock" | "build.rs")
            || matches!(ext.as_str(), "rs" | "toml" | "lock" | "yaml" | "yml" | "ps1" | "bat" | "cmd")
    }
    fn walk(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return; };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if path.is_dir() {
                if matches!(name.as_str(), "target" | ".git" | ".coolzhu" | "node_modules") { continue; }
                walk(&path, files);
            } else if relevant(&path) { files.push(path); }
        }
    }
    let mut files = Vec::new();
    walk(root, &mut files);
    files.sort();
    let mut hasher = sha2::Sha256::new();
    hasher.update(root.to_string_lossy().as_bytes());
    for file in files.into_iter().take(4096) {
        let bytes = std::fs::read(&file).unwrap_or_default();
        hasher.update(file.to_string_lossy().as_bytes());
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    format!("{:x}", hasher.finalize())
}

pub(crate) async fn run(command:&str,cwd:&Path,timeout_ms:u64)->Result<(),String> {
    let result=run_receipt(command,cwd,timeout_ms).await;
    if result.status==Status::Passed {Ok(())}else{Err(result.reason)}
}
pub(crate) async fn run_receipt(command:&str,cwd:&Path,timeout_ms:u64)->Receipt {
    static SEQUENCE:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(1);
    let now=crate::unix_timestamp_millis();
    let timeout_ms=crate::root_execution_budget::limit_current_ms(timeout_ms);
    let shell=shell_path();
    let receipt=Receipt {id:format!("goal-command-{now}-{}-{}",std::process::id(),SEQUENCE.fetch_add(1,std::sync::atomic::Ordering::Relaxed)),
        command:command.into(),cwd:cwd.to_string_lossy().into_owned(),shell:shell.clone(),timeout_ms,started_at_unix_ms:now,finished_at_unix_ms:now,
        status:Status::Unable,exit_code:None,reason:String::new(),input_digest:verification_digest(cwd)};
    if timeout_ms==0{return receipt.finish(Status::Unable,None,"Goal 验证命令未启动：执行时限已到".into());}
    let external_cancel=crate::current_turn_trace().as_deref().map(crate::tool_turn_cancellation_checker);
    let control=ExecutionControl::new(Some(Duration::from_millis(timeout_ms)),external_cancel);
    let _cancel_on_drop=CancelOnDrop(control.clone());
    let fallback=receipt.clone();let lost=receipt.clone();
    let worker=tokio::task::spawn_blocking(move || {
        managed_process::with_execution_control(control, || {
            let mut process=if cfg!(windows){let mut p=Command::new(&shell);p.args(["-NoProfile","-NonInteractive","-Command",&receipt.command]);p}
                else{let mut p=Command::new(&shell);p.args(["-c",&receipt.command]);p};
            process.current_dir(&receipt.cwd);
            #[cfg(windows)] {use std::os::windows::process::CommandExt;process.creation_flags(0x08000000);}
            let result=match managed_process::output(&mut process,None){Ok(r)=>r,Err(e)=>{
                let reason=format!("`{}` 未能完成：{e}",receipt.command);return receipt.finish(Status::Unable,None,reason);
            }};
            let code=result.output().status.code();
            if let Some(interruption)=result.interruption(){
                let (status,reason)=match interruption{Interruption::TimedOut=>(Status::TimedOut,"超时"),Interruption::Cancelled=>(Status::Cancelled,"已取消")};
                let reason=format!("`{}` {reason}；所属进程已回收",receipt.command);return receipt.finish(status,code,reason);
            }
            if result.output().status.success(){return receipt.finish(Status::Passed,code,String::new());}
            let combined=format!("{}\n{}",managed_process::decode_console_output(&result.output().stderr),managed_process::decode_console_output(&result.output().stdout));
            let lines=combined.lines().collect::<Vec<_>>();let tail=lines[lines.len().saturating_sub(20)..].join("\n");
            let reason=format!("`{}` 失败（exit {}）：\n{tail}",receipt.command,code.map(|c|c.to_string()).unwrap_or_else(||"unknown".into()));
            receipt.finish(Status::Failed,code,reason)
        })
    });
    let (sender,receiver)=tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result=worker.await.unwrap_or_else(|e|fallback.finish(Status::Unable,None,format!("Goal 验证 worker 异常：{e}")));
        if let Err(late)=sender.send(result){tracing::info!(receipt_id=%late.id,status=?late.status,"Goal 验证命令在等待者结束后完成收尾");}
    });
    receiver.await.unwrap_or_else(|e|lost.finish(Status::Unable,None,format!("Goal 验证监督回执丢失，结果未知：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    #[cfg(windows)]
    async fn output_overflow_cannot_be_reported_as_successful_goal_verification() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        for stream in ["Out", "Error"] {
            let command = format!("[Console]::{stream}.Write(('x' * {})); exit 0",
                managed_process::MAX_CAPTURE_BYTES_PER_PIPE + 8192);
            let error = run(&command, cwd, 30_000).await.unwrap_err();
            assert!(error.contains("输出超限"), "必须保留采集失败事实：{error}");
            assert!(error.contains("进程已启动"));
        }
    }

    #[tokio::test]
    async fn gate_exit_and_expired_root_use_the_same_managed_path() {
        let cwd = Path::new(env!("CARGO_MANIFEST_DIR"));
        let success_started = Instant::now();
        let success = run("exit 0", cwd, 90_000).await;
        assert!(
            success.is_ok(),
            "正常 shell 探针失败（elapsed={:?}）：{success:?}",
            success_started.elapsed()
        );
        let failure_started = Instant::now();
        let failure = run("exit 1", cwd, 90_000).await;
        assert!(
            matches!(&failure, Err(message) if message.contains("失败（exit 1）")),
            "非零退出探针未返回明确退出码（elapsed={:?}）：{failure:?}",
            failure_started.elapsed()
        );
        crate::root_execution_budget::scope(
            crate::root_execution_budget::RootExecutionBudget::from_started_at(1, 1),
            async { assert!(run("exit 0", cwd, 30_000).await.unwrap_err().contains("未启动")); }
        ).await;
    }
}
