//! 统一工具调用协议（REQ-TOOL-007/008/009/010/011、REQ-CORE-TOOL-001）。
//!
//! 契约层：定义 `ToolInvoke` / `ToolOutcome` / `PermissionGateReport`，以及
//! 调用来源 (`ToolCaller`) 与权限判定结果 (`PermissionDecision`)。该层不直接
//! 执行工具，也不读取任何全局状态，仅提供可测试的纯类型 + serde 序列化。
//!
//! 行为层（`runtime_tool_execute`）在后续步骤接入；此处只保留一个 `evaluate`
//! 钩子 (`ToolCallContext::with_grant`) 供调用方描述会话级授权状态。
//!
//! 参考：`docs/tool-calling-permission-plan-2026-05-10.md`。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::permission_gate::{evaluate_permission, PathTarget, ProtectedRule, SessionGrantView};
use crate::permissions::{PermissionMode, PermissionProfile};

/// LLM / Web / CLI / MCP / Plugin 五类调用来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolCaller {
    Llm,
    WebUi,
    Cli,
    Mcp,
    Plugin,
}

impl ToolCaller {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Llm => "llm",
            Self::WebUi => "web-ui",
            Self::Cli => "cli",
            Self::Mcp => "mcp",
            Self::Plugin => "plugin",
        }
    }
}

/// 统一工具调用请求。LLM / Web / CLI 全部通过此结构进入 `runtime_tool_execute`。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInvoke {
    pub call_id: String,
    pub tool_name: String,
    pub input: Value,
    pub caller: ToolCaller,
    pub workspace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub user_authorized: bool,
    #[serde(default)]
    pub user_confirmed_twice: bool,
}

/// 工具执行最终状态。与 OpenAI `is_error` 映射关系：
/// - `Ok`                    → `is_error=false`
/// - `DryRunOnly`            → `is_error=false`（模型应感知未执行）
/// - `Rejected/Failed/Timeout` → `is_error=true`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolOutcomeStatus {
    Ok,
    DryRunOnly,
    Rejected,
    Timeout,
    Failed,
}

impl ToolOutcomeStatus {
    #[must_use]
    pub fn is_error(self) -> bool {
        matches!(self, Self::Rejected | Self::Timeout | Self::Failed)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::DryRunOnly => "dry-run-only",
            Self::Rejected => "rejected",
            Self::Timeout => "timeout",
            Self::Failed => "failed",
        }
    }
}

/// 权限闸门判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionDecision {
    AllowAuto,
    AllowApproved,
    RequireApproval,
    RequireConfirm,
    Deny,
}

impl PermissionDecision {
    #[must_use]
    pub fn is_allowed(self) -> bool {
        matches!(self, Self::AllowAuto | Self::AllowApproved)
    }

    #[must_use]
    pub fn requires_ui(self) -> bool {
        matches!(self, Self::RequireApproval | Self::RequireConfirm)
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllowAuto => "allow-auto",
            Self::AllowApproved => "allow-approved",
            Self::RequireApproval => "require-approval",
            Self::RequireConfirm => "require-confirm",
            Self::Deny => "deny",
        }
    }
}

/// 权限判定完整报告。`runtime_tool_execute` 无论 allow 还是 deny 都需要写审计。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGateReport {
    pub required: PermissionMode,
    pub decision: PermissionDecision,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_match: Option<String>,
    pub workspace_relative: bool,
    /// 涉及到的路径摘要（相对 workspace 后的显示形式），供 UI / 审计展示。
    #[serde(default)]
    pub affected_paths: Vec<String>,
}

impl PermissionGateReport {
    #[must_use]
    pub fn allow_auto(required: PermissionMode, reason: impl Into<String>) -> Self {
        Self {
            required,
            decision: PermissionDecision::AllowAuto,
            reason: reason.into(),
            protected_match: None,
            workspace_relative: true,
            affected_paths: Vec::new(),
        }
    }

    #[must_use]
    pub fn deny(required: PermissionMode, reason: impl Into<String>) -> Self {
        Self {
            required,
            decision: PermissionDecision::Deny,
            reason: reason.into(),
            protected_match: None,
            workspace_relative: true,
            affected_paths: Vec::new(),
        }
    }
}

/// 单次工具调用最终结果。回灌到 LLM 的 ToolResult content block 必须来自该结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutcome {
    pub call_id: String,
    pub tool_name: String,
    pub status: ToolOutcomeStatus,
    pub output: Value,
    pub summary_text: String,
    pub elapsed_ms: u64,
    pub permission_gate: PermissionGateReport,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Value>,
}

impl ToolOutcome {
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status.is_error()
    }

    #[must_use]
    pub fn rejected(
        invoke: &ToolInvoke,
        gate: PermissionGateReport,
        summary_text: impl Into<String>,
    ) -> Self {
        Self {
            call_id: invoke.call_id.clone(),
            tool_name: invoke.tool_name.clone(),
            status: ToolOutcomeStatus::Rejected,
            output: Value::Null,
            summary_text: summary_text.into(),
            elapsed_ms: 0,
            permission_gate: gate,
            evidence: None,
        }
    }

    #[must_use]
    pub fn dry_run_only(
        invoke: &ToolInvoke,
        gate: PermissionGateReport,
        summary_text: impl Into<String>,
    ) -> Self {
        Self {
            call_id: invoke.call_id.clone(),
            tool_name: invoke.tool_name.clone(),
            status: ToolOutcomeStatus::DryRunOnly,
            output: Value::Null,
            summary_text: summary_text.into(),
            elapsed_ms: 0,
            permission_gate: gate,
            evidence: None,
        }
    }
}

/// 调用方上下文，记录会话/工作区授权状态，不参与 serde。
#[derive(Debug, Clone)]
pub struct ToolCallContext {
    pub call_id: String,
    pub workspace_id: String,
    pub workspace_root: std::path::PathBuf,
    pub session_id: Option<String>,
    pub caller: ToolCaller,
    pub user_authorized: bool,
    pub user_confirmed_twice: bool,
}

impl ToolCallContext {
    #[must_use]
    pub fn new(
        call_id: impl Into<String>,
        workspace_id: impl Into<String>,
        workspace_root: impl Into<std::path::PathBuf>,
        caller: ToolCaller,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            workspace_id: workspace_id.into(),
            workspace_root: workspace_root.into(),
            session_id: None,
            caller,
            user_authorized: false,
            user_confirmed_twice: false,
        }
    }

    #[must_use]
    pub fn with_grant(mut self, authorized: bool, confirmed_twice: bool) -> Self {
        self.user_authorized = authorized;
        self.user_confirmed_twice = confirmed_twice;
        self
    }

    #[must_use]
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    #[must_use]
    pub fn to_invoke(&self, tool_name: impl Into<String>, input: Value) -> ToolInvoke {
        ToolInvoke {
            call_id: self.call_id.clone(),
            tool_name: tool_name.into(),
            input,
            caller: self.caller,
            workspace_id: self.workspace_id.clone(),
            session_id: self.session_id.clone(),
            user_authorized: self.user_authorized,
            user_confirmed_twice: self.user_confirmed_twice,
        }
    }
}

/// 工具执行器：把已经通过权限闸门的 `ToolInvoke` 真正跑起来。
///
/// 为避免与既有 `conversation::ToolExecutor` 重名，这里使用 `ToolInvocationExecutor`。
/// 行为层（web-console / CLI / MCP）提供自己的实现，`runtime_tool_execute`
/// 负责先评估权限，再委托给对应实现。
///
/// trait object 兼容：`dyn ToolInvocationExecutor` 可以装 `Box`/`Arc`，便于
/// `runtime_tool_execute` 根据 `tool_name` 动态分发。
pub trait ToolInvocationExecutor: Send + Sync {
    /// 是否由本 executor 处理该工具。同一个 registry 可注册多个 executor。
    fn handles(&self, tool_name: &str) -> bool;

    /// 执行工具。返回 `ToolOutcome`（不抛 Result）——错误也必须走正常结构体，
    /// 以便回灌到 LLM 的 ToolResult block。
    ///
    /// 注意：权限闸门已由 `runtime_tool_execute` 判定并放行；实现内部不再
    /// 重复检查 `invoke.user_authorized`，但可以做更细粒度的 argument validation。
    fn execute(&self, invoke: &ToolInvoke) -> ToolOutcome;
}

/// `runtime_tool_execute` 的上下文输入：权限评估所需的外部数据。
///
/// 不直接读任何全局状态，方便 TDD。
pub struct RuntimeToolContext<'a> {
    pub workspace_root: &'a std::path::Path,
    pub required_permission: PermissionMode,
    pub path_targets: Vec<PathTarget>,
    pub protected_rules: &'a [ProtectedRule],
    pub session_grant: SessionGrantView,
    pub executors: &'a [&'a dyn ToolInvocationExecutor],
    pub profile: PermissionProfile,
}

/// REQ-TOOL-007/008：统一工具调用入口。
///
/// 流程（Phase B-2 脚手架，暂未接入 web-console）：
///   1. `evaluate_permission` → `PermissionGateReport`。
///   2. `AllowAuto`/`AllowApproved` → 选第一个 `handles == true` 的 executor 执行；
///      所有 executor 都不认 → `Failed{unknown-tool}`。
///   3. `RequireApproval`/`RequireConfirm` → 返回 `DryRunOnly`，由上层发起审批流程。
///   4. `Deny` → 返回 `Rejected`。
///
/// 不做超时/并发（Phase C 再加），保证 drop-in 替换旧路径时不引入新的 async 依赖。
pub fn runtime_tool_execute(invoke: ToolInvoke, ctx: &RuntimeToolContext<'_>) -> ToolOutcome {
    let gate = evaluate_permission(
        &invoke,
        ctx.required_permission,
        &ctx.path_targets,
        ctx.workspace_root,
        ctx.protected_rules,
        &ctx.session_grant,
        ctx.profile,
    );

    match gate.decision {
        PermissionDecision::Deny => {
            // 缺少最低权限元数据是配置错误，不能压成泛化的"denied"——审计与回灌
            // 给模型的原因必须能指向真正的问题。
            let summary = if gate.required.is_unspecified() {
                format!("tool '{}' rejected: {}", invoke.tool_name, gate.reason)
            } else {
                format!("tool '{}' denied by permission gate", invoke.tool_name)
            };
            ToolOutcome::rejected(&invoke, gate, summary)
        }
        PermissionDecision::RequireApproval | PermissionDecision::RequireConfirm => {
            ToolOutcome::dry_run_only(
                &invoke,
                gate,
                format!(
                    "tool '{}' awaiting user approval ({})",
                    invoke.tool_name,
                    match invoke.caller {
                        ToolCaller::Llm => "llm",
                        ToolCaller::WebUi => "web-ui",
                        ToolCaller::Cli => "cli",
                        ToolCaller::Mcp => "mcp",
                        ToolCaller::Plugin => "plugin",
                    }
                ),
            )
        }
        PermissionDecision::AllowAuto | PermissionDecision::AllowApproved => {
            for exec in ctx.executors {
                if exec.handles(&invoke.tool_name) {
                    let mut outcome = exec.execute(&invoke);
                    // 强制把 runtime 评估的 gate 写回，防止 executor 伪造。
                    outcome.permission_gate = gate;
                    return outcome;
                }
            }
            ToolOutcome {
                call_id: invoke.call_id.clone(),
                tool_name: invoke.tool_name.clone(),
                status: ToolOutcomeStatus::Failed,
                output: Value::Null,
                summary_text: format!("unknown tool '{}'", invoke.tool_name),
                elapsed_ms: 0,
                permission_gate: gate,
                evidence: None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission_gate::{default_protected_rules, PathAccess};

    fn invoke(name: &str) -> ToolInvoke {
        ToolInvoke {
            call_id: "c1".into(),
            tool_name: name.into(),
            input: serde_json::json!({}),
            caller: ToolCaller::Llm,
            workspace_id: "ws-1".into(),
            session_id: None,
            user_authorized: false,
            user_confirmed_twice: false,
        }
    }

    struct StubExec {
        name: &'static str,
        output: Value,
    }
    impl ToolInvocationExecutor for StubExec {
        fn handles(&self, tool_name: &str) -> bool {
            tool_name == self.name
        }
        fn execute(&self, invoke: &ToolInvoke) -> ToolOutcome {
            ToolOutcome {
                call_id: invoke.call_id.clone(),
                tool_name: invoke.tool_name.clone(),
                status: ToolOutcomeStatus::Ok,
                output: self.output.clone(),
                summary_text: format!("ran {}", invoke.tool_name),
                elapsed_ms: 7,
                permission_gate: PermissionGateReport::deny(
                    PermissionMode::ReadOnly,
                    "placeholder-overwritten",
                ),
                evidence: None,
            }
        }
    }

    #[test]
    fn tool_invoke_serde_roundtrip() {
        let invoke = ToolInvoke {
            call_id: "toolu_abc".into(),
            tool_name: "bash".into(),
            input: serde_json::json!({ "command": "ls" }),
            caller: ToolCaller::Llm,
            workspace_id: "ws-1234".into(),
            session_id: Some("ses-9".into()),
            user_authorized: false,
            user_confirmed_twice: false,
        };
        let encoded = serde_json::to_string(&invoke).unwrap();
        let decoded: ToolInvoke = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.call_id, invoke.call_id);
        assert_eq!(decoded.tool_name, invoke.tool_name);
        assert_eq!(decoded.caller, ToolCaller::Llm);
    }

    #[test]
    fn tool_outcome_status_is_error_matrix() {
        assert!(!ToolOutcomeStatus::Ok.is_error());
        assert!(!ToolOutcomeStatus::DryRunOnly.is_error());
        assert!(ToolOutcomeStatus::Rejected.is_error());
        assert!(ToolOutcomeStatus::Timeout.is_error());
        assert!(ToolOutcomeStatus::Failed.is_error());
    }

    #[test]
    fn permission_decision_helpers() {
        assert!(PermissionDecision::AllowAuto.is_allowed());
        assert!(PermissionDecision::AllowApproved.is_allowed());
        assert!(!PermissionDecision::RequireApproval.is_allowed());
        assert!(PermissionDecision::RequireApproval.requires_ui());
        assert!(PermissionDecision::RequireConfirm.requires_ui());
        assert!(!PermissionDecision::Deny.requires_ui());
        assert!(!PermissionDecision::AllowAuto.requires_ui());
    }

    #[test]
    fn permission_decision_serde_as_kebab() {
        let json = serde_json::to_string(&PermissionDecision::RequireApproval).unwrap();
        assert_eq!(json, "\"require-approval\"");
    }

    #[test]
    fn tool_caller_serde_as_kebab() {
        let json = serde_json::to_string(&ToolCaller::WebUi).unwrap();
        assert_eq!(json, "\"web-ui\"");
    }

    #[test]
    fn tool_outcome_rejected_helper_sets_fields() {
        let invoke = invoke("write_file");
        let gate = PermissionGateReport::deny(PermissionMode::WorkspaceWrite, "test-deny");
        let outcome = ToolOutcome::rejected(&invoke, gate, "no");
        assert_eq!(outcome.status, ToolOutcomeStatus::Rejected);
        assert!(outcome.is_error());
        assert_eq!(outcome.call_id, "c1");
        assert_eq!(outcome.permission_gate.reason, "test-deny");
    }

    #[test]
    fn tool_call_context_builder() {
        let ctx = ToolCallContext::new("c1", "ws-x", "/tmp/ws", ToolCaller::Llm)
            .with_session("ses-1")
            .with_grant(true, false);
        assert!(ctx.user_authorized);
        assert!(!ctx.user_confirmed_twice);
        assert_eq!(ctx.session_id.as_deref(), Some("ses-1"));
        let invoke = ctx.to_invoke("read_file", serde_json::json!({"path": "a"}));
        assert_eq!(invoke.tool_name, "read_file");
        assert_eq!(invoke.workspace_id, "ws-x");
        assert!(invoke.user_authorized);
    }

    #[test]
    fn runtime_tool_execute_readonly_runs_executor() {
        let rules = default_protected_rules();
        let stub = StubExec {
            name: "read_file",
            output: serde_json::json!({"text": "hello"}),
        };
        let executors: [&dyn ToolInvocationExecutor; 1] = [&stub];
        let ctx = RuntimeToolContext {
            workspace_root: std::path::Path::new("/ws"),
            required_permission: PermissionMode::ReadOnly,
            path_targets: vec![PathTarget::new("/ws/a.rs", PathAccess::Read)],
            protected_rules: &rules,
            session_grant: SessionGrantView::default(),
            executors: &executors,
            profile: PermissionProfile::OutsideApproval,
        };
        let out = runtime_tool_execute(invoke("read_file"), &ctx);
        assert_eq!(out.status, ToolOutcomeStatus::Ok);
        assert_eq!(out.permission_gate.decision, PermissionDecision::AllowAuto);
        assert_eq!(out.output, serde_json::json!({"text": "hello"}));
    }

    #[test]
    fn runtime_tool_execute_workspace_outside_returns_dry_run() {
        let rules = default_protected_rules();
        let stub = StubExec {
            name: "write_file",
            output: Value::Null,
        };
        let executors: [&dyn ToolInvocationExecutor; 1] = [&stub];
        let ctx = RuntimeToolContext {
            workspace_root: std::path::Path::new("/ws"),
            required_permission: PermissionMode::WorkspaceWrite,
            path_targets: vec![PathTarget::new("/tmp/other", PathAccess::Write)],
            protected_rules: &rules,
            session_grant: SessionGrantView::default(),
            executors: &executors,
            profile: PermissionProfile::OutsideApproval,
        };
        let out = runtime_tool_execute(invoke("write_file"), &ctx);
        assert_eq!(out.status, ToolOutcomeStatus::DryRunOnly);
        assert_eq!(
            out.permission_gate.decision,
            PermissionDecision::RequireApproval
        );
    }

    #[test]
    fn runtime_tool_execute_protected_returns_dry_run_with_confirm() {
        let rules = default_protected_rules();
        let stub = StubExec {
            name: "write_file",
            output: Value::Null,
        };
        let executors: [&dyn ToolInvocationExecutor; 1] = [&stub];
        let ctx = RuntimeToolContext {
            workspace_root: std::path::Path::new("/ws"),
            required_permission: PermissionMode::WorkspaceWrite,
            path_targets: vec![PathTarget::new("/ws/coolzhu.toml", PathAccess::Write)],
            protected_rules: &rules,
            session_grant: SessionGrantView::default(),
            executors: &executors,
            profile: PermissionProfile::OutsideApproval,
        };
        let out = runtime_tool_execute(invoke("write_file"), &ctx);
        assert_eq!(out.status, ToolOutcomeStatus::DryRunOnly);
        assert_eq!(
            out.permission_gate.decision,
            PermissionDecision::RequireConfirm
        );
        assert_eq!(
            out.permission_gate.protected_match.as_deref(),
            Some("coolzhu-config")
        );
    }

    #[test]
    fn runtime_tool_execute_unknown_tool_returns_failed() {
        let rules = default_protected_rules();
        let executors: [&dyn ToolInvocationExecutor; 0] = [];
        let ctx = RuntimeToolContext {
            workspace_root: std::path::Path::new("/ws"),
            required_permission: PermissionMode::ReadOnly,
            path_targets: vec![],
            protected_rules: &rules,
            session_grant: SessionGrantView::default(),
            executors: &executors,
            profile: PermissionProfile::OutsideApproval,
        };
        let out = runtime_tool_execute(invoke("nonexistent"), &ctx);
        assert_eq!(out.status, ToolOutcomeStatus::Failed);
        assert!(out.summary_text.contains("unknown tool"));
    }

    #[test]
    fn runtime_tool_execute_overwrites_executor_placeholder_gate() {
        let rules = default_protected_rules();
        let stub = StubExec {
            name: "read_file",
            output: Value::Null,
        };
        let executors: [&dyn ToolInvocationExecutor; 1] = [&stub];
        let ctx = RuntimeToolContext {
            workspace_root: std::path::Path::new("/ws"),
            required_permission: PermissionMode::ReadOnly,
            path_targets: vec![],
            protected_rules: &rules,
            session_grant: SessionGrantView::default(),
            executors: &executors,
            profile: PermissionProfile::OutsideApproval,
        };
        let out = runtime_tool_execute(invoke("read_file"), &ctx);
        assert_eq!(
            out.permission_gate.decision,
            PermissionDecision::AllowAuto,
            "gate must come from runtime, not executor"
        );
    }
}
