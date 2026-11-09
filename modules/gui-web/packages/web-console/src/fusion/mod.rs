//! Fusion Team（模型聚合会话）P0 实现。
//! 设计：docs/plans/2026-11-09-fusion-team-design.md。
//! 三角色：主持（强模型，规划/审核/验收/用户接口）、执行者（写代码/跑测试）、
//! 查询者（只读检索）。角色上下文独立，只交换 brief/结果；编排归宿主进程。
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::PathBuf;

pub(super) mod runtime;
pub(super) mod store;
pub(super) mod team_tools;

pub(crate) const FUSION_TARGET_PREFIX: &str = "fusion:";

/// 会话发送对象是否为 Fusion 伪目标（含混入情况；严格校验在 fusion_team_id_of）。
pub(crate) fn targets_include_fusion(target_agent_ids: &[String]) -> bool {
    target_agent_ids
        .iter()
        .any(|id| id.trim().starts_with(FUSION_TARGET_PREFIX))
}

/// Fusion 必须单独作为发送对象；混选普通会话直接拒绝。
pub(crate) fn fusion_team_id_of(target_agent_ids: &[String]) -> Result<String, ApiError> {
    let fusion = target_agent_ids
        .iter()
        .filter(|id| id.trim().starts_with(FUSION_TARGET_PREFIX))
        .count();
    if fusion != 1 || target_agent_ids.len() != 1 {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "Fusion Team 必须单独作为发送对象，不能与普通会话混选或群发",
        ));
    }
    let team_id = target_agent_ids[0]
        .trim()
        .strip_prefix(FUSION_TARGET_PREFIX)
        .unwrap_or_default()
        .trim()
        .to_string();
    if team_id.is_empty() {
        return Err(api_error(StatusCode::BAD_REQUEST, "Fusion Team 标识无效"));
    }
    Ok(team_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FusionRole {
    Host,
    Executor,
    Querier,
}

impl FusionRole {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Executor => "executor",
            Self::Querier => "querier",
        }
    }

    pub(crate) fn display_name(self) -> &'static str {
        match self {
            Self::Host => "主持",
            Self::Executor => "执行者",
            Self::Querier => "查询者",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "host" => Some(Self::Host),
            "executor" => Some(Self::Executor),
            "querier" => Some(Self::Querier),
            _ => None,
        }
    }
}

/// 注入角色模型循环的 task-local：team_* 工具的执行身份与数据归属。
/// brief 不能携带/放大授权——派发矩阵与权限由宿主按此作用域判定。
#[derive(Debug, Clone)]
pub(crate) struct FusionToolScope {
    pub team_id: String,
    pub room_id: String,
    pub workspace_id: String,
    pub db_path: PathBuf,
    pub role: FusionRole,
    /// 当前角色轮次对应的 brief（执行者/查询者在 brief 上下文中运行；主持为 None）。
    pub brief_id: Option<String>,
    /// 本角色轮次归属的任务（主持首轮尚未建任务时为 None，工具执行时按房间+团队就近解析）。
    pub task_id: Option<String>,
    pub host_web_access: bool,
    /// 查询者是否独立于执行者模型；同模型时执行者不暴露 team_query。
    pub querier_is_separate: bool,
}

tokio::task_local! {
    static FUSION_SCOPE: FusionToolScope;
}

pub(crate) fn current_scope() -> Option<FusionToolScope> {
    FUSION_SCOPE.try_with(Clone::clone).ok()
}

pub(crate) async fn scope<F: std::future::Future>(value: FusionToolScope, future: F) -> F::Output {
    FUSION_SCOPE.scope(value, future).await
}

/// 该角色轮次归属的任务：优先 scope 固定值，再按房间+团队取最近打开的 task。
pub(super) fn resolve_task_id(scope: &FusionToolScope) -> Option<String> {
    if let Some(task_id) = scope.task_id.as_deref() {
        return Some(task_id.to_string());
    }
    store::fusion_task_latest_open(&scope.db_path, &scope.room_id, &scope.team_id)
        .ok()
        .flatten()
        .map(|task| task.id)
}

// ── 角色工具策略（接入 llm_tool_definitions_for_session 的融合分支）────────────

/// 各角色不允许使用的内建工具（无论房间权限多大）。
fn role_keeps_builtin_tool(role: FusionRole, name: &str, host_web_access: bool) -> bool {
    let normalized = name.replace('-', "_");
    match normalized.as_str() {
        // 角色之间不走 chat_handoff/Agent/语义调度递归派发；协调只走 team_*。
        "Agent" | "chat_handoff" | "chat.handoff" | "tools_semantic_dispatch"
        | "tools.semantic_dispatch" | "ToolSearch" => false,
        // 桌面/视觉/计算机操作不进角色工具表。
        "computer_use" | "computer_use_perform" => false,
        // 房间 TODO 清单只给主持（规划落点）；执行者/查询者不写清单。
        "TodoWrite" => role == FusionRole::Host,
        // 后台任务与进程管理只属于执行者。
        "Sleep" | "TaskOutput" | "TaskStop" | "SendUserMessage" => role == FusionRole::Executor,
        "WebSearch" | "WebFetch" => match role {
            FusionRole::Querier => true,
            FusionRole::Executor => true,
            FusionRole::Host => host_web_access,
        },
        _ if normalized.starts_with("mcp__")
            || normalized.starts_with(dsh_execution::PREFIX)
            || plugin_runtime::is_plugin_name(name) => false,
        _ => true,
    }
}

/// 角色权限上限：主持/查询者恒只读；执行者沿用房间权限（WorkspaceWrite/完全访问）。
fn role_permission(role: FusionRole, room_permission: PermissionMode) -> PermissionMode {
    match role {
        FusionRole::Host | FusionRole::Querier => PermissionMode::ReadOnly,
        FusionRole::Executor => room_permission,
    }
}

/// `llm_tool_definitions_for_session` 的融合分支：scope 命中即返回角色工具表。
pub(crate) fn role_tool_definitions(
    scope: &FusionToolScope,
    room_permission: PermissionMode,
    settings: &SessionModelLimitOverride,
) -> Option<Vec<ToolDefinition>> {
    let granted = role_permission(scope.role, room_permission);
    let mut defs = llm_tool_definitions_with_settings(granted, Some(settings)).unwrap_or_default();
    defs.retain(|definition| {
        role_keeps_builtin_tool(scope.role, &definition.name, scope.host_web_access)
    });
    defs.extend(team_tools::definitions_for(
        scope.role,
        scope.querier_is_separate,
    ));
    (!defs.is_empty()).then_some(defs)
}

/// `run_model_tool_dispatch_within_root` 的 team_* 分支。
pub(crate) async fn execute_team_tool(
    normalized_name: &str,
    input: &JsonValue,
) -> ApiResult<ToolDispatchResponse> {
    team_tools::execute(normalized_name, input).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fusion_target_requires_single_pseudo_id() {
        assert_eq!(
            fusion_team_id_of(&["fusion:t1".to_string()]).expect("单个伪目标"),
            "t1"
        );
        assert!(fusion_team_id_of(&["fusion:t1".to_string(), "sess-1".to_string()]).is_err());
        assert!(fusion_team_id_of(&["fusion:".to_string()]).is_err());
        assert!(fusion_team_id_of(&[]).is_err());
        assert!(targets_include_fusion(&["fusion:t1".to_string()]));
        assert!(!targets_include_fusion(&["sess-1".to_string()]));
    }

    #[test]
    fn host_web_search_follows_switch() {
        // 默认关闭：主持没有联网工具；开启后可用。
        assert!(!role_keeps_builtin_tool(FusionRole::Host, "WebSearch", false));
        assert!(!role_keeps_builtin_tool(FusionRole::Host, "WebFetch", false));
        assert!(role_keeps_builtin_tool(FusionRole::Host, "WebSearch", true));
        assert!(role_keeps_builtin_tool(FusionRole::Host, "WebFetch", true));
        // 执行者/查询者不受开关影响。
        assert!(role_keeps_builtin_tool(FusionRole::Querier, "WebSearch", false));
        assert!(role_keeps_builtin_tool(FusionRole::Executor, "WebFetch", false));
    }

    #[test]
    fn role_tool_policy_blocks_delegation_and_desktop() {
        for role in [FusionRole::Host, FusionRole::Executor, FusionRole::Querier] {
            assert!(!role_keeps_builtin_tool(role, "chat_handoff", true));
            assert!(!role_keeps_builtin_tool(role, "computer_use_perform", true));
            assert!(!role_keeps_builtin_tool(role, "tools_semantic_dispatch", true));
        }
        // 房间 TODO 只给主持。
        assert!(role_keeps_builtin_tool(FusionRole::Host, "TodoWrite", false));
        assert!(!role_keeps_builtin_tool(FusionRole::Executor, "TodoWrite", false));
        assert!(!role_keeps_builtin_tool(FusionRole::Querier, "TodoWrite", false));
        // 后台任务/进程管理只给执行者。
        assert!(role_keeps_builtin_tool(FusionRole::Executor, "Sleep", false));
        assert!(!role_keeps_builtin_tool(FusionRole::Host, "TaskStop", false));
        assert!(!role_keeps_builtin_tool(FusionRole::Querier, "TaskOutput", false));
    }

    #[test]
    fn role_permission_caps_host_and_querier_readonly() {
        assert_eq!(
            role_permission(FusionRole::Host, PermissionMode::DangerFullAccess),
            PermissionMode::ReadOnly
        );
        assert_eq!(
            role_permission(FusionRole::Querier, PermissionMode::WorkspaceWrite),
            PermissionMode::ReadOnly
        );
        assert_eq!(
            role_permission(FusionRole::Executor, PermissionMode::DangerFullAccess),
            PermissionMode::DangerFullAccess
        );
    }
}
