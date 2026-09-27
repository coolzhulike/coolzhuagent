//! CU 级预算事实：根 deadline 的接线状态、真实建立的 CU 截止时间，以及可随运行记录保存的事实投影。
//!
//! 本模块只表达事实，不做等待、不发起请求、不碰数据库。它刻意把三件事分开：
//!
//! 1. **根 deadline 未接线**（`RootDeadlineState::NotWired`）——宿主根本没有把外层
//!    deadline 传进来。这是"没有事实"，**不等于**"用户选择了无限时间"。
//! 2. **根 deadline 缺席**（`RootDeadline::Absent`）——absent 就是 absent，不允许用
//!    任何数值（例如 120 秒、600 秒）顶替成"好像有一个 deadline"。
//! 3. **CU 截止时间**（`CuDeadline`）——CU 任务被接纳并进入调度时建立的**唯一**
//!    deadline，是真实存在的时刻，所有阶段共用它。
//!
//! 因此"CU 级预算已闭环"**不等于**"整个聊天已有根 deadline"：前者只说明本 CU 运行
//! 内部的观察/规划/验收/输入共用同一个截止时间，后者需要宿主把根 deadline 接进来。

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::ComputerUseBudgets;

/// 外层根 deadline（用户运行 / 整个聊天轮）是否已经接线。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootDeadlineState {
    /// 宿主没有把根 deadline 接进来：这是"没有事实"。
    ///
    /// **不等于**"用户选择了无限时间"——后者是一个明确的策略选择，必须由宿主显式表达，
    /// 不能被默认值悄悄代替。
    NotWired,
    /// 根 deadline 已接线，真实时刻由 `RootDeadline` 给出。
    Wired,
}

impl RootDeadlineState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotWired => "not_wired",
            Self::Wired => "wired",
        }
    }

    #[must_use]
    pub const fn is_wired(self) -> bool {
        matches!(self, Self::Wired)
    }
}

/// 根 deadline 的事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RootDeadline {
    /// 不存在根 deadline 这个事实。**不得**用数值顶替。
    Absent,
    /// 真实存在的根 deadline 时刻（unix 毫秒）。
    At { unix_ms: u64 },
}

impl RootDeadline {
    #[must_use]
    pub const fn absent() -> Self {
        Self::Absent
    }

    #[must_use]
    pub const fn at(unix_ms: u64) -> Self {
        Self::At { unix_ms }
    }

    /// 只有真实存在时才给出时刻；`Absent` 恒为 `None`，绝不回退成某个默认值。
    #[must_use]
    pub const fn unix_ms(self) -> Option<u64> {
        match self {
            Self::Absent => None,
            Self::At { unix_ms } => Some(unix_ms),
        }
    }

    #[must_use]
    pub const fn state(self) -> RootDeadlineState {
        match self {
            Self::Absent => RootDeadlineState::NotWired,
            Self::At { .. } => RootDeadlineState::Wired,
        }
    }
}

impl Default for RootDeadline {
    fn default() -> Self {
        Self::Absent
    }
}

/// CU 级截止时间：在 CU 任务**被接纳并进入调度**时建立，且只建立一次。
///
/// 建立点必须早于该任务的租约等待、模型切换、初始观察与规划请求——否则这些耗时
/// 会从预算里消失。此后同一任务中的观察、规划、验收、重试、重规划和受限恢复
/// **共用**这一个截止时间；内部恢复不重置它。
///
/// 当前有效调用预算 = `min(CU 剩余时间, 当前阶段上限, 实际模型调用上限)`。
/// 根 deadline 缺席时，`min` 链里**少一项**，此时不得伪造一个父运行剩余时间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CuDeadline {
    established_at_ms: u64,
    cu_deadline_ms: u64,
    cu_budget_ms: u64,
    root: RootDeadline,
}

impl CuDeadline {
    /// 在任务被接纳时建立 CU 截止时间。
    ///
    /// `cu_deadline_ms = min(接纳时刻 + CU 任务预算, 根 deadline)`；根 deadline 缺席时
    /// 只取前半项，并且 `root_deadline_state` 明确写成 `not_wired`。
    #[must_use]
    pub fn establish(
        budgets: &ComputerUseBudgets,
        established_at_ms: u64,
        root: RootDeadline,
    ) -> Self {
        let from_budget = established_at_ms.saturating_add(budgets.timeout_ms);
        let cu_deadline_ms = match root.unix_ms() {
            Some(root_ms) => from_budget.min(root_ms),
            None => from_budget,
        };
        Self {
            established_at_ms,
            cu_deadline_ms,
            cu_budget_ms: budgets.timeout_ms,
            root,
        }
    }

    /// 未接线任何根 deadline 时的接纳：`root_deadline_state = not_wired`、
    /// `root_deadline = absent`、`cu_deadline = 接纳时刻 + CU 任务预算`。
    #[must_use]
    pub fn establish_without_root(
        budgets: &ComputerUseBudgets,
        established_at_ms: u64,
    ) -> Self {
        Self::establish(budgets, established_at_ms, RootDeadline::Absent)
    }

    #[must_use]
    pub const fn established_at_ms(self) -> u64 {
        self.established_at_ms
    }

    /// CU 任务的实际截止时刻（unix 毫秒）。
    #[must_use]
    pub const fn cu_deadline_ms(self) -> u64 {
        self.cu_deadline_ms
    }

    /// CU 任务自身的预算上限（配置值），不是"父运行剩余"。
    #[must_use]
    pub const fn cu_budget_ms(self) -> u64 {
        self.cu_budget_ms
    }

    #[must_use]
    pub const fn root_deadline(self) -> RootDeadline {
        self.root
    }

    #[must_use]
    pub const fn root_deadline_state(self) -> RootDeadlineState {
        self.root.state()
    }

    #[must_use]
    pub fn remaining_ms(self, now_ms: u64) -> u64 {
        self.cu_deadline_ms.saturating_sub(now_ms)
    }

    #[must_use]
    pub fn remaining(self, now_ms: u64) -> Duration {
        Duration::from_millis(self.remaining_ms(now_ms))
    }

    #[must_use]
    pub fn is_expired_at(self, now_ms: u64) -> bool {
        now_ms >= self.cu_deadline_ms
    }

    /// 随运行记录一起保存的预算事实。
    #[must_use]
    pub const fn facts(self) -> CuBudgetFacts {
        CuBudgetFacts {
            root_deadline_state: self.root.state(),
            root_deadline: self.root,
            cu_deadline_ms: Some(self.cu_deadline_ms),
            cu_deadline_established_at_ms: Some(self.established_at_ms),
            cu_budget_ms: Some(self.cu_budget_ms),
        }
    }
}

/// 运行记录里的 CU 预算事实。
///
/// 三个字段必须同时存在且互不冒充：`root_deadline_state` 说明根 deadline 是否接线，
/// `root_deadline` 给出它的事实（缺席就是 `absent`），`cu_deadline_ms` 是**真实建立过**
/// 的 CU 截止时间。参数未通过、尚未接纳执行的请求使用 [`CuBudgetFacts::not_accepted`]，
/// 不得伪装成已经启动的 CU 任务。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuBudgetFacts {
    pub root_deadline_state: RootDeadlineState,
    pub root_deadline: RootDeadline,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cu_deadline_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cu_deadline_established_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cu_budget_ms: Option<u64>,
}

impl CuBudgetFacts {
    /// 请求还没被接纳成 CU 任务：没有 CU 截止时间。
    ///
    /// 根 deadline 仍是 `not_wired` + `absent`（这是环境事实，与是否接纳无关），
    /// 但不写任何 CU 截止时间，避免把"参数纠错"说成"已经启动的 CU 任务"。
    #[must_use]
    pub const fn not_accepted() -> Self {
        Self {
            root_deadline_state: RootDeadlineState::NotWired,
            root_deadline: RootDeadline::Absent,
            cu_deadline_ms: None,
            cu_deadline_established_at_ms: None,
            cu_budget_ms: None,
        }
    }

    /// 是否已经建立了真实的 CU 截止时间。
    #[must_use]
    pub const fn is_accepted(self) -> bool {
        self.cu_deadline_ms.is_some()
    }
}

impl Default for CuBudgetFacts {
    fn default() -> Self {
        Self::not_accepted()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn budgets() -> ComputerUseBudgets {
        ComputerUseBudgets::default()
    }

    #[test]
    fn not_wired_root_is_recorded_as_absent_and_never_as_an_infinite_choice() {
        let deadline = CuDeadline::establish_without_root(&budgets(), 1_000);
        let facts = deadline.facts();

        assert_eq!(facts.root_deadline_state, RootDeadlineState::NotWired);
        assert_eq!(facts.root_deadline_state.as_str(), "not_wired");
        assert_eq!(facts.root_deadline, RootDeadline::Absent);
        assert_eq!(facts.root_deadline.unix_ms(), None);
        // 未接线不等于"用户选择了无限时间"：CU 截止时间仍然是一个真实时刻。
        assert_eq!(facts.cu_deadline_ms, Some(1_000 + 120_000));
        assert_eq!(facts.cu_deadline_established_at_ms, Some(1_000));
        assert!(facts.is_accepted());
    }

    #[test]
    fn absent_root_is_not_replaced_by_a_fabricated_parent_budget() {
        let deadline = CuDeadline::establish_without_root(&budgets(), 5_000);
        assert_eq!(deadline.root_deadline().unix_ms(), None);
        assert_eq!(deadline.cu_deadline_ms(), 125_000);
        // CU 截止时间只能由"接纳时刻 + CU 任务预算"决定，不能引用不存在的父预算。
        assert_eq!(deadline.remaining_ms(5_000), 120_000);
        assert_eq!(deadline.remaining_ms(120_000), 5_000);
        assert!(deadline.is_expired_at(125_000));
    }

    #[test]
    fn wired_root_deadline_shrinks_the_cu_deadline_to_the_smaller_value() {
        let shorter_root = CuDeadline::establish(&budgets(), 1_000, RootDeadline::at(30_000));
        assert_eq!(shorter_root.cu_deadline_ms(), 30_000);
        assert_eq!(shorter_root.root_deadline_state(), RootDeadlineState::Wired);
        assert_eq!(shorter_root.remaining_ms(10_000), 20_000);

        let longer_root = CuDeadline::establish(&budgets(), 1_000, RootDeadline::at(10_000_000));
        assert_eq!(longer_root.cu_deadline_ms(), 121_000);
    }

    #[test]
    fn not_accepted_requests_have_no_cu_deadline() {
        let facts = CuBudgetFacts::not_accepted();
        assert!(!facts.is_accepted());
        assert_eq!(facts.cu_deadline_ms, None);
        assert_eq!(facts.cu_deadline_established_at_ms, None);
        assert_eq!(facts.cu_budget_ms, None);
        assert_eq!(facts.root_deadline_state, RootDeadlineState::NotWired);
        assert_eq!(facts.root_deadline, RootDeadline::Absent);
        assert_eq!(CuBudgetFacts::default(), facts);
    }

    #[test]
    fn budget_facts_round_trip_with_stable_snake_case() {
        let facts = CuDeadline::establish_without_root(&budgets(), 7).facts();
        let json = serde_json::to_string(&facts).unwrap();
        assert!(json.contains("\"root_deadline_state\":\"not_wired\""), "{json}");
        assert!(json.contains("\"root_deadline\":\"absent\""), "{json}");
        let decoded: CuBudgetFacts = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, facts);
    }
}
