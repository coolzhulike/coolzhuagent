//! ACP 发送前台账。未知执行保留锁，不能因重启或新请求编号自动重发。
use super::protocol::{ExecutionScope, ModelSelection};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(super) struct Journal {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub(super) struct Binding {
    pub remote_session_id: Option<String>,
    pub cwd: String,
    pub cli_identity: String,
    pub context_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AttemptStatus {
    pub state: String,
    pub cancel_requested: bool,
    pub protocol_stop: Option<String>,
    pub process_drained: bool,
}

fn fail(message: &str) -> String {
    message.into()
}

impl Journal {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        let journal = Self {
            path: path.to_owned(),
        };
        journal.connection()?.execute_batch(
            "CREATE TABLE IF NOT EXISTS devin_acp_bindings (
                workspace_id TEXT NOT NULL, room_id TEXT NOT NULL, agent_id TEXT NOT NULL,
                cwd TEXT NOT NULL, cli_identity TEXT NOT NULL, context_digest TEXT NOT NULL,
                remote_session_id TEXT, owner_epoch INTEGER NOT NULL, generation INTEGER NOT NULL,
                locked_attempt TEXT,
                PRIMARY KEY(workspace_id,room_id,agent_id));
             CREATE TABLE IF NOT EXISTS devin_acp_attempts (
                attempt_id TEXT PRIMARY KEY, scope_json TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('prepared','submitted','terminal','unknown','not_sent')),
                cancel_requested INTEGER NOT NULL DEFAULT 0,
                protocol_stop TEXT, process_drained INTEGER NOT NULL DEFAULT 0,
                model_json TEXT);"
        ).map_err(|_| fail("ACP 台账初始化失败。"))?;
        // 兼容本地早期台账样本；只加可空回执列，不重写历史状态或解除执行锁。
        let mut connection = journal.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 台账升级锁失败。"))?;
        let has_model = tx
            .prepare("PRAGMA table_info(devin_acp_attempts)")
            .map_err(|_| fail("ACP 台账结构读取失败。"))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|_| fail("ACP 台账列读取失败。"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| fail("ACP 台账列无效。"))?
            .iter()
            .any(|name| name == "model_json");
        if !has_model {
            tx.execute_batch("ALTER TABLE devin_acp_attempts ADD COLUMN model_json TEXT;")
                .map_err(|_| fail("ACP 模型回执列升级失败。"))?;
        }
        tx.commit().map_err(|_| fail("ACP 台账升级提交失败。"))?;
        Ok(journal)
    }

    fn connection(&self) -> Result<Connection, String> {
        let connection = crate::open_session_connection(&self.path)
            .map_err(|_| fail("ACP 台账不可用或版本超前。"))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| fail("ACP 台账锁配置失败。"))?;
        Ok(connection)
    }

    /// epoch/generation 由本地事务分配；不是 CLI 通知中的自报身份。
    /// 上一连接终止并有确定终态后才能取得下一代。同文新意图仍需独立 attempt。
    pub(super) fn claim(
        &self,
        mut scope: ExecutionScope,
        binding: &Binding,
    ) -> Result<ExecutionScope, String> {
        scope.owner_epoch = 1;
        scope.generation = 1;
        if !scope.accepts(&scope)
            || !Path::new(&binding.cwd).is_absolute()
            || binding.cli_identity.is_empty()
            || binding.context_digest.is_empty()
        {
            return Err(fail("ACP 接纳身份、目录或冻结版本不完整。"));
        }
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 台账正在被占用。"))?;
        let old = tx
            .query_row(
                "SELECT cwd,cli_identity,context_digest,owner_epoch,generation,locked_attempt
             FROM devin_acp_bindings WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3",
                params![scope.workspace_id, scope.room_id, scope.agent_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, u64>(3)?,
                        row.get::<_, u64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| fail("ACP 会话绑定读取失败。"))?;
        if let Some((cwd, cli, context, epoch, generation, locked)) = old {
            if locked.is_some() {
                return Err(fail("ACP 上次执行仍在途或结果未知，禁止自动重发。"));
            }
            if (cwd, cli, context)
                != (
                    binding.cwd.clone(),
                    binding.cli_identity.clone(),
                    binding.context_digest.clone(),
                )
            {
                return Err(fail(
                    "ACP 工作目录、CLI 或上下文版本已变化，需要显式建立新绑定。",
                ));
            }
            scope.owner_epoch = epoch
                .checked_add(1)
                .filter(|v| *v <= i64::MAX as u64)
                .ok_or_else(|| fail("ACP 所有权代际已耗尽。"))?;
            scope.generation = generation
                .checked_add(1)
                .filter(|v| *v <= i64::MAX as u64)
                .ok_or_else(|| fail("ACP 连接代际已耗尽。"))?;
        }
        let encoded = serde_json::to_string(&scope).map_err(|_| fail("ACP 身份编码失败。"))?;
        tx.execute(
            "INSERT INTO devin_acp_attempts(attempt_id,scope_json,state) VALUES(?1,?2,'prepared')",
            params![scope.attempt_id, encoded],
        )
        .map_err(|_| fail("ACP attempt 已存在，不能重复提交。"))?;
        tx.execute("INSERT INTO devin_acp_bindings
            (workspace_id,room_id,agent_id,cwd,cli_identity,context_digest,remote_session_id,owner_epoch,generation,locked_attempt)
            VALUES(?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9)
            ON CONFLICT(workspace_id,room_id,agent_id) DO UPDATE SET
            owner_epoch=excluded.owner_epoch,generation=excluded.generation,locked_attempt=excluded.locked_attempt",
            params![scope.workspace_id,scope.room_id,scope.agent_id,binding.cwd,binding.cli_identity,
                binding.context_digest,scope.owner_epoch,scope.generation,scope.attempt_id])
            .map_err(|_| fail("ACP 会话锁保存失败。"))?;
        tx.commit()
            .map_err(|_| fail("ACP 接纳提交未确认，不能发送任务。"))?;
        Ok(scope)
    }

    fn check(connection: &Connection, scope: &ExecutionScope) -> Result<(), String> {
        if !scope.accepts(scope) {
            return Err(fail("ACP 身份不完整。"));
        }
        let encoded = serde_json::to_string(scope).map_err(|_| fail("ACP 身份编码失败。"))?;
        let live: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM devin_acp_bindings b
            JOIN devin_acp_attempts a ON a.attempt_id=b.locked_attempt
            WHERE b.workspace_id=?1 AND b.room_id=?2 AND b.agent_id=?3
              AND b.owner_epoch=?4 AND b.generation=?5 AND b.locked_attempt=?6 AND a.scope_json=?7)",
            params![scope.workspace_id,scope.room_id,scope.agent_id,scope.owner_epoch,scope.generation,scope.attempt_id,encoded],
            |row| row.get(0)).map_err(|_| fail("ACP 所有权核对失败。"))?;
        if live {
            Ok(())
        } else {
            Err(fail("ACP 执行所有权或连接代际已失效。"))
        }
    }

    pub(super) fn binding(&self, scope: &ExecutionScope) -> Result<Binding, String> {
        let connection = self.connection()?;
        Self::check(&connection, scope)?;
        connection
            .query_row(
                "SELECT remote_session_id,cwd,cli_identity,context_digest FROM devin_acp_bindings
            WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3",
                params![scope.workspace_id, scope.room_id, scope.agent_id],
                |row| {
                    Ok(Binding {
                        remote_session_id: row.get(0)?,
                        cwd: row.get(1)?,
                        cli_identity: row.get(2)?,
                        context_digest: row.get(3)?,
                    })
                },
            )
            .map_err(|_| fail("ACP 绑定读取失败。"))
    }

    pub(super) fn save_remote(&self, scope: &ExecutionScope, remote: &str) -> Result<(), String> {
        if remote.trim().is_empty() || remote.len() > 4096 {
            return Err(fail("ACP 远端会话 ID 无效。"));
        }
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 台账锁失败。"))?;
        Self::check(&tx, scope)?;
        let old = tx
            .query_row(
                "SELECT remote_session_id FROM devin_acp_bindings WHERE locked_attempt=?1",
                [&scope.attempt_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(|_| fail("ACP 旧绑定读取失败。"))?;
        if old.as_deref().is_some_and(|id| id != remote) {
            return Err(fail("ACP 会话绑定不能静默替换。"));
        }
        let used_elsewhere: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM devin_acp_bindings WHERE remote_session_id=?1
            AND NOT(workspace_id=?2 AND room_id=?3 AND agent_id=?4))",
                params![remote, scope.workspace_id, scope.room_id, scope.agent_id],
                |row| row.get(0),
            )
            .map_err(|_| fail("ACP 远端会话唯一性核对失败。"))?;
        if used_elsewhere {
            return Err(fail(
                "ACP 远端会话已归属另一工程、聊天室或 Agent，不能复用。",
            ));
        }
        tx.execute(
            "UPDATE devin_acp_bindings SET remote_session_id=?1 WHERE locked_attempt=?2",
            params![remote, scope.attempt_id],
        )
        .map_err(|_| fail("ACP 远端绑定未保存，不能发送任务。"))?;
        tx.commit().map_err(|_| fail("ACP 远端绑定提交失败。"))
    }

    pub(super) fn save_model(
        &self,
        scope: &ExecutionScope,
        model: &ModelSelection,
    ) -> Result<(), String> {
        if model.requested.trim().is_empty()
            || model.requested.len() > 4096
            || model.effective.as_deref() != Some(model.requested.as_str())
            || model.resolved_model.is_some()
        {
            return Err(fail("ACP 模型未被完整配置回执确认，不能登记。"));
        }
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 模型台账锁失败。"))?;
        Self::check(&tx, scope)?;
        let status = Self::status_on(&tx, &scope.attempt_id)?;
        if status.state != "prepared" || status.cancel_requested {
            return Err(fail("ACP 运行中或取消后不能变更冻结模型。"));
        }
        let encoded = serde_json::to_string(model).map_err(|_| fail("ACP 模型编码失败。"))?;
        let changed=tx.execute("UPDATE devin_acp_attempts SET model_json=?1 WHERE attempt_id=?2 AND model_json IS NULL",params![encoded,scope.attempt_id])
            .map_err(|_|fail("ACP 模型回执保存失败。"))?;
        if changed != 1 {
            return Err(fail("ACP 本轮模型已经冻结，不能重复替换。"));
        }
        tx.commit()
            .map_err(|_| fail("ACP 模型回执提交失败，不能发送任务。"))
    }

    pub(super) fn model(&self, scope: &ExecutionScope) -> Result<Option<ModelSelection>, String> {
        self.status(scope)?;
        let encoded = self
            .connection()?
            .query_row(
                "SELECT model_json FROM devin_acp_attempts WHERE attempt_id=?1",
                [&scope.attempt_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .map_err(|_| fail("ACP 模型回执读取失败。"))?;
        encoded
            .map(|value| serde_json::from_str(&value).map_err(|_| fail("ACP 模型回执损坏。")))
            .transpose()
    }

    /// 每次状态转换都核对完整身份，并由同一事务串行化取消与发送。
    pub(super) fn transition(
        &self,
        scope: &ExecutionScope,
        from: &[&str],
        state: &str,
        stop: Option<&str>,
        require_uncancelled: bool,
    ) -> Result<(), String> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 台账锁失败。"))?;
        Self::check(&tx, scope)?;
        let current = Self::status_on(&tx, &scope.attempt_id)?;
        let edge = matches!(
            (current.state.as_str(), state),
            ("prepared", "submitted" | "not_sent" | "unknown")
                | ("submitted", "terminal" | "unknown")
        );
        let valid_stop = if state == "terminal" {
            stop.is_some_and(|value| {
                matches!(
                    value,
                    "end_turn" | "max_tokens" | "max_turn_requests" | "refusal" | "cancelled"
                )
            })
        } else {
            stop.is_none()
        };
        if !edge || !valid_stop {
            return Err(fail("ACP 状态转换或终态证据无效。"));
        }
        if !from.contains(&current.state.as_str())
            || (require_uncancelled && current.cancel_requested)
        {
            return Err(fail("ACP 状态已变化或已取消，不能提交新的请求。"));
        }
        tx.execute(
            "UPDATE devin_acp_attempts SET state=?1,protocol_stop=?2 WHERE attempt_id=?3",
            params![state, stop, scope.attempt_id],
        )
        .map_err(|_| fail("ACP 状态保存失败。"))?;
        tx.commit()
            .map_err(|_| fail("ACP 状态提交失败，不能自动重试。"))
    }

    pub(super) fn request_cancel(&self, scope: &ExecutionScope) -> Result<(), String> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 台账锁失败。"))?;
        Self::check(&tx, scope)?;
        tx.execute(
            "UPDATE devin_acp_attempts SET cancel_requested=1 WHERE attempt_id=?1",
            [&scope.attempt_id],
        )
        .map_err(|_| fail("ACP 取消记录保存失败。"))?;
        tx.commit().map_err(|_| fail("ACP 取消提交未确认。"))
    }

    pub(super) fn can_dispatch(&self, scope: &ExecutionScope) -> Result<(), String> {
        let connection = self.connection()?;
        Self::check(&connection, scope)?;
        let status = Self::status_on(&connection, &scope.attempt_id)?;
        if status.state == "submitted" && !status.cancel_requested && !status.process_drained {
            Ok(())
        } else {
            Err(fail("ACP 已取消、未提交或已结束，不能派发工具。"))
        }
    }

    /// 仅进程监督者在实际 wait/树终止之后调用。未知状态即使排空也不解除发送锁。
    pub(super) fn record_drained(&self, scope: &ExecutionScope) -> Result<(), String> {
        let mut connection = self.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 台账锁失败。"))?;
        Self::check(&tx, scope)?;
        let status = Self::status_on(&tx, &scope.attempt_id)?;
        let has_tools:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='tool_calls')",[],|row|row.get(0))
            .map_err(|_|fail("ACP 工具台账结构核对失败。"))?;
        let uncertain_tools = if has_tools {
            let source =
                serde_json::to_string(scope).map_err(|_| fail("ACP 工具身份编码失败。"))?;
            tx.query_row("SELECT EXISTS(SELECT 1 FROM tool_calls WHERE run_id=?1 AND source_request_key=?2 AND status NOT IN ('completed','failed'))",
                params![scope.run_id,source],|row|row.get::<_,bool>(0)).map_err(|_|fail("ACP 工具收尾核对失败。"))?
        } else {
            false
        };
        tx.execute(
            "UPDATE devin_acp_attempts SET process_drained=1 WHERE attempt_id=?1",
            [&scope.attempt_id],
        )
        .map_err(|_| fail("ACP 排空记录失败。"))?;
        if uncertain_tools {
            // CLI 排空不等于宿主 worker 已排空；迟到事实不能自动解锁未知写入。
            tx.execute(
                "UPDATE devin_acp_attempts SET state='unknown' WHERE attempt_id=?1",
                [&scope.attempt_id],
            )
            .map_err(|_| fail("ACP 工具未知记录失败。"))?;
        } else if matches!(status.state.as_str(), "terminal" | "not_sent") {
            tx.execute(
                "UPDATE devin_acp_bindings SET locked_attempt=NULL WHERE locked_attempt=?1",
                [&scope.attempt_id],
            )
            .map_err(|_| fail("ACP 会话锁释放失败。"))?;
        }
        tx.commit().map_err(|_| fail("ACP 排空提交失败。"))
    }

    fn status_on(connection: &Connection, attempt: &str) -> Result<AttemptStatus, String> {
        connection.query_row("SELECT state,cancel_requested,protocol_stop,process_drained FROM devin_acp_attempts WHERE attempt_id=?1",
            [attempt],|row| Ok(AttemptStatus{state:row.get(0)?,cancel_requested:row.get(1)?,protocol_stop:row.get(2)?,process_drained:row.get(3)?}))
            .map_err(|_| fail("ACP attempt 不存在。"))
    }
    pub(super) fn status(&self, scope: &ExecutionScope) -> Result<AttemptStatus, String> {
        let connection = self.connection()?;
        let encoded = serde_json::to_string(scope).map_err(|_| fail("ACP 身份编码失败。"))?;
        let matches:bool=connection.query_row("SELECT EXISTS(SELECT 1 FROM devin_acp_attempts WHERE attempt_id=?1 AND scope_json=?2)",
            params![scope.attempt_id,encoded],|r|r.get(0)).map_err(|_| fail("ACP 查询失败。"))?;
        if !matches {
            return Err(fail("ACP 状态查询身份不匹配。"));
        }
        Self::status_on(&connection, &scope.attempt_id)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    pub(crate) fn scope(attempt: &str) -> ExecutionScope {
        ExecutionScope {
            workspace_id: "w".into(),
            room_id: "r".into(),
            agent_id: "a".into(),
            run_id: "run".into(),
            turn_id: "turn".into(),
            attempt_id: attempt.into(),
            owner_epoch: 0,
            generation: 0,
        }
    }
    pub(crate) fn setup() -> (tempfile::TempDir, Journal, Binding) {
        let dir = tempfile::tempdir().unwrap();
        let journal = Journal::open(&dir.path().join("sessions.sqlite3")).unwrap();
        let binding = Binding {
            remote_session_id: None,
            cwd: dir.path().to_string_lossy().into(),
            cli_identity: "fixture-1".into(),
            context_digest: "context-1".into(),
        };
        (dir, journal, binding)
    }
    #[test]
    fn unknown_survives_reopen_and_draining_and_blocks_new_attempts() {
        let (_dir, journal, binding) = setup();
        let a = journal.claim(scope("a1"), &binding).unwrap();
        journal
            .transition(&a, &["prepared"], "submitted", None, true)
            .unwrap();
        journal
            .transition(&a, &["submitted"], "unknown", None, false)
            .unwrap();
        journal.record_drained(&a).unwrap();
        let reopened = Journal::open(&journal.path).unwrap();
        assert_eq!(reopened.status(&a).unwrap().state, "unknown");
        assert!(reopened.claim(scope("a2"), &binding).is_err());
        assert!(reopened.claim(scope("a1"), &binding).is_err());
    }
    #[test]
    fn terminal_and_process_drain_are_distinct_and_generation_is_allocated() {
        let (_dir, journal, binding) = setup();
        let a = journal.claim(scope("a1"), &binding).unwrap();
        journal.save_remote(&a, "remote-1").unwrap();
        journal
            .transition(&a, &["prepared"], "submitted", None, true)
            .unwrap();
        journal
            .transition(&a, &["submitted"], "terminal", Some("end_turn"), false)
            .unwrap();
        assert!(!journal.status(&a).unwrap().process_drained);
        assert!(journal.claim(scope("a2"), &binding).is_err());
        journal.record_drained(&a).unwrap();
        let b = journal.claim(scope("a2"), &binding).unwrap();
        assert_eq!((b.owner_epoch, b.generation), (2, 2));
        assert_eq!(
            journal.binding(&b).unwrap().remote_session_id.as_deref(),
            Some("remote-1")
        );
        assert!(journal.can_dispatch(&a).is_err());
        assert!(journal.request_cancel(&a).is_err());
    }
    #[test]
    fn remote_session_cannot_be_bound_to_two_rooms() {
        let (_dir, journal, binding) = setup();
        let a = journal.claim(scope("a1"), &binding).unwrap();
        journal.save_remote(&a, "remote-one").unwrap();
        let mut other = scope("a2");
        other.room_id = "other-room".into();
        let b = journal.claim(other, &binding).unwrap();
        assert!(journal.save_remote(&b, "remote-one").is_err());
        assert!(journal.binding(&b).unwrap().remote_session_id.is_none());
    }
    #[test]
    fn early_ledger_upgrade_preserves_unknown_lock_and_rejects_future_session_schema() {
        let (dir, journal, binding) = setup();
        let a = journal.claim(scope("early"), &binding).unwrap();
        journal
            .transition(&a, &["prepared"], "unknown", None, false)
            .unwrap();
        journal
            .connection()
            .unwrap()
            .execute_batch("ALTER TABLE devin_acp_attempts DROP COLUMN model_json;")
            .unwrap();
        let reopened = Journal::open(&journal.path).unwrap();
        assert_eq!(reopened.status(&a).unwrap().state, "unknown");
        assert!(reopened.claim(scope("new"), &binding).is_err());
        let future = dir.path().join("future.sqlite3");
        let connection = Connection::open(&future).unwrap();
        connection
            .execute_batch("PRAGMA user_version=999999;")
            .unwrap();
        drop(connection);
        assert!(Journal::open(&future).is_err());
        let connection = Connection::open(&future).unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE name='devin_acp_attempts'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
    }
    #[test]
    fn effective_model_receipt_is_frozen_and_survives_restart() {
        let (_dir, journal, binding) = setup();
        let scope = journal.claim(scope("model-attempt"), &binding).unwrap();
        assert!(
            journal
                .save_model(
                    &scope,
                    &ModelSelection {
                        requested: "alias".into(),
                        effective: None,
                        resolved_model: None
                    }
                )
                .is_err()
        );
        let model = ModelSelection {
            requested: "alias".into(),
            effective: Some("alias".into()),
            resolved_model: None,
        };
        journal.save_model(&scope, &model).unwrap();
        assert_eq!(
            Journal::open(&journal.path).unwrap().model(&scope).unwrap(),
            Some(model.clone())
        );
        assert!(journal.save_model(&scope, &model).is_err());
        journal
            .transition(&scope, &["prepared"], "submitted", None, true)
            .unwrap();
        assert!(journal.save_model(&scope, &model).is_err());
    }
    #[test]
    fn cli_drain_does_not_unlock_unsettled_host_tool() {
        let (_dir, journal, binding) = setup();
        let a = journal.claim(scope("a1"), &binding).unwrap();
        let connection = journal.connection().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE tool_calls(run_id TEXT,source_request_key TEXT,status TEXT);",
            )
            .unwrap();
        let source = serde_json::to_string(&a).unwrap();
        connection
            .execute(
                "INSERT INTO tool_calls VALUES(?1,?2,'dispatched')",
                params![a.run_id, source],
            )
            .unwrap();
        journal
            .transition(&a, &["prepared"], "submitted", None, true)
            .unwrap();
        journal
            .transition(&a, &["submitted"], "terminal", Some("end_turn"), false)
            .unwrap();
        journal.record_drained(&a).unwrap();
        let status = journal.status(&a).unwrap();
        assert!(status.process_drained);
        assert_eq!(status.state, "unknown");
        assert_eq!(status.protocol_stop.as_deref(), Some("end_turn"));
        assert!(journal.claim(scope("a2"), &binding).is_err());
        connection
            .execute("UPDATE tool_calls SET status='completed'", [])
            .unwrap();
        assert!(
            journal.claim(scope("a2"), &binding).is_err(),
            "迟到成功不自动解除未知执行锁"
        );
    }
    #[test]
    fn concurrent_claims_use_database_ownership_and_only_one_is_admitted() {
        let (_dir, journal, binding) = setup();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let tasks = (0..2)
            .map(|n| {
                let journal = journal.clone();
                let binding = binding.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    journal
                        .claim(scope(&format!("attempt-{n}")), &binding)
                        .is_ok()
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            tasks
                .into_iter()
                .map(|t| usize::from(t.join().unwrap()))
                .sum::<usize>(),
            1
        );
    }
    #[test]
    fn cancellation_and_identity_checks_precede_send_and_tools() {
        let (_dir, journal, binding) = setup();
        let a = journal.claim(scope("a1"), &binding).unwrap();
        let mut wrong = a.clone();
        wrong.room_id = "other".into();
        assert!(
            journal
                .transition(&wrong, &["prepared"], "submitted", None, true)
                .is_err()
        );
        journal.request_cancel(&a).unwrap();
        assert!(
            journal
                .transition(&a, &["prepared"], "submitted", None, true)
                .is_err()
        );
        assert!(journal.can_dispatch(&a).is_err());
        journal
            .transition(&a, &["prepared"], "not_sent", None, false)
            .unwrap();
        journal.record_drained(&a).unwrap();
        assert!(journal.claim(scope("a1"), &binding).is_err());
        assert!(journal.claim(scope("a2"), &binding).is_ok());
    }
}
