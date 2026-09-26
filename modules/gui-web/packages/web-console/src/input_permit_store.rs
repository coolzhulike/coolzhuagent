//! **8.2b／8.3b 联合持久化**（2026-09-26 授权）：许可登记与执行者实例登记。
//!
//! ## 边界（裁决 §3.1／§2.3）
//!
//! - **只落已有契约**：六态语义、状态转换、失败表全部来自 `runtime::input_safety` 的**纯契约**；
//!   本模块**不在 SQL 层重新解释**它们——状态列存契约的 `as_str()`，校验与转换仍调用契约函数。
//! - **不新增平行权威**：不建第二份事故库、不建第二套恢复状态机；复用既有安全事件与资源状态。
//! - **不把活句柄落库**：执行者登记只存**身份与证据引用**；PID 是"待核对线索"，**不是**执行能力。
//! - **事务包住"读取—判断—写入"**（§3.3）：三处原子操作各自在**同一短事务**内读 revision、
//!   跑纯逻辑判定、做条件更新；冲突返回明确结果，不靠最后写入覆盖。
//! - **不跨等待持事务**：不跨 helper 等待、模型请求或人工确认持有事务。
//! - 本模块留在**宿主的输入安全存储**边界（web-console），**不把 rusqlite 引入 core-runtime**。
//!
//! ## 为什么暂时允许 dead_code（显式，不是掩盖）
//!
//! 本模块是本批（8.2b／8.3b）交付的**存储适配**；把它接进生产调用点是下一步 8.2c／8.3c
//! 的工作（裁决 §8 把生产接线列在迁移验收之后）。因此当前**没有生产调用者**，
//! 但**不是**被丢弃的代码：它已由本模块的迁移验收用例（DB-1…DB-8）真实驱动。
//! 接线落地后应移除此豁免——留着它才是问题。
#![allow(dead_code)]

use rusqlite::{Connection, OptionalExtension};
use runtime::{
    disposition_for, ExecutorDisposition, ExecutorInstanceState, ExecutorObservation,
    ExecutorRegistration, HelperIdentity, InputPermit, InputPermitState, InputSafetyResourceScope,
    PermitTransitionError,
};

/// 本模块的存储错误：**可分辨**，不与 sqlite 通用错误混为一谈。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PermitStoreError {
    Sqlite(String),
    /// 事务期间与其它写者冲突（例如 `SQLITE_BUSY`）：**不当作成功**。
    WriteContention { detail: String },
    /// 许可不存在。
    PermitNotFound { permit_id: String },
    /// 执行者不存在。
    ExecutorNotFound { executor_instance_id: String },
    /// 状态转换非法（理由来自纯契约，不在 SQL 层另写一套）。
    IllegalTransition(PermitTransitionError),
    /// 用户构造的许可/登记结构不合法。
    InvalidPermit { field: &'static str, reason: &'static str },
    InvalidExecutor { field: &'static str, reason: &'static str },
    /// 条件更新没有命中（revision / 状态 / gate 已变）：调用方必须重新读取后再决定。
    ConditionNotMet { what: &'static str },
}

impl std::fmt::Display for PermitStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(detail) => write!(formatter, "许可存储 sqlite 错误：{detail}"),
            Self::WriteContention { detail } => {
                write!(formatter, "许可存储写锁竞争（不得当作成功）：{detail}")
            }
            Self::PermitNotFound { permit_id } => write!(formatter, "许可不存在：{permit_id}"),
            Self::ExecutorNotFound { executor_instance_id } => {
                write!(formatter, "执行者不存在：{executor_instance_id}")
            }
            Self::IllegalTransition(error) => write!(formatter, "{error}"),
            Self::InvalidPermit { field, reason } => {
                write!(formatter, "许可字段 {field} 无效：{reason}")
            }
            Self::InvalidExecutor { field, reason } => {
                write!(formatter, "执行者字段 {field} 无效：{reason}")
            }
            Self::ConditionNotMet { what } => {
                write!(formatter, "条件更新未命中（{what}）：必须重新读取后再决定")
            }
        }
    }
}

fn sqlite_error(error: rusqlite::Error) -> PermitStoreError {
    PermitStoreError::Sqlite(error.to_string())
}

/// 把 rusqlite 的忙/锁错误映射成**显式的写锁竞争**（裁决 §3.3：`SQLITE_BUSY` 不是成功）。
fn map_write_error(error: rusqlite::Error) -> PermitStoreError {
    let text = error.to_string();
    if text.contains("locked") || text.contains("busy") {
        PermitStoreError::WriteContention { detail: text }
    } else {
        PermitStoreError::Sqlite(text)
    }
}

/// v3 新增对象。**幂等**（`IF NOT EXISTS`），由唯一的迁移入口调用。
///
/// 表设计刻意保持"只落已有契约"：
/// - 状态列存 `InputPermitState::as_str()` / `ExecutorInstanceState::as_str()`；
/// - 转换合法性由**契约函数**判定，不在 SQL 里写第二套；
/// - 执行者只存身份与证据引用，**不存句柄**。
pub(crate) fn ensure_v3_objects(connection: &Connection) -> Result<(), PermitStoreError> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS input_safety_permits (
                permit_id TEXT PRIMARY KEY,
                action_id TEXT NOT NULL,
                scope TEXT NOT NULL,
                execution_context_ref TEXT NOT NULL,
                frozen_action_digest TEXT NOT NULL,
                policy_revision INTEGER NOT NULL,
                gate_revision INTEGER NOT NULL,
                issued_owner_id TEXT NOT NULL,
                issued_epoch INTEGER NOT NULL,
                expires_at_unix_ms INTEGER NOT NULL,
                executor_instance_id TEXT,
                state TEXT NOT NULL,
                revision INTEGER NOT NULL,
                revocation_reason TEXT,
                updated_at_unix_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS input_safety_permits_action
                ON input_safety_permits (action_id);
            CREATE INDEX IF NOT EXISTS input_safety_permits_scope_state
                ON input_safety_permits (scope, state);

            CREATE TABLE IF NOT EXISTS input_safety_executors (
                executor_instance_id TEXT PRIMARY KEY,
                launch_operation_id TEXT NOT NULL,
                coordinator_instance_id TEXT NOT NULL,
                scope TEXT NOT NULL,
                host_launch_instance TEXT NOT NULL,
                action_id TEXT NOT NULL,
                pid INTEGER NOT NULL,
                creation_time_100ns INTEGER,
                user_session TEXT,
                host_process_path TEXT NOT NULL,
                script_or_program_digest TEXT NOT NULL,
                protocol_version INTEGER NOT NULL,
                supervision_bound INTEGER NOT NULL,
                state TEXT NOT NULL,
                revision INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS input_safety_executors_scope_state
                ON input_safety_executors (scope, state);
            "#,
        )
        .map_err(sqlite_error)
}

fn parse_permit_state(raw: &str) -> Result<InputPermitState, PermitStoreError> {
    for state in [
        InputPermitState::PendingActivation,
        InputPermitState::DispatchCommitted,
        InputPermitState::Executing,
        InputPermitState::Finished,
        InputPermitState::Revoked,
        InputPermitState::OutcomeUnknown,
    ] {
        if state.as_str() == raw {
            return Ok(state);
        }
    }
    Err(PermitStoreError::Sqlite(format!(
        "未知的许可状态 `{raw}`：不得当作任何已知状态"
    )))
}

fn parse_executor_state(raw: &str) -> Result<ExecutorInstanceState, PermitStoreError> {
    for state in [
        ExecutorInstanceState::RegisteredPendingVerification,
        ExecutorInstanceState::VerifiedAlive,
        ExecutorInstanceState::ExitedConfirmed,
        ExecutorInstanceState::UnverifiableUnknown,
        ExecutorInstanceState::EscalatedHumanReview,
    ] {
        if state.as_str() == raw {
            return Ok(state);
        }
    }
    Err(PermitStoreError::Sqlite(format!(
        "未知的执行者状态 `{raw}`"
    )))
}

fn scope_of(raw: &str) -> Result<InputSafetyResourceScope, PermitStoreError> {
    InputSafetyResourceScope::parse(raw)
        .map_err(|error| PermitStoreError::Sqlite(format!("scope 非法：{error:?}")))
}

/// 许可存储适配器。
pub(crate) struct PermitStore<'a> {
    connection: &'a Connection,
}

impl<'a> PermitStore<'a> {
    pub(crate) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// 在一个**短事务**里读取—判断—写入（裁决 §3.3）。
    ///
    /// 用 `BEGIN IMMEDIATE` 建立写入顺序：拿不到写锁时**显式**返回写锁竞争，不重试到看起来成功。
    fn in_immediate_transaction<T>(
        &self,
        _operation: &'static str,
        run: impl FnOnce(&Connection) -> Result<T, PermitStoreError>,
    ) -> Result<T, PermitStoreError> {
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(map_write_error)?;
        match run(self.connection) {
            Ok(value) => {
                self.connection
                    .execute_batch("COMMIT")
                    .map_err(map_write_error)?;
                Ok(value)
            }
            Err(error) => {
                // 回滚失败也不掩盖原始错误：两者都保留在错误文本里。
                if let Err(rollback) = self.connection.execute_batch("ROLLBACK") {
                    return Err(PermitStoreError::Sqlite(format!(
                        "{error}（且回滚失败：{rollback}）"
                    )));
                }
                Err(error)
            }
        }
    }

    /// 登记一个**待激活**许可。
    ///
    /// 允许 `executor_instance_id` 为空：裁决 §3.2 明确"尚未启动执行者的 Pending 记录
    /// **不要求伪造** executor ID"。越界前必须齐备由消费路径与契约校验共同保证。
    pub(crate) fn register_pending(
        &self,
        permit: &InputPermit,
        defined_state: InputPermitState,
        now_unix_ms: u64,
    ) -> Result<(), PermitStoreError> {
        if defined_state != InputPermitState::PendingActivation {
            return Err(PermitStoreError::ConditionNotMet {
                what: "登记路径只接受待激活状态（其它状态由转换路径产生）",
            });
        }
        if let Err(error) = permit.validate_structure() {
            return Err(PermitStoreError::InvalidPermit {
                field: error.field,
                reason: error.reason,
            });
        }
        self.in_immediate_transaction("register_pending", |connection| {
            // 同 action 已存在：按契约"返回已有状态、不重新创建可执行资格"，这里显式拒绝重复登记。
            let existing: Option<(String, String)> = connection
                .query_row(
                    "SELECT permit_id, frozen_action_digest FROM input_safety_permits WHERE action_id = ?1",
                    rusqlite::params![permit.action_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(sqlite_error)?;
            if let Some((_existing_id, existing_digest)) = existing {
                return Err(if existing_digest == permit.frozen_action_digest {
                    PermitStoreError::ConditionNotMet {
                        what: "同一 action 与同一内容已登记：调用方应读取已有许可状态，而不是新建",
                    }
                } else {
                    PermitStoreError::ConditionNotMet {
                        what: "同一 action 换内容：拒绝（这是错误的复用）",
                    }
                });
            }
            connection
                .execute(
                    "INSERT INTO input_safety_permits (
                        permit_id, action_id, scope, execution_context_ref, frozen_action_digest,
                        policy_revision, gate_revision, issued_owner_id, issued_epoch,
                        expires_at_unix_ms, executor_instance_id, state, revision,
                        revocation_reason, updated_at_unix_ms
                     ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                    rusqlite::params![
                        permit.permit_id,
                        permit.action_id,
                        permit.scope.as_str(),
                        permit.execution_context_ref,
                        permit.frozen_action_digest,
                        permit.policy_revision as i64,
                        permit.gate_revision as i64,
                        permit.issued_owner_id,
                        permit.issued_epoch as i64,
                        permit.expires_at_unix_ms as i64,
                        permit.executor_instance_id,
                        defined_state.as_str(),
                        permit.revision as i64,
                        permit.revocation_reason,
                        now_unix_ms as i64,
                    ],
                )
                .map_err(sqlite_error)?;
            Ok(())
        })
    }

    pub(crate) fn load_permit(
        &self,
        permit_id: &str,
    ) -> Result<InputPermit, PermitStoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT permit_id, action_id, scope, execution_context_ref, frozen_action_digest,
                        policy_revision, gate_revision, issued_owner_id, issued_epoch,
                        expires_at_unix_ms, executor_instance_id, state, revision, revocation_reason
                   FROM input_safety_permits WHERE permit_id = ?1",
                rusqlite::params![permit_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                        row.get::<_, i64>(12)?,
                        row.get::<_, Option<String>>(13)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite_error)?
            .ok_or_else(|| PermitStoreError::PermitNotFound {
                permit_id: permit_id.to_string(),
            })?;
        Ok(InputPermit {
            permit_id: row.0,
            action_id: row.1,
            scope: scope_of(&row.2)?,
            execution_context_ref: row.3,
            frozen_action_digest: row.4,
            policy_revision: u64::try_from(row.5).unwrap_or_default(),
            gate_revision: u64::try_from(row.6).unwrap_or_default(),
            issued_owner_id: row.7,
            issued_epoch: u64::try_from(row.8).unwrap_or_default(),
            expires_at_unix_ms: u64::try_from(row.9).unwrap_or_default(),
            executor_instance_id: row.10,
            state: parse_permit_state(&row.11)?,
            revision: u64::try_from(row.12).unwrap_or_default(),
            revocation_reason: row.13,
        })
    }

    /// **消费许可**：核对 gate／epoch／期限／绑定，做**单次**状态转换，同一事务内记事件。
    ///
    /// 这是 DB-6／DB-7 的核心：条件全部在事务内读取并核对，只有一个写入者能把它推到
    /// `DispatchCommitted`；第二次调用因为状态已变而**必然失败**（不产生第二次执行资格）。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn consume_permit(
        &self,
        permit_id: &str,
        budget: PermitConsumptionBudget,
        execution_bound: PermitExecutorBinding,
        now_unix_ms: u64,
    ) -> Result<InputPermitState, PermitStoreError> {
        self.in_immediate_transaction("consume_permit", |connection| {
            let current = read_permit_state(connection, permit_id)?;
            let row = read_permit_columns(connection, permit_id)?;
            // 纯契约判定：只能从待激活出发。
            let next = current
                .transition_to(InputPermitState::DispatchCommitted)
                .map_err(PermitStoreError::IllegalTransition)?;
            // 期限：到期即不得激活（契约同口径）。
            if now_unix_ms >= row.expires_at_unix_ms {
                return Err(PermitStoreError::ConditionNotMet {
                    what: "许可已过期，不得激活",
                });
            }
            // gate／政策：**必须与当前一致**，否则说明关闸已经发生过。
            if row.gate_revision != budget.gate_revision {
                return Err(PermitStoreError::ConditionNotMet {
                    what: "gate revision 已变化（关闸在先）：消费必须失败",
                });
            }
            if row.policy_revision != budget.policy_revision {
                return Err(PermitStoreError::ConditionNotMet {
                    what: "安全政策已变化：按最新政策重新登记后再说",
                });
            }
            if row.issued_epoch != budget.held_epoch {
                return Err(PermitStoreError::ConditionNotMet {
                    what: "epoch 不是当前持有的资格",
                });
            }
            // 越界前执行者关联必须齐备（§3.2）；要么登记时已有，要么此刻显式绑定。
            let executor = match (row.executor_instance_id.as_deref(), execution_bound) {
                (Some(bound), PermitExecutorBinding::AlreadyBound(bound_id))
                    if bound == bound_id =>
                {
                    bound_id.to_string()
                }
                (Some(bound), PermitExecutorBinding::BindTo(bound_id)) => {
                    let _ = bound;
                    bound_id.to_string()
                }
                (None, PermitExecutorBinding::BindTo(bound_id)) => bound_id.to_string(),
                _ => {
                    return Err(PermitStoreError::ConditionNotMet {
                        what: "越过派发边界前必须建立执行者实例关联",
                    })
                }
            };
            // 条件更新：把状态与 revision 一起推进（乐观并发，不用最后写入覆盖）。
            let changed = connection
                .execute(
                    "UPDATE input_safety_permits
                        SET state = ?1, revision = revision + 1, executor_instance_id = ?2,
                            updated_at_unix_ms = ?3
                      WHERE permit_id = ?4 AND state = ?5 AND revision = ?6",
                    rusqlite::params![
                        next.as_str(),
                        executor,
                        now_unix_ms as i64,
                        permit_id,
                        current.as_str(),
                        row.revision as i64,
                    ],
                )
                .map_err(map_write_error)?;
            if changed != 1 {
                return Err(PermitStoreError::ConditionNotMet {
                    what: "许可状态或 revision 已被他人改变",
                });
            }
            Ok(next)
        })
    }

    /// **撤销**尚未消费的许可（只收紧入口；不得回到待激活）。
    pub(crate) fn revoke(
        &self,
        permit_id: &str,
        reason: &str,
        now_unix_ms: u64,
    ) -> Result<InputPermitState, PermitStoreError> {
        self.in_immediate_transaction("revoke", |connection| {
            let current = read_permit_state(connection, permit_id)?;
            let next = current
                .transition_to(InputPermitState::Revoked)
                .map_err(PermitStoreError::IllegalTransition)?;
            let changed = connection
                .execute(
                    "UPDATE input_safety_permits
                        SET state = ?1, revision = revision + 1, revocation_reason = ?2,
                            updated_at_unix_ms = ?3
                      WHERE permit_id = ?4 AND state = ?5",
                    rusqlite::params![
                        next.as_str(),
                        reason,
                        now_unix_ms as i64,
                        permit_id,
                        current.as_str()
                    ],
                )
                .map_err(map_write_error)?;
            if changed != 1 {
                return Err(PermitStoreError::ConditionNotMet {
                    what: "许可状态已被他人改变",
                });
            }
            Ok(next)
        })
    }

    /// **关闸**：推进 gate revision，并**原子撤销**该 scope 下未消费的许可。
    ///
    /// 与 `consume_permit` 配对：两者都经 `BEGIN IMMEDIATE`，因此顺序是确定的
    /// （先关闸 ⇒ 消费看到新 gate 必失败；先消费 ⇒ 已越界，关闸不再改写它）。
    pub(crate) fn close_intake_and_revoke_pending(
        &self,
        scope: &InputSafetyResourceScope,
        new_gate_revision: u64,
        now_unix_ms: u64,
    ) -> Result<Vec<String>, PermitStoreError> {
        self.in_immediate_transaction("close_intake", |connection| {
            let mut statement = connection
                .prepare(
                    "SELECT permit_id FROM input_safety_permits
                      WHERE scope = ?1 AND state = ?2",
                )
                .map_err(sqlite_error)?;
            let ids = statement
                .query_map(
                    rusqlite::params![scope.as_str(), InputPermitState::PendingActivation.as_str()],
                    |row| row.get::<_, String>(0),
                )
                .map_err(sqlite_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(sqlite_error)?;
            drop(statement);
            // 只撤销**未消费**的：已越界的许可保持原状（不得改写成"未发送"）。
            connection
                .execute(
                    "UPDATE input_safety_permits
                        SET state = ?1, revision = revision + 1, revocation_reason = ?2,
                            updated_at_unix_ms = ?3
                      WHERE scope = ?4 AND state = ?5",
                    rusqlite::params![
                        InputPermitState::Revoked.as_str(),
                        "关闸撤销未消费许可",
                        now_unix_ms as i64,
                        scope.as_str(),
                        InputPermitState::PendingActivation.as_str(),
                    ],
                )
                .map_err(map_write_error)?;
            // gate 推进与撤销在同一事务内：提交后"关闸在先"成为确定事实。
            connection
                .execute(
                    "UPDATE input_safety_resource_state SET revision = ?1 WHERE scope = ?2",
                    rusqlite::params![new_gate_revision as i64, scope.as_str()],
                )
                .map_err(map_write_error)?;
            Ok(ids)
        })
    }

    /// **对账**：把 `OutcomeUnknown` 结清为 `Finished`（不是恢复执行）。
    pub(crate) fn reconcile_outcome_unknown(
        &self,
        permit_id: &str,
        now_unix_ms: u64,
    ) -> Result<InputPermitState, PermitStoreError> {
        self.in_immediate_transaction("reconcile", |connection| {
            let current = read_permit_state(connection, permit_id)?;
            let next = current
                .transition_to(InputPermitState::Finished)
                .map_err(PermitStoreError::IllegalTransition)?;
            let changed = connection
                .execute(
                    "UPDATE input_safety_permits
                        SET state = ?1, revision = revision + 1, updated_at_unix_ms = ?2
                      WHERE permit_id = ?3 AND state = ?4",
                    rusqlite::params![
                        next.as_str(),
                        now_unix_ms as i64,
                        permit_id,
                        current.as_str()
                    ],
                )
                .map_err(map_write_error)?;
            if changed != 1 {
                return Err(PermitStoreError::ConditionNotMet {
                    what: "许可状态已被他人改变",
                });
            }
            Ok(next)
        })
    }

    /// 该 scope 下的许可状态分布（供 DB-8 之类"新表为空不等于历史安全"的核对使用）。
    pub(crate) fn count_by_state(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<Vec<(InputPermitState, u64)>, PermitStoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT state, COUNT(*) FROM input_safety_permits WHERE scope = ?1 GROUP BY state")
            .map_err(sqlite_error)?;
        let rows = statement
            .query_map(rusqlite::params![scope.as_str()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(sqlite_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(sqlite_error)?;
        rows.into_iter()
            .map(|(state, count)| {
                Ok((parse_permit_state(&state)?, u64::try_from(count).unwrap_or_default()))
            })
            .collect()
    }
}

/// 消费许可时要求核对的条件（都在同一事务内读取并比对）。
#[derive(Debug, Clone, Copy)]
pub(crate) struct PermitConsumptionBudget {
    pub gate_revision: u64,
    pub policy_revision: u64,
    pub held_epoch: u64,
}

/// 越界前如何取得执行者关联（§3.2 的创建顺序）。
#[derive(Debug, Clone, Copy)]
pub(crate) enum PermitExecutorBinding<'a> {
    /// 登记时已绑定（Pending 记录也可以先不绑定，见 `register_pending`）。
    AlreadyBound(&'a str),
    /// 此刻显式绑定。
    BindTo(&'a str),
}

fn read_permit_state(
    connection: &Connection,
    permit_id: &str,
) -> Result<InputPermitState, PermitStoreError> {
    let raw: String = connection
        .query_row(
            "SELECT state FROM input_safety_permits WHERE permit_id = ?1",
            rusqlite::params![permit_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sqlite_error)?
        .ok_or_else(|| PermitStoreError::PermitNotFound {
            permit_id: permit_id.to_string(),
        })?;
    parse_permit_state(&raw)
}

struct PermitRow {
    expires_at_unix_ms: u64,
    gate_revision: u64,
    policy_revision: u64,
    issued_epoch: u64,
    revision: u64,
    executor_instance_id: Option<String>,
}

fn read_permit_columns(
    connection: &Connection,
    permit_id: &str,
) -> Result<PermitRow, PermitStoreError> {
    connection
        .query_row(
            "SELECT expires_at_unix_ms, gate_revision, policy_revision, issued_epoch, revision,
                    executor_instance_id
               FROM input_safety_permits WHERE permit_id = ?1",
            rusqlite::params![permit_id],
            |row| {
                Ok(PermitRow {
                    expires_at_unix_ms: u64::try_from(row.get::<_, i64>(0)?).unwrap_or_default(),
                    gate_revision: u64::try_from(row.get::<_, i64>(1)?).unwrap_or_default(),
                    policy_revision: u64::try_from(row.get::<_, i64>(2)?).unwrap_or_default(),
                    issued_epoch: u64::try_from(row.get::<_, i64>(3)?).unwrap_or_default(),
                    revision: u64::try_from(row.get::<_, i64>(4)?).unwrap_or_default(),
                    executor_instance_id: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(sqlite_error)?
        .ok_or_else(|| PermitStoreError::PermitNotFound {
            permit_id: permit_id.to_string(),
        })
}

/// 执行者登记存储适配器（8.3b）。
pub(crate) struct ExecutorStore<'a> {
    connection: &'a Connection,
}

impl<'a> ExecutorStore<'a> {
    pub(crate) fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    /// 登记**启动意图**：此时还没有实例证据，因此创建时间等可为空。
    ///
    /// 这就是 §3.2 的第一、二步（登记意图 → 创建尚不能输入的执行者），
    /// **不需要**在创建之前就拥有"创建结果"，不制造循环前置。
    pub(crate) fn register_launch_intent(
        &self,
        registration: &ExecutorRegistration,
        now_unix_ms: u64,
    ) -> Result<(), PermitStoreError> {
        if let Err(error) = registration.validate_structure() {
            return Err(PermitStoreError::InvalidExecutor {
                field: error.field,
                reason: error.reason,
            });
        }
        if registration.state != ExecutorInstanceState::RegisteredPendingVerification {
            return Err(PermitStoreError::ConditionNotMet {
                what: "登记启动意图只接受「待核查」状态",
            });
        }
        self.connection
            .execute(
                "INSERT INTO input_safety_executors (
                    executor_instance_id, launch_operation_id, coordinator_instance_id, scope,
                    host_launch_instance, action_id, pid, creation_time_100ns, user_session,
                    host_process_path, script_or_program_digest, protocol_version,
                    supervision_bound, state, revision, updated_at_unix_ms
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                rusqlite::params![
                    registration.executor_instance_id,
                    registration.launch_operation_id,
                    registration.coordinator_instance_id,
                    registration.scope.as_str(),
                    registration.host_launch_instance,
                    registration.action_id,
                    i64::from(registration.pid),
                    registration.creation_time_100ns.map(|value| value as i64),
                    registration.user_session,
                    registration.helper.host_process_path,
                    registration.helper.script_or_program_digest,
                    i64::from(registration.protocol_version),
                    i64::from(registration.supervision_bound),
                    registration.state.as_str(),
                    registration.revision as i64,
                    now_unix_ms as i64,
                ],
            )
            .map_err(map_write_error)?;
        Ok(())
    }

    /// 取得**实际实例证据**后推进状态（并绑定监督关系）。
    ///
    /// 用契约的失败表把观测映射成处置：调用方拿到的是**可分辨的处置**，而不是一个布尔值。
    pub(crate) fn record_instance_evidence(
        &self,
        executor_instance_id: &str,
        observation: ExecutorObservation,
        creation_time_100ns: Option<u64>,
        supervision_bound: bool,
        now_unix_ms: u64,
    ) -> Result<ExecutorDisposition, PermitStoreError> {
        let next = match observation {
            ExecutorObservation::MatchesAndAlive => ExecutorInstanceState::VerifiedAlive,
            ExecutorObservation::ExitedButReleaseUnknown
            | ExecutorObservation::DirectHelperExited => ExecutorInstanceState::UnverifiableUnknown,
            ExecutorObservation::AccessDeniedOrUnreadable
            | ExecutorObservation::NoTrustedRegistration
            | ExecutorObservation::PidReused => ExecutorInstanceState::UnverifiableUnknown,
            ExecutorObservation::JobClosedOutcomeUnverified
            | ExecutorObservation::TerminateRequestedNotConfirmed => {
                ExecutorInstanceState::RegisteredPendingVerification
            }
            ExecutorObservation::LateReceipt => ExecutorInstanceState::RegisteredPendingVerification,
        };
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(map_write_error)?;
        let result = (|| -> Result<(), PermitStoreError> {
            let changed = self
                .connection
                .execute(
                    "UPDATE input_safety_executors
                        SET state = ?1, revision = revision + 1,
                            creation_time_100ns = COALESCE(?2, creation_time_100ns),
                            supervision_bound = ?3, updated_at_unix_ms = ?4
                      WHERE executor_instance_id = ?5",
                    rusqlite::params![
                        next.as_str(),
                        creation_time_100ns.map(|value| value as i64),
                        i64::from(supervision_bound),
                        now_unix_ms as i64,
                        executor_instance_id,
                    ],
                )
                .map_err(map_write_error)?;
            if changed != 1 {
                return Err(PermitStoreError::ExecutorNotFound {
                    executor_instance_id: executor_instance_id.to_string(),
                });
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.connection
                    .execute_batch("COMMIT")
                    .map_err(map_write_error)?;
                Ok(disposition_for(observation))
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    pub(crate) fn load_executor(
        &self,
        executor_instance_id: &str,
    ) -> Result<ExecutorRegistration, PermitStoreError> {
        let row = self
            .connection
            .query_row(
                "SELECT executor_instance_id, launch_operation_id, coordinator_instance_id, scope,
                        host_launch_instance, action_id, pid, creation_time_100ns, user_session,
                        host_process_path, script_or_program_digest, protocol_version,
                        supervision_bound, state, revision
                   FROM input_safety_executors WHERE executor_instance_id = ?1",
                rusqlite::params![executor_instance_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, String>(9)?,
                        row.get::<_, String>(10)?,
                        row.get::<_, i64>(11)?,
                        row.get::<_, i64>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, i64>(14)?,
                    ))
                },
            )
            .optional()
            .map_err(sqlite_error)?
            .ok_or_else(|| PermitStoreError::ExecutorNotFound {
                executor_instance_id: executor_instance_id.to_string(),
            })?;
        Ok(ExecutorRegistration {
            executor_instance_id: row.0,
            launch_operation_id: row.1,
            coordinator_instance_id: row.2,
            scope: scope_of(&row.3)?,
            host_launch_instance: row.4,
            action_id: row.5,
            pid: u32::try_from(row.6).unwrap_or_default(),
            creation_time_100ns: row.7.and_then(|value| u64::try_from(value).ok()),
            user_session: row.8,
            helper: HelperIdentity {
                host_process_path: row.9,
                script_or_program_digest: row.10,
            },
            protocol_version: u32::try_from(row.11).unwrap_or_default(),
            supervision_bound: row.12 != 0,
            state: parse_executor_state(&row.13)?,
            revision: u64::try_from(row.14).unwrap_or_default(),
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_safety_store::{InputSafetyStore, INPUT_SAFETY_DB_FILE};
    use std::path::Path;
    use runtime::InputPermitState;

    fn temp_root() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }

    fn scope() -> InputSafetyResourceScope {
        InputSafetyResourceScope::parse("windows-session-permit-test").expect("scope")
    }

    /// 建一个**合法 v2 形状**的旧库（含真实遗留事实），供升级验收使用。
    ///
    /// 做法与既有 `input_safety_store::tests` 同源：先让当前实现建出 schema，
    /// 再把它**降级**成 v2 形状（去掉 v3 对象、建一条旧恢复操作），不伪造"历史许可"。
    fn seed_v2_database(root: &Path) -> String {
        let store = InputSafetyStore::open_at(root).expect("open");
        let store_id = store.store_id().as_str().to_string();
        drop(store);
        let connection =
            Connection::open(root.join(INPUT_SAFETY_DB_FILE)).expect("raw connection");
        connection
            .execute_batch(
                "DROP TABLE IF EXISTS input_safety_permits;
                 DROP TABLE IF EXISTS input_safety_executors;
                 INSERT INTO input_safety_resource_state
                     (scope, state, revision, coordinator_instance_id, recovery_epoch,
                      accepts_new_input, updated_at_unix_ms)
                     VALUES ('windows-session-permit-test', 'isolated', 7, 'coordinator-old', 3,
                             0, 11)
                     ON CONFLICT(scope) DO UPDATE SET
                         state = 'isolated', revision = 7, recovery_epoch = 3,
                         accepts_new_input = 0;
                 INSERT INTO input_safety_recovery_operations
                     (recovery_operation_id, coordinator_instance_id, recovery_epoch, gate_revision, scope,
                      source_database_identity, candidate_run_ids_json, allowed_operations_json, stage,
                      recorded_at_unix_ms, committed, disposition)
                     VALUES ('recovery-legacy', 'coordinator-old', 3, 7, 'windows-session-permit-test',
                             'session-db:default', '[]', '[]', 'r5_incident_established', 11, 0, 'pending');
                 PRAGMA user_version = 2;",
            )
            .expect("v2 shape");
        store_id
    }

    fn permit(permit_id: &str, action_id: &str) -> InputPermit {
        InputPermit {
            permit_id: permit_id.to_string(),
            action_id: action_id.to_string(),
            scope: scope(),
            execution_context_ref: "cu-run-1".to_string(),
            frozen_action_digest: "digest-a".to_string(),
            policy_revision: 4,
            gate_revision: 7,
            issued_owner_id: "owner-1".to_string(),
            issued_epoch: 3,
            expires_at_unix_ms: 10_000,
            executor_instance_id: None,
            state: InputPermitState::PendingActivation,
            revision: 1,
            revocation_reason: None,
        }
    }

    fn budget() -> PermitConsumptionBudget {
        PermitConsumptionBudget {
            gate_revision: 7,
            policy_revision: 4,
            held_epoch: 3,
        }
    }

    fn executor() -> ExecutorRegistration {
        ExecutorRegistration {
            executor_instance_id: "exec-1".to_string(),
            launch_operation_id: "launch-1".to_string(),
            coordinator_instance_id: "coordinator-1".to_string(),
            scope: scope(),
            host_launch_instance: "host-launch-1".to_string(),
            action_id: "action-1".to_string(),
            pid: 4242,
            creation_time_100ns: None,
            user_session: Some("session-1".to_string()),
            helper: HelperIdentity {
                host_process_path: "powershell.exe".to_string(),
                script_or_program_digest: "script-digest-a".to_string(),
            },
            protocol_version: 2,
            supervision_bound: false,
            state: ExecutorInstanceState::RegisteredPendingVerification,
            revision: 1,
        }
    }

    /// **DB-1**：现有受支持版本升级 ⇒ 版本与全部必要对象一致；
    /// 原 store ID、资源 scope、恢复操作**不变**。
    #[test]
    fn db1_upgrade_from_supported_version_preserves_legacy_facts() {
        let root = temp_root();
        let store_id = seed_v2_database(root.path());
        let upgraded = InputSafetyStore::open_at(root.path()).expect("升级必须成功");
        assert_eq!(upgraded.store_id().as_str(), store_id, "升级不得换身份");
        // 遗留恢复操作仍在，且**仍按"在办"读回**（不得把未知当已结账）。
        let legacy = upgraded
            .recovery_operation("recovery-legacy")
            .expect("read")
            .expect("旧行必须保留");
        assert!(!legacy.committed, "旧行不得被升级改写为已结账");
        // 资源 scope 与 revision 保持原值。
        let state = upgraded
            .resource_state(&scope())
            .expect("资源状态必须保留");
        assert_eq!(state.revision, 7, "gate revision 不得被迁移改动");
        assert_eq!(state.recovery_epoch, 3);
        // 版本推进到支持版本，且 v3 对象齐备可查询。
        let version: i64 = upgraded
            .connection_for_test()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, runtime::INPUT_SAFETY_SCHEMA_VERSION);
        let permits = PermitStore::new(upgraded.connection_for_test());
        assert!(permits.count_by_state(&scope()).expect("可查询").is_empty());
    }

    /// **DB-2**：同一新版本重复打开 ⇒ 不降版本、不重复迁移、不丢记录。
    #[test]
    fn db2_reopening_the_new_version_is_idempotent() {
        let root = temp_root();
        seed_v2_database(root.path());
        let first = InputSafetyStore::open_at(root.path()).expect("first");
        let events_after_first = first.event_count().expect("events");
        drop(first);
        let second = InputSafetyStore::open_at(root.path()).expect("second");
        assert_eq!(
            second.event_count().expect("events"),
            events_after_first,
            "重复打开不得再记一次迁移/初始化事件"
        );
        let version: i64 = second
            .connection_for_test()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, runtime::INPUT_SAFETY_SCHEMA_VERSION, "不得降版本");
        assert!(second
            .recovery_operation("recovery-legacy")
            .expect("read")
            .is_some(), "重复打开不得丢记录");
    }

    /// **DB-3**：迁移中途失败 ⇒ 不留下"版本已升级、必要对象未完成"的可接纳状态。
    ///
    /// 本用例用**真实的** `ensure_v3_objects` 与生产同一套事务纪律：在 `BEGIN IMMEDIATE` 之后
    /// 建 v3 对象、再故意执行一条失败语句，然后回滚；断言版本**仍为 2** 且 v3 对象**不存在**，
    /// 且随后真实迁移仍能干净通过（没有残留半套 schema）。
    #[test]
    fn db3_failed_migration_leaves_no_acceptable_partial_state() {
        let root = temp_root();
        seed_v2_database(root.path());
        let connection =
            Connection::open(root.path().join(INPUT_SAFETY_DB_FILE)).expect("raw connection");
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .expect("begin immediate");
        ensure_v3_objects(&connection).expect("v3 对象可建");
        // 注入失败：一条必然非法的语句（模拟迁移中途出错）。
        let failure = connection.execute_batch("THIS IS NOT SQL;");
        assert!(failure.is_err(), "注入的失败必须真的失败");
        connection.execute_batch("ROLLBACK").expect("rollback");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, 2, "失败的迁移不得推进版本");
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN
                     ('input_safety_permits', 'input_safety_executors')",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(tables, 0, "回滚后不得留下半套 v3 对象");
        // 随后真实迁移必须干净通过。
        let upgraded = InputSafetyStore::open_at(root.path()).expect("随后迁移必须成功");
        let version: i64 = upgraded
            .connection_for_test()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, runtime::INPUT_SAFETY_SCHEMA_VERSION);
    }

    /// **DB-4**：超前版本 ⇒ 明确拒绝；不得补空表后冒充兼容。
    #[test]
    fn db4_future_version_is_rejected_without_masquerading() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let store_id = store.store_id().as_str().to_string();
        drop(store);
        let connection =
            Connection::open(root.path().join(INPUT_SAFETY_DB_FILE)).expect("raw connection");
        let future = runtime::INPUT_SAFETY_SCHEMA_VERSION + 1;
        connection
            .execute_batch(&format!("PRAGMA user_version = {future};"))
            .expect("bump");
        drop(connection);
        let refused = InputSafetyStore::open_at(root.path());
        let Err(error) = refused else {
            panic!("超前版本必须被拒绝，而不是被接纳");
        };
        let text = format!("{error:?}");
        assert!(
            text.contains("SchemaFromTheFuture") || text.contains("超前"),
            "必须是明确的超前版本拒绝：{text}"
        );
        // 身份未被改动（拒绝不等于重建）。
        let connection =
            Connection::open(root.path().join(INPUT_SAFETY_DB_FILE)).expect("raw connection");
        let stored: String = connection
            .query_row(
                "SELECT store_id FROM input_safety_store_identity WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .expect("store id");
        assert_eq!(stored, store_id, "被拒绝时不得改动既有身份");
    }

    /// **DB-5**：两个进程同时打开待迁移库 ⇒ 只有受控迁移顺序，另一方等待或明确返回忙，
    /// **不跑半套 schema**。
    #[test]
    fn db5_concurrent_openers_do_not_run_half_a_schema() {
        let root = temp_root();
        seed_v2_database(root.path());
        let path = root.path().join(INPUT_SAFETY_DB_FILE);
        // 第一个连接持有写事务（模拟"正在迁移中"）。
        let holder = Connection::open(&path).expect("holder");
        holder
            .execute_batch("BEGIN IMMEDIATE")
            .expect("holder begins");
        // 第二个连接尝试迁移：必须**等待**（busy_timeout 生效），而不是跑半套 schema。
        let second = Connection::open(&path).expect("second");
        second
            .busy_timeout(std::time::Duration::from_millis(200))
            .expect("busy timeout");
        let blocked = second.execute_batch("BEGIN IMMEDIATE");
        assert!(
            blocked.is_err(),
            "另一个写事务在场时，第二个写事务必须被拒绝（不得并发迁移）"
        );
        // 放掉写事务后，第二个连接可以正常迁移到目标版本。
        holder.execute_batch("ROLLBACK").expect("holder rollback");
        let upgraded = InputSafetyStore::open_at(root.path()).expect("随后迁移成功");
        let version: i64 = upgraded
            .connection_for_test()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(version, runtime::INPUT_SAFETY_SCHEMA_VERSION);
    }

    /// **DB-6**（驱动真实 SQLite）：关闸与许可消费的竞争顺序。
    #[test]
    fn db6_close_versus_consume_order_is_determined_by_real_transactions() {
        // 情形一：关闸在先 ⇒ 消费必然失败，且许可被撤销。
        {
            let root = temp_root();
            let store = InputSafetyStore::open_at(root.path()).expect("open");
            let connection = store.connection_for_test();
            let permits = PermitStore::new(connection);
            permits
                .register_pending(&permit("permit-1", "action-1"), InputPermitState::PendingActivation, 1)
                .expect("register");
            let revoked = permits
                .close_intake_and_revoke_pending(&scope(), 8, 2)
                .expect("关闸");
            assert_eq!(revoked, vec!["permit-1".to_string()], "未消费许可必须被撤销");
            let outcome = permits.consume_permit(
                "permit-1",
                PermitConsumptionBudget { gate_revision: 8, ..budget() },
                PermitExecutorBinding::BindTo("exec-1"),
                3,
            );
            assert!(outcome.is_err(), "先关闸则消费必须失败");
            assert_eq!(
                permits.load_permit("permit-1").expect("load").state,
                InputPermitState::Revoked
            );
        }
        // 情形二：消费在先 ⇒ 属在途，关闸**不得**把它改写成"未发送"。
        {
            let root = temp_root();
            let store = InputSafetyStore::open_at(root.path()).expect("open");
            let connection = store.connection_for_test();
            let permits = PermitStore::new(connection);
            permits
                .register_pending(&permit("permit-1", "action-1"), InputPermitState::PendingActivation, 1)
                .expect("register");
            let consumed = permits
                .consume_permit(
                    "permit-1",
                    budget(),
                    PermitExecutorBinding::BindTo("exec-1"),
                    2,
                )
                .expect("先消费必须成功");
            assert_eq!(consumed, InputPermitState::DispatchCommitted);
            let revoked = permits
                .close_intake_and_revoke_pending(&scope(), 9, 3)
                .expect("关闸");
            assert!(revoked.is_empty(), "已消费的许可不得被关闸撤销");
            assert_eq!(
                permits.load_permit("permit-1").expect("load").state,
                InputPermitState::DispatchCommitted,
                "在途动作不得被改写成未发送"
            );
        }
    }

    /// **DB-7**（驱动真实 SQLite）：重复消费／重复对账不产生第二次执行资格。
    #[test]
    fn db7_duplicate_consume_and_reconcile_do_not_regrant_eligibility() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let permits = PermitStore::new(store.connection_for_test());
        permits
            .register_pending(&permit("permit-1", "action-1"), InputPermitState::PendingActivation, 1)
            .expect("register");
        permits
            .consume_permit("permit-1", budget(), PermitExecutorBinding::BindTo("exec-1"), 2)
            .expect("first consume");
        let again = permits.consume_permit(
            "permit-1",
            budget(),
            PermitExecutorBinding::BindTo("exec-1"),
            3,
        );
        assert!(
            matches!(again, Err(PermitStoreError::IllegalTransition(_))),
            "重复消费必须因状态已变而被拒（不产生第二次执行资格）：{again:?}"
        );
        // 同一 action 重复登记也必须被拒（不新建许可）。
        let duplicate = permits.register_pending(
            &permit("permit-2", "action-1"),
            InputPermitState::PendingActivation,
            4,
        );
        assert!(duplicate.is_err(), "同一 action 不得重复登记");

        // 重复对账：只有 OutcomeUnknown → Finished 一次；再来一次必然被拒。
        let connection = store.connection_for_test();
        connection
            .execute(
                "UPDATE input_safety_permits SET state = 'outcome_unknown' WHERE permit_id = 'permit-1'",
                [],
            )
            .expect("force unknown");
        permits
            .reconcile_outcome_unknown("permit-1", 5)
            .expect("首次对账");
        let second = permits.reconcile_outcome_unknown("permit-1", 6);
        assert!(
            matches!(second, Err(PermitStoreError::IllegalTransition(_))),
            "重复对账不得再次结清：{second:?}"
        );
    }

    /// **DB-8**：新表为空但旧事故未解决 ⇒ 资源仍隔离；
    /// "没有新许可记录"**不等于**"历史安全"。
    #[test]
    fn db8_empty_new_tables_do_not_erase_historical_isolation() {
        let root = temp_root();
        seed_v2_database(root.path());
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        // 新表为空。
        let permits = PermitStore::new(store.connection_for_test());
        assert!(
            permits.count_by_state(&scope()).expect("count").is_empty(),
            "新表为空是预期的"
        );
        // 但旧恢复操作仍在办、资源状态仍是隔离——不得因为"表是空的"而被当成安全。
        let legacy = store
            .recovery_operation("recovery-legacy")
            .expect("read")
            .expect("旧行必须仍在");
        assert!(!legacy.committed, "旧事故不得因新表为空而被清掉");
        let state = store
            .resource_state(&scope())
            .expect("资源状态必须仍在");
        assert_eq!(
            state.state,
            runtime::ResourceSafetyState::Isolated,
            "资源必须仍然隔离"
        );
        // 空表下消费不存在 ⇒ 明确拒绝，而不是"没有许可所以可以输入"。
        assert!(matches!(
            permits.consume_permit(
                "permit-missing",
                budget(),
                PermitExecutorBinding::BindTo("exec-1"),
                1
            ),
            Err(PermitStoreError::PermitNotFound { .. })
        ));
    }


    /// **使能步骤验收**：许可的签发与消费都能经**生产可达的窄口**（`store.permit_store()`）完成，
    /// 而不是只能在测试里自造连接。这条直接解掉 8.2c 的接线阻塞。
    #[test]
    fn permits_are_reachable_through_the_production_accessor() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        // 生产形态：从 store 拿适配器，而不是自己 Connection::open。
        let permits = store.permit_store();
        permits
            .register_pending(&permit("permit-1", "action-1"), InputPermitState::PendingActivation, 1)
            .expect("经窄口登记");
        let consumed = permits
            .consume_permit("permit-1", budget(), PermitExecutorBinding::BindTo("exec-1"), 2)
            .expect("经窄口消费");
        assert_eq!(consumed, InputPermitState::DispatchCommitted);

        // 关闸竞争在**同一条生产可达路径**上同样成立：先关闸 ⇒ 新的消费必失败。
        permits
            .register_pending(&permit("permit-2", "action-2"), InputPermitState::PendingActivation, 3)
            .expect("登记第二条");
        permits
            .close_intake_and_revoke_pending(&scope(), 8, 4)
            .expect("关闸");
        let refused = permits.consume_permit(
            "permit-2",
            PermitConsumptionBudget { gate_revision: 8, ..budget() },
            PermitExecutorBinding::BindTo("exec-1"),
            5,
        );
        assert!(refused.is_err(), "关闸在先时，经窄口的消费同样必须失败");

        // 执行者适配器同样可达。
        let executors = store.executor_store();
        executors
            .register_launch_intent(&executor(), 6)
            .expect("经窄口登记执行者意图");
        assert_eq!(
            executors.load_executor("exec-1").expect("load").state,
            ExecutorInstanceState::RegisteredPendingVerification
        );
    }

    /// 8.3b：创建顺序支持"先登记待核查、后取得实际实例证据"（§3.2），
    /// 且失败表处置由契约给出（不在 SQL 层另写一套）。
    #[test]
    fn executor_registration_follows_the_ruled_creation_order() {
        let root = temp_root();
        let store = InputSafetyStore::open_at(root.path()).expect("open");
        let executors = ExecutorStore::new(store.connection_for_test());
        // 第一步：登记启动意图——此时**没有**创建结果，也不需要占位 ID。
        executors
            .register_launch_intent(&executor(), 1)
            .expect("登记启动意图");
        let loaded = executors.load_executor("exec-1").expect("load");
        assert_eq!(
            loaded.state,
            ExecutorInstanceState::RegisteredPendingVerification
        );
        assert!(!loaded.supervision_bound, "登记时尚未绑定监督");
        assert!(
            loaded.creation_time_100ns.is_none(),
            "尚未取得实例证据时创建时间必须为空，而不是编造"
        );
        // 第二步：取得实际实例证据（匹配且存活）⇒ 契约处置是"先协作停止、必要时限定范围终止"。
        let disposition = executors
            .record_instance_evidence(
                "exec-1",
                ExecutorObservation::MatchesAndAlive,
                Some(133_000_000_000_000_000),
                true,
                2,
            )
            .expect("记录实例证据");
        assert_eq!(
            disposition,
            disposition_for(ExecutorObservation::MatchesAndAlive)
        );
        let loaded = executors.load_executor("exec-1").expect("reload");
        assert_eq!(loaded.state, ExecutorInstanceState::VerifiedAlive);
        assert!(loaded.supervision_bound, "核查通过前必须已绑定监督");
        assert_eq!(
            loaded.creation_time_100ns,
            Some(133_000_000_000_000_000),
            "创建时间来自实际实例证据"
        );
        // 失败表另一行：AccessDenied ⇒ 保持阻断的未知处置（不自动提权、不当不存在）。
        let denied = executors
            .record_instance_evidence(
                "exec-1",
                ExecutorObservation::AccessDeniedOrUnreadable,
                None,
                true,
                3,
            )
            .expect("记录未知");
        assert_eq!(
            denied,
            ExecutorDisposition::KeepBlockedAsUnknown,
            "AccessDenied 必须是 Unknown 且保持阻断"
        );
    }
}
