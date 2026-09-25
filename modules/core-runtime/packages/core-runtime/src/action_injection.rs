//! 动作注入的一次性契约（§2.1）。
//!
//! 规范原文：
//!
//! > 动作重传 | 同 action_id 不重新注入输入；未知输入结果先观察/对账，不自动重放
//!
//! 现有的 `ActionFingerprint` + `max_same_signature`（见 `computer-use-core` 的
//! `RunBudgetGuard`）是**内容指纹**：它限制"完全相同的动作重复多少次"，但**不是**
//! 动作身份检查——同一个 `action_id` 只要参数或观察代际略有不同，指纹就不同，仍会
//! 再次注入输入。规范要的是按**身份**去重。
//!
//! 本模块把两条规则做成可回归的纯逻辑；**尚未接线**（接线属 S2 的迁移工作）。

use std::collections::BTreeMap;

/// 注入请求的判决。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectionDecision {
    /// 首次见到该 action_id：允许注入输入。
    Inject,
    /// 同 action_id 已注入且已有已知结果：返回该结果引用，**不再注入**。
    AlreadyInjected { outcome_ref: String },
    /// 同 action_id 已注入但结果**未知**（可能已发送）：先观察/对账，**不自动重放**。
    ReconcileFirst,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum InjectionRecord {
    /// 已注入，但结果未知（`may_have_been_sent` 一类）。
    AwaitingReconciliation,
    /// 已注入且结果已知；保存结果引用。
    Resolved { outcome_ref: String },
}

/// 按 `action_id` 记录"是否已经注入过输入"。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActionInjectionRegistry {
    records: BTreeMap<String, InjectionRecord>,
}

impl ActionInjectionRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 请求为某个 action_id 注入输入。只有返回 `Inject` 才允许真正发送输入，
    /// 且调用方必须在注入后调用 `resolve` 登记结果（已知或未知）。
    pub fn begin_injection(&mut self, action_id: &str) -> InjectionDecision {
        match self.records.get(action_id) {
            None => {
                self.records
                    .insert(action_id.to_string(), InjectionRecord::AwaitingReconciliation);
                InjectionDecision::Inject
            }
            Some(InjectionRecord::Resolved { outcome_ref }) => InjectionDecision::AlreadyInjected {
                outcome_ref: outcome_ref.clone(),
            },
            Some(InjectionRecord::AwaitingReconciliation) => InjectionDecision::ReconcileFirst,
        }
    }

    /// 登记一次注入的结果。`outcome_ref = None` 表示结果**未知**（可能已发送），
    /// 此时后续同 ID 传输会走"先对账"，而不是重放。
    ///
    /// 返回 `false` 表示该 action_id 从未登记过注入（调用方顺序有误）。
    pub fn resolve(&mut self, action_id: &str, outcome_ref: Option<String>) -> bool {
        match self.records.get_mut(action_id) {
            None => false,
            Some(record) => {
                *record = match outcome_ref {
                    Some(outcome_ref) => InjectionRecord::Resolved { outcome_ref },
                    None => InjectionRecord::AwaitingReconciliation,
                };
                true
            }
        }
    }

    /// 已登记的 action 数量。
    #[must_use]
    pub fn tracked(&self) -> usize {
        self.records.len()
    }

    /// 有多少个 action 仍处于"结果未知、需要先对账"。
    #[must_use]
    pub fn awaiting_reconciliation(&self) -> usize {
        self.records
            .values()
            .filter(|record| matches!(record, InjectionRecord::AwaitingReconciliation))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::{ActionInjectionRegistry, InjectionDecision};

    /// 首次请求放行，且登记为"结果未知"。
    #[test]
    fn first_injection_is_allowed_and_marked_unknown() {
        let mut registry = ActionInjectionRegistry::new();
        assert_eq!(
            registry.begin_injection("action-1"),
            InjectionDecision::Inject
        );
        assert_eq!(registry.tracked(), 1);
        assert_eq!(registry.awaiting_reconciliation(), 1);
    }

    /// 结果未知时再次传输：**先对账，不自动重放**。
    #[test]
    fn retransmission_with_unknown_outcome_requires_reconciliation() {
        let mut registry = ActionInjectionRegistry::new();
        registry.begin_injection("action-1");

        assert_eq!(
            registry.begin_injection("action-1"),
            InjectionDecision::ReconcileFirst
        );
        assert_eq!(registry.tracked(), 1, "对账路径不得新增登记");
    }

    /// 结果已知后再次传输：返回既有结果引用，**不再注入**。
    #[test]
    fn retransmission_after_a_known_outcome_returns_the_existing_outcome() {
        let mut registry = ActionInjectionRegistry::new();
        registry.begin_injection("action-1");
        assert!(registry.resolve("action-1", Some("step-7".to_string())));

        assert_eq!(
            registry.begin_injection("action-1"),
            InjectionDecision::AlreadyInjected {
                outcome_ref: "step-7".to_string()
            }
        );
        assert_eq!(registry.awaiting_reconciliation(), 0);
    }

    /// 同一个 action_id 在任何情况下都不会被注入第二次。
    #[test]
    fn an_action_id_is_never_injected_twice() {
        let mut registry = ActionInjectionRegistry::new();
        assert_eq!(
            registry.begin_injection("action-1"),
            InjectionDecision::Inject
        );
        // 无论解析成已知还是未知，第二次都不再是 Inject。
        assert!(registry.resolve("action-1", None));
        assert_ne!(
            registry.begin_injection("action-1"),
            InjectionDecision::Inject
        );
        assert!(registry.resolve("action-1", Some("step-9".to_string())));
        assert_ne!(
            registry.begin_injection("action-1"),
            InjectionDecision::Inject
        );
    }

    /// 不同 action_id 各自独立。
    #[test]
    fn distinct_action_ids_are_independent() {
        let mut registry = ActionInjectionRegistry::new();
        assert_eq!(
            registry.begin_injection("action-1"),
            InjectionDecision::Inject
        );
        assert_eq!(
            registry.begin_injection("action-2"),
            InjectionDecision::Inject
        );
        assert_eq!(registry.tracked(), 2);
    }

    /// 未登记就 resolve 属于顺序错误，返回 false 而不是静默成功。
    #[test]
    fn resolving_an_unknown_action_reports_a_sequencing_error() {
        let mut registry = ActionInjectionRegistry::new();
        assert!(!registry.resolve("action-never-started", Some("step-1".to_string())));
        assert_eq!(registry.tracked(), 0);
    }
}
