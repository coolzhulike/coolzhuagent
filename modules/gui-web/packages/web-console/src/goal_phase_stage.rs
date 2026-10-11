//! 原计划角色保持不变，回退阶段另行决定实际接管角色与提交语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PhaseStage {
    Execution,
    Replanning,
    Repair,
}

impl PhaseStage {
    pub(super) fn for_phase(phase: &super::GoalPhaseDto) -> Self {
        Self::resolve(&phase.assigned_role,phase.route_hint.as_deref(),phase.retry_count,
            phase.last_verdict.as_deref()==Some("fail"))
    }

    pub(super) fn resolve(original_role: &str, route: Option<&str>, failures: u32, failed: bool) -> Self {
        if failures == 0 || !failed {
            return Self::Execution;
        }
        match (original_role.trim().to_ascii_lowercase().as_str(), route) {
            ("implementer" | "verifier", Some("planner")) => Self::Replanning,
            ("verifier", Some("implementer")) => Self::Repair,
            _ => Self::Execution,
        }
    }

    pub(super) fn routed_role<'a>(self, original_role: &'a str) -> &'a str {
        match self {
            Self::Execution => original_role,
            Self::Replanning => "planner",
            Self::Repair => "implementer",
        }
    }

    /// 只有原执行阶段可提交完成；规划和修复成功都必须继续原任务链。
    pub(super) fn continuation_role(self) -> Option<&'static str> {
        match self {
            Self::Execution => None,
            Self::Replanning => Some("implementer"),
            Self::Repair => Some("verifier"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_routes_preserve_original_role_and_require_durable_failure() {
        let stage = PhaseStage::resolve("implementer", Some("planner"), 1, true);
        assert_eq!(stage, PhaseStage::Replanning);
        assert_eq!(stage.routed_role("implementer"), "planner");
        assert_eq!(PhaseStage::resolve("verifier", Some("planner"), 2, true), PhaseStage::Replanning);
        assert_eq!(PhaseStage::resolve("verifier", Some("implementer"), 1, true), PhaseStage::Repair);
        for (failures, failed) in [(0, true), (1, false), (0, false)] {
            assert_eq!(PhaseStage::resolve("implementer", Some("planner"), failures, failed), PhaseStage::Execution);
        }
        assert_eq!(PhaseStage::resolve("planner", Some("planner"), 1, true), PhaseStage::Execution);
        assert_eq!(PhaseStage::resolve("implementer", Some("manual_reconcile"), 3, true), PhaseStage::Execution);
        assert_eq!(PhaseStage::resolve("verifier", Some("verifier"), 1, true).routed_role("verifier"), "verifier");
    }

    #[test]
    fn successful_recovery_stages_do_not_complete_original_verification() {
        let planner = PhaseStage::resolve("verifier", Some("planner"), 2, true);
        let repair = PhaseStage::resolve("verifier", planner.continuation_role(), 2, true);
        assert_eq!(repair, PhaseStage::Repair);
        let verifier = PhaseStage::resolve("verifier", repair.continuation_role(), 2, true);
        assert_eq!(verifier.routed_role("verifier"), "verifier");
        assert_eq!(verifier.continuation_role(), None);
        let implementer = PhaseStage::resolve("implementer", planner.continuation_role(), 2, true);
        assert_eq!(implementer.routed_role("implementer"), "implementer");
        assert_eq!(implementer.continuation_role(), None);
    }
}
