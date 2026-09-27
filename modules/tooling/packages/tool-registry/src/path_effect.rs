//! 工具路径效果抽取（REQ-TOOL-008）。
//!
//! `TargetPathsExtractor` 从工具入参中解析出它将要读 / 写 / 执行的路径集合，供
//! `core-runtime::permission_gate::evaluate_permission` 判断是否越出 workspace
//! 或命中 Protected 规则。`bash` / `PowerShell` / `REPL` / `Agent` 等无法
//! 静态分析命令内容的工具始终返回空 `Vec<PathTarget>`，由闸门视为范围未知。
//! `cwd` 只是启动目录，不证明命令无法访问目录外文件。
//!
//! 该模块只解析 JSON 入参、返回 `PathTarget`，不读文件系统，便于 TDD。
//!
//! 参考：`docs/tool-calling-permission-plan-2026-05-10.md` §5.2。

use std::path::PathBuf;

use runtime::{PathAccess, PathTarget};
use serde_json::Value;

/// 为每个工具提供入参 → 路径效果映射。
pub trait TargetPathsExtractor: Send + Sync {
    /// 返回工具将要读 / 写的绝对或工程相对路径（未归一化）。
    fn extract(&self, input: &Value) -> Vec<PathTarget>;
}

/// `bash` / `PowerShell` / `REPL` / `Agent`：无法从启动目录证明实际访问范围。
pub struct OpaqueCommandExtractor;
impl TargetPathsExtractor for OpaqueCommandExtractor {
    fn extract(&self, _input: &Value) -> Vec<PathTarget> {
        Vec::new()
    }
}

/// `read_file` / `glob_search` / `grep_search` / `Skill`（带 `skill` 路径）。
/// 字段：`path`（优先）、`file_path`（兼容），access = Read。
pub struct ReadPathExtractor {
    pub field: &'static str,
    pub access: PathAccess,
}

impl ReadPathExtractor {
    #[must_use]
    pub fn new(field: &'static str) -> Self {
        Self {
            field,
            access: PathAccess::Read,
        }
    }
}

impl TargetPathsExtractor for ReadPathExtractor {
    fn extract(&self, input: &Value) -> Vec<PathTarget> {
        let candidates = [self.field, "path", "file_path"];
        let mut seen = String::new();
        for key in candidates {
            if let Some(v) = input.get(key).and_then(Value::as_str) {
                seen = v.to_string();
                break;
            }
        }
        if seen.is_empty() {
            return Vec::new();
        }
        vec![PathTarget::new(PathBuf::from(seen), self.access)]
    }
}

/// `write_file` / `edit_file` / `NotebookEdit` / `Config`：字段 `path` / `file_path`，access = Write。
pub struct WritePathExtractor;
impl TargetPathsExtractor for WritePathExtractor {
    fn extract(&self, input: &Value) -> Vec<PathTarget> {
        let candidates = ["path", "file_path", "notebook_path"];
        let mut out = Vec::new();
        for key in candidates {
            if let Some(v) = input.get(key).and_then(Value::as_str) {
                out.push(PathTarget::new(PathBuf::from(v), PathAccess::Write));
                break;
            }
        }
        out
    }
}

/// `TodoWrite`：无明确路径目标，视为空（后续由运行时按 workspace 默认 audit 目录登记）。
pub struct NoPathExtractor;
impl TargetPathsExtractor for NoPathExtractor {
    fn extract(&self, _input: &Value) -> Vec<PathTarget> {
        Vec::new()
    }
}

/// 根据工具名称返回对应抽取器；未知工具的最低权限另由门禁元数据拒绝。
#[must_use]
pub fn extractor_for(tool_name: &str) -> Box<dyn TargetPathsExtractor> {
    match tool_name {
        "bash" | "PowerShell" | "REPL" | "Agent" => Box::new(OpaqueCommandExtractor),
        "read_file" => Box::new(ReadPathExtractor::new("path")),
        "glob_search" | "grep_search" => Box::new(ReadPathExtractor::new("path")),
        "write_file" | "edit_file" | "Config" | "NotebookEdit" => Box::new(WritePathExtractor),
        // 其它内置工具 (WebFetch/WebSearch/Skill/ToolSearch/Sleep/SendUserMessage/StructuredOutput)
        // 无文件系统副作用，由 ReadOnly 自动放行即可。
        _ => Box::new(NoPathExtractor),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn opaque_bash_returns_empty() {
        let e = extractor_for("bash");
        let out = e.extract(&json!({"command": "ls -al /etc"}));
        assert!(out.is_empty());
    }

    #[test]
    fn opaque_command_cwd_does_not_prove_workspace_boundary() {
        for name in ["bash", "PowerShell", "REPL", "Agent"] {
            let targets = extractor_for(name).extract(&json!({
                "command": "Get-Content C:/outside/secret.txt",
                "cwd": "C:/workspace"
            }));
            assert!(targets.is_empty(), "{name} 的 cwd 不能冒充文件系统隔离");
        }
    }

    #[test]
    fn workspace_auto_shell_with_workspace_cwd_still_needs_confirm() {
        let input = json!({"command": "Get-Content C:/outside/secret.txt", "cwd": "C:/workspace"});
        let targets = extractor_for("PowerShell").extract(&input);
        let mut invoke = runtime::ToolInvoke {
            call_id: "cwd-gate-test".to_string(), tool_name: "PowerShell".to_string(), input,
            caller: runtime::ToolCaller::Llm, workspace_id: "test-workspace".to_string(),
            session_id: None, user_authorized: false, user_confirmed_twice: false,
        };
        let evaluate = |invoke: &runtime::ToolInvoke, profile| runtime::evaluate_permission(
            invoke, runtime::PermissionMode::DangerFullAccess, &targets,
            std::path::Path::new("C:/workspace"), &[],
            &runtime::SessionGrantView::default(), profile,
        );
        let report = evaluate(&invoke, runtime::PermissionProfile::WorkspaceAuto);
        assert_eq!(report.decision, runtime::PermissionDecision::RequireConfirm);
        assert!(!report.workspace_relative);
        invoke.user_authorized = true;
        invoke.user_confirmed_twice = true;
        assert_eq!(evaluate(&invoke, runtime::PermissionProfile::WorkspaceAuto).decision,
            runtime::PermissionDecision::AllowApproved);
        invoke.user_authorized = false;
        invoke.user_confirmed_twice = false;
        assert_eq!(evaluate(&invoke, runtime::PermissionProfile::FullAccess).decision,
            runtime::PermissionDecision::AllowAuto);
    }

    #[test]
    fn read_file_extracts_path() {
        let e = extractor_for("read_file");
        let out = e.extract(&json!({"path": "src/main.rs"}));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].access, PathAccess::Read);
        assert_eq!(out[0].raw.to_string_lossy(), "src/main.rs");
    }

    #[test]
    fn write_file_extracts_path_as_write() {
        let e = extractor_for("write_file");
        let out = e.extract(&json!({"path": "src/new.rs", "content": "..."}));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].access, PathAccess::Write);
    }

    #[test]
    fn notebook_edit_uses_notebook_path() {
        let e = extractor_for("NotebookEdit");
        let out = e.extract(&json!({
            "notebook_path": "/abs/a.ipynb",
            "new_source": "..."
        }));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].raw.to_string_lossy(), "/abs/a.ipynb");
    }

    #[test]
    fn unknown_tool_returns_no_path() {
        let e = extractor_for("WebFetch");
        let out = e.extract(&json!({"url": "https://example.com", "prompt": "x"}));
        assert!(out.is_empty());
    }

    #[test]
    fn missing_path_returns_empty() {
        let e = extractor_for("read_file");
        let out = e.extract(&json!({"query": "x"}));
        assert!(out.is_empty());
    }
}
