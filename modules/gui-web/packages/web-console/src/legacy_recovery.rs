//! RD4-03 的语义基础：遗留 CU 运行的 **owner 真实关系解析** 与 **资源不确定性评估**
//! （第八轮 §4 组合用例 4／5；权威正文见 `round8-rulings-and-execution-order.md`）。
//!
//! 两条硬约束来自裁决：
//!
//! 1. **owner 只能经真实关系解析**：`turn_id`/`session_id` → `runtime_runs` → `owner`。
//!    **查不到映射、查到多个候选、旧运行缺字段 ⇒ 都是"未知"**，不是"没有 owner"；
//!    **不得把 `goal_id` 复制成 `chat_turn_id`**（Goal 运行只作其**真实执行锚点**）。
//! 2. **`Interrupted` ≠ 释放安全**：旧运行不再执行，不等于输入资源已安全。
//!    没有**独立证明**时必须表达 `run stopped / resource uncertain / input blocked`，
//!    并**保持隔离**（建事故 + 开阻断；R9 不得重新开放新输入）。

use std::path::Path;

use rusqlite::Connection;

use crate::computer_use_store::ComputerUseRunStore;

/// owner 解析结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyRunOwner {
    /// 经真实关系解析到**唯一**候选（`runtime_runs` 行）。
    Resolved {
        runtime_run_id: String,
        owner_id: Option<String>,
        kind: String,
    },
    /// **未知**（不是"没有 owner"）。三种成因分别可辨，便于审计与纠错。
    Unknown { reason: LegacyRunOwnerUnknown },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyRunOwnerUnknown {
    /// 旧运行缺 `session_id`/`turn_id`：无法发起解析。
    MissingFields,
    /// 查不到任何映射。
    NoMapping,
    /// 查到多个候选：不得任选一个。
    MultipleCandidates,
}

impl LegacyRunOwnerUnknown {
    #[must_use]
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::MissingFields => "legacy_run_owner_missing_fields",
            Self::NoMapping => "legacy_run_owner_no_mapping",
            Self::MultipleCandidates => "legacy_run_owner_multiple_candidates",
        }
    }
}

/// 经 `runtime_runs` 的**真实关系**解析遗留 CU run 的 owner。
///
/// 只读；只按 `(session_id, legacy_turn_id)` 查，**不**把任何一种 id 复制到另一种字段里。
#[must_use]
pub(crate) fn resolve_legacy_run_owner(
    db_path: &Path,
    session_id: &str,
    turn_id: &str,
) -> LegacyRunOwner {
    if session_id.trim().is_empty() || turn_id.trim().is_empty() {
        return LegacyRunOwner::Unknown {
            reason: LegacyRunOwnerUnknown::MissingFields,
        };
    }
    if !db_path.exists() {
        return LegacyRunOwner::Unknown {
            reason: LegacyRunOwnerUnknown::NoMapping,
        };
    }
    let Ok(connection) = Connection::open_with_flags(
        db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return LegacyRunOwner::Unknown {
            reason: LegacyRunOwnerUnknown::NoMapping,
        };
    };
    let Ok(mut statement) = connection.prepare(
        "SELECT id, kind, owner_id FROM runtime_runs
         WHERE legacy_turn_id = ?1 AND session_id = ?2
         ORDER BY created_at, id",
    ) else {
        return LegacyRunOwner::Unknown {
            reason: LegacyRunOwnerUnknown::NoMapping,
        };
    };
    let rows = statement.query_map(rusqlite::params![turn_id, session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    });
    let Ok(rows) = rows else {
        return LegacyRunOwner::Unknown {
            reason: LegacyRunOwnerUnknown::NoMapping,
        };
    };
    let candidates: Vec<(String, String, Option<String>)> =
        rows.filter_map(Result::ok).collect();
    match candidates.as_slice() {
        [] => LegacyRunOwner::Unknown {
            reason: LegacyRunOwnerUnknown::NoMapping,
        },
        [single] => LegacyRunOwner::Resolved {
            runtime_run_id: single.0.clone(),
            kind: single.1.clone(),
            owner_id: single.2.clone(),
        },
        _ => LegacyRunOwner::Unknown {
            reason: LegacyRunOwnerUnknown::MultipleCandidates,
        },
    }
}


/// 事实日志里**是否已有该运行的终态控制事实**（= 已有真实终态提交或待确认提交的候选）。
///
/// 收敛必须拒绝覆盖它（RD4-01 的 `CommitCandidatePending`），所以这个检查是收敛的前置输入。
/// **只读**；无库、无表、无该事实一律返回 `false`——"没有事实日志"是"没有任何提交候选"的
/// 真实情形（v21 之前根本没有这张表），不是"无法判定"。
#[must_use]
pub(crate) fn has_terminal_commit_candidate(session_db_path: &Path, run_id: &str) -> bool {
    if run_id.trim().is_empty() || !session_db_path.exists() {
        return false;
    }
    let Ok(connection) = Connection::open_with_flags(
        session_db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return false;
    };
    let has_table: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='fact_log_records')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .unwrap_or(false);
    if !has_table {
        return false;
    }
    connection
        .query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM fact_log_records
                  WHERE record_kind = 'terminal_control'
                    AND (record_subject = ?1 OR record_subject LIKE ?1 || '#%')
             )",
            rusqlite::params![run_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value != 0)
        .unwrap_or(false)
}

/// 资源不确定性评估（组合用例 4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyResourceAssessment {
    /// 有**独立证明**且没有未结清的释放义务 ⇒ 允许在 R9 重新开放（仍需无未关闭阻断）。
    Safe,
    /// **未知**：必须保持隔离、不得表达为安全、不得开放新输入。
    Uncertain { reason: String },
}

impl LegacyResourceAssessment {
    #[must_use]
    pub(crate) fn is_safe(&self) -> bool {
        matches!(self, Self::Safe)
    }

    #[must_use]
    pub(crate) fn describe(&self) -> String {
        match self {
            Self::Safe => "run stopped / resource proven safe / input may reopen".to_string(),
            // 裁决要求的表达：run 已停止，但资源不确定，因此输入保持阻断。
            Self::Uncertain { reason } => {
                format!("run stopped / resource uncertain / input blocked（{reason}）")
            }
        }
    }
}

/// 评估遗留运行的资源状态。
///
/// 规则（裁决 §4 组合用例 4）：
/// - 该 scope 仍有**未确认的释放义务** ⇒ `Uncertain`（资源阻断**优先**，不看别的证据）；
/// - 否则必须由调用方提供**独立**的安全检查（`runtime::CurrentResourceSafetyCheck`）才可能 `Safe`；
/// - 拿不出独立证明 ⇒ `Uncertain`（缺证据不是安全）。
pub(crate) fn assess_legacy_run_resource(
    store: &ComputerUseRunStore,
    session_id: &str,
    turn_id: &str,
    independent_check: Option<&runtime::CurrentResourceSafetyCheck>,
) -> LegacyResourceAssessment {
    match store.unconfirmed_release_facts(session_id, turn_id) {
        Ok(facts) if !facts.is_empty() => LegacyResourceAssessment::Uncertain {
            reason: format!(
                "该 scope 仍有 {} 个运行、{} 条步骤未确认释放：资源阻断优先于任何其它证据",
                facts.run_ids.len(),
                facts.step_count
            ),
        },
        Err(error) => LegacyResourceAssessment::Uncertain {
            reason: format!("无法核对未确认释放义务：{error}"),
        },
        Ok(_) => match independent_check {
            Some(_) => LegacyResourceAssessment::Safe,
            None => LegacyResourceAssessment::Uncertain {
                reason: "缺少独立资源安全检查：旧运行不再执行不构成释放安全的证据".to_string(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_use_store::{seed_legacy_unrecorded_run_for_test, ComputerUseRunStore};

    fn seeded_db() -> (tempfile::TempDir, std::path::PathBuf) {
        let directory = tempfile::TempDir::new().expect("tempdir");
        let db_path = directory.path().join("web-sessions.sqlite3");
        {
            let connection = crate::open_session_connection(&db_path).expect("open");
            crate::initialize_session_schema(&connection).expect("schema");
        }
        (directory, db_path)
    }

    /// **组合用例 5**：owner 只经真实关系解析；三种"未知"分别可辨，且**不复制 ID**。
    #[test]
    fn combined_5_owner_resolution_uses_real_relations_only() {
        let (_directory, db_path) = seeded_db();

        // ① 缺字段 ⇒ MissingFields（不是"没有 owner"）。
        assert_eq!(
            resolve_legacy_run_owner(&db_path, "", "turn-1"),
            LegacyRunOwner::Unknown {
                reason: LegacyRunOwnerUnknown::MissingFields
            }
        );
        // ② 查不到映射 ⇒ NoMapping。
        assert_eq!(
            resolve_legacy_run_owner(&db_path, "session-missing", "turn-missing"),
            LegacyRunOwner::Unknown {
                reason: LegacyRunOwnerUnknown::NoMapping
            }
        );
        // ③ 唯一候选（chat_turn）⇒ Resolved，且 owner 来自运行行本身。
        crate::create_chat_runtime_run_sqlite(
            &db_path,
            "run-chat-1",
            "claim",
            "ws-0123456789abcdef",
            Some("session-1"),
            "room-1",
            "turn-1",
        )
        .expect("seed chat run");
        assert_eq!(
            resolve_legacy_run_owner(&db_path, "session-1", "turn-1"),
            LegacyRunOwner::Resolved {
                runtime_run_id: "run-chat-1".to_string(),
                kind: "chat_turn".to_string(),
                owner_id: Some("coolzhu-web-console".to_string()),
            }
        );
        // ④ 当前 schema **保证** `legacy_turn_id` 全局唯一（`idx_runtime_runs_legacy_turn`
        //    的谓词只有 `legacy_turn_id IS NOT NULL`，不限 kind）：因此"多候选"在现有
        //    数据模型下**不可达**，`MultipleCandidates` 属**防御性分支**，此处以断言这一
        //    结构事实代替伪造场景。
        let second = crate::create_chat_runtime_run_sqlite(
            &db_path,
            "run-chat-2",
            "claim",
            "ws-0123456789abcdef",
            Some("session-1"),
            "room-1",
            "turn-1",
        );
        assert!(second.is_err(), "同一 legacy_turn_id 不得有第二个运行行（唯一索引）");

        // ⑤ Goal 锚点：Goal 运行**可以**是真实执行锚点，但若 CU run 的字段无法推出该关系
        //    （如 goal_phase 行的 `legacy_turn_id` 为空），结论是**未知**而**不是**"没有 owner"。
        assert_eq!(
            resolve_legacy_run_owner(&db_path, "session-1", "turn-of-goal-phase"),
            LegacyRunOwner::Unknown {
                reason: LegacyRunOwnerUnknown::NoMapping
            },
            "缺映射一律记未知；不得把缺映射当无 owner，也不得复制 goal_id 冒充 turn"
        );
    }

    /// **组合用例 4**：`Interrupted` ≠ 释放安全——缺独立证明 ⇒ `Uncertain` 且表达"输入保持阻断"。
    #[test]
    fn combined_4_interrupted_run_does_not_imply_release_safety() {
        let directory = tempfile::TempDir::new().expect("tempdir");
        let db_path = directory.path().join("web-sessions.sqlite3");
        {
            let connection = crate::open_session_connection(&db_path).expect("open");
            crate::initialize_session_schema(&connection).expect("schema");
        }
        let store = ComputerUseRunStore::open(&db_path).expect("store");
        // 一条遗留运行：NULL 归属、未收尾（正是本轮要收敛的对象）。
        seed_legacy_unrecorded_run_for_test(&store, "legacy-cu-1", "session-1", "turn-1", true);

        // ① 没有独立安全检查 ⇒ Uncertain，且措辞必须同时表达"运行已停止 / 资源不确定 / 输入阻断"。
        let assessment = assess_legacy_run_resource(&store, "session-1", "turn-1", None);
        assert!(!assessment.is_safe(), "不得把'旧运行不再执行'当成释放安全");
        let text = assessment.describe();
        assert!(text.contains("run stopped"), "{text}");
        assert!(text.contains("resource uncertain"), "{text}");
        assert!(text.contains("input blocked"), "{text}");

        // ② 该 scope 存在**未确认的释放义务**时：即使调用方给出独立检查，也必须 Uncertain
        //    （裁决：资源阻断**优先**于任何其它证据）。
        let connection = crate::open_session_connection(&db_path).expect("open");
        connection
            .execute_batch(
                "INSERT INTO computer_use_runs(
                     call_id, provider_tool_call_id, turn_id, session_id, chat_room_id,
                     idempotency_key, objective_json, surface, state, state_version,
                     deadline_ms, created_at_ms, updated_at_ms, workspace_id, workspace_context_version
                 ) VALUES ('cu-release-pending', 'toolu-1', 'turn-1', 'session-1', 'room-1',
                     'idem-1', '{\"objective\":\"x\"}', 'desktop', 'executing', 1,
                     60000, 1, 1, 'ws-0123456789abcdef', 1);
                 INSERT INTO computer_use_steps(
                     run_id, step_index, observation_generation, action_type, normalized_target,
                     action_fingerprint, status, input_delivery, input_release_status, started_at_ms
                 ) VALUES ('cu-release-pending', 0, 0, 'click', 'target', 'fp', 'failed', 'sent', 'unknown', 1);",
            )
            .expect("seed unconfirmed release obligation");
        drop(connection);

        // 有独立检查也**不得**变成 Safe：未确认释放优先。
        let check = runtime::CurrentResourceSafetyCheck {
            basis: runtime::CurrentResourceCandidateBasis::CurrentMayBeAffectedNotHistoricalScope,
            session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            checked_at_unix_ms: 1,
        };
        let assessment = assess_legacy_run_resource(&store, "session-1", "turn-1", Some(&check));
        assert!(!assessment.is_safe(), "资源阻断优先于独立检查");
        assert!(assessment.describe().contains("未确认释放"), "{}", assessment.describe());
    }

    /// 提交候选检查：事实日志里已有该运行的 `terminal_control` ⇒ 必须判为"有候选"
    /// （否则收敛会覆盖真实终态，正是 RD4-01 的 `CommitCandidatePending` 要防的事）。
    #[test]
    fn terminal_commit_candidate_is_detected_from_the_fact_log() {
        let (_directory, db_path) = seeded_db();
        assert!(
            !has_terminal_commit_candidate(&db_path, "legacy-cu-1"),
            "没有事实时应为 false"
        );
        {
            let connection = crate::open_session_connection(&db_path).expect("open");
            // v21 的事实日志表由迁移创建；这里只写一条最小行验证**读取判据**。
            connection
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS fact_log_records (
                         record_seq     INTEGER PRIMARY KEY AUTOINCREMENT,
                         record_kind    TEXT NOT NULL,
                         record_subject TEXT NOT NULL,
                         scope_kind     TEXT,
                         content_digest TEXT NOT NULL,
                         payload_json   TEXT NOT NULL
                     );
                     CREATE UNIQUE INDEX IF NOT EXISTS idx_fact_log_records_kind_subject
                         ON fact_log_records(record_kind, record_subject);
                     INSERT INTO fact_log_records (record_kind, record_subject, content_digest, payload_json)
                     VALUES ('terminal_control', 'legacy-cu-1', 'digest', '{}');",
                )
                .expect("seed fact");
        }
        assert!(
            has_terminal_commit_candidate(&db_path, "legacy-cu-1"),
            "已有终态控制事实 ⇒ 必须判为有提交候选"
        );
    }
}
