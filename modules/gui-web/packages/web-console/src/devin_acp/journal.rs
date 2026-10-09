//! ACP 发送前台账。未知执行保留锁，不能因重启或新请求编号自动重发。
use super::protocol::{ExecutionScope, ModelSelection};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RemoteBinding {
    pub lane: String,
    pub remote_session_id: Option<String>,
}

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
        journal.connection()?.execute_batch("CREATE TABLE IF NOT EXISTS devin_acp_retired_bindings (
            remote_session_id TEXT PRIMARY KEY, binding_json TEXT NOT NULL);")
            .map_err(|_|fail("ACP 历史绑定表初始化失败。"))?;
        journal.connection()?.execute_batch("CREATE TABLE IF NOT EXISTS devin_acp_config_receipts (
            attempt_id TEXT NOT NULL, stage TEXT NOT NULL, receipt_json TEXT NOT NULL,
            PRIMARY KEY(attempt_id,stage));")
            .map_err(|_| fail("ACP 配置回执表初始化失败。"))?;
        // 兼容本地早期台账样本；只加可空回执列，不重写历史状态或解除执行锁。
        let mut connection = journal.connection()?;
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 台账升级锁失败。"))?;
        // 扩展同库绑定主键，保留历史聊天绑定、在途锁及完整 attempt 原文。
        let has_lane = tx.prepare("PRAGMA table_info(devin_acp_bindings)")
            .map_err(|_| fail("ACP 绑定结构读取失败。"))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|_| fail("ACP 绑定列读取失败。"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| fail("ACP 绑定列无效。"))?.iter().any(|name| name == "lane");
        if !has_lane {
            tx.execute_batch("ALTER TABLE devin_acp_bindings RENAME TO devin_acp_bindings_before_lane;
                CREATE TABLE devin_acp_bindings (
                    workspace_id TEXT NOT NULL, room_id TEXT NOT NULL, agent_id TEXT NOT NULL,
                    lane TEXT NOT NULL DEFAULT '', cwd TEXT NOT NULL, cli_identity TEXT NOT NULL,
                    context_digest TEXT NOT NULL, remote_session_id TEXT, owner_epoch INTEGER NOT NULL,
                    generation INTEGER NOT NULL, locked_attempt TEXT,
                    PRIMARY KEY(workspace_id,room_id,agent_id,lane));
                INSERT INTO devin_acp_bindings(workspace_id,room_id,agent_id,cwd,cli_identity,context_digest,
                    remote_session_id,owner_epoch,generation,locked_attempt)
                    SELECT workspace_id,room_id,agent_id,cwd,cli_identity,context_digest,
                    remote_session_id,owner_epoch,generation,locked_attempt FROM devin_acp_bindings_before_lane;
                DROP TABLE devin_acp_bindings_before_lane;")
                .map_err(|_| fail("ACP 内部请求绑定升级失败，原事务回滚。"))?;
        }
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

    pub(super) fn connection(&self) -> Result<Connection, String> {
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
             FROM devin_acp_bindings WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3 AND lane=?4",
                params![scope.workspace_id, scope.room_id, scope.agent_id, scope.lane],
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
            (workspace_id,room_id,agent_id,cwd,cli_identity,context_digest,remote_session_id,owner_epoch,generation,locked_attempt,lane)
            VALUES(?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9,?10)
            ON CONFLICT(workspace_id,room_id,agent_id,lane) DO UPDATE SET
            owner_epoch=excluded.owner_epoch,generation=excluded.generation,locked_attempt=excluded.locked_attempt",
            params![scope.workspace_id,scope.room_id,scope.agent_id,binding.cwd,binding.cli_identity,
                binding.context_digest,scope.owner_epoch,scope.generation,scope.attempt_id,scope.lane])
            .map_err(|_| fail("ACP 会话锁保存失败。"))?;
        tx.commit()
            .map_err(|_| fail("ACP 接纳提交未确认，不能发送任务。"))?;
        Ok(scope)
    }

    /// 上下文显式变化时归档远端绑定；稳定绑定也须核对旧执行的真实收尾。
    /// 已取消且排空、宿主工具结账的旧回合仅释放发送锁，不改判成功、不重发旧提示。
    pub(super) fn rotate_idle_context(&self,scope:&ExecutionScope,binding:&Binding)->Result<(),String> {
        let mut connection=self.connection()?;
        let tx=connection.transaction_with_behavior(TransactionBehavior::Immediate).map_err(|_|fail("ACP 重置锁失败。"))?;
        let old=tx.query_row("SELECT cwd,cli_identity,context_digest,remote_session_id,locked_attempt FROM devin_acp_bindings
            WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3 AND lane=?4",params![scope.workspace_id,scope.room_id,scope.agent_id,scope.lane],
            |row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,Option<String>>(3)?,row.get::<_,Option<String>>(4)?)))
            .optional().map_err(|_|fail("ACP 重置绑定读取失败。"))?;
        if let Some((cwd,cli,context,remote,locked))=old {
            if cli!=binding.cli_identity {return Err(fail("CLI 版本变化需要重新验收。"));}
            if context==binding.context_digest && cwd!=binding.cwd {
                return Err(fail("ACP 工作目录变化，请明确重置上下文。"));
            }
            if let Some(attempt)=locked.as_deref() {
                    let status=Self::status_on(&tx,attempt)?;
                    if status.state!="unknown" || !status.process_drained {
                        return Err(fail("ACP 旧执行仍在途或未知，不能重置绑定。"));
                    }
                    let old_scope:String=tx.query_row("SELECT scope_json FROM devin_acp_attempts WHERE attempt_id=?1",
                        [attempt],|row|row.get(0)).map_err(|_|fail("ACP 旧回合身份读取失败。"))?;
                    let old_scope_value:ExecutionScope=serde_json::from_str(&old_scope).map_err(|_|fail("ACP 旧回合身份无效。"))?;
                    // 兼容旧版消费者先关闭、ACP 尚未来得及记录取消的竞争。
                    // 只承认同库、精确宿主身份的持久停止记录，不推断远端取消成功。
                    let has_runs:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='runtime_runs')",[],|row|row.get(0))
                        .map_err(|_|fail("ACP 宿主停止记录结构核对失败。"))?;
                    let host_cancelled = old_scope_value.lane.is_empty() && has_runs && tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM runtime_runs WHERE id=?1 AND workspace_id=?2 AND chat_room_id=?3 AND legacy_turn_id=?4 AND stop_requested_at IS NOT NULL)",
                        params![old_scope_value.run_id,old_scope_value.workspace_id,old_scope_value.room_id,old_scope_value.turn_id],
                        |row|row.get::<_,bool>(0)).map_err(|_|fail("ACP 宿主停止身份核对失败。"))?
                        && (tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_runs WHERE id=?1 AND session_id=?2)",
                            params![old_scope_value.run_id,old_scope_value.agent_id], |row|row.get::<_,bool>(0))
                            .map_err(|_|fail("ACP 宿主停止主会话核对失败。"))?
                            || crate::chat_run_admission::target_is_admitted_on(&tx, &old_scope_value.run_id, &old_scope_value.agent_id)
                                .map_err(|_|fail("ACP 宿主停止目标核对失败。"))?);
                    if old_scope_value.lane.is_empty() {
                        crate::tool_dispatch_settlement::reconcile_drained_acp_cu(&tx,&old_scope_value.run_id,
                            &old_scope_value.workspace_id,&old_scope_value.room_id,&old_scope_value.agent_id,&old_scope_value.turn_id,&old_scope)?;
                        crate::tool_dispatch_settlement::reconcile_drained_acp_approval(&tx,&old_scope_value.run_id,
                            &old_scope_value.workspace_id,&old_scope_value.room_id,&old_scope_value.agent_id,&old_scope_value.turn_id,&old_scope)?;
                    }
                    // 协议终态已确认且所有宿主工具真实结账时，可以归档失败回合；
                    // 不改写旧 ACP unknown，不重发旧提示，不将取消绘图判为成功。
                    if !status.cancel_requested && !host_cancelled && status.protocol_stop.as_deref()!=Some("end_turn") {
                        return Err(fail("ACP 旧执行没有取消事实，不能建立新会话。"));
                    }
                    let has_tools:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='tool_calls')",[],|row|row.get(0))
                        .map_err(|_|fail("ACP 工具台账结构核对失败。"))?;
                    if has_tools && tx.query_row("SELECT EXISTS(SELECT 1 FROM tool_calls WHERE run_id=?1 AND source_request_key=?2 AND status NOT IN ('completed','failed'))",
                        params![old_scope_value.run_id,old_scope],|row|row.get::<_,bool>(0)).map_err(|_|fail("ACP 旧工具收尾核对失败。"))? {
                        return Err(fail("ACP 旧宿主工具尚未结账，不能建立新会话。"));
                    }
                tx.execute("UPDATE devin_acp_bindings SET locked_attempt=NULL WHERE locked_attempt=?1",
                    [attempt]).map_err(|_|fail("ACP 已收尾发送锁释放失败。"))?;
            }
            if context!=binding.context_digest {
                if let Some(remote)=remote {
                    let archive=serde_json::json!({"workspace_id":scope.workspace_id,"room_id":scope.room_id,
                        "agent_id":scope.agent_id,"lane":scope.lane,"cwd":cwd,"cli_identity":cli,"context_digest":context,
                        "remote_session_id":remote,"locked_attempt":locked});
                    tx.execute("INSERT INTO devin_acp_retired_bindings(remote_session_id,binding_json) VALUES(?1,?2)",
                        params![remote,archive.to_string()]).map_err(|_|fail("ACP 旧会话归档失败。"))?;
                }
                tx.execute("DELETE FROM devin_acp_bindings WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3 AND lane=?4",
                    params![scope.workspace_id,scope.room_id,scope.agent_id,scope.lane]).map_err(|_|fail("ACP 重置提交失败。"))?;
            }
        }
        tx.commit().map_err(|_|fail("ACP 重置提交未确认。"))
    }

    pub(super) fn remote_bindings(&self, workspace: &str, room: &str, agent: &str) -> Result<Vec<RemoteBinding>, String> {
        Self::remote_bindings_on(&self.connection()?, workspace, room, agent)
    }

    fn remote_bindings_on(connection: &Connection, workspace: &str, room: &str, agent: &str) -> Result<Vec<RemoteBinding>, String> {
        let mut query = connection.prepare("SELECT lane,remote_session_id FROM devin_acp_bindings
            WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3 ORDER BY lane")
            .map_err(|_| fail("ACP 远端绑定查询失败。"))?;
        let rows = query.query_map(params![workspace,room,agent], |row| Ok(RemoteBinding {
            lane: row.get(0)?, remote_session_id: row.get(1)?,
        })).map_err(|_| fail("ACP 远端绑定读取失败。"))?;
        rows.collect::<Result<Vec<_>,_>>().map_err(|_| fail("ACP 远端绑定无效。"))
    }

    /// 用户明确清除了远端后，只解除当前身份的闲置绑定，保留所有消息和执行历史。
    /// 不发送云端请求；下一轮正常接纳时才创建并保存新的远端 ID。
    pub(super) fn detach_remote_bindings(&self, workspace: &str, room: &str, agent: &str,
        expected: &[RemoteBinding]) -> Result<(), String> {
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 重新绑定锁失败。"))?;
        let current = Self::remote_bindings_on(&tx,workspace,room,agent)?;
        if current != expected || current.is_empty() {
            return Err(fail("远端绑定已变化，请刷新后重试；未修改任何绑定。"));
        }
        let locked: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM devin_acp_bindings
            WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3 AND locked_attempt IS NOT NULL)",
            params![workspace,room,agent], |row| row.get(0)).map_err(|_| fail("ACP 在途状态核对失败。"))?;
        if locked { return Err(fail("Devin 仍有在途或未知执行，请先停止并确认收尾；未解除绑定。")); }
        // 主聊天可能已接纳，但还没走到 ACP claim；同库事务同时检查这一窗口。
        let has_runs: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='runtime_runs')",
            [], |row| row.get(0)).map_err(|_| fail("宿主运行结构读取失败。"))?;
        if has_runs && tx.query_row("SELECT EXISTS(SELECT 1 FROM runtime_runs WHERE workspace_id=?1 AND chat_room_id=?2
            AND state NOT IN ('completed','failed','cancelled','blocked','interrupted','timed_out'))",
            params![workspace,room], |row| row.get::<_,bool>(0)).map_err(|_| fail("宿主运行状态读取失败。"))? {
            return Err(fail("当前聊天室仍有运行中的任务，请结束后再重新绑定。"));
        }
        for item in &current {
            if let Some(remote) = item.remote_session_id.as_deref() {
                let archive: String = tx.query_row("SELECT json_object('workspace_id',workspace_id,'room_id',room_id,
                    'agent_id',agent_id,'lane',lane,'cwd',cwd,'cli_identity',cli_identity,'context_digest',context_digest,
                    'remote_session_id',remote_session_id,'locked_attempt',locked_attempt,'reason','explicit_remote_rebind')
                    FROM devin_acp_bindings WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3 AND lane=?4",
                    params![workspace,room,agent,item.lane], |row| row.get(0)).map_err(|_| fail("旧绑定归档读取失败。"))?;
                tx.execute("INSERT INTO devin_acp_retired_bindings(remote_session_id,binding_json) VALUES(?1,?2)",
                    params![remote,archive]).map_err(|_| fail("旧远端绑定归档失败；未解除绑定。"))?;
            }
        }
        tx.execute("UPDATE devin_acp_bindings SET remote_session_id=NULL WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3",
            params![workspace,room,agent]).map_err(|_| fail("重新绑定提交失败。"))?;
        tx.commit().map_err(|_| fail("重新绑定提交未确认。"))
    }

    pub(super) fn check(connection: &Connection, scope: &ExecutionScope) -> Result<(), String> {
        if !scope.accepts(scope) {
            return Err(fail("ACP 身份不完整。"));
        }
        let encoded = serde_json::to_string(scope).map_err(|_| fail("ACP 身份编码失败。"))?;
        let live: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM devin_acp_bindings b
            JOIN devin_acp_attempts a ON a.attempt_id=b.locked_attempt
            WHERE b.workspace_id=?1 AND b.room_id=?2 AND b.agent_id=?3
              AND b.owner_epoch=?4 AND b.generation=?5 AND b.locked_attempt=?6 AND a.scope_json=?7 AND b.lane=?8)",
            params![scope.workspace_id,scope.room_id,scope.agent_id,scope.owner_epoch,scope.generation,scope.attempt_id,encoded,scope.lane],
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
            WHERE workspace_id=?1 AND room_id=?2 AND agent_id=?3 AND lane=?4",
                params![scope.workspace_id, scope.room_id, scope.agent_id, scope.lane],
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
        let retired:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM devin_acp_retired_bindings WHERE remote_session_id=?1)",
            [remote],|row|row.get(0)).map_err(|_|fail("ACP 历史会话身份核对失败。"))?;
        if retired {return Err(fail("ACP 已重置的旧会话不能重新绑定。"));}
        let used_elsewhere: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM devin_acp_bindings WHERE remote_session_id=?1
            AND NOT(workspace_id=?2 AND room_id=?3 AND agent_id=?4 AND lane=?5))",
                params![remote, scope.workspace_id, scope.room_id, scope.agent_id, scope.lane],
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

    /// 只记录配置/能力投影和提示大小计数；不保存 MCP 地址、图片、Authorization 或其它原文。
    pub(super) fn save_config_receipt(
        &self, scope: &ExecutionScope, stage: &str, receipt: &serde_json::Value,
    ) -> Result<(), String> {
        let image_payload = stage.strip_prefix("image_payload_").and_then(|number|number.parse::<u16>().ok())
            .is_some_and(|number|(1..=128).contains(&number));
        if !image_payload && !matches!(stage, "initial" | "selected" | "capabilities" | "prompt_payload") { return Err(fail("ACP 配置回执阶段无效。")); }
        if (stage == "prompt_payload" || image_payload) && !receipt.as_object().is_some_and(|object|
            object.len() == 3 && ["serialized_bytes","image_count","outgoing_limit_bytes"].iter()
                .all(|key| object.get(*key).is_some_and(|value| value.as_u64().is_some()))) {
            return Err(fail("ACP 提示大小回执仅允许数字计数。"));
        }
        let encoded = serde_json::to_string(receipt).map_err(|_| fail("ACP 配置回执编码失败。"))?;
        if encoded.len() > 512 * 1024 { return Err(fail("ACP 配置回执超过限额。")); }
        let mut connection = self.connection()?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| fail("ACP 配置回执锁失败。"))?;
        Self::check(&tx, scope)?;
        tx.execute("INSERT INTO devin_acp_config_receipts(attempt_id,stage,receipt_json) VALUES(?1,?2,?3)",
            params![scope.attempt_id, stage, encoded]).map_err(|_| fail("ACP 配置回执保存失败。"))?;
        tx.commit().map_err(|_| fail("ACP 配置回执提交失败。"))
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

    /// 用户明确停止已失败运行时，登记停止意图；旧 unknown 和远端绑定不改写。
    /// 只处理监督者已确认排空、宿主工具已结账的普通聊天回合。
    pub(super) fn stop_drained_run(&self, run: &str) -> Result<usize, String> {
        let connection = self.connection()?;
        let scopes = connection.prepare("SELECT a.scope_json FROM devin_acp_attempts a
            JOIN devin_acp_bindings b ON b.locked_attempt=a.attempt_id
            WHERE json_extract(a.scope_json,'$.run_id')=?1 AND b.lane=''
              AND a.state='unknown' AND a.process_drained=1 AND a.cancel_requested=0")
            .map_err(|_| fail("ACP 旧回合查询失败。"))?
            .query_map([run], |row| row.get::<_, String>(0))
            .map_err(|_| fail("ACP 旧回合读取失败。"))?
            .collect::<Result<Vec<_>, _>>().map_err(|_| fail("ACP 旧回合读取失败。"))?;
        let has_tools: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='tool_calls')",
            [], |row| row.get(0)).map_err(|_| fail("ACP 工具台账结构核对失败。"))?;
        let mut stopped = 0;
        for source in scopes {
            let scope: ExecutionScope = serde_json::from_str(&source).map_err(|_| fail("ACP 旧回合身份无效。"))?;
            if has_tools && connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM tool_calls WHERE run_id=?1 AND source_request_key=?2 AND status NOT IN ('completed','failed'))",
                params![run, source], |row| row.get::<_, bool>(0))
                .map_err(|_| fail("ACP 旧工具收尾核对失败。"))? {
                return Err(fail("旧宿主工具尚未结账，请等待真实收尾；未解除会话锁。"));
            }
            self.request_cancel(&scope)?;
            stopped += 1;
        }
        Ok(stopped)
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
            lane: String::new(),
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
    fn explicit_rebind_preserves_attempts_and_other_agents_and_rejects_stale_requests() {
        let (_dir,journal,binding) = setup();
        let first = journal.claim(scope("first"),&binding).unwrap();
        journal.save_remote(&first,"old-chat").unwrap();
        journal.transition(&first,&["prepared"],"not_sent",None,false).unwrap();
        journal.record_drained(&first).unwrap();
        let mut other = scope("other"); other.agent_id = "other-agent".into();
        let other = journal.claim(other,&binding).unwrap();
        journal.save_remote(&other,"other-chat").unwrap();
        let expected = journal.remote_bindings("w","r","a").unwrap();
        journal.detach_remote_bindings("w","r","a",&expected).unwrap();
        assert_eq!(journal.status(&first).unwrap().state,"not_sent");
        assert_eq!(journal.binding(&other).unwrap().remote_session_id.as_deref(),Some("other-chat"));
        assert!(journal.detach_remote_bindings("w","r","a",&expected).is_err());
        let next = journal.claim(scope("next"),&binding).unwrap();
        assert!(journal.binding(&next).unwrap().remote_session_id.is_none());
        assert!(journal.save_remote(&next,"old-chat").is_err());
        journal.save_remote(&next,"new-chat").unwrap();
    }

    #[test]
    fn explicit_rebind_does_not_release_an_active_or_unknown_attempt() {
        let (_dir,journal,binding) = setup();
        let first = journal.claim(scope("active"),&binding).unwrap();
        journal.save_remote(&first,"active-chat").unwrap();
        let expected = journal.remote_bindings("w","r","a").unwrap();
        assert!(journal.detach_remote_bindings("w","r","a",&expected).is_err());
        journal.transition(&first,&["prepared"],"unknown",None,false).unwrap();
        journal.record_drained(&first).unwrap();
        assert!(journal.detach_remote_bindings("w","r","a",&expected).is_err());
        assert_eq!(journal.binding(&first).unwrap().remote_session_id.as_deref(),Some("active-chat"));
    }

    #[test]
    fn internal_lane_preserves_actual_identity_without_reusing_outer_lock() {
        let (_dir, journal, binding) = setup();
        let outer = journal.claim(scope("outer"), &binding).unwrap();
        journal.transition(&outer, &["prepared"], "submitted", None, true).unwrap();
        let mut inner = scope("inner"); inner.lane = "internal".into();
        let inner = journal.claim(inner, &binding).unwrap();
        assert_eq!(inner.agent_id, outer.agent_id);
        assert_eq!(inner.room_id, outer.room_id);
        journal.save_remote(&outer, "outer-remote").unwrap();
        assert!(journal.save_remote(&inner, "outer-remote").is_err());
        journal.save_remote(&inner, "inner-remote").unwrap();
        let mut wrong = inner.clone(); wrong.lane.clear();
        assert!(journal.binding(&wrong).is_err());
        let reopened = Journal::open(journal.path()).unwrap();
        assert_eq!(reopened.binding(&outer).unwrap().remote_session_id.as_deref(), Some("outer-remote"));
        assert!(reopened.claim(scope("new-outer"), &binding).is_err());
    }

    #[test]
    fn stopped_consumer_requires_exact_host_cancel_and_settled_tools_before_new_binding() {
        let (_dir,journal,mut binding)=setup();
        let old=journal.claim(scope("old-consumer"),&binding).unwrap();
        journal.save_remote(&old,"old-consumer-remote").unwrap();
        journal.transition(&old,&["prepared"],"unknown",None,false).unwrap();
        journal.record_drained(&old).unwrap();
        let c=journal.connection().unwrap();
        c.execute_batch("CREATE TABLE runtime_runs(id TEXT,workspace_id TEXT,chat_room_id TEXT,session_id TEXT,legacy_turn_id TEXT,stop_requested_at INTEGER);
            CREATE TABLE tool_calls(run_id TEXT,source_request_key TEXT,status TEXT);").unwrap();
        c.execute("INSERT INTO runtime_runs VALUES(?1,?2,'wrong-room',?3,?4,1)",
            params![old.run_id,old.workspace_id,old.agent_id,old.turn_id]).unwrap();
        assert!(journal.rotate_idle_context(&scope("fresh-consumer"),&binding).is_err());
        c.execute("UPDATE runtime_runs SET chat_room_id=?1",[&old.room_id]).unwrap();
        c.execute("INSERT INTO tool_calls VALUES(?1,?2,'dispatched')",
            params![old.run_id,serde_json::to_string(&old).unwrap()]).unwrap();
        assert!(journal.rotate_idle_context(&scope("fresh-consumer"),&binding).is_err());
        c.execute("UPDATE tool_calls SET status='completed'",[]).unwrap();
        journal.rotate_idle_context(&scope("fresh-consumer"),&binding).unwrap();
        assert_eq!(journal.status(&old).unwrap().state,"unknown");
        assert!(journal.can_dispatch(&old).is_err());
        let fresh=journal.claim(scope("fresh-consumer"),&binding).unwrap();
        assert_eq!(journal.binding(&fresh).unwrap().remote_session_id.as_deref(),Some("old-consumer-remote"));
        journal.transition(&fresh,&["prepared"],"not_sent",None,false).unwrap();
        journal.record_drained(&fresh).unwrap();
        binding.context_digest="explicit-reset".into();
        binding.cwd=Path::new(&binding.cwd).join("reset-workspace").to_string_lossy().into();
        journal.rotate_idle_context(&scope("reset-consumer"),&binding).unwrap();
        let fresh=journal.claim(scope("reset-consumer"),&binding).unwrap();
        assert!(journal.binding(&fresh).unwrap().remote_session_id.is_none());
        assert!(journal.save_remote(&fresh,"old-consumer-remote").is_err());
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
        let c = reopened.connection().unwrap();
        c.execute_batch("CREATE TABLE tool_calls(run_id TEXT,source_request_key TEXT,status TEXT);").unwrap();
        c.execute("INSERT INTO tool_calls VALUES(?1,?2,'running')",
            params![a.run_id, serde_json::to_string(&a).unwrap()]).unwrap();
        assert!(reopened.stop_drained_run(&a.run_id).is_err());
        assert!(!reopened.status(&a).unwrap().cancel_requested);
        c.execute("UPDATE tool_calls SET status='failed'", []).unwrap();
        assert_eq!(reopened.stop_drained_run(&a.run_id).unwrap(), 1);
        assert_eq!(reopened.stop_drained_run(&a.run_id).unwrap(), 0);
        assert_eq!(reopened.status(&a).unwrap().state, "unknown");
        reopened.rotate_idle_context(&scope("a2"), &binding).unwrap();
        assert!(reopened.claim(scope("a2"), &binding).is_ok());
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
