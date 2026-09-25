//! 工具路径效果抽取（REQ-TOOL-008）。
//!
//! `TargetPathsExtractor` 从工具入参中解析出它将要读 / 写 / 执行的路径集合，供
//! `core-runtime::permission_gate::evaluate_permission` 判断是否越出 workspace
//! 或命中 Protected 规则。`bash` / `PowerShell` / `REPL` / `Agent` 等无法
//! 静态分析命令内容的工具会优先抽取 `cwd` 作为执行边界；没有 `cwd` 时返回空
//! `Vec<PathTarget>`，由闸门视为 workspace 外。
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

/// `bash` / `PowerShell` / `REPL` / `Agent`：命令内容无法可靠静态分析，使用 cwd 做边界。
pub struct OpaqueCommandExtractor;
impl TargetPathsExtractor for OpaqueCommandExtractor {
    fn extract(&self, input: &Value) -> Vec<PathTarget> {
        input
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|cwd| vec![PathTarget::new(PathBuf::from(cwd), PathAccess::Execute)])
            .unwrap_or_default()
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

/// 根据工具名称返回对应抽取器。未知工具回退到 `OpaqueCommandExtractor`。
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
    fn opaque_command_uses_cwd_as_execute_boundary() {
        let e = extractor_for("PowerShell");
        let out = e.extract(&json!({
            "command": "Get-ChildItem",
            "cwd": "C:/workspace"
        }));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].access, PathAccess::Execute);
        assert_eq!(out[0].raw.to_string_lossy(), "C:/workspace");
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
