//! 显式恢复 / 新尝试的身份契约（§2.1）。
//!
//! 规范原文：
//!
//! > 显式恢复/新尝试 | 新 recovery/attempt 身份，保留父关联、旧效果事实及剩余预算；
//! > 不能命中旧失败缓存而永远无法恢复，也不能靠换 ID 重置预算
//!
//! 两条禁令对应代码里两个真实存在的东西：
//!
//! 1. **旧失败缓存**：`computer-use-core` 的 `TurnComputerUseSupervisor` 用
//!    `TaskIdempotencyKey` 做 `terminal_cache`，`before_run` 命中就 `ReturnCached`。
//!    若恢复沿用父尝试的 key，就永远拿到同一个失败终态——所以恢复必须换身份。
//! 2. **换 ID 重置预算**：`RunBudget` 给出 deadline 与各项上限。若恢复带着一个
//!    "更宽"的预算（更晚的 deadline 或更大的上限），等于用换 ID 绕过原 run 的约束。
//!
//! 本模块把这两条做成可回归的纯逻辑；**尚未接线**，接线属 S2 的迁移工作。

use crate::run_contract::RunBudget;
use serde::{Deserialize, Serialize};

/// 派生恢复身份时被拒绝的原因。
///
/// RPR-03 追加：`Serialize`/`Deserialize` 派生只为让"拒绝决定"能被原样持久化
/// （事实日志要记的是**最终决定**，不是一句自由文本），语义未变。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryError {
    /// 恢复沿用了父尝试的身份：会命中旧终态缓存，恢复无法真正开始。
    SameAttemptId,
    /// 恢复的剩余预算比父尝试**更宽**：等于靠换 ID 重置预算。
    BudgetNotCarriedOver,
}

impl RecoveryError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::SameAttemptId => "recovery_attempt_id_reused",
            Self::BudgetNotCarriedOver => "recovery_budget_widened",
        }
    }
}

/// 一次显式恢复 / 新尝试的身份与预算。
///
/// 它**必须是新身份**（`attempt_id != parent_attempt_id`），同时保留父关联
/// （`parent_attempt_id` / `parent_run_id`）与从父结转的剩余预算。
///
/// RPR-03 追加：`Serialize`/`Deserialize` 派生只为让**同一类型**可被事实日志持久化。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryAttempt {
    pub attempt_id: String,
    pub parent_attempt_id: String,
    pub parent_run_id: String,
    /// 从父尝试结转后的剩余预算；任何维度都不得比父更宽。
    pub remaining_budget: RunBudget,
}

impl RecoveryAttempt {
    /// 由父尝试派生一次恢复。
    ///
    /// `parent_budget` 是父尝试**当时**的预算（用于比较是否被放宽）。
    /// 收紧预算是允许的（例如上层要求更少动作），放宽一律拒绝。
    pub fn derive(
        parent_attempt_id: &str,
        parent_run_id: &str,
        attempt_id: impl Into<String>,
        remaining_budget: RunBudget,
        parent_budget: RunBudget,
    ) -> Result<Self, RecoveryError> {
        let attempt_id = attempt_id.into();
        if attempt_id == parent_attempt_id {
            return Err(RecoveryError::SameAttemptId);
        }
        if !budget_is_within(remaining_budget, parent_budget) {
            return Err(RecoveryError::BudgetNotCarriedOver);
        }
        Ok(Self {
            attempt_id,
            parent_attempt_id: parent_attempt_id.to_string(),
            parent_run_id: parent_run_id.to_string(),
            remaining_budget,
        })
    }

    /// 是否是合法的"新身份"（与父不同）。
    #[must_use]
    pub fn has_new_identity(&self) -> bool {
        self.attempt_id != self.parent_attempt_id
    }

    /// 在给定时刻是否仍可用（单向 deadline 未过期）。
    #[must_use]
    pub fn is_usable_at(&self, now_unix_ms: u64) -> bool {
        !self.remaining_budget.is_expired_at(now_unix_ms)
    }
}

/// `candidate` 是否**不宽于** `limit`：deadline 不更晚，各项上限不更大。
fn budget_is_within(candidate: RunBudget, limit: RunBudget) -> bool {
    candidate.deadline_unix_ms <= limit.deadline_unix_ms
        && candidate.max_actions <= limit.max_actions
        && candidate.max_replans <= limit.max_replans
        && candidate.max_request_attempts <= limit.max_request_attempts
}

#[cfg(test)]
mod tests {
    use super::{RecoveryAttempt, RecoveryError};
    use crate::run_contract::RunBudget;

    fn budget(deadline: u64, actions: u32, replans: u32, attempts: u32) -> RunBudget {
        RunBudget {
            deadline_unix_ms: deadline,
            max_actions: actions,
            max_replans: replans,
            max_request_attempts: attempts,
        }
    }

    /// 正常派生：换身份 + 结转（收紧）预算。
    #[test]
    fn recovery_with_a_new_identity_and_tightened_budget_is_allowed() {
        let parent = budget(10_000, 4, 2, 3);
        let derived = RecoveryAttempt::derive(
            "attempt-0",
            "run-1",
            "attempt-1",
            budget(9_000, 2, 1, 2),
            parent,
        )
        .expect("a new identity with a carried-over budget must be allowed");

        assert!(derived.has_new_identity());
        assert_eq!(derived.parent_attempt_id, "attempt-0");
        assert_eq!(derived.parent_run_id, "run-1");
        assert!(derived.is_usable_at(8_999));
        assert!(!derived.is_usable_at(9_000));
    }

    /// 沿用父身份被拒绝：那会命中旧终态缓存，恢复永远无法开始。
    #[test]
    fn reusing_the_parent_attempt_id_is_rejected() {
        let parent = budget(10_000, 4, 2, 3);
        assert_eq!(
            RecoveryAttempt::derive("attempt-0", "run-1", "attempt-0", parent, parent),
            Err(RecoveryError::SameAttemptId)
        );
        assert_eq!(
            RecoveryError::SameAttemptId.code(),
            "recovery_attempt_id_reused"
        );
    }

    /// 放宽 deadline 被拒绝：等于靠换 ID 重置预算。
    #[test]
    fn extending_the_deadline_is_rejected() {
        let parent = budget(10_000, 4, 2, 3);
        assert_eq!(
            RecoveryAttempt::derive(
                "attempt-0",
                "run-1",
                "attempt-1",
                budget(20_000, 4, 2, 3),
                parent
            ),
            Err(RecoveryError::BudgetNotCarriedOver)
        );
    }

    /// 放大任一上限同样被拒绝。
    #[test]
    fn inflating_any_upper_bound_is_rejected() {
        let parent = budget(10_000, 4, 2, 3);
        for wider in [
            budget(10_000, 5, 2, 3),
            budget(10_000, 4, 3, 3),
            budget(10_000, 4, 2, 4),
        ] {
            assert_eq!(
                RecoveryAttempt::derive("attempt-0", "run-1", "attempt-1", wider, parent),
                Err(RecoveryError::BudgetNotCarriedOver),
                "widening {wider:?} must be refused"
            );
        }
        assert_eq!(
            RecoveryError::BudgetNotCarriedOver.code(),
            "recovery_budget_widened"
        );
    }

    /// 预算恰好等于父尝试（完全结转、不收紧）是允许的。
    #[test]
    fn exactly_carrying_over_the_parent_budget_is_allowed() {
        let parent = budget(10_000, 4, 2, 3);
        assert!(RecoveryAttempt::derive("attempt-0", "run-1", "attempt-1", parent, parent).is_ok());
    }
}
