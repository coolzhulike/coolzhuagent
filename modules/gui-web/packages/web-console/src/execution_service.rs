//! 子 Agent 与定时投递共用执行索引。会话、权限、模型循环和 usage 仍归原运行模块。
use crate::{root_execution_budget::RootExecutionBudget, ChatTurnCancellation};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    SessionTool,
    Scheduled,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ExecutionSpec {
    pub id: String,
    pub workspace_id: String,
    pub room_id: String,
    pub session_id: String,
    pub provider: String,
    pub model: String,
    pub source: Source,
    pub source_id: String,
    pub parent_run_id: Option<String>,
    pub parent_execution_id: Option<String>,
    pub call_id: Option<String>,
    pub summary: String,
    pub deadline_ms: u64,
}

/// 契约按实际入口声明；ACP普通聊天不能推出ACP子Agent或Goal可用。
pub(crate) fn require_capability(provider: &str, source: Source) -> Result<(), String> {
    use crate::agent_session_backend::AgentSessionBackend as B;
    match (B::for_provider(provider), source) {
        (B::LlmHttp, _) | (B::DevinAcp, Source::Scheduled) => Ok(()),
        (B::DevinAcp, Source::SessionTool) => Err("Devin ACP 尚未提供冻结模型快照的会话工具契约，子 Agent 未执行；普通聊天及定时 poll 不受此限制".into()),
        (B::DevinCloud, _) => Err("Devin Cloud 尚未接入共享执行服务，未派发请求".into()),
    }
}

struct Live {
    path: PathBuf,
    workspace: String,
    cancel: Arc<ChatTurnCancellation>,
}
fn live() -> &'static Mutex<HashMap<String, Live>> {
    static LIVE: OnceLock<Mutex<HashMap<String, Live>>> = OnceLock::new();
    LIVE.get_or_init(Default::default)
}
#[derive(Clone)]
struct Current {
    path: PathBuf,
    id: String,
    workspace: String,
}
tokio::task_local! { static CURRENT: Current; }

pub(crate) fn apply_session_migration_v29(connection: &Connection) -> rusqlite::Result<()> {
    // 同一session库内的关联索引，无第二份会话、凭据、记忆或调度计划。
    connection.execute_batch("CREATE TABLE IF NOT EXISTS execution_index (
      id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL, spec_json TEXT NOT NULL,
      state TEXT NOT NULL, started_ms INTEGER NOT NULL, finished_ms INTEGER,
      run_id TEXT, result TEXT, error TEXT);
      CREATE INDEX IF NOT EXISTS execution_workspace_started ON execution_index(workspace_id,started_ms);
      CREATE UNIQUE INDEX IF NOT EXISTS execution_bound_run ON execution_index(run_id) WHERE run_id IS NOT NULL;")?;
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version < 29 {
        connection.execute_batch("PRAGMA user_version = 29;")?;
    }
    Ok(())
}
fn open(path: &Path) -> Result<Connection, String> {
    let connection = crate::open_session_connection(path).map_err(|e| e.to_string())?;
    Ok(connection)
}

pub(crate) struct ExecutionService;
pub(crate) struct TurnRunner;
struct Guard {
    current: Current,
    finished: bool,
}
impl Guard {
    fn finish(
        &mut self,
        state: &str,
        result: Option<&str>,
        error: Option<&str>,
    ) -> Result<(), String> {
        let changed = open(&self.current.path)?.execute(
            "UPDATE execution_index SET state=?1,finished_ms=?2,result=?3,error=?4 WHERE id=?5 AND workspace_id=?6 AND state IN ('running','cancel_requested')",
            params![state,crate::unix_timestamp_millis(),result,error,self.current.id,self.current.workspace]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("执行索引终态未确认，不可自动重试".into());
        }
        self.finished = true;
        Ok(())
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut entries) = live().lock() {
            entries.remove(&self.current.id);
        }
        if !self.finished {
            if let Ok(connection) = open(&self.current.path) {
                let _=connection.execute("UPDATE execution_index SET state='outcome_unknown',finished_ms=?1,error='执行等待被丢弃，未确认结果，不自动重放' WHERE id=?2 AND state IN ('running','cancel_requested')",
                    params![crate::unix_timestamp_millis(),self.current.id]);
            }
        }
    }
}
impl ExecutionService {
    /// 只接纳已冻结的来源。重复ID拒绝，不重放；定时领取仍由scheduled_jobs负责。
    fn begin(
        path: &Path,
        mut spec: ExecutionSpec,
        cancel: Arc<ChatTurnCancellation>,
    ) -> Result<Guard, String> {
        require_capability(&spec.provider, spec.source)?;
        if [
            &spec.id,
            &spec.workspace_id,
            &spec.room_id,
            &spec.session_id,
            &spec.source_id,
        ]
        .iter()
        .any(|v| v.trim().is_empty())
        {
            return Err("执行缺少来源、工程、房间或会话身份".into());
        }
        if spec.source == Source::SessionTool
            && (spec.parent_run_id.is_none() || spec.call_id.is_none())
        {
            return Err("会话工具缺少真实父运行或调用身份".into());
        }
        spec.parent_execution_id = CURRENT.try_with(|c| c.id.clone()).ok();
        let connection = open(path)?;
        if spec.parent_execution_id.is_none() {
            // 原并行工具派发spawn不会隐式继承task-local；按真实父run关联已接纳执行。
            if let Some(run) = spec.parent_run_id.as_deref() {
                spec.parent_execution_id = connection
                    .query_row(
                        "SELECT id FROM execution_index WHERE run_id=?1 AND workspace_id=?2",
                        params![run, spec.workspace_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
            }
        }
        connection.execute("INSERT INTO execution_index(id,workspace_id,spec_json,state,started_ms) VALUES(?1,?2,?3,'running',?4)",
            params![spec.id,spec.workspace_id,serde_json::to_string(&spec).map_err(|e|e.to_string())?,crate::unix_timestamp_millis()]).map_err(|e|format!("执行未领取（不重放）：{e}"))?;
        let current = Current {
            path: path.into(),
            id: spec.id.clone(),
            workspace: spec.workspace_id.clone(),
        };
        let guard = Guard {
            current,
            finished: false,
        };
        live().lock().map_err(|_| "执行控制锁不可用")?.insert(
            spec.id,
            Live {
                path: path.into(),
                workspace: spec.workspace_id,
                cancel,
            },
        );
        Ok(guard)
    }

    /// 调用原聊天接纳点时绑定真实run及取消token；不另建聊天运行行。
    pub(crate) fn bind_chat(
        path: &Path,
        workspace: &str,
        run: &str,
        cancel: Arc<ChatTurnCancellation>,
    ) -> Result<(), String> {
        let Ok(current) = CURRENT.try_with(Clone::clone) else {
            return Ok(());
        };
        if current.path != path || current.workspace != workspace {
            return Err("执行服务与聊天接纳工程不一致".into());
        }
        let changed=open(path)?.execute("UPDATE execution_index SET run_id=?1 WHERE id=?2 AND workspace_id=?3 AND state IN ('running','cancel_requested')",
            params![run,current.id,workspace]).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("聊天接纳没有对应执行索引".into());
        }
        let mut entries = live().lock().map_err(|_| "执行控制锁不可用")?;
        let entry = entries.get_mut(&current.id).ok_or("执行控制已结束")?;
        if entry.cancel.is_requested() {
            cancel.request();
        }
        entry.cancel = cancel;
        Ok(())
    }

    pub(crate) fn cancel(path: &Path, workspace: &str, id: &str) -> Result<(), String> {
        let entries = live().lock().map_err(|_| "执行控制锁不可用")?;
        let entry = entries
            .get(id)
            .filter(|e| e.path == path && e.workspace == workspace)
            .ok_or("本执行无存活控制句柄；结果可能未知，请检查轨迹，不自动重试")?;
        // 先持久意图，再通知实际worker；不把请求取消声称为已取消。
        open(path)?.execute("UPDATE execution_index SET state='cancel_requested' WHERE id=?1 AND workspace_id=?2 AND state='running'",params![id,workspace]).map_err(|e|e.to_string())?;
        let run: Option<String> = open(path)?
            .query_row(
                "SELECT run_id FROM execution_index WHERE id=?1 AND workspace_id=?2",
                params![id, workspace],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if let Some(run) = run {
            let response = crate::interrupt_runtime_run_sqlite(path, &run, Some("执行页请求停止"))
                .map_err(|e| e.to_string())?;
            crate::signal_runtime_run_after_interrupt(path, &response);
        }
        entry.cancel.request();
        Ok(())
    }

    pub(crate) fn list(
        path: &Path,
        workspace: &str,
        id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let connection = open(path)?;
        let mut statement=connection.prepare("SELECT spec_json,state,started_ms,finished_ms,run_id,result,error FROM execution_index WHERE workspace_id=?1 AND (?2 IS NULL OR id=?2) ORDER BY started_ms DESC,id DESC LIMIT 200").map_err(|e|e.to_string())?;
        let rows = statement
            .query_map(params![workspace, id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, Option<u64>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        let entries = live().lock().map_err(|_| "执行控制锁不可用")?;
        let mut items = Vec::new();
        for row in rows {
            let (encoded, state, started, finished, run, result, error) =
                row.map_err(|e| e.to_string())?;
            let spec: ExecutionSpec = serde_json::from_str(&encoded).map_err(|e| e.to_string())?;
            let active = entries
                .get(&spec.id)
                .is_some_and(|e| e.path == path && e.workspace == workspace);
            let visible_state =
                if !active && matches!(state.as_str(), "running" | "cancel_requested") {
                    "outcome_unknown"
                } else {
                    &state
                };
            let usage_run = run.as_deref().or(spec.parent_run_id.as_deref());
            let usage = usage_run
                .map(|run| {
                    crate::request_usage::requests_for_run(
                        &connection,
                        workspace,
                        &spec.room_id,
                        run,
                        None,
                    )
                })
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or_default()
                .into_iter()
                .filter(|fact| {
                    spec.source != Source::SessionTool
                        || fact["call_id"].as_str() == spec.call_id.as_deref()
                })
                .collect::<Vec<_>>();
            // 逐attempt真实分账；未知值保留null，禁止用本地估算伪装token。
            let sum = |field: &str| -> Option<i64> {
                if usage.is_empty() || usage.iter().any(|v| v[field].as_i64().is_none()) {
                    None
                } else {
                    Some(usage.iter().filter_map(|v| v[field].as_i64()).sum())
                }
            };
            let input_tokens = sum("input_tokens");
            let output_tokens = sum("output_tokens");
            let usage_count = usage.len();
            let usage = if id.is_some() {
                usage
            } else {
                usage
                    .into_iter()
                    .rev()
                    .take(8)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect()
            };
            let preview = |value: Option<String>| {
                value.map(|text| {
                    if id.is_some() {
                        text
                    } else {
                        text.chars().take(320).collect()
                    }
                })
            };
            items.push(serde_json::json!({"spec":spec,"status":visible_state,"started_ms":started,"finished_ms":finished,
                "elapsed_ms":finished.or_else(||active.then(crate::unix_timestamp_millis)).map(|end|end.saturating_sub(started)),
                "run_id":run,"result":preview(result),"error":preview(error),"can_cancel":active && matches!(state.as_str(),"running"|"cancel_requested"),
                "input_tokens":input_tokens,"output_tokens":output_tokens,"usage_attempt_count":usage_count,"usage_attempts":usage}));
        }
        Ok(items)
    }
}

impl TurnRunner {
    /// 共享生命周期，不spawn、不复制模型循环；保持原task-local权限和原终态提交。
    pub(crate) async fn run<T, F: Future<Output = Result<T, String>>>(
        path: &Path,
        spec: ExecutionSpec,
        cancel: Arc<ChatTurnCancellation>,
        future: F,
        result_text: impl FnOnce(&T) -> String,
    ) -> Result<T, String> {
        let mut guard = ExecutionService::begin(path, spec, cancel.clone())?;
        let result = CURRENT.scope(guard.current.clone(), future).await;
        match &result {
            Ok(value) => guard.finish("completed", Some(&result_text(value)), None)?,
            Err(error) => {
                let cancelled = cancel.is_requested()
                    || live()
                        .lock()
                        .ok()
                        .and_then(|entries| {
                            entries
                                .get(&guard.current.id)
                                .map(|entry| entry.cancel.is_requested())
                        })
                        .unwrap_or(false);
                guard.finish(
                    if cancelled { "interrupted" } else { "failed" },
                    None,
                    Some(error),
                )?;
            }
        }
        result
    }

    /// 子执行有自己的取消token，父取消/预算到期向下传播，取消子执行不取消父聊天室。
    pub(crate) async fn child<T, F: Future<Output = Result<T, String>>>(
        parent: Arc<ChatTurnCancellation>,
        child: Arc<ChatTurnCancellation>,
        budget: RootExecutionBudget,
        future: F,
    ) -> Result<T, String> {
        tokio::pin!(future);
        tokio::select! {
            biased;
            _=observe_cancelled(&parent)=>{child.request();Err("父运行已取消，子执行已停止等待".into())},
            _=budget.expired()=>{child.request_timeout();Err("父运行已超时，子执行已停止等待".into())},
            _=observe_cancelled(&child)=>Err("子执行已请求取消；已发生的效果须查看轨迹".into()),
            result=&mut future=>result,
        }
    }
}

/// 原Notify只保留一个permit，附加观察者不能抢走聊天主等待器的通知。
async fn observe_cancelled(cancel: &ChatTurnCancellation) {
    while !cancel.is_requested() {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

fn request_pin(
    headers: &crate::HeaderMap,
) -> crate::ApiResult<crate::workspace_activity::WorkspacePin> {
    // 执行控制必须携带冻结工程和配置加载身份；不能隐式作用于当前工程。
    for name in ["x-coolzhu-workspace-id", "x-coolzhu-configuration-scope"] {
        if headers
            .get(name)
            .is_none_or(|value| value.as_bytes().is_empty())
        {
            return Err(crate::api_error(
                crate::StatusCode::BAD_REQUEST,
                "执行请求缺少冻结工程及配置身份",
            ));
        }
    }
    crate::session_config_service::configuration_request_pin(headers)
}

pub(crate) async fn api_trace(
    crate::AxumPath(id): crate::AxumPath<String>,
    headers: crate::HeaderMap,
    query: crate::Query<crate::chat_insights::TraceQuery>,
) -> crate::ApiResult<crate::Json<serde_json::Value>> {
    let _pin = request_pin(&headers)?;
    let item = ExecutionService::list(
        &crate::default_session_sqlite_path(),
        &crate::workspace_identity(&crate::active_workspace_path()),
        Some(&id),
    )
    .map_err(|e| crate::api_error(crate::StatusCode::CONFLICT, &e))?
    .pop()
    .ok_or_else(|| {
        crate::api_error(
            crate::StatusCode::NOT_FOUND,
            "执行记录不存在或不属于当前工程",
        )
    })?;
    let room = item["spec"]["room_id"]
        .as_str()
        .ok_or_else(|| crate::api_error(crate::StatusCode::CONFLICT, "执行房间身份无效"))?;
    crate::chat_insights::trace(crate::AxumPath(room.to_owned()), query).await
}

pub(crate) async fn api_list(
    headers: crate::HeaderMap,
) -> crate::ApiResult<crate::Json<serde_json::Value>> {
    let _pin = request_pin(&headers)?;
    let items = ExecutionService::list(
        &crate::default_session_sqlite_path(),
        &crate::workspace_identity(&crate::active_workspace_path()),
        None,
    )
    .map_err(|e| crate::api_error(crate::StatusCode::CONFLICT, &e))?;
    Ok(crate::Json(serde_json::json!({"executions":items})))
}
pub(crate) async fn api_cancel(
    crate::AxumPath(id): crate::AxumPath<String>,
    headers: crate::HeaderMap,
) -> crate::ApiResult<crate::Json<serde_json::Value>> {
    let _pin = request_pin(&headers)?;
    ExecutionService::cancel(
        &crate::default_session_sqlite_path(),
        &crate::workspace_identity(&crate::active_workspace_path()),
        &id,
    )
    .map_err(|e| crate::api_error(crate::StatusCode::CONFLICT, &e))?;
    Ok(crate::Json(
        serde_json::json!({"id":id,"status":"cancel_requested"}),
    ))
}

pub(crate) async fn api_detail(
    crate::AxumPath(id): crate::AxumPath<String>,
    headers: crate::HeaderMap,
) -> crate::ApiResult<crate::Json<serde_json::Value>> {
    let _pin = request_pin(&headers)?;
    let mut items = ExecutionService::list(
        &crate::default_session_sqlite_path(),
        &crate::workspace_identity(&crate::active_workspace_path()),
        Some(&id),
    )
    .map_err(|e| crate::api_error(crate::StatusCode::CONFLICT, &e))?;
    let item = items.pop().ok_or_else(|| {
        crate::api_error(
            crate::StatusCode::NOT_FOUND,
            "执行记录不存在或不属于当前工程",
        )
    })?;
    Ok(crate::Json(item))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opening_does_not_migrate_and_explicit_step_preserves_newer_version() {
        let temp = tempfile::tempdir().unwrap();
        let connection = open(&temp.path().join("uninitialized.sqlite3")).unwrap();
        let count: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='execution_index'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        apply_session_migration_v29(&connection).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            29
        );
        connection
            .execute_batch("PRAGMA user_version = 30;")
            .unwrap();
        apply_session_migration_v29(&connection).unwrap();
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            30
        );
    }

    #[test]
    fn execution_requests_require_both_scope_headers_and_reject_stale_workspace() {
        let mut headers = crate::HeaderMap::new();
        assert!(request_pin(&headers).is_err());
        headers.insert("x-coolzhu-workspace-id", "stale-workspace".parse().unwrap());
        assert!(request_pin(&headers).is_err());
        headers.insert(
            "x-coolzhu-configuration-scope",
            "stale-configuration".parse().unwrap(),
        );
        assert!(request_pin(&headers).is_err());
    }

    fn spec() -> ExecutionSpec {
        ExecutionSpec {
            id: "execution-one".into(),
            workspace_id: "workspace".into(),
            room_id: "room".into(),
            session_id: "session".into(),
            provider: "OpenAI".into(),
            model: "test".into(),
            source: Source::Scheduled,
            source_id: "task:100".into(),
            parent_run_id: None,
            parent_execution_id: None,
            call_id: None,
            summary: "任务".into(),
            deadline_ms: 100,
        }
    }
    #[tokio::test]
    async fn durable_id_rejects_replay_and_drop_keeps_unknown_outcome() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        crate::initialize_session_schema(&open(&path).unwrap()).unwrap();
        let guard =
            ExecutionService::begin(&path, spec(), Arc::new(ChatTurnCancellation::new())).unwrap();
        assert!(
            ExecutionService::begin(&path, spec(), Arc::new(ChatTurnCancellation::new())).is_err()
        );
        assert!(ExecutionService::cancel(&path, "other", "execution-one").is_err());
        drop(guard);
        let connection = open(&path).unwrap();
        let state: String = connection
            .query_row("SELECT state FROM execution_index", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "outcome_unknown");
        assert!(
            ExecutionService::begin(&path, spec(), Arc::new(ChatTurnCancellation::new())).is_err()
        );
    }
    #[tokio::test]
    async fn cancelling_child_does_not_cancel_parent() {
        let parent = Arc::new(ChatTurnCancellation::new());
        let child = Arc::new(ChatTurnCancellation::new());
        child.request();
        assert!(TurnRunner::child(
            parent.clone(),
            child,
            RootExecutionBudget::establish(60000),
            std::future::pending::<Result<(), String>>()
        )
        .await
        .is_err());
        assert!(!parent.is_requested());
        assert!(require_capability("devin", Source::SessionTool).is_err());
        assert!(require_capability("devin", Source::Scheduled).is_ok());
        assert!(require_capability("devin_cloud", Source::Scheduled).is_err());
    }

    #[tokio::test]
    async fn spawned_child_finds_durable_parent_without_inherited_task_local() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("session.sqlite3");
        crate::initialize_session_schema(&open(&path).unwrap()).unwrap();
        let workspace = crate::workspace_identity(temp.path());
        crate::create_chat_runtime_run_sqlite(
            &path,
            "real-run",
            "claim",
            &workspace,
            Some("session"),
            "room",
            "turn",
        )
        .unwrap();
        let mut root = spec();
        root.workspace_id = workspace.clone();
        let guard =
            ExecutionService::begin(&path, root, Arc::new(ChatTurnCancellation::new())).unwrap();
        CURRENT
            .scope(guard.current.clone(), async {
                ExecutionService::bind_chat(
                    &path,
                    &workspace,
                    "real-run",
                    Arc::new(ChatTurnCancellation::new()),
                )
                .unwrap();
                let path = path.clone();
                let workspace = workspace.clone();
                tokio::spawn(async move {
                    assert!(CURRENT.try_with(|_| ()).is_err());
                    let mut child = spec();
                    child.id = "child".into();
                    child.workspace_id = workspace;
                    child.source = Source::SessionTool;
                    child.parent_run_id = Some("real-run".into());
                    child.call_id = Some("call".into());
                    let _child = ExecutionService::begin(
                        &path,
                        child,
                        Arc::new(ChatTurnCancellation::new()),
                    )
                    .unwrap();
                    let encoded: String = open(&path)
                        .unwrap()
                        .query_row(
                            "SELECT spec_json FROM execution_index WHERE id='child'",
                            [],
                            |r| r.get(0),
                        )
                        .unwrap();
                    let saved: ExecutionSpec = serde_json::from_str(&encoded).unwrap();
                    assert_eq!(saved.parent_execution_id.as_deref(), Some("execution-one"));
                })
                .await
                .unwrap();
            })
            .await;
        drop(guard);
    }

    #[tokio::test]
    async fn parent_cancel_wakes_children_without_consuming_main_waiter_notification() {
        let parent = Arc::new(ChatTurnCancellation::new());
        let first = Arc::new(ChatTurnCancellation::new());
        let second = Arc::new(ChatTurnCancellation::new());
        let budget = RootExecutionBudget::establish(60000);
        let a = TurnRunner::child(
            parent.clone(),
            first.clone(),
            budget.clone(),
            std::future::pending::<Result<(), String>>(),
        );
        let b = TurnRunner::child(
            parent.clone(),
            second.clone(),
            budget,
            std::future::pending::<Result<(), String>>(),
        );
        let stop = async {
            tokio::task::yield_now().await;
            parent.request();
        };
        let (a, b, ()) = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::join!(a, b, stop)
        })
        .await
        .unwrap();
        assert!(a.is_err() && b.is_err());
        assert!(first.is_requested() && second.is_requested());
        // 原聊天室等待器在请求前未被轮询，保留通知也必须可用。
        tokio::time::timeout(std::time::Duration::from_secs(1), parent.cancelled())
            .await
            .unwrap();
    }
}
