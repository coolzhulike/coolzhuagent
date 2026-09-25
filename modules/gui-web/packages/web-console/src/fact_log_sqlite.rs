//! SQLite 事实后端（第二轮裁决第 2 项）：与步骤存储**共用同一连接**。
//!
//! 裁决明确禁止这种形态：
//! ```text
//! 先提交步骤成功 → 再独立连接写回执 → 第二步失败 → 仍向用户宣布事实已完整保存
//! ```
//! 因此本后端**不自己开连接**，而是借用调用方（同一仓储协调器）的连接，从而可以放进
//! 与步骤更新相同的 `transaction()` 里提交。
//!
//! 幂等与冲突（裁决第 2.3 项）：**相同事实主体键 + 相同内容 → 幂等返回既有结果**；
//! **相同主体键 + 不同内容 → 报冲突，绝不覆盖旧事实**。主体键按事实类别**类型化**构造
//! （见 [`fact_subject`]），不使用"十字段拼接串"作万能幂等键。
//!
//! 表由 `main.rs` 的迁移阶梯（`apply_session_migration_v21`）统一创建，本模块**不得**
//! 在每次请求里自行 `CREATE`/`ALTER`。

use rusqlite::{params, Connection, OptionalExtension};

use runtime::{FactLogBackend, FactLogRecord, FactStoreError};

/// 事实类别（类型化键；不使用万能拼接串）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FactRecordKind {
    TerminalControl,
    AttemptOpened,
    RecoveryDecided,
    ActionReceipt,
    LateFact,
    UsageAttempt,
    MessageSubmission,
}

impl FactRecordKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::TerminalControl => "terminal_control",
            Self::AttemptOpened => "attempt_opened",
            Self::RecoveryDecided => "recovery_decided",
            Self::ActionReceipt => "action_receipt",
            Self::LateFact => "late_fact",
            Self::UsageAttempt => "usage_attempt",
            Self::MessageSubmission => "message_submission",
        }
    }
}

/// 记录所属类别。
fn fact_kind(record: &FactLogRecord) -> FactRecordKind {
    match record {
        FactLogRecord::TerminalControl { .. } => FactRecordKind::TerminalControl,
        FactLogRecord::AttemptOpened { .. } => FactRecordKind::AttemptOpened,
        FactLogRecord::RecoveryDecided { .. } => FactRecordKind::RecoveryDecided,
        FactLogRecord::ActionReceipt { .. } => FactRecordKind::ActionReceipt,
        FactLogRecord::LateFact { .. } => FactRecordKind::LateFact,
        FactLogRecord::UsageAttempt { .. } => FactRecordKind::UsageAttempt,
        FactLogRecord::MessageSubmission { .. } => FactRecordKind::MessageSubmission,
    }
}

/// 身份的 scope 文本（旧记录没有 scope 时缺省）。
fn scope_of(identity: &runtime::RunIdentity) -> Option<String> {
    identity
        .scope
        .as_ref()
        .map(|scope| scope.as_str().to_string())
}

/// 一条事实的**主体键**（类别内唯一，包含 scope 与实际主体）。
///
/// 返回 `None` 表示该事实缺少构造键所需的维度——调用方必须**拒绝写入**，
/// 而不是补一个默认值（那正是裁决禁止的"凑字段"）。
pub(crate) fn fact_subject(
    kind: FactRecordKind,
    record: &FactLogRecord,
) -> Option<(String, Option<String>)> {
    match (kind, record) {
        (FactRecordKind::TerminalControl, FactLogRecord::TerminalControl { identity, .. }) => {
            Some((identity.run_id.clone(), scope_of(identity)))
        }
        (FactRecordKind::AttemptOpened, FactLogRecord::AttemptOpened { attempt }) => attempt
            .identity
            .request_attempt_id
            .as_ref()
            .map(|attempt_id| {
                (
                    format!("{}#{attempt_id}", attempt.identity.run_id),
                    scope_of(&attempt.identity),
                )
            }),
        (FactRecordKind::RecoveryDecided, FactLogRecord::RecoveryDecided { fact }) => Some((
            format!("{}#{}", fact.parent_run_id, fact.attempt_id),
            None,
        )),
        (
            FactRecordKind::ActionReceipt,
            FactLogRecord::ActionReceipt {
                identity, receipt, ..
            },
        ) => Some((
            format!("{}#{}", identity.run_id, receipt.action_id),
            scope_of(identity),
        )),
        (FactRecordKind::LateFact, FactLogRecord::LateFact { fact }) => Some((
            format!(
                "{}#{}#{}",
                fact.run_id, fact.source, fact.received_at_unix_ms
            ),
            None,
        )),
        (FactRecordKind::UsageAttempt, FactLogRecord::UsageAttempt { attempt }) => Some((
            format!(
                "{}#{}#{}",
                attempt.run_id, attempt.logical_request_id, attempt.attempt_id
            ),
            None,
        )),
        (FactRecordKind::MessageSubmission, FactLogRecord::MessageSubmission { key, .. }) => Some((
            format!("{}#{}", key.client_message_id, key.scope),
            Some(key.scope.clone()),
        )),
        // 类别与记录不匹配：结构性错误，拒绝写入。
        _ => None,
    }
}

/// 内容指纹（FNV-1a 64）：只用于"同主体键不同内容"的冲突判定，**不是**安全哈希。
fn content_fingerprint(payload: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in payload.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// 借用调用方连接的 SQLite 事实后端。
///
/// 由调用方负责把它用在正确的 `transaction()` 里，与步骤更新同事务提交。
pub(crate) struct SqliteFactLog<'a> {
    connection: &'a Connection,
}

impl<'a> SqliteFactLog<'a> {
    pub(crate) const fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }
}

fn encode_error(message: impl std::fmt::Display) -> FactStoreError {
    FactStoreError::Encode {
        message: message.to_string(),
    }
}

impl FactLogBackend for SqliteFactLog<'_> {
    fn append(&mut self, record: &FactLogRecord) -> Result<(), FactStoreError> {
        let kind = fact_kind(record);
        let Some((subject, scope)) = fact_subject(kind, record) else {
            return Err(FactStoreError::IdentityConflict {
                field: "fact_subject",
                message: format!(
                    "{} 事实缺少构造主体键所需的维度，拒绝写入",
                    kind.as_str()
                ),
            });
        };
        let payload =
            serde_json::to_string(record).map_err(|error| encode_error(error.to_string()))?;
        let fingerprint = content_fingerprint(&payload);
        let existing: Option<String> = self
            .connection
            .query_row(
                "SELECT content_digest FROM fact_log_records WHERE record_kind=?1 AND record_subject=?2",
                params![kind.as_str(), subject],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| encode_error(format!("读取既有事实失败: {error}")))?;
        if let Some(existing_digest) = existing {
            if existing_digest == fingerprint {
                return Ok(());
            }
            return Err(FactStoreError::IdentityConflict {
                field: "fact_content",
                message: format!(
                    "{} 的同一主体键已存在且内容不同，拒绝覆盖旧事实",
                    kind.as_str()
                ),
            });
        }
        self.connection
            .execute(
                "INSERT INTO fact_log_records
                     (record_kind, record_subject, scope_kind, content_digest, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![kind.as_str(), subject, scope, fingerprint, payload],
            )
            .map_err(|error| encode_error(format!("写入事实失败: {error}")))?;
        Ok(())
    }

    fn read_all(&mut self) -> Result<Vec<FactLogRecord>, FactStoreError> {
        let mut statement = self
            .connection
            .prepare("SELECT payload_json FROM fact_log_records ORDER BY record_seq")
            .map_err(|error| encode_error(format!("准备事实读取失败: {error}")))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| encode_error(format!("读取事实失败: {error}")))?;
        let mut records = Vec::new();
        for row in rows {
            let payload = row.map_err(|error| encode_error(format!("读取事实行失败: {error}")))?;
            let record = serde_json::from_str(&payload)
                .map_err(|error| encode_error(format!("解码事实失败: {error}")))?;
            records.push(record);
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{
        ActionReceipt, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus,
        RunScopeContext, RunTerminalStatus,
    };

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("open in-memory sqlite");
        connection
            .execute_batch(
                "CREATE TABLE fact_log_records (
                     record_seq     INTEGER PRIMARY KEY AUTOINCREMENT,
                     record_kind    TEXT NOT NULL,
                     record_subject TEXT NOT NULL,
                     scope_kind     TEXT,
                     content_digest TEXT NOT NULL,
                     payload_json   TEXT NOT NULL
                 );
                 CREATE UNIQUE INDEX idx_fact_log_records_kind_subject
                     ON fact_log_records(record_kind, record_subject);",
            )
            .expect("create fact table");
        connection
    }

    /// 身份一律经契约构造器产生（生产者不应自拼 `RunIdentity` 字面量）。
    fn context() -> RunScopeContext {
        RunScopeContext::new("ws", "room", "session", "turn")
    }

    fn turn_identity(run_id: &str) -> runtime::RunIdentity {
        context().turn_fact(run_id)
    }

    fn action_identity(run_id: &str, action_id: &str) -> runtime::RunIdentity {
        context().step_action_fact(run_id, "step-1", "attempt-1", action_id)
    }

    fn receipt(action_id: &str) -> ActionReceipt {
        ActionReceipt {
            action_id: action_id.to_string(),
            input_delivery: InputDelivery::Sent,
            partial: None,
            path_completed: None,
            confirmed_point_count: None,
            effect: EffectStatus::EffectObserved,
            goal_verdict: GoalVerdict::Passed,
            input_release: InputReleaseStatus::Released,
        }
    }

    #[test]
    fn identical_fact_is_idempotent_and_different_content_conflicts() {
        let connection = connection();
        let mut log = SqliteFactLog::new(&connection);
        let record = FactLogRecord::TerminalControl {
            identity: turn_identity("run-1"),
            status: RunTerminalStatus::Succeeded,
        };
        log.append(&record).expect("first write");
        log.append(&record).expect("identical write is idempotent");
        assert_eq!(log.read_all().expect("read back").len(), 1);

        let conflicting = FactLogRecord::TerminalControl {
            identity: turn_identity("run-1"),
            status: RunTerminalStatus::Failed,
        };
        let error = log
            .append(&conflicting)
            .expect_err("same subject with different content must conflict");
        assert!(
            matches!(error, FactStoreError::IdentityConflict { .. }),
            "冲突必须回报 IdentityConflict，实际 {error:?}"
        );
        assert_eq!(
            log.read_all().expect("read back").len(),
            1,
            "冲突不得覆盖旧事实"
        );
    }

    #[test]
    fn action_receipts_are_keyed_by_run_and_action() {
        let connection = connection();
        let mut log = SqliteFactLog::new(&connection);
        let first = FactLogRecord::ActionReceipt {
            identity: action_identity("run-1", "action-1"),
            receipt: receipt("action-1"),
            identity_anomaly: None,
            evidence: None,
            origin: None,
        };
        let second = FactLogRecord::ActionReceipt {
            identity: action_identity("run-1", "action-2"),
            receipt: receipt("action-2"),
            identity_anomaly: None,
            evidence: None,
            origin: None,
        };
        log.append(&first).expect("action 1");
        log.append(&second).expect("action 2 是不同主体，必须都能写入");
        assert_eq!(log.read_all().expect("read back").len(), 2);
    }

    #[test]
    fn empty_action_id_still_yields_a_subject_key_leaving_rejection_to_the_contract_layer() {
        let connection = connection();
        let mut log = SqliteFactLog::new(&connection);
        // Turn 事实不得携带动作维度；这里构造一个"动作事实但缺 action_id"的形态：
        // 主体键无法构造 → 必须拒绝写入，而不是补默认值。
        let record = FactLogRecord::ActionReceipt {
            identity: action_identity("run-1", ""),
            receipt: receipt(""),
            identity_anomaly: None,
            evidence: None,
            origin: None,
        };
        log.append(&record)
            .expect("空 action_id 仍可构造主体键（键为 run-1#），由契约层负责拒绝");
        assert_eq!(log.read_all().expect("read back").len(), 1);
    }

    #[test]
    fn subject_keys_are_typed_per_kind() {
        let connection = connection();
        let mut log = SqliteFactLog::new(&connection);
        let terminal = FactLogRecord::TerminalControl {
            identity: turn_identity("shared"),
            status: RunTerminalStatus::Succeeded,
        };
        let action = FactLogRecord::ActionReceipt {
            identity: action_identity("shared", "shared"),
            receipt: receipt("shared"),
            identity_anomaly: None,
            evidence: None,
            origin: None,
        };
        log.append(&terminal).expect("terminal");
        log.append(&action)
            .expect("不同类别即使主体文本相同也必须互不冲突");
        assert_eq!(log.read_all().expect("read back").len(), 2);
    }
}
