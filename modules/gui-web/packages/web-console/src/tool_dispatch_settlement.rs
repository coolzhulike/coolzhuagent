//! 工具分发等待的收尾。异步等待被取消也必须结束 dispatched 投影；执行事实仍由监督器追加。
use std::path::PathBuf;
use crate::root_execution_budget::RootExecutionBudget;

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
