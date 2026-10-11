//! 工具分发等待的收尾。异步等待被取消也必须结束 dispatched 投影；执行事实仍由监督器追加。
use std::path::PathBuf;
use crate::root_execution_budget::RootExecutionBudget;

/// 旧 ACP 已结束却误记为待审批的 DSH 调用不支持跨回合续接。
/// 仅在正常续发的精确旧身份上结账；不改旧 unknown、不派发工具或创建远端会话。
pub(crate) fn reconcile_drained_acp_approval(
    connection: &rusqlite::Connection, run_id: &str, workspace_id: &str,
    room_id: &str, agent_id: &str, turn_id: &str, source: &str,
) -> Result<(), String> {
    let tables: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('tool_calls','runtime_runs','runtime_run_events','devin_acp_attempts')", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if tables != 4 { return Ok(()); }
    let confirmed: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM devin_acp_attempts a JOIN runtime_runs r ON r.id=?1
        WHERE a.scope_json=?2 AND a.state='unknown' AND a.protocol_stop='end_turn' AND a.process_drained=1
        AND json_extract(a.scope_json,'$.run_id')=r.id AND COALESCE(json_extract(a.scope_json,'$.lane'),'')=''
        AND json_extract(a.scope_json,'$.workspace_id')=?3 AND json_extract(a.scope_json,'$.room_id')=?4
        AND json_extract(a.scope_json,'$.agent_id')=?5 AND json_extract(a.scope_json,'$.turn_id')=?6
        AND r.kind='chat_turn' AND r.workspace_id=?3 AND r.chat_room_id=?4 AND r.legacy_turn_id=?6
        AND r.state IN ('completed','failed','cancelled','interrupted','timed_out'))",
        rusqlite::params![run_id,source,workspace_id,room_id,agent_id,turn_id], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !confirmed { return Ok(()); }
    let mut query = connection.prepare("SELECT tool_call_id,tool_name FROM tool_calls
        WHERE run_id=?1 AND source_request_key=?2 AND status='awaiting_approval'")
        .map_err(|error| error.to_string())?;
    let rows = query.query_map(rusqlite::params![run_id,source], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))
        .map_err(|error| error.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|error| error.to_string())?;
    for (tool,name) in rows {
        if !name.starts_with(crate::dsh_execution::PREFIX) { continue; }
        let evidence = serde_json::json!({"tool_call_id":tool,"previous_status":"awaiting_approval",
            "settled_status":"failed","executed":false,
            "reason":"acp_approval_not_resumable_terminal_parent_drained"});
        connection.execute("INSERT INTO runtime_run_events(run_id,event_type,payload_json,created_at) VALUES(?1,'tool.approval_not_resumable',?2,?3)",
            rusqlite::params![run_id,evidence.to_string(),crate::unix_timestamp_millis() as i64]).map_err(|error| error.to_string())?;
        connection.execute("UPDATE tool_calls SET status='failed',updated_at_unix_ms=?1
            WHERE tool_call_id=?2 AND run_id=?3 AND source_request_key=?4 AND status='awaiting_approval'",
            rusqlite::params![crate::unix_timestamp_millis() as i64,tool,run_id,source]).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// HTTP 消费者断连后，CU 控制器已收尾且输入全部释放时，追加失败结账。
/// 原步骤和 unknown 事实保留在事件里；不能据进程退出推断输入已释放。
pub(crate) fn reconcile_drained_acp_cu(
    connection: &rusqlite::Connection, run_id: &str, workspace_id: &str,
    room_id: &str, agent_id: &str, turn_id: &str, source: &str,
) -> Result<(), String> {
    let tables: i64 = connection.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('tool_calls','computer_use_runs','computer_use_steps','runtime_run_events')", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if tables != 4 { return Ok(()); }
    let mut pending = connection.prepare("SELECT scope_json FROM devin_acp_attempts WHERE process_drained=0")
        .map_err(|error| error.to_string())?;
    for raw in pending.query_map([], |row| row.get::<_, String>(0)).map_err(|error| error.to_string())? {
        let other: serde_json::Value = serde_json::from_str(&raw.map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
        if other["run_id"].as_str() == Some(run_id) { return Ok(()); }
    }
    let mut query = connection.prepare("SELECT t.tool_call_id,c.call_id FROM tool_calls t JOIN computer_use_runs c ON c.provider_tool_call_id=t.tool_call_id
        WHERE t.run_id=?1 AND t.source_request_key=?2 AND t.tool_name='computer_use_perform'
        AND t.status='cancelled_outcome_unknown' AND c.workspace_id=?3 AND c.chat_room_id=?4 AND c.session_id=?5 AND c.turn_id=?6
        AND c.state='cancelled' AND c.terminal_result_json IS NOT NULL")
        .map_err(|error| error.to_string())?;
    let rows = query.query_map(rusqlite::params![run_id,source,workspace_id,room_id,agent_id,turn_id],
        |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?)))
        .map_err(|error| error.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|error| error.to_string())?;
    for (tool, cu_run) in rows {
        let uncertain: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM computer_use_steps WHERE run_id=?1 AND
            (completed_at_ms IS NULL OR NOT COALESCE((input_delivery='sent' AND input_release_status IN ('released','not_needed'))
                OR (input_delivery='not_sent' AND input_release_status='not_needed'),0)))", [&cu_run], |row| row.get(0))
            .map_err(|error| error.to_string())?;
        if uncertain { continue; }
        let evidence = serde_json::json!({"tool_call_id":tool,"cu_run_id":cu_run,
            "previous_status":"cancelled_outcome_unknown","settled_status":"failed",
            "reason":"controller_cancelled_inputs_released_and_model_processes_drained"});
        connection.execute("INSERT INTO runtime_run_events(run_id,event_type,payload_json,created_at) VALUES(?1,'tool.worker_settled',?2,?3)",
            rusqlite::params![run_id,evidence.to_string(),crate::unix_timestamp_millis() as i64]).map_err(|error| error.to_string())?;
        connection.execute("UPDATE tool_calls SET status='failed',updated_at_unix_ms=?1 WHERE tool_call_id=?2 AND status='cancelled_outcome_unknown'",
            rusqlite::params![crate::unix_timestamp_millis() as i64,tool]).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) struct ToolDispatchSettlement {
    path: PathBuf,
    tool_call_id: String,
    run_id: Option<String>,
    tool_name: String,
    arguments_digest: String,
    root_budget: Option<RootExecutionBudget>,
    settled: bool,
}
impl ToolDispatchSettlement {
    pub(crate) fn is_settled(&self) -> bool { self.settled }
    /// 先持久化审批等待，再暴露审批事件；原派发不得覆盖随后审批领取/执行的投影。
    pub(crate) fn handoff_approval(&mut self) -> Result<(), String> { self.finish("awaiting_approval") }
    /// 首次登记成功才返回收尾所有权；失败者不能执行，也不能收尾另一调用的记录。
    pub(crate) fn admit(path: PathBuf, tool_call_id: String, run_id: Option<String>,
        tool_name: String, arguments_digest: String, root_budget: Option<RootExecutionBudget>) -> Result<Self, String> {
        Self::admit_with_source(path, tool_call_id, run_id, tool_name, arguments_digest, root_budget, None)
    }
    pub(crate) fn admit_with_source(path: PathBuf, tool_call_id: String, run_id: Option<String>,
        tool_name: String, arguments_digest: String, root_budget: Option<RootExecutionBudget>,
        source: Option<&crate::tool_invocation_identity::ModelToolIdentity>) -> Result<Self, String> {
        if source.is_some_and(|source| source.execution_id != tool_call_id) {
            return Err("工具执行编号与冻结请求来源不匹配，未登记".into());
        }
        let store = crate::computer_use_store::ComputerUseRunStore::open(&path).map_err(|error| error.to_string())?;
        let registration = match source {
            Some(source) => store.register_model_tool_call(source, run_id.as_deref(), &tool_name, &arguments_digest),
            None => store.register_tool_call(&tool_call_id, run_id.as_deref(), None, &tool_name, &arguments_digest, "dispatched"),
        };
        if let Err(error) = registration {
            if let Ok(Some(existing)) = store.tool_call_record(&tool_call_id) {
                return Err(format!("工具调用标识已登记，原状态={}；本次未执行，也未更改原记录", existing.status));
            }
            return Err(format!("工具登记失败，未执行：{error}"));
        }
        Ok(Self { path, tool_call_id, run_id, tool_name, arguments_digest, root_budget, settled: false })
    }
    pub(crate) fn finish(&mut self, status: &str) -> Result<(), String> {
        let status = if status == "failed" && self.root_budget.as_ref().is_some_and(RootExecutionBudget::is_expired) {
            "timed_out_outcome_unknown"
        } else { status };
        self.record(status)?;
        self.settled = true;
        Ok(())
    }
    fn record(&self, status: &str) -> Result<(), String> {
        let connection = crate::open_session_connection(&self.path).map_err(|error| error.to_string())?;
        // 超时、取消和迟到结果不可覆盖已经结束的投影；真实迟到输入/进程事实走各自追加日志。
        let changed = connection.execute("UPDATE tool_calls SET status=?1,updated_at_unix_ms=?2 WHERE tool_call_id=?3 AND run_id IS ?4 AND tool_name=?5 AND arguments_digest=?6 AND status='dispatched'",
            rusqlite::params![status, crate::unix_timestamp_millis() as i64, self.tool_call_id, self.run_id, self.tool_name, self.arguments_digest])
            .map_err(|error| error.to_string())?;
        if changed != 1 { return Err("工具收尾归属或状态已改变，未覆盖原记录".to_string()); }
        Ok(())
    }
}
impl Drop for ToolDispatchSettlement {
    fn drop(&mut self) {
        if self.settled { return; }
        let status = if self.root_budget.as_ref().is_some_and(RootExecutionBudget::is_expired) {
            "timed_out_outcome_unknown"
        } else { "cancelled_outcome_unknown" };
        if let Err(error) = self.record(status) {
            tracing::error!(tool_call_id=%self.tool_call_id, "工具分发收尾未能持久化，结果待确认：{error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nonresumable_approval_requires_exact_terminal_drained_acp_scope() {
        let connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE tool_calls(tool_call_id TEXT,run_id TEXT,source_request_key TEXT,tool_name TEXT,status TEXT,updated_at_unix_ms INTEGER);
            CREATE TABLE runtime_runs(id TEXT,kind TEXT,workspace_id TEXT,chat_room_id TEXT,legacy_turn_id TEXT,state TEXT);
            CREATE TABLE runtime_run_events(run_id TEXT,event_type TEXT,payload_json TEXT,created_at INTEGER);
            CREATE TABLE devin_acp_attempts(scope_json TEXT,state TEXT,protocol_stop TEXT,process_drained INTEGER);
            INSERT INTO runtime_runs VALUES('parent','chat_turn','workspace','room','turn','running');").unwrap();
        // 空 lane 由真实协议省略；没有排空、没有终态或身份不符时都不能结账。
        let source = serde_json::json!({"run_id":"parent","workspace_id":"workspace","room_id":"room","agent_id":"agent","turn_id":"turn"}).to_string();
        connection.execute("INSERT INTO devin_acp_attempts VALUES(?1,'unknown','end_turn',0)",[&source]).unwrap();
        for (id,name,status) in [("pending","dsh__net_fetch","awaiting_approval"),("executing","dsh__net_fetch","approval_running"),("other","ordinary_tool","awaiting_approval")] {
            connection.execute("INSERT INTO tool_calls VALUES(?1,'parent',?2,?3,?4,1)",rusqlite::params![id,source,name,status]).unwrap();
        }
        let pending = || connection.query_row("SELECT status FROM tool_calls WHERE tool_call_id='pending'",[],|row|row.get::<_,String>(0)).unwrap();
        reconcile_drained_acp_approval(&connection,"parent","workspace","room","agent","turn",&source).unwrap();
        assert_eq!(pending(),"awaiting_approval");
        connection.execute("UPDATE runtime_runs SET state='completed'",[]).unwrap();
        reconcile_drained_acp_approval(&connection,"parent","workspace","room","agent","turn",&source).unwrap();
        assert_eq!(pending(),"awaiting_approval");
        connection.execute("UPDATE devin_acp_attempts SET process_drained=1",[]).unwrap();
        reconcile_drained_acp_approval(&connection,"parent","workspace","other-room","agent","turn",&source).unwrap();
        assert_eq!(pending(),"awaiting_approval");
        connection.execute("UPDATE devin_acp_attempts SET protocol_stop=NULL",[]).unwrap();
        reconcile_drained_acp_approval(&connection,"parent","workspace","room","agent","turn",&source).unwrap();
        assert_eq!(pending(),"awaiting_approval");
        connection.execute("UPDATE devin_acp_attempts SET protocol_stop='end_turn'",[]).unwrap();
        reconcile_drained_acp_approval(&connection,"parent","workspace","room","agent","turn",&source).unwrap();
        reconcile_drained_acp_approval(&connection,"parent","workspace","room","agent","turn",&source).unwrap();
        assert_eq!(pending(),"failed");
        let unchanged: i64 = connection.query_row("SELECT COUNT(*) FROM tool_calls WHERE (tool_call_id='executing' AND status='approval_running') OR (tool_call_id='other' AND status='awaiting_approval')",[],|row|row.get(0)).unwrap();
        assert_eq!(unchanged,2);
        assert_eq!(connection.query_row("SELECT COUNT(*) FROM runtime_run_events WHERE event_type='tool.approval_not_resumable'",[],|row|row.get::<_,i64>(0)).unwrap(),1);
        assert_eq!(connection.query_row("SELECT state FROM devin_acp_attempts",[],|row|row.get::<_,String>(0)).unwrap(),"unknown");
    }

    #[test]
    fn cancelled_cu_reconciliation_requires_release_exact_scope_and_drained_workers() {
        let mut connection = rusqlite::Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE tool_calls(tool_call_id TEXT,run_id TEXT,source_request_key TEXT,tool_name TEXT,status TEXT,updated_at_unix_ms INTEGER);
            CREATE TABLE computer_use_runs(call_id TEXT,provider_tool_call_id TEXT,workspace_id TEXT,chat_room_id TEXT,session_id TEXT,turn_id TEXT,state TEXT,terminal_result_json TEXT);
            CREATE TABLE computer_use_steps(run_id TEXT,completed_at_ms INTEGER,input_delivery TEXT,input_release_status TEXT);
            CREATE TABLE runtime_run_events(run_id TEXT,event_type TEXT,payload_json TEXT,created_at INTEGER);
            CREATE TABLE devin_acp_attempts(scope_json TEXT,process_drained INTEGER);
            INSERT INTO tool_calls VALUES('tool','parent','source','computer_use_perform','cancelled_outcome_unknown',1);
            INSERT INTO computer_use_runs VALUES('cu','tool','workspace','room','agent','turn','cancelled','{}');
            INSERT INTO computer_use_steps VALUES('cu',2,'sent',NULL);
            INSERT INTO devin_acp_attempts VALUES('{\"run_id\":\"parent\",\"lane\":\"internal\"}',0);").unwrap();
        let status = |c: &rusqlite::Connection| c.query_row("SELECT status FROM tool_calls",[],|row|row.get::<_,String>(0)).unwrap();
        for release in [None,Some("unknown"),Some("released")] {
            connection.execute("UPDATE computer_use_steps SET input_release_status=?1",[release]).unwrap();
            let tx=connection.transaction().unwrap();
            reconcile_drained_acp_cu(&tx,"parent","workspace","room","agent","turn","source").unwrap();
            tx.commit().unwrap();
            assert_eq!(status(&connection),"cancelled_outcome_unknown");
        }
        connection.execute("UPDATE devin_acp_attempts SET process_drained=1",[]).unwrap();
        for release in [None,Some("unknown")] {
            connection.execute("UPDATE computer_use_steps SET input_release_status=?1",[release]).unwrap();
            reconcile_drained_acp_cu(&connection,"parent","workspace","room","agent","turn","source").unwrap();
            assert_eq!(status(&connection),"cancelled_outcome_unknown");
        }
        connection.execute("UPDATE computer_use_steps SET input_release_status='released'",[]).unwrap();
        reconcile_drained_acp_cu(&connection,"parent","workspace","other-room","agent","turn","source").unwrap();
        assert_eq!(status(&connection),"cancelled_outcome_unknown");
        let tx=connection.transaction().unwrap();
        reconcile_drained_acp_cu(&tx,"parent","workspace","room","agent","turn","source").unwrap();
        tx.commit().unwrap();
        assert_eq!(status(&connection),"failed");
        let audit:String=connection.query_row("SELECT payload_json FROM runtime_run_events",[],|row|row.get(0)).unwrap();
        assert!(audit.contains("cancelled_outcome_unknown"));
        assert_eq!(connection.query_row("SELECT input_release_status FROM computer_use_steps",[],|row|row.get::<_,String>(0)).unwrap(),"released");
    }

    /// 走真实通用工具入口；登记碰撞和库故障都发生在 write_file 副作用之前。
    #[tokio::test]
    async fn refused_registration_never_reaches_file_write_or_changes_original_owner() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("session.sqlite3");
        let output = root.path().join("must-not-exist.txt");
        let store = crate::computer_use_store::ComputerUseRunStore::open(&db).unwrap();
        let source = crate::tool_invocation_identity::ModelToolIdentity::from_source(Some("new-run"), "request-one", "collision").unwrap();
        store.register_tool_call(&source.execution_id, Some("original-run"), None, "read_file", "original", "completed").unwrap();
        let mut parent = crate::FrozenParentContext::new("registry-test",
            &crate::workspace_identity(root.path()), None, None, None, Some("new-run")).unwrap();
        parent.runtime_db_path = Some(db.clone());
        let input = serde_json::json!({"path": output, "content": "must never be written"});
        let result = crate::tool_invocation_identity::scope("request-one".into(),
            crate::run_model_tool_dispatch_for_session_with_identity("write_file", &input,
            None, Some("collision"), None, None, Some(&parent), None)).await;
        assert!(result.is_err());
        assert!(!output.exists());
        let original = store.tool_call_record(&source.execution_id).unwrap().unwrap();
        assert_eq!(original.run_id.as_deref(), Some("original-run"));
        assert_eq!(original.status, "completed");
        assert_eq!(original.tool_name, "read_file");
        parent.runtime_db_path = Some(root.path().to_path_buf()); // 目录不是可写的 SQLite 文件。
        assert!(crate::tool_invocation_identity::scope("request-two".into(),
            crate::run_model_tool_dispatch_for_session_with_identity("write_file", &input,
            None, Some("new-call"), None, None, Some(&parent), None)).await.is_err());
        assert!(!output.exists());
    }

    #[test]
    fn settlement_does_not_overwrite_changed_ownership() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("session.sqlite3");
        let mut guard = ToolDispatchSettlement::admit(db.clone(), "id".into(), Some("run-1".into()),
            "write_file".into(), "digest".into(), None).unwrap();
        let connection = crate::open_session_connection(&db).unwrap();
        connection.execute("UPDATE tool_calls SET run_id='foreign-run' WHERE tool_call_id='id'", []).unwrap();
        assert!(guard.finish("completed").is_err());
        drop(guard);
        let row = connection.query_row("SELECT run_id,status FROM tool_calls WHERE tool_call_id='id'", [],
            |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).unwrap();
        assert_eq!(row, ("foreign-run".into(), "dispatched".into()));
    }
}
