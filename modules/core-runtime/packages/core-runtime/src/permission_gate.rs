//! 工具权限闸门（REQ-TOOL-008/010）。
//!
//! `evaluate_permission` 是一个纯函数：接收 `ToolInvoke` + `PermissionMode` +
//! 路径效果 + workspace + Protected 规则，返回 `PermissionGateReport`，不读
//! 任何全局状态、不触发 I/O，方便 TDD。
//!
//! 三种授权模式：
//! 1. `FullAccess`     完全访问已开启 → 不拦截、不要求确认。
//! 2. `WorkspaceAuto`  workspace 内默认放行，Protected 路径除外；workspace 外要求授权。
//! 3. `OutsideApproval` 保守审批模式，workspace 外路径访问要求授权。
//!
//! 参考：`docs/tool-calling-permission-plan-2026-05-10.md` §5。

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::permissions::{PermissionMode, PermissionProfile};
use crate::tool::{PermissionDecision, PermissionGateReport, ToolInvoke};

/// 工具对路径的访问类型。`bash` 等无法静态分析的工具返回空 `Vec`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PathAccess {
    Read,
    Write,
    Execute,
}

impl PathAccess {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Execute => "execute",
        }
    }
}

/// 工具调用时声明将要访问的路径（由 `TargetPathsExtractor` 产出）。
#[derive(Debug, Clone)]
pub struct PathTarget {
    pub raw: PathBuf,
    pub access: PathAccess,
}

impl PathTarget {
    #[must_use]
    pub fn new(raw: impl Into<PathBuf>, access: PathAccess) -> Self {
        Self {
            raw: raw.into(),
            access,
        }
    }
}

/// Protected 路径规则。`access = "any"` 表示任何访问类型都命中。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedRule {
    pub id: String,
    pub glob: String,
    #[serde(default = "default_access_any")]
    pub access: String,
}

fn default_access_any() -> String {
    "any".to_string()
}

/// 默认 Protected 规则。`coolzhu.toml` 中 `[tool.protected_paths]` 未配置时生效。
#[must_use]
pub fn default_protected_rules() -> Vec<ProtectedRule> {
    vec![
        ProtectedRule {
            id: "coolzhu-config".into(),
            glob: "**/coolzhu.toml".into(),
            access: "any".into(),
        },
        ProtectedRule {
            id: "coolzhu-data".into(),
            glob: "**/.coolzhu/**".into(),
            access: "any".into(),
        },
        ProtectedRule {
            id: "ssh-keys".into(),
            glob: "**/.ssh/**".into(),
            access: "any".into(),
        },
        ProtectedRule {
            id: "git-internal".into(),
            glob: "**/.git/**".into(),
            access: "write".into(),
        },
        ProtectedRule {
            id: "env-file".into(),
            glob: "**/.env*".into(),
            access: "any".into(),
        },
    ]
}

#[must_use]
fn access_matches(rule: &str, access: PathAccess) -> bool {
    match rule {
        "any" | "*" => true,
        "read" => matches!(access, PathAccess::Read),
        "write" => matches!(access, PathAccess::Write),
        "execute" => matches!(access, PathAccess::Execute),
        _ => false,
    }
}

/// 归一化路径以便匹配和 workspace 前缀判定。
///
/// 不跟随 symlink；`..` 组件被逻辑上合并（不落盘检查）。实际 symlink 逃逸在
/// 调用方（web-console）侧再做一次 `symlink_metadata` 检查。
#[must_use]
pub fn normalize_for_match(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            Component::Prefix(p) => {
                out.push(p.as_os_str());
            }
            Component::RootDir => {
                out.push(comp.as_os_str());
            }
            Component::Normal(seg) => {
                out.push(seg);
            }
        }
    }
    // 统一为小写（Windows 不区分大小写；Linux 精确匹配也兼容）
    let mut s = out.to_string_lossy().replace('\\', "/").to_lowercase();
    if let Some(stripped) = s.strip_prefix("//?/") {
        s = stripped.to_string();
    } else if let Some(stripped) = s.strip_prefix("//./") {
        s = stripped.to_string();
    }
    PathBuf::from(s)
}

/// 判断一个路径是否在 workspace 根目录之内。
#[must_use]
pub fn is_path_inside_workspace(path: &Path, workspace_root: &Path) -> bool {
    let path = normalize_for_match(path);
    let root = normalize_for_match(workspace_root);
    path.starts_with(&root)
}

/// 匹配 Protected 规则。返回命中的 rule id。
#[must_use]
pub fn match_protected_rule<'a>(
    rules: &'a [ProtectedRule],
    path: &Path,
    access: PathAccess,
) -> Option<&'a ProtectedRule> {
    let normalized = normalize_for_match(path);
    let target = normalized.to_string_lossy().to_string();
    for rule in rules {
        if !access_matches(rule.access.as_str(), access) {
            continue;
        }
        match glob::Pattern::new(&rule.glob.to_lowercase()) {
            Ok(pattern) => {
                if pattern.matches(&target) {
                    return Some(rule);
                }
            }
            Err(_) => continue,
        }
    }
    None
}

/// 会话级授权状态。`SessionGrants` 由 runtime 维护，这里仅接受一个副本用来判定。
#[derive(Debug, Default, Clone)]
pub struct SessionGrantView {
    pub session_authorized: bool,
    pub session_confirmed_twice: bool,
}

impl SessionGrantView {
    #[must_use]
    pub fn combine_with_invoke(&self, invoke: &ToolInvoke) -> (bool, bool) {
        (
            self.session_authorized || invoke.user_authorized,
            self.session_confirmed_twice || invoke.user_confirmed_twice,
        )
    }
}

/// 闸门判定主入口。
///
/// - `targets` 为空（例如 `bash`）：视为"workspace 外"，走 DangerFullAccess 外部规则。
/// - 任一 target 命中 Protected：直接 `RequireApproval + RequireConfirm`，短路。
/// - 所有 targets 都在 workspace 内：`workspace_relative = true`。
/// - `profile` 控制三种权限模式：
///   - `FullAccess`：全部工具 AllowAuto，不再被 Protected 规则拦截。
///   - `WorkspaceAuto`：workspace 内全部 AllowAuto；外按等级审批。
///   - `OutsideApproval`：保持原有分级审批行为。
#[must_use]
pub fn evaluate_permission(
    invoke: &ToolInvoke,
    required: PermissionMode,
    targets: &[PathTarget],
    workspace_root: &Path,
    protected_rules: &[ProtectedRule],
    session_grant: &SessionGrantView,
    profile: PermissionProfile,
) -> PermissionGateReport {
    // 0) 未知/动态工具缺少最低权限元数据：明确配置错误，fail-closed。
    //    先于 FullAccess 分支返回，避免 dev-open / full-access 把它放行——
    //    这里要拒绝的不是"权限不够"，而是"根本没有权威元数据可判定"。
    if required.is_unspecified() {
        return PermissionGateReport::deny(
            required,
            PermissionMode::missing_metadata_reason(&invoke.tool_name),
        );
    }

    let (authorized, confirmed_twice) = session_grant.combine_with_invoke(invoke);

    let mut workspace_relative = true;
    let mut affected_paths = Vec::new();
    let mut protected_hit: Option<String> = None;

    let has_path_targets = !targets.is_empty();

    if targets.is_empty() {
        // 无法静态分析的工具（bash / PowerShell / REPL / Agent）。
        workspace_relative = false;
        affected_paths.push("<opaque-command>".to_string());
    } else {
        for t in targets {
            if !is_path_inside_workspace(&t.raw, workspace_root) {
                workspace_relative = false;
            }
            affected_paths.push(display_path_summary(&t.raw));
            if let Some(rule) = match_protected_rule(protected_rules, &t.raw, t.access) {
                protected_hit = Some(rule.id.clone());
            }
        }
    }

    // 1) 完全访问权限开启时不做任何拦截。仍保留 affected_paths 便于审计。
    if matches!(profile, PermissionProfile::FullAccess) {
        return PermissionGateReport {
            required,
            decision: PermissionDecision::AllowAuto,
            reason: "full-access-profile".to_string(),
            protected_match: None,
            workspace_relative,
            affected_paths,
        };
    }

    // 2) Protected 优先级高于普通 workspace 自动放行。
    if let Some(rule_id) = protected_hit.clone() {
        let decision = if confirmed_twice && authorized {
            PermissionDecision::AllowApproved
        } else {
            PermissionDecision::RequireConfirm
        };
        return PermissionGateReport {
            required,
            decision,
            reason: format!("protected-rule-hit:{rule_id}"),
            protected_match: Some(rule_id),
            workspace_relative,
            affected_paths,
        };
    }

    // 3) 根据 required + workspace 边界 + profile 分级
    match profile {
        PermissionProfile::FullAccess => PermissionGateReport {
            required,
            decision: PermissionDecision::AllowAuto,
            reason: "full-access-profile".to_string(),
            protected_match: None,
            workspace_relative,
            affected_paths,
        },

        PermissionProfile::WorkspaceAuto => {
            if workspace_relative {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::AllowAuto,
                    reason: "inside-workspace-workspace-auto-profile".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            } else {
                apply_standard_permission(
                    required,
                    workspace_relative,
                    has_path_targets,
                    authorized,
                    confirmed_twice,
                    affected_paths,
                )
            }
        }

        PermissionProfile::OutsideApproval => apply_standard_permission(
            required,
            workspace_relative,
            has_path_targets,
            authorized,
            confirmed_twice,
            affected_paths,
        ),
    }
}

fn apply_standard_permission(
    required: PermissionMode,
    workspace_relative: bool,
    has_path_targets: bool,
    authorized: bool,
    confirmed_twice: bool,
    affected_paths: Vec<String>,
) -> PermissionGateReport {
    match required {
        // 缺少最低权限元数据：明确配置错误，不按任何档位放行。
        // 纵深防御——正常入口已在 `evaluate_permission` 顶部拒绝（那里带工具名）。
        PermissionMode::Unspecified => PermissionGateReport::deny(
            required,
            "tool has no declared minimum-permission metadata (configuration error)",
        ),
        PermissionMode::ReadOnly | PermissionMode::Allow => {
            if !has_path_targets || workspace_relative {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::AllowAuto,
                    reason: if workspace_relative {
                        "read-only-inside-workspace".to_string()
                    } else {
                        "pathless-read-only-tool".to_string()
                    },
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            } else if authorized {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::AllowApproved,
                    reason: "read-only-outside-with-grant".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            } else {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::RequireApproval,
                    reason: "read-only-outside-workspace".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            }
        }

        PermissionMode::WorkspaceWrite => {
            if workspace_relative {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::AllowAuto,
                    reason: "inside-workspace".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            } else if authorized {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::AllowApproved,
                    reason: "workspace-write-outside-with-grant".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            } else {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::RequireApproval,
                    reason: "workspace-write-outside".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            }
        }

        PermissionMode::DangerFullAccess | PermissionMode::Prompt => {
            if workspace_relative {
                if authorized {
                    PermissionGateReport {
                        required,
                        decision: PermissionDecision::AllowApproved,
                        reason: "danger-inside-with-grant".to_string(),
                        protected_match: None,
                        workspace_relative,
                        affected_paths,
                    }
                } else {
                    PermissionGateReport {
                        required,
                        decision: PermissionDecision::RequireApproval,
                        reason: "danger-inside-needs-first-approval".to_string(),
                        protected_match: None,
                        workspace_relative,
                        affected_paths,
                    }
                }
            } else if authorized && confirmed_twice {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::AllowApproved,
                    reason: "danger-outside-with-confirm".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            } else {
                PermissionGateReport {
                    required,
                    decision: PermissionDecision::RequireConfirm,
                    reason: "danger-outside".to_string(),
                    protected_match: None,
                    workspace_relative,
                    affected_paths,
                }
            }
        }
    }
}

fn display_path_summary(p: &Path) -> String {
    let s = p.to_string_lossy();
    if s.len() > 120 {
        format!("{}…", &s[..120])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::ToolCaller;
    use serde_json::json;

    fn invoke(name: &str) -> ToolInvoke {
        ToolInvoke {
            call_id: "c1".into(),
            tool_name: name.into(),
            input: json!({}),
            caller: ToolCaller::Llm,
            workspace_id: "ws-1".into(),
            session_id: None,
            user_authorized: false,
            user_confirmed_twice: false,
        }
    }

    fn outside_approval() -> PermissionProfile {
        PermissionProfile::OutsideApproval
    }

    #[test]
    fn readonly_path_outside_requires_approval() {
        let inv = invoke("read_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::ReadOnly,
            &[PathTarget::new("/other/place/x", PathAccess::Read)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::RequireApproval);
    }

    #[test]
    fn pathless_readonly_tool_is_auto() {
        let inv = invoke("WebSearch");
        let report = evaluate_permission(
            &inv,
            PermissionMode::ReadOnly,
            &[],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::AllowAuto);
    }

    #[test]
    fn workspace_write_inside_is_auto() {
        let inv = invoke("write_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::WorkspaceWrite,
            &[PathTarget::new("/home/u/ws/sub/x.rs", PathAccess::Write)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::AllowAuto);
        assert!(report.workspace_relative);
    }

    #[cfg(windows)]
    #[test]
    fn extended_length_workspace_prefix_matches_normal_windows_path() {
        assert!(is_path_inside_workspace(
            Path::new(r"C:\Users\example\coolzhuagent\tool-regression.txt"),
            Path::new(r"\\?\C:\Users\example\coolzhuagent")
        ));
    }

    #[test]
    fn workspace_write_outside_needs_approval() {
        let inv = invoke("write_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::WorkspaceWrite,
            &[PathTarget::new("/tmp/evil", PathAccess::Write)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::RequireApproval);
        assert!(!report.workspace_relative);
    }

    #[test]
    fn danger_bash_empty_targets_treated_as_outside() {
        let inv = invoke("bash");
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::RequireConfirm);
        assert!(!report.workspace_relative);
    }

    #[test]
    fn danger_bash_after_confirm_allowed() {
        let mut inv = invoke("bash");
        inv.user_authorized = true;
        inv.user_confirmed_twice = true;
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::AllowApproved);
    }

    #[test]
    fn protected_coolzhu_toml_always_blocks_read() {
        let inv = invoke("read_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::ReadOnly,
            &[PathTarget::new("/home/u/ws/coolzhu.toml", PathAccess::Read)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::RequireConfirm);
        assert_eq!(report.protected_match.as_deref(), Some("coolzhu-config"));
    }

    #[test]
    fn protected_dotcoolzhu_dir_blocks_any_access() {
        let inv = invoke("read_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::ReadOnly,
            &[PathTarget::new(
                "/home/u/ws/.coolzhu/web-sessions.sqlite3",
                PathAccess::Read,
            )],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::RequireConfirm);
        assert_eq!(report.protected_match.as_deref(), Some("coolzhu-data"));
    }

    #[test]
    fn protected_confirmed_grants_run() {
        let mut inv = invoke("write_file");
        inv.user_authorized = true;
        inv.user_confirmed_twice = true;
        let report = evaluate_permission(
            &inv,
            PermissionMode::WorkspaceWrite,
            &[PathTarget::new(
                "/home/u/ws/coolzhu.toml",
                PathAccess::Write,
            )],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::AllowApproved);
        assert_eq!(report.protected_match.as_deref(), Some("coolzhu-config"));
    }

    #[test]
    fn dotdot_escape_detected() {
        let inv = invoke("write_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::WorkspaceWrite,
            &[PathTarget::new(
                "/home/u/ws/sub/../../outside",
                PathAccess::Write,
            )],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert!(!report.workspace_relative);
        assert_eq!(report.decision, PermissionDecision::RequireApproval);
    }

    #[test]
    fn session_grant_propagates_to_danger_inside() {
        let inv = invoke("bash");
        let grant = SessionGrantView {
            session_authorized: true,
            session_confirmed_twice: false,
        };
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[PathTarget::new("/home/u/ws/target/x.log", PathAccess::Read)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &grant,
            outside_approval(),
        );
        assert_eq!(report.decision, PermissionDecision::AllowApproved);
    }

    #[test]
    fn env_file_protected_rule() {
        let inv = invoke("read_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::ReadOnly,
            &[PathTarget::new("/home/u/ws/.env.local", PathAccess::Read)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            outside_approval(),
        );
        assert_eq!(report.protected_match.as_deref(), Some("env-file"));
    }

    #[test]
    fn workspace_auto_profile_danger_inside_auto() {
        let inv = invoke("bash");
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[PathTarget::new(
                "/home/u/ws/target/x.sh",
                PathAccess::Execute,
            )],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            PermissionProfile::WorkspaceAuto,
        );
        assert_eq!(report.decision, PermissionDecision::AllowAuto);
        assert!(report.workspace_relative);
    }

    #[test]
    fn workspace_auto_profile_danger_outside_confirm() {
        let inv = invoke("write_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[PathTarget::new("/tmp/outside_script.sh", PathAccess::Write)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            PermissionProfile::WorkspaceAuto,
        );
        assert_eq!(report.decision, PermissionDecision::RequireConfirm);
        assert!(!report.workspace_relative);
    }

    #[test]
    fn workspace_auto_profile_opaque_command_without_cwd_requires_confirm() {
        let inv = invoke("bash");
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            PermissionProfile::WorkspaceAuto,
        );
        assert_eq!(report.decision, PermissionDecision::RequireConfirm);
    }

    #[test]
    fn workspace_auto_profile_command_with_workspace_cwd_auto() {
        let inv = invoke("bash");
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[PathTarget::new("/home/u/ws", PathAccess::Execute)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            PermissionProfile::WorkspaceAuto,
        );
        assert_eq!(report.decision, PermissionDecision::AllowAuto);
    }

    #[test]
    fn workspace_auto_profile_write_outside_approval() {
        let inv = invoke("write_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::WorkspaceWrite,
            &[PathTarget::new("/tmp/other", PathAccess::Write)],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            PermissionProfile::WorkspaceAuto,
        );
        assert_eq!(report.decision, PermissionDecision::RequireApproval);
    }

    #[test]
    fn full_access_profile_allows_danger_outside() {
        let inv = invoke("bash");
        let report = evaluate_permission(
            &inv,
            PermissionMode::DangerFullAccess,
            &[],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            PermissionProfile::FullAccess,
        );
        assert_eq!(report.decision, PermissionDecision::AllowAuto);
    }

    #[test]
    fn full_access_profile_bypasses_protected() {
        let inv = invoke("write_file");
        let report = evaluate_permission(
            &inv,
            PermissionMode::WorkspaceWrite,
            &[PathTarget::new(
                "/home/u/ws/coolzhu.toml",
                PathAccess::Write,
            )],
            Path::new("/home/u/ws"),
            &default_protected_rules(),
            &SessionGrantView::default(),
            PermissionProfile::FullAccess,
        );
        assert_eq!(report.decision, PermissionDecision::AllowAuto);
        assert_eq!(report.protected_match, None);
    }
}
