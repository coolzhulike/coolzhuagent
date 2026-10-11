//! 工具结果的审计投影：授权、成功和是否产生执行分别表达，不推断失败无副作用。
use super::{tool_audit_log_path, ComputerUseAuditRecord, ToolOutcome, ToolOutcomeStatus};

pub(super) fn from_outcome(outcome: &ToolOutcome, mode: &str) -> ComputerUseAuditRecord {
    ComputerUseAuditRecord {
        audit_id: outcome.call_id.clone(),
        mode: mode.into(),
        execute_requested: true,
        execute_allowed: outcome.permission_gate.decision.is_allowed(),
        // 执行中取消、超时或失败都可能已产生副作用。缺少动作层事实时保留未知，
        // 不能用status != ok推导零派发；调用方仍须依据原回执决定是否可以重试。
        executed: match outcome.status {
            ToolOutcomeStatus::Ok => Some(true),
            ToolOutcomeStatus::Rejected | ToolOutcomeStatus::DryRunOnly => Some(false),
            ToolOutcomeStatus::Failed | ToolOutcomeStatus::Timeout => None,
        },
        requires_human_confirmation: outcome.permission_gate.decision.requires_ui(),
        requires_screenshot_evidence: false,
        screenshot_path: None,
        permission_gate: outcome.permission_gate.decision.as_str().into(),
        log_path: tool_audit_log_path().display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_dispatch_never_claims_no_execution_or_permission_denial() {
        let mut outcome = ToolOutcome {
            call_id: "actual-call".into(),
            tool_name: "dsh__net_fetch".into(),
            status: ToolOutcomeStatus::Failed,
            output: serde_json::json!({"code":"cancelled","cleanup_confirmed":true}),
            summary_text: "函数执行后取消".into(),
            elapsed_ms: 1000,
            permission_gate: runtime::PermissionGateReport::allow_auto(
                runtime::PermissionMode::DangerFullAccess,
                "full-access-profile",
            ),
            evidence: None,
        };
        for status in [ToolOutcomeStatus::Failed, ToolOutcomeStatus::Timeout] {
            outcome.status = status;
            let response = super::super::tool_outcome_to_dispatch_response(
                "dsh__net_fetch",
                &serde_json::json!({}),
                outcome.clone(),
            );
            let audit = serde_json::to_value(response.dispatch_plan.unwrap().audit).unwrap();
            assert_eq!(audit["execute_requested"], true);
            assert_eq!(audit["execute_allowed"], true);
            assert!(audit["executed"].is_null());
            assert_eq!(outcome.output["cleanup_confirmed"], true);
        }
        outcome.status = ToolOutcomeStatus::Rejected;
        outcome.permission_gate = runtime::PermissionGateReport::deny(
            runtime::PermissionMode::DangerFullAccess,
            "not-authorized",
        );
        let rejected = from_outcome(&outcome, "runtime");
        assert_eq!(rejected.executed, Some(false));
        assert!(!rejected.execute_allowed);
    }
}
