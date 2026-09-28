//! 通用同步工具的执行所有权：等待超时不等于 worker 已退出。
use super::*;
use runtime::managed_process::{with_execution_control, ExecutionControl};

#[derive(Clone)]
struct WorkerScope {
    workspace: String,
    session: Option<String>,
    unsettled: bool,
}

fn workers() -> &'static Mutex<HashMap<u64, WorkerScope>> {
    static WORKERS: OnceLock<Mutex<HashMap<u64, WorkerScope>>> = OnceLock::new();
    WORKERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mark_unsettled(id: u64) {
    if let Ok(mut workers) = workers().lock() {
        if let Some(worker) = workers.get_mut(&id) { worker.unsettled = true; }
    }
}

struct CallerGuard { id: u64, control: ExecutionControl, finished: bool }
impl Drop for CallerGuard {
    fn drop(&mut self) {
        if !self.finished {
            mark_unsettled(self.id);
            self.control.cancel();
        }
    }
}

/// blocking worker 始终由独立监督 task 持有；HTTP future 被取消也不会丢弃回收与事实。
pub(super) async fn execute(
    invoke: ToolInvoke,
    workspace_root: PathBuf,
    timeout_ms: u64,
    chat_room_id: Option<String>,
) -> ToolOutcome {
    execute_with_executor(invoke, workspace_root, timeout_ms, chat_room_id, None).await
}

pub(super) async fn execute_with_executor(
    invoke: ToolInvoke,
    workspace_root: PathBuf,
    timeout_ms: u64,
    chat_room_id: Option<String>,
    executor: Option<Arc<dyn ToolInvocationExecutor>>,
) -> ToolOutcome {
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    let id = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    {
        let Ok(mut entries) = workers().lock() else {
            return runtime_tool_failed_outcome(&invoke, "工具监督器不可用，未开始执行".into());
        };
        if entries.values().any(|entry| entry.unsettled
            && entry.workspace == invoke.workspace_id && entry.session == invoke.session_id) {
            return runtime_tool_failed_outcome(&invoke,
                "该会话仍有已请求停止但未确认退出的工具；本次未执行，等待真实收尾后再继续".into());
        }
        entries.insert(id, WorkerScope { workspace: invoke.workspace_id.clone(),
            session: invoke.session_id.clone(), unsettled: false });
    }
    let external_cancel = current_turn_trace().as_deref().map(tool_turn_cancellation_checker);
    let control = ExecutionControl::new(Some(Duration::from_millis(timeout_ms)), external_cancel);
    let mut guard = CallerGuard { id, control: control.clone(), finished: false };
    let scheduled_grant = scheduled_execution::current();
    let invoke_for_worker = invoke.clone();
    let invoke_for_report = invoke.clone();
    let (sender, mut receiver) = tokio::sync::oneshot::channel();
    let worker_control = control.clone();
    let worker = tokio::task::spawn_blocking(move || {
        if worker_control.interruption().is_some() {
            return runtime_tool_failed_outcome(&invoke_for_worker,
                "工具在开始前已取消或截止，未执行".into());
        }
        with_execution_control(worker_control, || match executor {
            Some(executor) => execute_runtime_tool_blocking_with_executor(
                invoke_for_worker, workspace_root, chat_room_id, scheduled_grant,
                executor.as_ref()),
            None => execute_runtime_tool_blocking(
                invoke_for_worker, workspace_root, chat_room_id, scheduled_grant),
        })
    });
    tokio::spawn(async move {
        let outcome = worker.await.unwrap_or_else(|error| runtime_tool_failed_outcome(
            &invoke_for_report, format!("工具 worker 异常退出：{error}")));
        if let Ok(mut entries) = workers().lock() { entries.remove(&id); }
        if let Err(mut late) = sender.send(outcome) {
            // 调用者已经结束，但执行事实仍要留下；不把迟到成功覆盖先前的超时记录。
            late.summary_text = format!("[迟到收尾 supervisor={id}] {}", late.summary_text);
            append_tool_audit_record(&invoke_for_report, &late);
        }
    });
    let outcome = match tokio::time::timeout(Duration::from_millis(timeout_ms), &mut receiver).await {
        Ok(result) => received(&invoke, result),
        Err(_) => {
            mark_unsettled(id);
            control.cancel();
            // 这是有界回收宽限，不增加业务执行预算；进程执行器已收到停止信号。
            match tokio::time::timeout(Duration::from_secs(5), &mut receiver).await {
                Ok(result) => {
                    let mut actual = received(&invoke, result);
                    let actual_status = actual.status.as_str();
                    actual.output = json!({ "wait_budget_exceeded": true, "worker_finished": true,
                        "actual_status": actual_status, "result": actual.output });
                    actual.status = ToolOutcomeStatus::Timeout;
                    actual.summary_text = format!("等待预算已耗尽；worker 已真实结束：{}", actual.summary_text);
                    actual
                }
                Err(_) => {
                    let mut pending = runtime_tool_timeout_outcome(&invoke, timeout_ms);
                    pending.output = json!({ "timeout_ms": timeout_ms, "supervisor_id": id,
                        "execution_state": "running_unconfirmed", "cancellation_requested": true,
                        "retry_safe": false, "permission_evaluation": "awaiting_actual_result" });
                    pending.summary_text = "等待超时，已请求停止；worker 尚未确认退出，后台继续监督，同会话新工具暂时阻断".into();
                    pending.permission_gate = runtime::PermissionGateReport::deny(
                        required_permission_for_tool(&invoke.tool_name),
                        "尚未收到实际权限结果；此处禁止新派发，不推断原调用已获授权或已退出");
                    pending
                }
            }
        }
    };
    guard.finished = true;
    outcome
}

fn received(invoke: &ToolInvoke, result: Result<ToolOutcome, tokio::sync::oneshot::error::RecvError>) -> ToolOutcome {
    result.unwrap_or_else(|error| runtime_tool_failed_outcome(invoke,
        format!("工具监督回执丢失，执行结果未知：{error}")))
}
