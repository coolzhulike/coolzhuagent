//! 迟到事实的信封与追加契约（§2.1）。
//!
//! 规范原文：
//!
//! > 终态 first-wins 仅指控制状态。取消、超时后到达的事实追加到原 run，标明迟到、
//! > 来源和接收时间；UI 不复活运行，统计不丢已发生的输入和计费用量。
//!
//! 已有的是 `RunTerminalStatus::accepts_late_facts()` 这个**判定**（恒为 true，且有
//! `terminal_control_never_rejects_late_facts_and_budget_has_single_deadline` 覆盖）。
//! 缺的是"追加"这件事本身的承载物：事实必须**带来源与接收时间**、必须**标明迟到**，
//! 且追加**不得改变控制终态**（不复活运行），计费/输入类事实**不得被丢弃**。
//!
//! 本模块把它做成可回归的纯逻辑；**尚未接线**（接线属 S2 的迁移工作）。

use crate::run_contract::RunTerminalStatus;
use serde::{Deserialize, Serialize};

/// 迟到事实的种类。分成种类是为了让"不丢已发生的输入和计费用量"可以被断言，
/// 而不是一句口号。
///
/// RPR-03 追加：`Serialize`/`Deserialize` 派生只为让**同一类型**可被事实日志持久化，
/// 不改变任何既有语义（没有新增字段、没有默认值补齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LateFactKind {
    /// 输入类事实（例如事后才确认的 `sent` / 释放）。
    Input,
    /// 观察/截图类事实。
    Capture,
    /// 用量与计费类事实——**不得被丢弃**。
    Usage,
    /// 验收结论类事实。
    Verdict,
}

/// 一条迟到事实的信封。
///
/// RPR-03 追加：`Serialize`/`Deserialize` 派生只为持久化**同一类型**。注意
/// `received_at_unix_ms == 0` 表示"缺少接收时间"，`observed_at_unix_ms == None`
/// 表示"发生时刻未知"——两者都必须在往返后**原样保留**，不得被补成 0 或某个默认值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LateFact {
    pub run_id: String,
    /// 事实来源（哪个入口/阶段产生）。空字符串视为缺少来源。
    pub source: String,
    /// 本进程**接收**到该事实的时刻。0 视为缺少接收时间。
    pub received_at_unix_ms: u64,
    pub kind: LateFactKind,
    /// 该事实在原始 run 内声称的发生时刻（可未知）。
    pub observed_at_unix_ms: Option<u64>,
}

impl LateFact {
    /// 是否确实"迟到"：事实声称的发生时刻早于接收时刻。
    ///
    /// 发生时刻未知时按迟到处理（保守：不确定就标注，便于对账时区分）。
    #[must_use]
    pub fn is_late(&self) -> bool {
        match self.observed_at_unix_ms {
            Some(observed) => observed < self.received_at_unix_ms,
            None => true,
        }
    }

    fn missing_provenance(&self) -> bool {
        self.source.trim().is_empty() || self.received_at_unix_ms == 0
    }
}

/// 追加迟到事实被拒绝的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LateFactError {
    /// 缺少来源或接收时间：迟到事实必须可追溯，不能匿名追加。
    MissingProvenance,
    /// run_id 为空。
    MissingRunId,
}

impl LateFactError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingProvenance => "late_fact_missing_provenance",
            Self::MissingRunId => "late_fact_missing_run_id",
        }
    }
}

/// 追加的结果：**控制终态与追加前完全相同**，运行不被复活。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LateFactAppend {
    /// 追加后的控制终态，**必须等于追加前**。
    pub control_status: RunTerminalStatus,
    pub kind: LateFactKind,
}

impl LateFactAppend {
    /// 追加是否复活了运行。按契约**永远为 false**。
    #[must_use]
    pub const fn revives_run(self) -> bool {
        false
    }
}

/// 把一条迟到事实追加到已终态的 run。
///
/// 只校验来源完整性并回显"控制状态未变"；事实本身的存储由上层负责（本模块不持有状态）。
pub fn append_late_fact(
    status: RunTerminalStatus,
    fact: &LateFact,
) -> Result<LateFactAppend, LateFactError> {
    if fact.run_id.trim().is_empty() {
        return Err(LateFactError::MissingRunId);
    }
    if fact.missing_provenance() {
        return Err(LateFactError::MissingProvenance);
    }
    Ok(LateFactAppend {
        control_status: status,
        kind: fact.kind,
    })
}

#[cfg(test)]
mod tests {
    use super::{append_late_fact, LateFact, LateFactError, LateFactKind};
    use crate::run_contract::RunTerminalStatus;

    fn fact(source: &str, received_at: u64, kind: LateFactKind) -> LateFact {
        LateFact {
            run_id: "run-1".into(),
            source: source.into(),
            received_at_unix_ms: received_at,
            kind,
            observed_at_unix_ms: Some(1_000),
        }
    }

    /// 五个控制终态都必须接受迟到事实，且**控制状态不变**（不复活运行）。
    #[test]
    fn late_facts_never_change_the_control_status() {
        for status in [
            RunTerminalStatus::Succeeded,
            RunTerminalStatus::Failed,
            RunTerminalStatus::Blocked,
            RunTerminalStatus::Cancelled,
            RunTerminalStatus::TimedOut,
        ] {
            let append = append_late_fact(status, &fact("web-console", 2_000, LateFactKind::Input))
                .expect("late fact must be accepted");
            assert_eq!(
                append.control_status, status,
                "追加迟到事实不得改变控制终态（{status:?}）"
            );
            assert!(!append.revives_run(), "迟到事实不得复活运行");
        }
    }

    /// 计费与输入类事实都必须被接受——"统计不丢已发生的输入和计费用量"。
    #[test]
    fn input_and_usage_facts_are_both_accepted() {
        let status = RunTerminalStatus::Cancelled;
        assert_eq!(
            append_late_fact(status, &fact("helper", 2_000, LateFactKind::Input))
                .unwrap()
                .kind,
            LateFactKind::Input
        );
        assert_eq!(
            append_late_fact(status, &fact("provider", 2_000, LateFactKind::Usage))
                .unwrap()
                .kind,
            LateFactKind::Usage
        );
    }

    /// 缺少来源或接收时间必须被拒绝：迟到事实要可追溯，不能匿名追加。
    #[test]
    fn missing_provenance_is_rejected() {
        let status = RunTerminalStatus::Failed;
        assert_eq!(
            append_late_fact(status, &fact("", 2_000, LateFactKind::Usage)),
            Err(LateFactError::MissingProvenance)
        );
        assert_eq!(
            append_late_fact(status, &fact("   ", 2_000, LateFactKind::Usage)),
            Err(LateFactError::MissingProvenance)
        );
        assert_eq!(
            append_late_fact(status, &fact("provider", 0, LateFactKind::Usage)),
            Err(LateFactError::MissingProvenance)
        );
        assert_eq!(
            LateFactError::MissingProvenance.code(),
            "late_fact_missing_provenance"
        );
    }

    /// 空的 run_id 被拒绝。
    #[test]
    fn missing_run_id_is_rejected() {
        let mut fact = fact("provider", 2_000, LateFactKind::Verdict);
        fact.run_id = "  ".into();
        assert_eq!(
            append_late_fact(RunTerminalStatus::Succeeded, &fact),
            Err(LateFactError::MissingRunId)
        );
    }

    /// "迟到"的判定：发生时刻早于接收时刻为迟到；发生时刻未知按迟到保守处理。
    #[test]
    fn lateness_is_derived_from_observation_and_receipt_times() {
        let mut early = fact("provider", 2_000, LateFactKind::Input);
        assert!(early.is_late());

        early.observed_at_unix_ms = Some(2_500);
        assert!(!early.is_late(), "接收时间早于发生时间不算迟到");

        early.observed_at_unix_ms = None;
        assert!(early.is_late(), "发生时刻未知时保守标注为迟到");
    }
}
