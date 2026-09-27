use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{OnceLock, RwLock};
use std::time::{Duration, Instant};

pub mod path_effect;

use api::{
    max_tokens_for_model, resolve_model_alias, ContentBlockDelta, InputContentBlock, InputMessage,
    MessageRequest, MessageResponse, OutputContentBlock, ProviderClient,
    StreamEvent as ApiStreamEvent, ToolChoice, ToolDefinition, ToolResultContentBlock,
};
use plugins::PluginTool;
use reqwest::blocking::Client;
use runtime::{
    edit_file, execute_bash, glob_search, grep_search, load_system_prompt, read_file, write_file,
    ApiClient, ApiRequest, AssistantEvent, BashCommandInput, ContentBlock, ConversationMessage,
    ConversationRuntime, GrepSearchInput, MessageRole, PermissionMode, PermissionPolicy,
    RuntimeError, Session, TokenUsage, ToolError, ToolExecutor,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Decode child-process output with a GB18030 fallback so Chinese (codepage 936)
/// Windows console output is not rendered as 乱码 in the terminal / tool windows.
/// Tries UTF-8 first (covers programs that already emit UTF-8), then GB18030,
/// and only falls back to lossy UTF-8 when both fail.
fn decode_console_output(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    let (cow, _, had_errors) = encoding_rs::GB18030.decode(bytes);
    if had_errors {
        String::from_utf8_lossy(bytes).to_string()
    } else {
        cow.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolManifestEntry {
    pub name: String,
    pub source: ToolSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolSource {
    Base,
    Conditional,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolRegistry {
    entries: Vec<ToolManifestEntry>,
}

impl ToolRegistry {
    #[must_use]
    pub fn new(entries: Vec<ToolManifestEntry>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub fn entries(&self) -> &[ToolManifestEntry] {
        &self.entries
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
    pub required_permission: PermissionMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GlobalToolRegistry {
    plugin_tools: Vec<PluginTool>,
}

/// Sets the workspace root used by tools that read `coolzhu.toml`.
///
/// This avoids custom environment variables for business configuration. CLI-like callers
/// can leave it unset and use the current working directory fallback; embedded callers
/// such as the Web UI set it from their active workspace before invoking tools.
pub fn set_project_config_root(root: Option<PathBuf>) {
    if let Ok(mut guard) = project_config_root().write() {
        *guard = root;
    }
}

fn project_config_root() -> &'static RwLock<Option<PathBuf>> {
    static ROOT: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();
    ROOT.get_or_init(|| RwLock::new(None))
}

impl GlobalToolRegistry {
    #[must_use]
    pub fn builtin() -> Self {
        Self {
            plugin_tools: Vec::new(),
        }
    }

    pub fn with_plugin_tools(plugin_tools: Vec<PluginTool>) -> Result<Self, String> {
        let builtin_names = mvp_tool_specs()
            .into_iter()
            .map(|spec| spec.name.to_string())
            .collect::<BTreeSet<_>>();
        let mut seen_plugin_names = BTreeSet::new();

        for tool in &plugin_tools {
            let name = tool.definition().name.clone();
            if builtin_names.contains(&name) {
                return Err(format!(
                    "plugin tool `{name}` conflicts with a built-in tool name"
                ));
            }
            if !seen_plugin_names.insert(name.clone()) {
                return Err(format!("duplicate plugin tool name `{name}`"));
            }
            // 权限元数据缺失或非法：注册期就明确报错，不把问题留到首个调用
            // （旧实现会在调用期 panic，等于用崩溃代替配置错误）。
            let permission = tool.required_permission();
            if !is_supported_plugin_permission(permission) {
                return Err(format!(
                    "plugin tool `{name}` declares unsupported permission `{permission}`; expected read-only, workspace-write or danger-full-access"
                ));
            }
        }

        Ok(Self { plugin_tools })
    }

    pub fn normalize_allowed_tools(
        &self,
        values: &[String],
    ) -> Result<Option<BTreeSet<String>>, String> {
        if values.is_empty() {
            return Ok(None);
        }

        let builtin_specs = mvp_tool_specs();
        let canonical_names = builtin_specs
            .iter()
            .map(|spec| spec.name.to_string())
            .chain(
                self.plugin_tools
                    .iter()
                    .map(|tool| tool.definition().name.clone()),
            )
            .collect::<Vec<_>>();
        let mut name_map = canonical_names
            .iter()
            .map(|name| (normalize_tool_name(name), name.clone()))
            .collect::<BTreeMap<_, _>>();

        for (alias, canonical) in [
            ("read", "read_file"),
            ("write", "write_file"),
            ("edit", "edit_file"),
            ("glob", "glob_search"),
            ("grep", "grep_search"),
        ] {
            name_map.insert(alias.to_string(), canonical.to_string());
        }

        let mut allowed = BTreeSet::new();
        for value in values {
            for token in value
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .filter(|token| !token.is_empty())
            {
                let normalized = normalize_tool_name(token);
                let canonical = name_map.get(&normalized).ok_or_else(|| {
                    format!(
                        "unsupported tool in --allowedTools: {token} (expected one of: {})",
                        canonical_names.join(", ")
                    )
                })?;
                allowed.insert(canonical.clone());
            }
        }

        Ok(Some(allowed))
    }

    #[must_use]
    pub fn definitions(&self, allowed_tools: Option<&BTreeSet<String>>) -> Vec<ToolDefinition> {
        let builtin = mvp_tool_specs()
            .into_iter()
            .filter(|spec| allowed_tools.is_none_or(|allowed| allowed.contains(spec.name)))
            .map(|spec| ToolDefinition {
                name: spec.name.to_string(),
                description: Some(spec.description.to_string()),
                input_schema: spec.input_schema,
            });
        let plugin = self
            .plugin_tools
            .iter()
            .filter(|tool| {
                allowed_tools
                    .is_none_or(|allowed| allowed.contains(tool.definition().name.as_str()))
            })
            .map(|tool| ToolDefinition {
                name: tool.definition().name.clone(),
                description: tool.definition().description.clone(),
                input_schema: tool.definition().input_schema.clone(),
            });
        builtin.chain(plugin).collect()
    }

    #[must_use]
    pub fn permission_specs(
        &self,
        allowed_tools: Option<&BTreeSet<String>>,
    ) -> Vec<(String, PermissionMode)> {
        let builtin = mvp_tool_specs()
            .into_iter()
            .filter(|spec| allowed_tools.is_none_or(|allowed| allowed.contains(spec.name)))
            .map(|spec| (spec.name.to_string(), spec.required_permission));
        let plugin = self
            .plugin_tools
            .iter()
            .filter(|tool| {
                allowed_tools
                    .is_none_or(|allowed| allowed.contains(tool.definition().name.as_str()))
            })
            .map(|tool| {
                (
                    tool.definition().name.clone(),
                    permission_mode_from_plugin(tool.required_permission()),
                )
            });
        builtin.chain(plugin).collect()
    }

    pub fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        if mvp_tool_specs().iter().any(|spec| spec.name == name) {
            return execute_tool(name, input);
        }
        self.plugin_tools
            .iter()
            .find(|tool| tool.definition().name == name)
            .ok_or_else(|| format!("unsupported tool: {name}"))?
            .execute(input)
            .map_err(|error| error.to_string())
    }
}

fn normalize_tool_name(value: &str) -> String {
    value.trim().replace('-', "_").to_ascii_lowercase()
}

/// 插件声明的权限串 → 档位。
///
/// 未知串**不再 panic**（那会让首个调用直接把进程带走），而是返回
/// `PermissionMode::Unspecified`：闸门据此给出明确配置错误并 fail-closed。
/// 注册期另有校验，正常情况下不会走到这里。
fn permission_mode_from_plugin(value: &str) -> PermissionMode {
    match value {
        "read-only" => PermissionMode::ReadOnly,
        "workspace-write" => PermissionMode::WorkspaceWrite,
        "danger-full-access" => PermissionMode::DangerFullAccess,
        _ => PermissionMode::Unspecified,
    }
}

/// 插件权限串是否可识别；注册期用它拒绝带非法权限声明的插件。
#[must_use]
pub fn is_supported_plugin_permission(value: &str) -> bool {
    !permission_mode_from_plugin(value).is_unspecified()
}

fn bash_tool_description() -> &'static str {
    if cfg!(windows) {
        "Execute a Windows cmd.exe compatibility command in the current workspace. On Windows, prefer PowerShell for shell work; this tool does not support POSIX-only syntax such as mkdir -p, heredocs, or cat > file <<EOF. Do not use it to find files, search contents, or read files — use glob_search, grep_search, and read_file instead."
    } else if cfg!(target_os = "macos") {
        "Execute a POSIX shell command through sh -lc in the current workspace on macOS."
    } else {
        "Execute a POSIX shell command through sh -lc in the current workspace on Linux/Unix."
    }
}

fn powershell_tool_description() -> &'static str {
    if cfg!(windows) {
        "Execute a PowerShell command with optional timeout on Windows. Prefer this tool for filesystem and process commands on Windows."
    } else {
        "Execute a PowerShell command with optional timeout when pwsh is installed; prefer bash for native Unix shell commands."
    }
}

#[must_use]
#[allow(clippy::too_many_lines)]
pub fn mvp_tool_specs() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "bash",
            description: bash_tool_description(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "cwd": { "type": "string" },
                    "timeout": { "type": "integer", "minimum": 1, "description": "Maximum run time in SECONDS (e.g. 120 for two minutes). Omit for no limit; use a generous value for installs/builds that may take minutes." },
                    "description": { "type": "string" },
                    "run_in_background": { "type": "boolean" },
                    "dangerouslyDisableSandbox": { "type": "boolean" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        ToolSpec {
            name: "read_file",
            description: "Read a text file from the workspace. Returns content and version.sha256 of the FULL original byte stream. Pass that hash as expected_version when editing or overwriting. For large files use line offset/limit, or character_offset/max_chars for long single lines (Unicode characters, max 6000). Do not mix line and character ranges. Prefer absolute paths or paths relative to the workspace root.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "offset": { "type": "integer", "minimum": 0 },
                    "limit": { "type": "integer", "minimum": 1 },
                    "character_offset": { "type": "integer", "minimum": 0 },
                    "max_chars": { "type": "integer", "minimum": 1, "maximum": 6000 }
                },
                "required": ["path"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "write_file",
            description: "Write complete text to a workspace file. New files need no expected_version. Existing files REQUIRE expected_version from the latest read_file version.sha256; if changed, reread and reconsider the edit. Prefer edit_file for targeted changes.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string" },
                    "expected_version": { "type": "string", "description": "Full original file version.sha256 returned by read_file. Required when overwriting an existing file." }
                },
                "required": ["path", "content"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "edit_file",
            description: "Replace an exact text span in a workspace file. You must read_file the target first. old_string must match exactly (including whitespace) and be unique in the file; include enough surrounding context to disambiguate or the edit fails. Set replace_all to replace every occurrence. Prefer this over write_file for partial edits.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" },
                    "replace_all": { "type": "boolean" },
                    "expected_version": { "type": "string", "description": "Full original file version.sha256 returned by read_file. Required for every edit." }
                },
                "required": ["path", "old_string", "new_string"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "glob_search",
            description: "Find files by name/path glob pattern (e.g. **/*.rs). Searches file paths, not contents; to search inside files use grep_search.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "grep_search",
            description: "Search file contents with a regular expression. To find files by name use glob_search; prefer this over running grep/findstr through bash or powershell.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string" },
                    "path": { "type": "string" },
                    "glob": { "type": "string" },
                    "output_mode": { "type": "string" },
                    "-B": { "type": "integer", "minimum": 0 },
                    "-A": { "type": "integer", "minimum": 0 },
                    "-C": { "type": "integer", "minimum": 0 },
                    "context": { "type": "integer", "minimum": 0 },
                    "-n": { "type": "boolean" },
                    "-i": { "type": "boolean" },
                    "type": { "type": "string" },
                    "head_limit": { "type": "integer", "minimum": 1 },
                    "offset": { "type": "integer", "minimum": 0 },
                    "multiline": { "type": "boolean" }
                },
                "required": ["pattern"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "WebFetch",
            description:
                "Fetch a URL, convert it into readable text, and answer a prompt about it.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "format": "uri" },
                    "prompt": { "type": "string" }
                },
                "required": ["url", "prompt"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "WebSearch",
            description: "Search the web for current information and return cited results.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "minLength": 2 },
                    "allowed_domains": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "blocked_domains": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "TodoWrite",
            description: "Update the structured task list for the current session.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string" },
                                "activeForm": { "type": "string" },
                                "status": {
                                    "type": "string",
                                    "enum": ["pending", "in_progress", "completed"]
                                }
                            },
                            "required": ["content", "activeForm", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["todos"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "Skill",
            description: "Load a local skill definition and its instructions.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "skill": { "type": "string" },
                    "args": { "type": "string" }
                },
                "required": ["skill"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "Agent",
            description: "Launch a specialized agent task and persist its handoff metadata.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "description": { "type": "string" },
                    "prompt": { "type": "string" },
                    "subagent_type": { "type": "string" },
                    "name": { "type": "string" },
                    "model": { "type": "string" }
                },
                "required": ["description", "prompt"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        ToolSpec {
            name: "ToolSearch",
            description: "Search for deferred or specialized tools by exact name or keywords.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "max_results": { "type": "integer", "minimum": 1 }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "NotebookEdit",
            description: "Replace, insert, or delete a cell in a Jupyter notebook.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "notebook_path": { "type": "string" },
                    "cell_id": { "type": "string" },
                    "new_source": { "type": "string" },
                    "cell_type": { "type": "string", "enum": ["code", "markdown"] },
                    "edit_mode": { "type": "string", "enum": ["replace", "insert", "delete"] }
                },
                "required": ["notebook_path"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "Sleep",
            description: "Wait for a specified duration without holding a shell process.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "duration_ms": { "type": "integer", "minimum": 0 }
                },
                "required": ["duration_ms"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "SendUserMessage",
            description: "Send a message to the user.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" },
                    "attachments": {
                        "type": "array",
                        "items": { "type": "string" }
                    },
                    "status": {
                        "type": "string",
                        "enum": ["normal", "proactive"]
                    }
                },
                "required": ["message", "status"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "Config",
            description: "Get or set Claw Code settings.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "setting": { "type": "string" },
                    "value": {
                        "type": ["string", "boolean", "number"]
                    }
                },
                "required": ["setting"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::WorkspaceWrite,
        },
        ToolSpec {
            name: "StructuredOutput",
            description: "Return structured output in the requested format.",
            input_schema: json!({
                "type": "object",
                "additionalProperties": true
            }),
            required_permission: PermissionMode::ReadOnly,
        },
        ToolSpec {
            name: "REPL",
            description: "Execute code in a REPL-like subprocess.",
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": { "type": "string" },
                    "language": { "type": "string" },
                    "timeout_ms": { "type": "integer", "minimum": 1 }
                },
                "required": ["code", "language"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
        ToolSpec {
            name: "PowerShell",
            description: powershell_tool_description(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "cwd": { "type": "string" },
                    "timeout": { "type": "integer", "minimum": 1, "description": "Maximum run time in SECONDS (e.g. 120 for two minutes). Omit for no limit; use a generous value for installs/builds that may take minutes." },
                    "description": { "type": "string" },
                    "run_in_background": { "type": "boolean" }
                },
                "required": ["command"],
                "additionalProperties": false
            }),
            required_permission: PermissionMode::DangerFullAccess,
        },
    ]
}

pub fn execute_tool(name: &str, input: &Value) -> Result<String, String> {
    match name {
        "bash" => from_value::<BashCommandInput>(input).and_then(run_bash),
        "read_file" => from_value::<ReadFileInput>(input).and_then(run_read_file),
        "write_file" => from_value::<WriteFileInput>(input).and_then(run_write_file),
        "edit_file" => from_value::<EditFileInput>(input).and_then(run_edit_file),
        "glob_search" => from_value::<GlobSearchInputValue>(input).and_then(run_glob_search),
        "grep_search" => from_value::<GrepSearchInput>(input).and_then(run_grep_search),
        "WebFetch" => from_value::<WebFetchInput>(input).and_then(run_web_fetch),
        "WebSearch" => from_value::<WebSearchInput>(input).and_then(run_web_search),
        "TodoWrite" => from_value::<TodoWriteInput>(input).and_then(run_todo_write),
        "Skill" => from_value::<SkillInput>(input).and_then(run_skill),
        "Agent" => from_value::<AgentInput>(input).and_then(run_agent),
        "ToolSearch" => from_value::<ToolSearchInput>(input).and_then(run_tool_search),
        "NotebookEdit" => from_value::<NotebookEditInput>(input).and_then(run_notebook_edit),
        "Sleep" => from_value::<SleepInput>(input).and_then(run_sleep),
        "SendUserMessage" | "Brief" => from_value::<BriefInput>(input).and_then(run_brief),
        "Config" => from_value::<ConfigInput>(input).and_then(run_config),
        "StructuredOutput" => {
            from_value::<StructuredOutputInput>(input).and_then(run_structured_output)
        }
        "REPL" => from_value::<ReplInput>(input).and_then(run_repl),
        "PowerShell" => from_value::<PowerShellInput>(input).and_then(run_powershell),
        _ => Err(format!("unsupported tool: {name}")),
    }
}

fn from_value<T: for<'de> Deserialize<'de>>(input: &Value) -> Result<T, String> {
    serde_json::from_value(input.clone()).map_err(|error| error.to_string())
}

fn run_bash(mut input: BashCommandInput) -> Result<String, String> {
    // timeout 语义为秒（schema 标 SECONDS），core-runtime execute_bash 按毫秒判定，故 ×1000；上限 3600 秒。
    // 与 PowerShell 一致，修复历史 bug：秒值被当毫秒导致命令几十毫秒内即超时。
    input.timeout = input
        .timeout
        .map(|secs| secs.min(3600).saturating_mul(1000));
    serde_json::to_string_pretty(&execute_bash(input).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn run_read_file(input: ReadFileInput) -> Result<String, String> {
    if input.character_offset.is_some() || input.max_chars.is_some() {
        if input.offset.is_some() || input.limit.is_some() {
            return Err("行范围和字符范围不能混用".into());
        }
        return to_pretty_json(runtime::read_file_character_range(&input.path,
            input.character_offset.unwrap_or(0), input.max_chars.unwrap_or(4000)).map_err(io_to_string)?);
    }
    to_pretty_json(read_file(&input.path, input.offset, input.limit).map_err(io_to_string)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_write_file(input: WriteFileInput) -> Result<String, String> {
    to_pretty_json(write_file(&input.path, &input.content, input.expected_version.as_deref()).map_err(io_to_string)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_edit_file(input: EditFileInput) -> Result<String, String> {
    to_pretty_json(
        edit_file(
            &input.path,
            &input.old_string,
            &input.new_string,
            input.replace_all.unwrap_or(false),
            input.expected_version.as_deref(),
        )
        .map_err(io_to_string)?,
    )
}

#[allow(clippy::needless_pass_by_value)]
fn run_glob_search(input: GlobSearchInputValue) -> Result<String, String> {
    to_pretty_json(glob_search(&input.pattern, input.path.as_deref()).map_err(io_to_string)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_grep_search(input: GrepSearchInput) -> Result<String, String> {
    to_pretty_json(grep_search(&input).map_err(io_to_string)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_web_fetch(input: WebFetchInput) -> Result<String, String> {
    to_pretty_json(execute_web_fetch(&input)?)
}

#[allow(clippy::needless_pass_by_value)]
fn run_web_search(input: WebSearchInput) -> Result<String, String> {
    to_pretty_json(execute_web_search(&input)?)
}

fn run_todo_write(input: TodoWriteInput) -> Result<String, String> {
    to_pretty_json(execute_todo_write(input)?)
}

fn run_skill(input: SkillInput) -> Result<String, String> {
    to_pretty_json(execute_skill(input)?)
}

fn run_agent(input: AgentInput) -> Result<String, String> {
    to_pretty_json(execute_agent(input)?)
}

fn run_tool_search(input: ToolSearchInput) -> Result<String, String> {
    to_pretty_json(execute_tool_search(input))
}

fn run_notebook_edit(input: NotebookEditInput) -> Result<String, String> {
    to_pretty_json(execute_notebook_edit(input)?)
}

fn run_sleep(input: SleepInput) -> Result<String, String> {
    to_pretty_json(execute_sleep(input))
}

fn run_brief(input: BriefInput) -> Result<String, String> {
    to_pretty_json(execute_brief(input)?)
}

fn run_config(input: ConfigInput) -> Result<String, String> {
    to_pretty_json(execute_config(input)?)
}

fn run_structured_output(input: StructuredOutputInput) -> Result<String, String> {
    to_pretty_json(execute_structured_output(input))
}

fn run_repl(input: ReplInput) -> Result<String, String> {
    to_pretty_json(execute_repl(input)?)
}

fn run_powershell(input: PowerShellInput) -> Result<String, String> {
    to_pretty_json(execute_powershell(input).map_err(|error| error.to_string())?)
}

fn to_pretty_json<T: serde::Serialize>(value: T) -> Result<String, String> {
    serde_json::to_string_pretty(&value).map_err(|error| error.to_string())
}

#[allow(clippy::needless_pass_by_value)]
fn io_to_string(error: std::io::Error) -> String {
    error.to_string()
}

#[derive(Debug, Deserialize)]
struct ReadFileInput {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
    character_offset: Option<usize>,
    max_chars: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct WriteFileInput {
    path: String,
    content: String,
    expected_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EditFileInput {
    path: String,
    old_string: String,
    new_string: String,
    replace_all: Option<bool>,
    expected_version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GlobSearchInputValue {
    pattern: String,
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WebFetchInput {
    url: String,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct WebSearchInput {
    query: String,
    allowed_domains: Option<Vec<String>>,
    blocked_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct ToolingProjectConfig {
    web_search: WebSearchConfig,
}

impl Default for ToolingProjectConfig {
    fn default() -> Self {
        Self {
            web_search: WebSearchConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
struct WebSearchConfig {
    base_url: String,
    transport: WebSearchTransport,
    force_powershell_fallback: bool,
    /// 主后端失败后按顺序尝试的后端 URL。
    fallback_urls: Vec<String>,
    /// 单次 HTTP 尝试的总超时（毫秒），含连接与读体。
    attempt_timeout_ms: u64,
    /// 连接建立超时（毫秒）。被墙主机通常表现为 connect 挂死而不是立刻拒绝，
    /// 没有这个上限时单次尝试会一直占住整个预算。
    connect_timeout_ms: u64,
    /// 整个 WebSearch 调用的内部总预算（毫秒）。必须显著小于调用方的工具 deadline，
    /// 否则工具会被外层强制终止，模型只能看到 timeout 而拿不到失败原因。
    total_budget_ms: u64,
}

impl WebSearchConfig {
    fn effective_transport(&self) -> WebSearchTransport {
        if self.force_powershell_fallback {
            WebSearchTransport::Powershell
        } else {
            self.transport
        }
    }

    /// 按顺序展开实际要尝试的后端 URL：主后端在前，其后是去重后的降级后端。
    fn candidate_urls(&self) -> Vec<String> {
        let primary = if self.base_url.trim().is_empty() {
            DEFAULT_WEB_SEARCH_BASE_URL.to_string()
        } else {
            self.base_url.trim().to_string()
        };
        let mut candidates = vec![primary];
        for fallback in &self.fallback_urls {
            let trimmed = fallback.trim();
            if !trimmed.is_empty() && !candidates.iter().any(|url| url == trimmed) {
                candidates.push(trimmed.to_string());
            }
        }
        candidates
    }

    fn effective_attempt_timeout_ms(&self) -> u64 {
        self.attempt_timeout_ms
            .clamp(MIN_SEARCH_ATTEMPT_TIMEOUT_MS, MAX_SEARCH_ATTEMPT_TIMEOUT_MS)
    }

    fn effective_connect_timeout_ms(&self) -> u64 {
        self.connect_timeout_ms
            .clamp(MIN_SEARCH_CONNECT_TIMEOUT_MS, MAX_SEARCH_CONNECT_TIMEOUT_MS)
    }

    fn effective_total_budget_ms(&self) -> u64 {
        self.total_budget_ms.clamp(MIN_SEARCH_BUDGET_MS, MAX_SEARCH_BUDGET_MS)
    }
}

impl Default for WebSearchConfig {
    fn default() -> Self {
        Self {
            base_url: String::from(DEFAULT_WEB_SEARCH_BASE_URL),
            transport: WebSearchTransport::Auto,
            force_powershell_fallback: false,
            // 默认只用一个后端：默认降级后端会隐式依赖另一个站点（境外可达性因网络而异），
            // 既让"只配了 base_url"的场景意外多打一次网络，也让测试无法与网络解耦。
            // 需要多后端降级时显式配置 `fallback_urls`。
            fallback_urls: Vec::new(),
            attempt_timeout_ms: DEFAULT_SEARCH_ATTEMPT_TIMEOUT_MS,
            connect_timeout_ms: DEFAULT_SEARCH_CONNECT_TIMEOUT_MS,
            total_budget_ms: DEFAULT_SEARCH_TOTAL_BUDGET_MS,
        }
    }
}

/// 默认搜索后端（Bing 国内站点）。历史默认是 `html.duckduckgo.com`，在部分网络下
/// 该域名 TCP 连不通，工具会一直挂到上层 deadline 被强杀，对外只表现为 timeout。
/// 需要多后端降级时在 `[web_search]` 里显式配置 `fallback_urls`。
const DEFAULT_WEB_SEARCH_BASE_URL: &str =
    "https://cn.bing.com/search?mkt=zh-CN&setlang=zh-CN";

const DEFAULT_SEARCH_ATTEMPT_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_SEARCH_CONNECT_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_SEARCH_TOTAL_BUDGET_MS: u64 = 20_000;
const MIN_SEARCH_ATTEMPT_TIMEOUT_MS: u64 = 1_000;
const MAX_SEARCH_ATTEMPT_TIMEOUT_MS: u64 = 60_000;
const MIN_SEARCH_CONNECT_TIMEOUT_MS: u64 = 500;
const MAX_SEARCH_CONNECT_TIMEOUT_MS: u64 = 30_000;
const MIN_SEARCH_BUDGET_MS: u64 = 2_000;
const MAX_SEARCH_BUDGET_MS: u64 = 60_000;
/// 剩余预算低于该值就不要再开新尝试：起一个必然超时的请求只会把终态拖成"无原因超时"。
const MIN_SEARCH_ATTEMPT_MS: u64 = 1_500;

/// `WebSearch` 自己声明的内部预算（毫秒），从当前 config root / 工作目录解析。
///
/// 调用方应当按"预算 + 少量收尾余量"来设定工具 deadline，否则工具的内部降级还没跑完
/// 就会被外层强杀。见 `modules/gui-web/.../main.rs::tool_timeout_ms_for`。
#[must_use]
pub fn web_search_total_budget_ms() -> u64 {
    load_web_search_config()
        .map(|config| config.effective_total_budget_ms())
        .unwrap_or(DEFAULT_SEARCH_TOTAL_BUDGET_MS)
}

/// 同上，但从指定 workspace 根读配置。
///
/// 供"只是想知道该给多少时间、并不属于本次工具执行"的调用方使用：这类调用方不该为了
/// 取值去改全局 config root，否则会覆盖真正执行工具的 workspace。
#[must_use]
pub fn web_search_total_budget_ms_at(root: &Path) -> u64 {
    load_web_search_config_from(root)
        .map(|config| config.effective_total_budget_ms())
        .unwrap_or(DEFAULT_SEARCH_TOTAL_BUDGET_MS)
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum WebSearchTransport {
    Auto,
    Reqwest,
    #[serde(rename = "powershell", alias = "power_shell")]
    Powershell,
}

impl Default for WebSearchTransport {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Deserialize)]
struct TodoWriteInput {
    todos: Vec<TodoItem>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
struct TodoItem {
    content: String,
    #[serde(rename = "activeForm")]
    active_form: String,
    status: TodoStatus,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Deserialize)]
struct SkillInput {
    skill: String,
    args: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AgentInput {
    description: String,
    prompt: String,
    subagent_type: Option<String>,
    name: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ToolSearchInput {
    query: String,
    max_results: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct NotebookEditInput {
    notebook_path: String,
    cell_id: Option<String>,
    new_source: Option<String>,
    cell_type: Option<NotebookCellType>,
    edit_mode: Option<NotebookEditMode>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum NotebookCellType {
    Code,
    Markdown,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum NotebookEditMode {
    Replace,
    Insert,
    Delete,
}

#[derive(Debug, Deserialize)]
struct SleepInput {
    duration_ms: u64,
}

#[derive(Debug, Deserialize)]
struct BriefInput {
    message: String,
    attachments: Option<Vec<String>>,
    status: BriefStatus,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum BriefStatus {
    Normal,
    Proactive,
}

#[derive(Debug, Deserialize)]
struct ConfigInput {
    setting: String,
    value: Option<ConfigValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ConfigValue {
    String(String),
    Bool(bool),
    Number(f64),
}

#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct StructuredOutputInput(BTreeMap<String, Value>);

#[derive(Debug, Deserialize)]
struct ReplInput {
    code: String,
    language: String,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct PowerShellInput {
    command: String,
    cwd: Option<String>,
    timeout: Option<u64>,
    description: Option<String>,
    run_in_background: Option<bool>,
}

#[derive(Debug, Serialize)]
struct WebFetchOutput {
    bytes: usize,
    code: u16,
    #[serde(rename = "codeText")]
    code_text: String,
    result: String,
    #[serde(rename = "durationMs")]
    duration_ms: u128,
    url: String,
}

#[derive(Debug, Serialize)]
struct WebSearchOutput {
    query: String,
    /// 实际给出结果的后端 URL；全部失败时为 `null`。
    #[serde(skip_serializing_if = "Option::is_none")]
    backend: Option<String>,
    /// 逐后端的尝试记录，用于区分"真的没有结果"和"后端不可达"。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    notes: Vec<String>,
    results: Vec<WebSearchResultItem>,
    #[serde(rename = "durationSeconds")]
    duration_seconds: f64,
}

#[derive(Debug, Serialize)]
struct TodoWriteOutput {
    #[serde(rename = "oldTodos")]
    old_todos: Vec<TodoItem>,
    #[serde(rename = "newTodos")]
    new_todos: Vec<TodoItem>,
    #[serde(rename = "verificationNudgeNeeded")]
    verification_nudge_needed: Option<bool>,
}

#[derive(Debug, Serialize)]
struct SkillOutput {
    skill: String,
    path: String,
    args: Option<String>,
    description: Option<String>,
    prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentOutput {
    #[serde(rename = "agentId")]
    agent_id: String,
    name: String,
    description: String,
    #[serde(rename = "subagentType")]
    subagent_type: Option<String>,
    model: Option<String>,
    status: String,
    #[serde(rename = "outputFile")]
    output_file: String,
    #[serde(rename = "manifestFile")]
    manifest_file: String,
    #[serde(rename = "createdAt")]
    created_at: String,
    #[serde(rename = "startedAt", skip_serializing_if = "Option::is_none")]
    started_at: Option<String>,
    #[serde(rename = "completedAt", skip_serializing_if = "Option::is_none")]
    completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct AgentJob {
    manifest: AgentOutput,
    prompt: String,
    system_prompt: Vec<String>,
    allowed_tools: BTreeSet<String>,
}

#[derive(Debug, Serialize)]
struct ToolSearchOutput {
    matches: Vec<String>,
    query: String,
    normalized_query: String,
    #[serde(rename = "total_deferred_tools")]
    total_deferred_tools: usize,
    #[serde(rename = "pending_mcp_servers")]
    pending_mcp_servers: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct NotebookEditOutput {
    new_source: String,
    cell_id: Option<String>,
    cell_type: Option<NotebookCellType>,
    language: String,
    edit_mode: String,
    error: Option<String>,
    notebook_path: String,
    original_file: String,
    updated_file: String,
}

#[derive(Debug, Serialize)]
struct SleepOutput {
    duration_ms: u64,
    message: String,
}

#[derive(Debug, Serialize)]
struct BriefOutput {
    message: String,
    attachments: Option<Vec<ResolvedAttachment>>,
    #[serde(rename = "sentAt")]
    sent_at: String,
}

#[derive(Debug, Serialize)]
struct ResolvedAttachment {
    path: String,
    size: u64,
    #[serde(rename = "isImage")]
    is_image: bool,
}

#[derive(Debug, Serialize)]
struct ConfigOutput {
    success: bool,
    operation: Option<String>,
    setting: Option<String>,
    value: Option<Value>,
    #[serde(rename = "previousValue")]
    previous_value: Option<Value>,
    #[serde(rename = "newValue")]
    new_value: Option<Value>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct StructuredOutputResult {
    data: String,
    structured_output: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
struct ReplOutput {
    language: String,
    stdout: String,
    stderr: String,
    #[serde(rename = "exitCode")]
    exit_code: i32,
    #[serde(rename = "durationMs")]
    duration_ms: u128,
    interrupted: bool,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum WebSearchResultItem {
    SearchResult {
        tool_use_id: String,
        content: Vec<SearchHit>,
    },
    Commentary(String),
}

#[derive(Debug, Serialize)]
struct SearchHit {
    title: String,
    url: String,
}

fn execute_web_fetch(input: &WebFetchInput) -> Result<WebFetchOutput, String> {
    let started = Instant::now();
    let client = build_http_client()?;
    let request_url = normalize_fetch_url(&input.url)?;
    let response = client
        .get(request_url.clone())
        .send()
        .map_err(|error| error.to_string())?;

    let status = response.status();
    let final_url = response.url().to_string();
    let code = status.as_u16();
    let code_text = status.canonical_reason().unwrap_or("Unknown").to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    let body = response.text().map_err(|error| error.to_string())?;
    let bytes = body.len();
    let normalized = normalize_fetched_content(&body, &content_type);
    let result = summarize_web_fetch(&final_url, &input.prompt, &normalized, &body, &content_type);

    Ok(WebFetchOutput {
        bytes,
        code,
        code_text,
        result,
        duration_ms: started.elapsed().as_millis(),
        url: final_url,
    })
}

fn execute_web_search(input: &WebSearchInput) -> Result<WebSearchOutput, String> {
    let started = Instant::now();
    let config = load_web_search_config()?;
    let budget = Duration::from_millis(config.effective_total_budget_ms());
    let deadline = started + budget;

    let mut notes: Vec<String> = Vec::new();
    let mut hits: Vec<SearchHit> = Vec::new();
    let mut backend: Option<String> = None;
    let mut fetched_any = false;

    for (index, candidate) in config.candidate_urls().into_iter().enumerate() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining < Duration::from_millis(MIN_SEARCH_ATTEMPT_MS) {
            notes.push(format!(
                "{candidate}: skipped, remaining budget too small ({}ms left of {}ms)",
                remaining.as_millis(),
                budget.as_millis()
            ));
            break;
        }

        let search_url = match build_search_url(&input.query, &candidate) {
            Ok(url) => url,
            Err(error) => {
                // 主后端 URL 非法属于配置错误，直接暴露原始解析错误，不要吞进降级记录。
                if index == 0 {
                    return Err(error);
                }
                notes.push(format!("{candidate}: {error}"));
                continue;
            }
        };

        match fetch_search_page(
            &search_url,
            config.effective_transport(),
            config.effective_attempt_timeout_ms(),
            config.effective_connect_timeout_ms(),
            remaining,
        ) {
            Ok(html) => {
                fetched_any = true;
                let page_hits = extract_search_hits_for_page(&html);
                if page_hits.is_empty() {
                    notes.push(format!(
                        "{search_url}: HTTP ok but no results parsed (captcha or anti-bot page?)"
                    ));
                    continue;
                }
                hits = page_hits;
                backend = Some(search_url.to_string());
                break;
            }
            Err(error) => notes.push(format!("{search_url}: {error}")),
        }
    }

    if hits.is_empty() && !fetched_any {
        return Err(format!(
            "WebSearch could not reach any configured search backend within {}ms. \
             Attempts: {}. Configure reachable backends via the [web_search] section of \
             coolzhu.toml (base_url / fallback_urls / connect_timeout_ms), or use WebFetch \
             to retrieve a search results page directly.",
            budget.as_millis(),
            render_attempt_notes(&notes)
        ));
    }

    if let Some(allowed) = input.allowed_domains.as_ref() {
        hits.retain(|hit| host_matches_list(&hit.url, allowed));
    }
    if let Some(blocked) = input.blocked_domains.as_ref() {
        hits.retain(|hit| !host_matches_list(&hit.url, blocked));
    }

    dedupe_hits(&mut hits);
    hits.truncate(8);

    let summary = if hits.is_empty() {
        format!(
            "No web search results matched the query {:?}. Attempts: {}",
            input.query,
            render_attempt_notes(&notes)
        )
    } else {
        let rendered_hits = hits
            .iter()
            .map(|hit| format!("- [{}]({})", hit.title, hit.url))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Search results for {:?}. Include a Sources section in the final answer.\n{}",
            input.query, rendered_hits
        )
    };

    Ok(WebSearchOutput {
        query: input.query.clone(),
        backend,
        notes,
        results: vec![
            WebSearchResultItem::Commentary(summary),
            WebSearchResultItem::SearchResult {
                tool_use_id: String::from("web_search_1"),
                content: hits,
            },
        ],
        duration_seconds: started.elapsed().as_secs_f64(),
    })
}

fn render_attempt_notes(notes: &[String]) -> String {
    if notes.is_empty() {
        return String::from("none");
    }
    notes.join(" | ")
}

/// 抓取单个后端的搜索结果页。`remaining` 是本次调用的剩余总预算，
/// PowerShell 降级会据此再收窄自身的超时，避免把终态拖成无原因超时。
fn fetch_search_page(
    search_url: &reqwest::Url,
    transport: WebSearchTransport,
    attempt_timeout_ms: u64,
    connect_timeout_ms: u64,
    remaining: Duration,
) -> Result<String, String> {
    if transport == WebSearchTransport::Powershell {
        return fetch_search_html_with_powershell(search_url, remaining);
    }

    match fetch_search_html_with_reqwest(search_url, attempt_timeout_ms, connect_timeout_ms) {
        Ok(html) => Ok(html),
        Err(error) => {
            if transport == WebSearchTransport::Reqwest {
                return Err(error);
            }
            #[cfg(windows)]
            {
                fetch_search_html_with_powershell(search_url, remaining)
                    .map_err(|fallback_error| {
                        format!(
                            "reqwest web search failed: {error}; PowerShell fallback failed: {fallback_error}"
                        )
                    })
            }
            #[cfg(not(windows))]
            {
                Err(error)
            }
        }
    }
}

fn fetch_search_html_with_reqwest(
    search_url: &reqwest::Url,
    attempt_timeout_ms: u64,
    connect_timeout_ms: u64,
) -> Result<String, String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_millis(connect_timeout_ms))
        .timeout(Duration::from_millis(attempt_timeout_ms))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("claw-rust-tools/0.1")
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(search_url.clone())
        .send()
        .map_err(|error| error.to_string())?;
    response.text().map_err(|error| error.to_string())
}

#[cfg(windows)]
fn fetch_search_html_with_powershell(
    search_url: &reqwest::Url,
    remaining: Duration,
) -> Result<String, String> {
    let timeout_secs = remaining
        .as_secs()
        .max(1)
        .min(DEFAULT_SEARCH_ATTEMPT_TIMEOUT_MS.div_ceil(1000));
    let script = concat!(
        "& {",
        "param([string]$SearchUrl,[int]$TimeoutSecs);",
        "$ProgressPreference='SilentlyContinue';",
        "[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);",
        "$r=Invoke-WebRequest -Uri $SearchUrl -UseBasicParsing -TimeoutSec $TimeoutSecs;",
        "[Console]::Out.Write($r.Content);",
        "}"
    );
    let output = Command::new("powershell.exe")
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-Command")
        .arg(script)
        .arg(search_url.as_str())
        .arg(timeout_secs.to_string())
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = decode_console_output(&output.stderr);
        return Err(format!(
            "PowerShell exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn fetch_search_html_with_powershell(
    _search_url: &reqwest::Url,
    _remaining: Duration,
) -> Result<String, String> {
    Err(String::from(
        "PowerShell fallback is only available on Windows",
    ))
}

fn build_http_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("claw-rust-tools/0.1")
        .build()
        .map_err(|error| error.to_string())
}

fn normalize_fetch_url(url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|error| error.to_string())?;
    if parsed.scheme() == "http" {
        let host = parsed.host_str().unwrap_or_default();
        if host != "localhost" && host != "127.0.0.1" && host != "::1" {
            let mut upgraded = parsed;
            upgraded
                .set_scheme("https")
                .map_err(|()| String::from("failed to upgrade URL to https"))?;
            return Ok(upgraded.to_string());
        }
    }
    Ok(parsed.to_string())
}

fn load_web_search_config() -> Result<WebSearchConfig, String> {
    let Some(path) = find_project_config_file()? else {
        return Ok(WebSearchConfig::default());
    };
    let content = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let config = toml::from_str::<ToolingProjectConfig>(&content)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    Ok(config.web_search)
}

/// 只从给定的 workspace 根读 `coolzhu.toml`，不回落 cwd、不碰全局 config root。
fn load_web_search_config_from(root: &Path) -> Result<WebSearchConfig, String> {
    let path = root.join("coolzhu.toml");
    if !path.exists() {
        return Ok(WebSearchConfig::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let config = toml::from_str::<ToolingProjectConfig>(&content)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
    Ok(config.web_search)
}

fn find_project_config_file() -> Result<Option<PathBuf>, String> {
    if let Ok(guard) = project_config_root().read() {
        if let Some(root) = guard.as_ref() {
            let candidate = root.join("coolzhu.toml");
            if candidate.exists() {
                return Ok(Some(candidate));
            }
        }
    }

    let mut current = std::env::current_dir().map_err(|error| error.to_string())?;
    loop {
        let candidate = current.join("coolzhu.toml");
        if candidate.exists() {
            return Ok(Some(candidate));
        }
        if !current.pop() {
            return Ok(None);
        }
    }
}

fn build_search_url(query: &str, base_url: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(base_url).map_err(|error| error.to_string())?;
    url.query_pairs_mut().append_pair("q", query);
    Ok(url)
}

fn normalize_fetched_content(body: &str, content_type: &str) -> String {
    if content_type.contains("html") {
        html_to_text(body)
    } else {
        body.trim().to_string()
    }
}

fn summarize_web_fetch(
    url: &str,
    prompt: &str,
    content: &str,
    raw_body: &str,
    content_type: &str,
) -> String {
    let lower_prompt = prompt.to_lowercase();
    let compact = collapse_whitespace(content);

    let detail = if lower_prompt.contains("title") {
        extract_title(content, raw_body, content_type).map_or_else(
            || preview_text(&compact, 600),
            |title| format!("Title: {title}"),
        )
    } else if lower_prompt.contains("summary") || lower_prompt.contains("summarize") {
        preview_text(&compact, 900)
    } else {
        let preview = preview_text(&compact, 900);
        format!("Prompt: {prompt}\nContent preview:\n{preview}")
    };

    format!("Fetched {url}\n{detail}")
}

fn extract_title(content: &str, raw_body: &str, content_type: &str) -> Option<String> {
    if content_type.contains("html") {
        let lowered = raw_body.to_lowercase();
        if let Some(start) = lowered.find("<title>") {
            let after = start + "<title>".len();
            if let Some(end_rel) = lowered[after..].find("</title>") {
                let title =
                    collapse_whitespace(&decode_html_entities(&raw_body[after..after + end_rel]));
                if !title.is_empty() {
                    return Some(title);
                }
            }
        }
    }

    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut previous_was_space = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag => {}
            '&' => {
                text.push('&');
                previous_was_space = false;
            }
            ch if ch.is_whitespace() => {
                if !previous_was_space {
                    text.push(' ');
                    previous_was_space = true;
                }
            }
            _ => {
                text.push(ch);
                previous_was_space = false;
            }
        }
    }

    collapse_whitespace(&decode_html_entities(&text))
}

fn decode_html_entities(input: &str) -> String {
    input
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn collapse_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn preview_text(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    let shortened = input.chars().take(max_chars).collect::<String>();
    format!("{}…", shortened.trim_end())
}

/// 按已知搜索引擎的 HTML 结构依次尝试解析，最后退到通用链接扫描。
///
/// 不同后端的结果页结构不同：DuckDuckGo 的 html 端点用 `result__a`，Bing 用 `b_algo`。
/// 只认一种结构会让换后端后的解析静默退化成抓到一堆导航链接。
fn extract_search_hits_for_page(html: &str) -> Vec<SearchHit> {
    let mut hits = extract_search_hits(html);
    if hits.is_empty() {
        hits = extract_bing_search_hits(html);
    }
    if hits.is_empty() {
        hits = extract_search_hits_from_generic_links(html);
    }
    hits
}

/// Bing 结果页：每条自然结果包在 `class="b_algo"` 的块里，
/// 块内 `<h2 class=""><a href="...">标题</a></h2>` 是标题锚点，块首的 `tilk` 锚点通常指向同一 URL。
fn extract_bing_search_hits(html: &str) -> Vec<SearchHit> {
    const BLOCK_MARKER: &str = "class=\"b_algo\"";

    let mut hits = Vec::new();
    let mut cursor = 0usize;
    while let Some(relative) = html[cursor..].find(BLOCK_MARKER) {
        let start = cursor + relative;
        let next = html[start + 1..]
            .find(BLOCK_MARKER)
            .map_or(html.len(), |offset| start + 1 + offset);
        if let Some(hit) = extract_bing_block_hit(&html[start..next]) {
            hits.push(hit);
        }
        if next >= html.len() {
            break;
        }
        cursor = next;
    }
    hits
}

fn extract_bing_block_hit(block: &str) -> Option<SearchHit> {
    block
        .find("<h2")
        .and_then(|index| extract_first_result_anchor(&block[index..]))
        .or_else(|| extract_first_result_anchor(block))
}

/// 在片段里找到第一个"看起来是搜索结果"的锚点（跳过引擎自身的导航/翻页链接）。
fn extract_first_result_anchor(region: &str) -> Option<SearchHit> {
    let mut remaining = region;
    while let Some(anchor_start) = remaining.find("<a") {
        let after_anchor = &remaining[anchor_start..];
        let Some(href_index) = after_anchor.find("href=") else {
            return None;
        };
        let href_slice = &after_anchor[href_index + 5..];
        let Some((url, rest)) = extract_quoted_value(href_slice) else {
            remaining = &after_anchor[2..];
            continue;
        };
        let Some(close_tag_index) = rest.find('>') else {
            remaining = &after_anchor[2..];
            continue;
        };
        let after_tag = &rest[close_tag_index + 1..];
        let Some(end_anchor_index) = after_tag.find("</a>") else {
            remaining = &after_anchor[2..];
            continue;
        };
        let title = html_to_text(&after_tag[..end_anchor_index]);
        if let Some(decoded) = decode_search_redirect(&url) {
            if !title.trim().is_empty() && !is_search_engine_internal_url(&decoded) {
                return Some(SearchHit {
                    title: title.trim().to_string(),
                    url: decoded,
                });
            }
        }
        remaining = &after_tag[end_anchor_index + 4..];
    }
    None
}

/// 搜索引擎自身的导航/翻页/同站查询链接不是搜索结果，会被下游当成有效命中污染来源列表。
fn is_search_engine_internal_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return true;
    };
    let Some(host) = parsed.host_str() else {
        return true;
    };
    let host = host.to_ascii_lowercase();
    ["bing.com", "duckduckgo.com", "baidu.com", "sogou.com", "so.com"]
        .iter()
        .any(|engine| host == *engine || host.ends_with(&format!(".{engine}")))
}

/// 统一处理搜索后端的跳转包裹链接：DuckDuckGo 的 `/l/?uddg=` 与 Bing 的 `/ck/a?u=a1<base64url>`。
fn decode_search_redirect(url: &str) -> Option<String> {
    decode_bing_redirect(url).or_else(|| decode_duckduckgo_redirect(url))
}

fn decode_bing_redirect(url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(url).ok()?;
    if parsed.path() != "/ck/a" && parsed.path() != "/ck/a/" {
        return None;
    }
    let encoded = parsed
        .query_pairs()
        .find(|(key, _)| key == "u")
        .map(|(_, value)| value.into_owned())?;
    let payload = encoded.strip_prefix("a1")?;
    let decoded = String::from_utf8(base64url_decode(payload)?).ok()?;
    if decoded.starts_with("http://") || decoded.starts_with("https://") {
        return Some(html_entity_decode_url(&decoded));
    }
    None
}

/// 无填充 base64url 解码（Bing 跳转参数用这种编码，标准 base64 也一并容忍）。
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some(u32::from(byte - b'A')),
            b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
            b'-' | b'+' => Some(62),
            b'_' | b'/' => Some(63),
            _ => None,
        }
    }

    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        let value = sextet(byte)?;
        // 只保留最近 24 位：bits 最多 7+6=13，掩码后不会发生左移溢出。
        buffer = ((buffer << 6) | value) & 0x00FF_FFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buffer >> bits) & 0xFF) as u8);
        }
    }
    Some(out)
}

fn extract_search_hits(html: &str) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut remaining = html;

    while let Some(anchor_start) = remaining.find("result__a") {
        let after_class = &remaining[anchor_start..];
        let Some(href_idx) = after_class.find("href=") else {
            remaining = &after_class[1..];
            continue;
        };
        let href_slice = &after_class[href_idx + 5..];
        let Some((url, rest)) = extract_quoted_value(href_slice) else {
            remaining = &after_class[1..];
            continue;
        };
        let Some(close_tag_idx) = rest.find('>') else {
            remaining = &after_class[1..];
            continue;
        };
        let after_tag = &rest[close_tag_idx + 1..];
        let Some(end_anchor_idx) = after_tag.find("</a>") else {
            remaining = &after_tag[1..];
            continue;
        };
        let title = html_to_text(&after_tag[..end_anchor_idx]);
        if let Some(decoded_url) = decode_duckduckgo_redirect(&url) {
            hits.push(SearchHit {
                title: title.trim().to_string(),
                url: decoded_url,
            });
        }
        remaining = &after_tag[end_anchor_idx + 4..];
    }

    hits
}

fn extract_search_hits_from_generic_links(html: &str) -> Vec<SearchHit> {
    let mut hits = Vec::new();
    let mut remaining = html;

    while let Some(anchor_start) = remaining.find("<a") {
        let after_anchor = &remaining[anchor_start..];
        let Some(href_idx) = after_anchor.find("href=") else {
            remaining = &after_anchor[2..];
            continue;
        };
        let href_slice = &after_anchor[href_idx + 5..];
        let Some((url, rest)) = extract_quoted_value(href_slice) else {
            remaining = &after_anchor[2..];
            continue;
        };
        let Some(close_tag_idx) = rest.find('>') else {
            remaining = &after_anchor[2..];
            continue;
        };
        let after_tag = &rest[close_tag_idx + 1..];
        let Some(end_anchor_idx) = after_tag.find("</a>") else {
            remaining = &after_anchor[2..];
            continue;
        };
        let title = html_to_text(&after_tag[..end_anchor_idx]);
        if title.trim().is_empty() {
            remaining = &after_tag[end_anchor_idx + 4..];
            continue;
        }
        let decoded_url = decode_duckduckgo_redirect(&url).unwrap_or(url);
        if decoded_url.starts_with("http://") || decoded_url.starts_with("https://") {
            hits.push(SearchHit {
                title: title.trim().to_string(),
                url: decoded_url,
            });
        }
        remaining = &after_tag[end_anchor_idx + 4..];
    }

    hits
}

fn extract_quoted_value(input: &str) -> Option<(String, &str)> {
    let quote = input.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &input[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some((rest[..end].to_string(), &rest[end + quote.len_utf8()..]))
}

fn decode_duckduckgo_redirect(url: &str) -> Option<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return Some(html_entity_decode_url(url));
    }

    let joined = if url.starts_with("//") {
        format!("https:{url}")
    } else if url.starts_with('/') {
        format!("https://duckduckgo.com{url}")
    } else {
        return None;
    };

    let parsed = reqwest::Url::parse(&joined).ok()?;
    if parsed.path() == "/l/" || parsed.path() == "/l" {
        for (key, value) in parsed.query_pairs() {
            if key == "uddg" {
                return Some(html_entity_decode_url(value.as_ref()));
            }
        }
    }
    Some(joined)
}

fn html_entity_decode_url(url: &str) -> String {
    decode_html_entities(url)
}

fn host_matches_list(url: &str, domains: &[String]) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    domains.iter().any(|domain| {
        let normalized = normalize_domain_filter(domain);
        !normalized.is_empty() && (host == normalized || host.ends_with(&format!(".{normalized}")))
    })
}

fn normalize_domain_filter(domain: &str) -> String {
    let trimmed = domain.trim();
    let candidate = reqwest::Url::parse(trimmed)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_else(|| trimmed.to_string());
    candidate
        .trim()
        .trim_start_matches('.')
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

fn dedupe_hits(hits: &mut Vec<SearchHit>) {
    let mut seen = BTreeSet::new();
    hits.retain(|hit| seen.insert(hit.url.clone()));
}

fn execute_todo_write(input: TodoWriteInput) -> Result<TodoWriteOutput, String> {
    validate_todos(&input.todos)?;
    let store_path = todo_store_path()?;
    let old_todos = if store_path.exists() {
        serde_json::from_str::<Vec<TodoItem>>(
            &std::fs::read_to_string(&store_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?
    } else {
        Vec::new()
    };

    let all_done = input
        .todos
        .iter()
        .all(|todo| matches!(todo.status, TodoStatus::Completed));
    let persisted = if all_done {
        Vec::new()
    } else {
        input.todos.clone()
    };

    if let Some(parent) = store_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        &store_path,
        serde_json::to_string_pretty(&persisted).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let verification_nudge_needed = (all_done
        && input.todos.len() >= 3
        && !input
            .todos
            .iter()
            .any(|todo| todo.content.to_lowercase().contains("verif")))
    .then_some(true);

    Ok(TodoWriteOutput {
        old_todos,
        new_todos: input.todos,
        verification_nudge_needed,
    })
}

fn execute_skill(input: SkillInput) -> Result<SkillOutput, String> {
    let skill_path = resolve_skill_path(&input.skill)?;
    let prompt = std::fs::read_to_string(&skill_path).map_err(|error| error.to_string())?;
    let description = parse_skill_description(&prompt);

    Ok(SkillOutput {
        skill: input.skill,
        path: skill_path.display().to_string(),
        args: input.args,
        description,
        prompt,
    })
}

fn validate_todos(todos: &[TodoItem]) -> Result<(), String> {
    if todos.is_empty() {
        return Err(String::from("todos must not be empty"));
    }
    // Allow multiple in_progress items for parallel workflows
    if todos.iter().any(|todo| todo.content.trim().is_empty()) {
        return Err(String::from("todo content must not be empty"));
    }
    if todos.iter().any(|todo| todo.active_form.trim().is_empty()) {
        return Err(String::from("todo activeForm must not be empty"));
    }
    Ok(())
}

fn todo_store_path() -> Result<std::path::PathBuf, String> {
    if let Ok(path) = std::env::var("CLAW_TODO_STORE") {
        return Ok(std::path::PathBuf::from(path));
    }
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    Ok(cwd.join(".claw-todos.json"))
}

fn resolve_skill_path(skill: &str) -> Result<std::path::PathBuf, String> {
    let requested = skill.trim().trim_start_matches('/').trim_start_matches('$');
    if requested.is_empty() {
        return Err(String::from("skill must not be empty"));
    }

    let mut candidates = Vec::new();
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        candidates.push(std::path::PathBuf::from(codex_home).join("skills"));
    }
    if let Ok(home) = std::env::var("HOME") {
        let home = std::path::PathBuf::from(home);
        candidates.push(home.join(".agents").join("skills"));
        candidates.push(home.join(".config").join("opencode").join("skills"));
        candidates.push(home.join(".codex").join("skills"));
    }
    for root in candidates {
        let direct = root.join(requested).join("SKILL.md");
        if direct.exists() {
            return Ok(direct);
        }

        if let Ok(entries) = std::fs::read_dir(&root) {
            for entry in entries.flatten() {
                let path = entry.path().join("SKILL.md");
                if !path.exists() {
                    continue;
                }
                if entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(requested)
                {
                    return Ok(path);
                }
            }
        }
    }

    Err(format!("unknown skill: {requested}"))
}

const DEFAULT_AGENT_MODEL: &str = "claude-opus-4-6";
const DEFAULT_AGENT_SYSTEM_DATE: &str = "2026-03-31";
const DEFAULT_AGENT_MAX_ITERATIONS: usize = 32;

fn execute_agent(input: AgentInput) -> Result<AgentOutput, String> {
    execute_agent_with_spawn(input, spawn_agent_job)
}

fn execute_agent_with_spawn<F>(input: AgentInput, spawn_fn: F) -> Result<AgentOutput, String>
where
    F: FnOnce(AgentJob) -> Result<(), String>,
{
    if input.description.trim().is_empty() {
        return Err(String::from("description must not be empty"));
    }
    if input.prompt.trim().is_empty() {
        return Err(String::from("prompt must not be empty"));
    }

    let agent_id = make_agent_id();
    let output_dir = agent_store_dir()?;
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let output_file = output_dir.join(format!("{agent_id}.md"));
    let manifest_file = output_dir.join(format!("{agent_id}.json"));
    let normalized_subagent_type = normalize_subagent_type(input.subagent_type.as_deref());
    let model = resolve_agent_model(input.model.as_deref());
    let agent_name = input
        .name
        .as_deref()
        .map(slugify_agent_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| slugify_agent_name(&input.description));
    let created_at = iso8601_now();
    let system_prompt = build_agent_system_prompt(&normalized_subagent_type)?;
    let allowed_tools = allowed_tools_for_subagent(&normalized_subagent_type);

    let output_contents = format!(
        "# Agent Task

- id: {}
- name: {}
- description: {}
- subagent_type: {}
- created_at: {}

## Prompt

{}
",
        agent_id, agent_name, input.description, normalized_subagent_type, created_at, input.prompt
    );
    std::fs::write(&output_file, output_contents).map_err(|error| error.to_string())?;

    let manifest = AgentOutput {
        agent_id,
        name: agent_name,
        description: input.description,
        subagent_type: Some(normalized_subagent_type),
        model: Some(model),
        status: String::from("running"),
        output_file: output_file.display().to_string(),
        manifest_file: manifest_file.display().to_string(),
        created_at: created_at.clone(),
        started_at: Some(created_at),
        completed_at: None,
        error: None,
    };
    write_agent_manifest(&manifest)?;

    let manifest_for_spawn = manifest.clone();
    let job = AgentJob {
        manifest: manifest_for_spawn,
        prompt: input.prompt,
        system_prompt,
        allowed_tools,
    };
    if let Err(error) = spawn_fn(job) {
        let error = format!("failed to spawn sub-agent: {error}");
        persist_agent_terminal_state(&manifest, "failed", None, Some(error.clone()))?;
        return Err(error);
    }

    Ok(manifest)
}

fn spawn_agent_job(job: AgentJob) -> Result<(), String> {
    let thread_name = format!("claw-agent-{}", job.manifest.agent_id);
    std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_agent_job(&job)));
            match result {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    let _ =
                        persist_agent_terminal_state(&job.manifest, "failed", None, Some(error));
                }
                Err(_) => {
                    let _ = persist_agent_terminal_state(
                        &job.manifest,
                        "failed",
                        None,
                        Some(String::from("sub-agent thread panicked")),
                    );
                }
            }
        })
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn run_agent_job(job: &AgentJob) -> Result<(), String> {
    let mut runtime = build_agent_runtime(job)?.with_max_iterations(DEFAULT_AGENT_MAX_ITERATIONS);
    let summary = runtime
        .run_turn(job.prompt.clone(), None)
        .map_err(|error| error.to_string())?;
    let final_text = final_assistant_text(&summary);
    persist_agent_terminal_state(&job.manifest, "completed", Some(final_text.as_str()), None)
}

fn build_agent_runtime(
    job: &AgentJob,
) -> Result<ConversationRuntime<ProviderRuntimeClient, SubagentToolExecutor>, String> {
    let model = job
        .manifest
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_AGENT_MODEL.to_string());
    let allowed_tools = job.allowed_tools.clone();
    let api_client = ProviderRuntimeClient::new(model, allowed_tools.clone())?;
    let tool_executor = SubagentToolExecutor::new(allowed_tools);
    Ok(ConversationRuntime::new(
        Session::new(),
        api_client,
        tool_executor,
        agent_permission_policy(),
        job.system_prompt.clone(),
    ))
}

fn build_agent_system_prompt(subagent_type: &str) -> Result<Vec<String>, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    let mut prompt = load_system_prompt(
        cwd,
        DEFAULT_AGENT_SYSTEM_DATE.to_string(),
        std::env::consts::OS,
        "unknown",
        Some(DEFAULT_AGENT_MODEL),
    )
    .map_err(|error| error.to_string())?;
    prompt.push(format!(
        "You are a background sub-agent of type `{subagent_type}`. Work only on the delegated task, use only the tools available to you, do not ask the user questions, and finish with a concise result."
    ));
    Ok(prompt)
}

fn resolve_agent_model(model: Option<&str>) -> String {
    model
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or(DEFAULT_AGENT_MODEL)
        .to_string()
}

fn allowed_tools_for_subagent(subagent_type: &str) -> BTreeSet<String> {
    let tools = match subagent_type {
        "Explore" => vec![
            "read_file",
            "glob_search",
            "grep_search",
            "WebFetch",
            "WebSearch",
            "ToolSearch",
            "Skill",
            "StructuredOutput",
        ],
        "Plan" => vec![
            "read_file",
            "glob_search",
            "grep_search",
            "WebFetch",
            "WebSearch",
            "ToolSearch",
            "Skill",
            "TodoWrite",
            "StructuredOutput",
            "SendUserMessage",
        ],
        "Verification" => vec![
            "bash",
            "read_file",
            "glob_search",
            "grep_search",
            "WebFetch",
            "WebSearch",
            "ToolSearch",
            "TodoWrite",
            "StructuredOutput",
            "SendUserMessage",
            "PowerShell",
        ],
        "claw-guide" => vec![
            "read_file",
            "glob_search",
            "grep_search",
            "WebFetch",
            "WebSearch",
            "ToolSearch",
            "Skill",
            "StructuredOutput",
            "SendUserMessage",
        ],
        "statusline-setup" => vec![
            "bash",
            "read_file",
            "write_file",
            "edit_file",
            "glob_search",
            "grep_search",
            "ToolSearch",
        ],
        _ => vec![
            "bash",
            "read_file",
            "write_file",
            "edit_file",
            "glob_search",
            "grep_search",
            "WebFetch",
            "WebSearch",
            "TodoWrite",
            "Skill",
            "ToolSearch",
            "NotebookEdit",
            "Sleep",
            "SendUserMessage",
            "Config",
            "StructuredOutput",
            "REPL",
            "PowerShell",
        ],
    };
    tools.into_iter().map(str::to_string).collect()
}

fn agent_permission_policy() -> PermissionPolicy {
    mvp_tool_specs().into_iter().fold(
        PermissionPolicy::new(PermissionMode::DangerFullAccess),
        |policy, spec| policy.with_tool_requirement(spec.name, spec.required_permission),
    )
}

fn write_agent_manifest(manifest: &AgentOutput) -> Result<(), String> {
    std::fs::write(
        &manifest.manifest_file,
        serde_json::to_string_pretty(manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn persist_agent_terminal_state(
    manifest: &AgentOutput,
    status: &str,
    result: Option<&str>,
    error: Option<String>,
) -> Result<(), String> {
    append_agent_output(
        &manifest.output_file,
        &format_agent_terminal_output(status, result, error.as_deref()),
    )?;
    let mut next_manifest = manifest.clone();
    next_manifest.status = status.to_string();
    next_manifest.completed_at = Some(iso8601_now());
    next_manifest.error = error;
    write_agent_manifest(&next_manifest)
}

fn append_agent_output(path: &str, suffix: &str) -> Result<(), String> {
    use std::io::Write as _;

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(suffix.as_bytes())
        .map_err(|error| error.to_string())
}

fn format_agent_terminal_output(status: &str, result: Option<&str>, error: Option<&str>) -> String {
    let mut sections = vec![format!("\n## Result\n\n- status: {status}\n")];
    if let Some(result) = result.filter(|value| !value.trim().is_empty()) {
        sections.push(format!("\n### Final response\n\n{}\n", result.trim()));
    }
    if let Some(error) = error.filter(|value| !value.trim().is_empty()) {
        sections.push(format!("\n### Error\n\n{}\n", error.trim()));
    }
    sections.join("")
}

struct ProviderRuntimeClient {
    runtime: tokio::runtime::Runtime,
    client: ProviderClient,
    model: String,
    allowed_tools: BTreeSet<String>,
}

impl ProviderRuntimeClient {
    fn new(model: String, allowed_tools: BTreeSet<String>) -> Result<Self, String> {
        let model = resolve_model_alias(&model).to_string();
        let client = ProviderClient::from_model(&model).map_err(|error| error.to_string())?;
        Ok(Self {
            runtime: tokio::runtime::Runtime::new().map_err(|error| error.to_string())?,
            client,
            model,
            allowed_tools,
        })
    }
}

impl ApiClient for ProviderRuntimeClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
        let tools = tool_specs_for_allowed_tools(Some(&self.allowed_tools))
            .into_iter()
            .map(|spec| ToolDefinition {
                name: spec.name.to_string(),
                description: Some(spec.description.to_string()),
                input_schema: spec.input_schema,
            })
            .collect::<Vec<_>>();
        let message_request = MessageRequest {
            model: self.model.clone(),
            max_tokens: max_tokens_for_model(&self.model),
            messages: convert_messages(&request.messages),
            system: (!request.system_prompt.is_empty()).then(|| request.system_prompt.join("\n\n")),
            tools: (!tools.is_empty()).then_some(tools),
            tool_choice: (!self.allowed_tools.is_empty()).then_some(ToolChoice::Auto),
            reasoning_effort: None,
            stream: true,
        };

        self.runtime.block_on(async {
            let mut stream = self
                .client
                .stream_message(&message_request)
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?;
            let mut events = Vec::new();
            let mut pending_tools: BTreeMap<u32, (String, String, String)> = BTreeMap::new();
            let mut saw_stop = false;

            while let Some(event) = stream
                .next_event()
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?
            {
                match event {
                    ApiStreamEvent::MessageStart(start) => {
                        for block in start.message.content {
                            push_output_block(block, 0, &mut events, &mut pending_tools, true);
                        }
                    }
                    ApiStreamEvent::ContentBlockStart(start) => {
                        push_output_block(
                            start.content_block,
                            start.index,
                            &mut events,
                            &mut pending_tools,
                            true,
                        );
                    }
                    ApiStreamEvent::ContentBlockDelta(delta) => match delta.delta {
                        ContentBlockDelta::TextDelta { text } => {
                            if !text.is_empty() {
                                events.push(AssistantEvent::TextDelta(text));
                            }
                        }
                        ContentBlockDelta::InputJsonDelta { partial_json } => {
                            if let Some((_, _, input)) = pending_tools.get_mut(&delta.index) {
                                input.push_str(&partial_json);
                            }
                        }
                        ContentBlockDelta::ThinkingDelta { thinking } => {
                            if !thinking.is_empty() {
                                events.push(AssistantEvent::ReasoningDelta {
                                    text: thinking,
                                    redacted: false,
                                });
                            }
                        }
                        ContentBlockDelta::SignatureDelta { .. } => {}
                    },
                    ApiStreamEvent::ContentBlockStop(stop) => {
                        if let Some((id, name, input)) = pending_tools.remove(&stop.index) {
                            events.push(AssistantEvent::ToolUse { id, name, input });
                        }
                    }
                    ApiStreamEvent::MessageDelta(delta) => {
                        events.push(AssistantEvent::Usage(TokenUsage {
                            input_tokens: delta.usage.input_tokens,
                            output_tokens: delta.usage.output_tokens,
                            cache_creation_input_tokens: 0,
                            cache_read_input_tokens: 0,
                        }));
                    }
                    ApiStreamEvent::MessageStop(_) => {
                        saw_stop = true;
                        events.push(AssistantEvent::MessageStop);
                    }
                }
            }

            if !saw_stop
                && events.iter().any(|event| {
                    matches!(event, AssistantEvent::TextDelta(text) if !text.is_empty())
                        || matches!(
                            event,
                            AssistantEvent::ReasoningDelta { text, redacted: false }
                                if !text.is_empty()
                        )
                        || matches!(event, AssistantEvent::ToolUse { .. })
                })
            {
                events.push(AssistantEvent::MessageStop);
            }

            if events
                .iter()
                .any(|event| matches!(event, AssistantEvent::MessageStop))
            {
                return Ok(events);
            }

            let response = self
                .client
                .send_message(&MessageRequest {
                    stream: false,
                    ..message_request.clone()
                })
                .await
                .map_err(|error| RuntimeError::new(error.to_string()))?;
            Ok(response_to_events(response))
        })
    }
}

struct SubagentToolExecutor {
    allowed_tools: BTreeSet<String>,
}

impl SubagentToolExecutor {
    fn new(allowed_tools: BTreeSet<String>) -> Self {
        Self { allowed_tools }
    }
}

impl ToolExecutor for SubagentToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        if !self.allowed_tools.contains(tool_name) {
            return Err(ToolError::new(format!(
                "tool `{tool_name}` is not enabled for this sub-agent"
            )));
        }
        let value = serde_json::from_str(input)
            .map_err(|error| ToolError::new(format!("invalid tool input JSON: {error}")))?;
        execute_tool(tool_name, &value).map_err(ToolError::new)
    }
}

fn tool_specs_for_allowed_tools(allowed_tools: Option<&BTreeSet<String>>) -> Vec<ToolSpec> {
    mvp_tool_specs()
        .into_iter()
        .filter(|spec| allowed_tools.is_none_or(|allowed| allowed.contains(spec.name)))
        .collect()
}

fn convert_messages(messages: &[ConversationMessage]) -> Vec<InputMessage> {
    messages
        .iter()
        .filter_map(|message| {
            let role = match message.role {
                MessageRole::System | MessageRole::User | MessageRole::Tool => "user",
                MessageRole::Assistant => "assistant",
            };
            let content = message
                .blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => InputContentBlock::Text { text: text.clone() },
                    ContentBlock::ToolUse { id, name, input } => InputContentBlock::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::from_str(input)
                            .unwrap_or_else(|_| serde_json::json!({ "raw": input })),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id,
                        output,
                        is_error,
                        ..
                    } => InputContentBlock::ToolResult {
                        tool_use_id: tool_use_id.clone(),
                        content: vec![ToolResultContentBlock::Text {
                            text: output.clone(),
                        }],
                        is_error: *is_error,
                    },
                })
                .collect::<Vec<_>>();
            (!content.is_empty()).then(|| InputMessage {
                role: role.to_string(),
                content,
            })
        })
        .collect()
}

fn push_output_block(
    block: OutputContentBlock,
    block_index: u32,
    events: &mut Vec<AssistantEvent>,
    pending_tools: &mut BTreeMap<u32, (String, String, String)>,
    streaming_tool_input: bool,
) {
    match block {
        OutputContentBlock::Text { text } => {
            if !text.is_empty() {
                events.push(AssistantEvent::TextDelta(text));
            }
        }
        OutputContentBlock::ToolUse { id, name, input } => {
            let initial_input = if streaming_tool_input
                && input.is_object()
                && input.as_object().is_some_and(serde_json::Map::is_empty)
            {
                String::new()
            } else {
                input.to_string()
            };
            pending_tools.insert(block_index, (id, name, initial_input));
        }
        OutputContentBlock::Thinking { thinking, .. } => {
            if !thinking.is_empty() {
                events.push(AssistantEvent::ReasoningDelta {
                    text: thinking,
                    redacted: false,
                });
            }
        }
        OutputContentBlock::RedactedThinking { .. } => {
            events.push(AssistantEvent::ReasoningDelta {
                text: String::new(),
                redacted: true,
            });
        }
    }
}

fn response_to_events(response: MessageResponse) -> Vec<AssistantEvent> {
    let mut events = Vec::new();
    let mut pending_tools = BTreeMap::new();

    for (index, block) in response.content.into_iter().enumerate() {
        let index = u32::try_from(index).expect("response block index overflow");
        push_output_block(block, index, &mut events, &mut pending_tools, false);
        if let Some((id, name, input)) = pending_tools.remove(&index) {
            events.push(AssistantEvent::ToolUse { id, name, input });
        }
    }

    events.push(AssistantEvent::Usage(TokenUsage {
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        cache_creation_input_tokens: response.usage.cache_creation_input_tokens,
        cache_read_input_tokens: response.usage.cache_read_input_tokens,
    }));
    events.push(AssistantEvent::MessageStop);
    events
}

fn final_assistant_text(summary: &runtime::TurnSummary) -> String {
    summary
        .assistant_messages
        .last()
        .map(|message| {
            message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

#[allow(clippy::needless_pass_by_value)]
fn execute_tool_search(input: ToolSearchInput) -> ToolSearchOutput {
    let deferred = deferred_tool_specs();
    let max_results = input.max_results.unwrap_or(5).max(1);
    let query = input.query.trim().to_string();
    let normalized_query = normalize_tool_search_query(&query);
    let matches = search_tool_specs(&query, max_results, &deferred);

    ToolSearchOutput {
        matches,
        query,
        normalized_query,
        total_deferred_tools: deferred.len(),
        pending_mcp_servers: None,
    }
}

fn deferred_tool_specs() -> Vec<ToolSpec> {
    mvp_tool_specs()
        .into_iter()
        .filter(|spec| {
            !matches!(
                spec.name,
                "bash" | "read_file" | "write_file" | "edit_file" | "glob_search" | "grep_search"
            )
        })
        .collect()
}

fn search_tool_specs(query: &str, max_results: usize, specs: &[ToolSpec]) -> Vec<String> {
    let lowered = query.to_lowercase();
    if let Some(selection) = lowered.strip_prefix("select:") {
        return selection
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .filter_map(|wanted| {
                let wanted = canonical_tool_token(wanted);
                specs
                    .iter()
                    .find(|spec| canonical_tool_token(spec.name) == wanted)
                    .map(|spec| spec.name.to_string())
            })
            .take(max_results)
            .collect();
    }

    let mut required = Vec::new();
    let mut optional = Vec::new();
    for term in lowered.split_whitespace() {
        if let Some(rest) = term.strip_prefix('+') {
            if !rest.is_empty() {
                required.push(rest);
            }
        } else {
            optional.push(term);
        }
    }
    let terms = if required.is_empty() {
        optional.clone()
    } else {
        required.iter().chain(optional.iter()).copied().collect()
    };

    let mut scored = specs
        .iter()
        .filter_map(|spec| {
            let name = spec.name.to_lowercase();
            let canonical_name = canonical_tool_token(spec.name);
            let normalized_description = normalize_tool_search_query(spec.description);
            let haystack = format!(
                "{name} {} {canonical_name}",
                spec.description.to_lowercase()
            );
            let normalized_haystack = format!("{canonical_name} {normalized_description}");
            if required.iter().any(|term| !haystack.contains(term)) {
                return None;
            }

            let mut score = 0_i32;
            for term in &terms {
                let canonical_term = canonical_tool_token(term);
                if haystack.contains(term) {
                    score += 2;
                }
                if name == *term {
                    score += 8;
                }
                if name.contains(term) {
                    score += 4;
                }
                if canonical_name == canonical_term {
                    score += 12;
                }
                if normalized_haystack.contains(&canonical_term) {
                    score += 3;
                }
            }

            if score == 0 && !lowered.is_empty() {
                return None;
            }
            Some((score, spec.name.to_string()))
        })
        .collect::<Vec<_>>();

    scored.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
    scored
        .into_iter()
        .map(|(_, name)| name)
        .take(max_results)
        .collect()
}

fn normalize_tool_search_query(query: &str) -> String {
    query
        .trim()
        .split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|term| !term.is_empty())
        .map(canonical_tool_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn canonical_tool_token(value: &str) -> String {
    let mut canonical = value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect::<String>();
    if let Some(stripped) = canonical.strip_suffix("tool") {
        canonical = stripped.to_string();
    }
    canonical
}

fn agent_store_dir() -> Result<std::path::PathBuf, String> {
    if let Ok(path) = std::env::var("CLAW_AGENT_STORE") {
        return Ok(std::path::PathBuf::from(path));
    }
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    if let Some(workspace_root) = cwd.ancestors().nth(2) {
        return Ok(workspace_root.join(".claw-agents"));
    }
    Ok(cwd.join(".claw-agents"))
}

fn make_agent_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("agent-{nanos}")
}

fn slugify_agent_name(description: &str) -> String {
    let mut out = description
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').chars().take(32).collect()
}

fn normalize_subagent_type(subagent_type: Option<&str>) -> String {
    let trimmed = subagent_type.map(str::trim).unwrap_or_default();
    if trimmed.is_empty() {
        return String::from("general-purpose");
    }

    match canonical_tool_token(trimmed).as_str() {
        "general" | "generalpurpose" | "generalpurposeagent" => String::from("general-purpose"),
        "explore" | "explorer" | "exploreagent" => String::from("Explore"),
        "plan" | "planagent" => String::from("Plan"),
        "verification" | "verificationagent" | "verify" | "verifier" => {
            String::from("Verification")
        }
        "clawguide" | "clawguideagent" | "guide" => String::from("claw-guide"),
        "statusline" | "statuslinesetup" => String::from("statusline-setup"),
        _ => trimmed.to_string(),
    }
}

fn iso8601_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[allow(clippy::too_many_lines)]
fn execute_notebook_edit(input: NotebookEditInput) -> Result<NotebookEditOutput, String> {
    let path = std::path::PathBuf::from(&input.notebook_path);
    if path.extension().and_then(|ext| ext.to_str()) != Some("ipynb") {
        return Err(String::from(
            "File must be a Jupyter notebook (.ipynb file).",
        ));
    }

    let original_file = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
    let mut notebook: serde_json::Value =
        serde_json::from_str(&original_file).map_err(|error| error.to_string())?;
    let language = notebook
        .get("metadata")
        .and_then(|metadata| metadata.get("kernelspec"))
        .and_then(|kernelspec| kernelspec.get("language"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("python")
        .to_string();
    let cells = notebook
        .get_mut("cells")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| String::from("Notebook cells array not found"))?;

    let edit_mode = input.edit_mode.unwrap_or(NotebookEditMode::Replace);
    let target_index = match input.cell_id.as_deref() {
        Some(cell_id) => Some(resolve_cell_index(cells, Some(cell_id), edit_mode)?),
        None if matches!(
            edit_mode,
            NotebookEditMode::Replace | NotebookEditMode::Delete
        ) =>
        {
            Some(resolve_cell_index(cells, None, edit_mode)?)
        }
        None => None,
    };
    let resolved_cell_type = match edit_mode {
        NotebookEditMode::Delete => None,
        NotebookEditMode::Insert => Some(input.cell_type.unwrap_or(NotebookCellType::Code)),
        NotebookEditMode::Replace => Some(input.cell_type.unwrap_or_else(|| {
            target_index
                .and_then(|index| cells.get(index))
                .and_then(cell_kind)
                .unwrap_or(NotebookCellType::Code)
        })),
    };
    let new_source = require_notebook_source(input.new_source, edit_mode)?;

    let cell_id = match edit_mode {
        NotebookEditMode::Insert => {
            let resolved_cell_type = resolved_cell_type.expect("insert cell type");
            let new_id = make_cell_id(cells.len());
            let new_cell = build_notebook_cell(&new_id, resolved_cell_type, &new_source);
            let insert_at = target_index.map_or(cells.len(), |index| index + 1);
            cells.insert(insert_at, new_cell);
            cells
                .get(insert_at)
                .and_then(|cell| cell.get("id"))
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        }
        NotebookEditMode::Delete => {
            let removed = cells.remove(target_index.expect("delete target index"));
            removed
                .get("id")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        }
        NotebookEditMode::Replace => {
            let resolved_cell_type = resolved_cell_type.expect("replace cell type");
            let cell = cells
                .get_mut(target_index.expect("replace target index"))
                .ok_or_else(|| String::from("Cell index out of range"))?;
            cell["source"] = serde_json::Value::Array(source_lines(&new_source));
            cell["cell_type"] = serde_json::Value::String(match resolved_cell_type {
                NotebookCellType::Code => String::from("code"),
                NotebookCellType::Markdown => String::from("markdown"),
            });
            match resolved_cell_type {
                NotebookCellType::Code => {
                    if !cell.get("outputs").is_some_and(serde_json::Value::is_array) {
                        cell["outputs"] = json!([]);
                    }
                    if cell.get("execution_count").is_none() {
                        cell["execution_count"] = serde_json::Value::Null;
                    }
                }
                NotebookCellType::Markdown => {
                    if let Some(object) = cell.as_object_mut() {
                        object.remove("outputs");
                        object.remove("execution_count");
                    }
                }
            }
            cell.get("id")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
        }
    };

    let updated_file =
        serde_json::to_string_pretty(&notebook).map_err(|error| error.to_string())?;
    std::fs::write(&path, &updated_file).map_err(|error| error.to_string())?;

    Ok(NotebookEditOutput {
        new_source,
        cell_id,
        cell_type: resolved_cell_type,
        language,
        edit_mode: format_notebook_edit_mode(edit_mode),
        error: None,
        notebook_path: path.display().to_string(),
        original_file,
        updated_file,
    })
}

fn require_notebook_source(
    source: Option<String>,
    edit_mode: NotebookEditMode,
) -> Result<String, String> {
    match edit_mode {
        NotebookEditMode::Delete => Ok(source.unwrap_or_default()),
        NotebookEditMode::Insert | NotebookEditMode::Replace => source
            .ok_or_else(|| String::from("new_source is required for insert and replace edits")),
    }
}

fn build_notebook_cell(cell_id: &str, cell_type: NotebookCellType, source: &str) -> Value {
    let mut cell = json!({
        "cell_type": match cell_type {
            NotebookCellType::Code => "code",
            NotebookCellType::Markdown => "markdown",
        },
        "id": cell_id,
        "metadata": {},
        "source": source_lines(source),
    });
    if let Some(object) = cell.as_object_mut() {
        match cell_type {
            NotebookCellType::Code => {
                object.insert(String::from("outputs"), json!([]));
                object.insert(String::from("execution_count"), Value::Null);
            }
            NotebookCellType::Markdown => {}
        }
    }
    cell
}

fn cell_kind(cell: &serde_json::Value) -> Option<NotebookCellType> {
    cell.get("cell_type")
        .and_then(serde_json::Value::as_str)
        .map(|kind| {
            if kind == "markdown" {
                NotebookCellType::Markdown
            } else {
                NotebookCellType::Code
            }
        })
}

#[allow(clippy::needless_pass_by_value)]
fn execute_sleep(input: SleepInput) -> SleepOutput {
    std::thread::sleep(Duration::from_millis(input.duration_ms));
    SleepOutput {
        duration_ms: input.duration_ms,
        message: format!("Slept for {}ms", input.duration_ms),
    }
}

fn execute_brief(input: BriefInput) -> Result<BriefOutput, String> {
    if input.message.trim().is_empty() {
        return Err(String::from("message must not be empty"));
    }

    let attachments = input
        .attachments
        .as_ref()
        .map(|paths| {
            paths
                .iter()
                .map(|path| resolve_attachment(path))
                .collect::<Result<Vec<_>, String>>()
        })
        .transpose()?;

    let message = match input.status {
        BriefStatus::Normal | BriefStatus::Proactive => input.message,
    };

    Ok(BriefOutput {
        message,
        attachments,
        sent_at: iso8601_timestamp(),
    })
}

fn resolve_attachment(path: &str) -> Result<ResolvedAttachment, String> {
    let resolved = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
    let metadata = std::fs::metadata(&resolved).map_err(|error| error.to_string())?;
    Ok(ResolvedAttachment {
        path: resolved.display().to_string(),
        size: metadata.len(),
        is_image: is_image_path(&resolved),
    })
}

fn is_image_path(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg")
    )
}

fn execute_config(input: ConfigInput) -> Result<ConfigOutput, String> {
    let setting = input.setting.trim();
    if setting.is_empty() {
        return Err(String::from("setting must not be empty"));
    }
    let Some(spec) = supported_config_setting(setting) else {
        return Ok(ConfigOutput {
            success: false,
            operation: None,
            setting: None,
            value: None,
            previous_value: None,
            new_value: None,
            error: Some(format!("Unknown setting: \"{setting}\"")),
        });
    };

    let path = config_file_for_scope(spec.scope)?;
    let mut document = read_json_object(&path)?;

    if let Some(value) = input.value {
        let normalized = normalize_config_value(spec, value)?;
        let previous_value = get_nested_value(&document, spec.path).cloned();
        set_nested_value(&mut document, spec.path, normalized.clone());
        write_json_object(&path, &document)?;
        Ok(ConfigOutput {
            success: true,
            operation: Some(String::from("set")),
            setting: Some(setting.to_string()),
            value: Some(normalized.clone()),
            previous_value,
            new_value: Some(normalized),
            error: None,
        })
    } else {
        Ok(ConfigOutput {
            success: true,
            operation: Some(String::from("get")),
            setting: Some(setting.to_string()),
            value: get_nested_value(&document, spec.path).cloned(),
            previous_value: None,
            new_value: None,
            error: None,
        })
    }
}

fn execute_structured_output(input: StructuredOutputInput) -> StructuredOutputResult {
    StructuredOutputResult {
        data: String::from("Structured output provided successfully"),
        structured_output: input.0,
    }
}

fn execute_repl(input: ReplInput) -> Result<ReplOutput, String> {
    if input.code.trim().is_empty() {
        return Err(String::from("code must not be empty"));
    }
    let runtime = resolve_repl_runtime(&input.language)?;
    let started = Instant::now();
    let mut command = Command::new(runtime.program);
    command.args(runtime.args).arg(&input.code);
    let managed = runtime::managed_process::output(&mut command, input.timeout_ms.map(Duration::from_millis))
        .map_err(|error| error.to_string())?;
    let output = managed.output;
    let mut stderr = decode_console_output(&output.stderr);
    if let Some(reason) = managed.interruption {
        if !stderr.is_empty() { stderr.push('\n'); }
        stderr.push_str(&format!("REPL 已停止，原因：{reason:?}；受管进程已退出"));
    }

    Ok(ReplOutput {
        language: input.language,
        stdout: decode_console_output(&output.stdout),
        stderr,
        exit_code: if managed.interruption.is_some() { 124 } else { output.status.code().unwrap_or(1) },
        duration_ms: started.elapsed().as_millis(),
        interrupted: managed.interruption.is_some(),
    })
}

struct ReplRuntime {
    program: &'static str,
    args: &'static [&'static str],
}

fn resolve_repl_runtime(language: &str) -> Result<ReplRuntime, String> {
    match language.trim().to_ascii_lowercase().as_str() {
        "python" | "py" => Ok(ReplRuntime {
            program: detect_first_command(&["python3", "python"])
                .ok_or_else(|| String::from("python runtime not found"))?,
            args: &["-c"],
        }),
        "javascript" | "js" | "node" => Ok(ReplRuntime {
            program: detect_first_command(&["node"])
                .ok_or_else(|| String::from("node runtime not found"))?,
            args: &["-e"],
        }),
        "sh" | "shell" | "bash" => Ok(ReplRuntime {
            program: detect_first_command(&["bash", "sh"])
                .ok_or_else(|| String::from("shell runtime not found"))?,
            args: &["-lc"],
        }),
        other => Err(format!("unsupported REPL language: {other}")),
    }
}

fn detect_first_command(commands: &[&'static str]) -> Option<&'static str> {
    commands
        .iter()
        .copied()
        .find(|command| command_runs_version(command))
}

fn command_runs_version(command: &str) -> bool {
    std::process::Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[derive(Clone, Copy)]
enum ConfigScope {
    Global,
    Settings,
}

#[derive(Clone, Copy)]
struct ConfigSettingSpec {
    scope: ConfigScope,
    kind: ConfigKind,
    path: &'static [&'static str],
    options: Option<&'static [&'static str]>,
}

#[derive(Clone, Copy)]
enum ConfigKind {
    Boolean,
    String,
}

fn supported_config_setting(setting: &str) -> Option<ConfigSettingSpec> {
    Some(match setting {
        "theme" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::String,
            path: &["theme"],
            options: None,
        },
        "editorMode" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::String,
            path: &["editorMode"],
            options: Some(&["default", "vim", "emacs"]),
        },
        "verbose" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::Boolean,
            path: &["verbose"],
            options: None,
        },
        "preferredNotifChannel" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::String,
            path: &["preferredNotifChannel"],
            options: None,
        },
        "autoCompactEnabled" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::Boolean,
            path: &["autoCompactEnabled"],
            options: None,
        },
        "autoMemoryEnabled" => ConfigSettingSpec {
            scope: ConfigScope::Settings,
            kind: ConfigKind::Boolean,
            path: &["autoMemoryEnabled"],
            options: None,
        },
        "autoDreamEnabled" => ConfigSettingSpec {
            scope: ConfigScope::Settings,
            kind: ConfigKind::Boolean,
            path: &["autoDreamEnabled"],
            options: None,
        },
        "fileCheckpointingEnabled" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::Boolean,
            path: &["fileCheckpointingEnabled"],
            options: None,
        },
        "showTurnDuration" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::Boolean,
            path: &["showTurnDuration"],
            options: None,
        },
        "terminalProgressBarEnabled" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::Boolean,
            path: &["terminalProgressBarEnabled"],
            options: None,
        },
        "todoFeatureEnabled" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::Boolean,
            path: &["todoFeatureEnabled"],
            options: None,
        },
        "model" => ConfigSettingSpec {
            scope: ConfigScope::Settings,
            kind: ConfigKind::String,
            path: &["model"],
            options: None,
        },
        "alwaysThinkingEnabled" => ConfigSettingSpec {
            scope: ConfigScope::Settings,
            kind: ConfigKind::Boolean,
            path: &["alwaysThinkingEnabled"],
            options: None,
        },
        "permissions.defaultMode" => ConfigSettingSpec {
            scope: ConfigScope::Settings,
            kind: ConfigKind::String,
            path: &["permissions", "defaultMode"],
            options: Some(&["default", "plan", "acceptEdits", "dontAsk", "auto"]),
        },
        "language" => ConfigSettingSpec {
            scope: ConfigScope::Settings,
            kind: ConfigKind::String,
            path: &["language"],
            options: None,
        },
        "teammateMode" => ConfigSettingSpec {
            scope: ConfigScope::Global,
            kind: ConfigKind::String,
            path: &["teammateMode"],
            options: Some(&["tmux", "in-process", "auto"]),
        },
        _ => return None,
    })
}

fn normalize_config_value(spec: ConfigSettingSpec, value: ConfigValue) -> Result<Value, String> {
    let normalized = match (spec.kind, value) {
        (ConfigKind::Boolean, ConfigValue::Bool(value)) => Value::Bool(value),
        (ConfigKind::Boolean, ConfigValue::String(value)) => {
            match value.trim().to_ascii_lowercase().as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(String::from("setting requires true or false")),
            }
        }
        (ConfigKind::Boolean, ConfigValue::Number(_)) => {
            return Err(String::from("setting requires true or false"))
        }
        (ConfigKind::String, ConfigValue::String(value)) => Value::String(value),
        (ConfigKind::String, ConfigValue::Bool(value)) => Value::String(value.to_string()),
        (ConfigKind::String, ConfigValue::Number(value)) => json!(value),
    };

    if let Some(options) = spec.options {
        let Some(as_str) = normalized.as_str() else {
            return Err(String::from("setting requires a string value"));
        };
        if !options.iter().any(|option| option == &as_str) {
            return Err(format!(
                "Invalid value \"{as_str}\". Options: {}",
                options.join(", ")
            ));
        }
    }

    Ok(normalized)
}

fn config_file_for_scope(scope: ConfigScope) -> Result<PathBuf, String> {
    let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
    Ok(match scope {
        ConfigScope::Global => config_home_dir()?.join("settings.json"),
        ConfigScope::Settings => cwd.join(".claw").join("settings.local.json"),
    })
}

fn config_home_dir() -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var("CLAW_CONFIG_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = std::env::var("HOME").map_err(|_| String::from("HOME is not set"))?;
    Ok(PathBuf::from(home).join(".claw"))
}

fn read_json_object(path: &Path) -> Result<serde_json::Map<String, Value>, String> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            if contents.trim().is_empty() {
                return Ok(serde_json::Map::new());
            }
            serde_json::from_str::<Value>(&contents)
                .map_err(|error| error.to_string())?
                .as_object()
                .cloned()
                .ok_or_else(|| String::from("config file must contain a JSON object"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(serde_json::Map::new()),
        Err(error) => Err(error.to_string()),
    }
}

fn write_json_object(path: &Path, value: &serde_json::Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_string_pretty(value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn get_nested_value<'a>(
    value: &'a serde_json::Map<String, Value>,
    path: &[&str],
) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut current = value.get(*first)?;
    for key in rest {
        current = current.as_object()?.get(*key)?;
    }
    Some(current)
}

fn set_nested_value(root: &mut serde_json::Map<String, Value>, path: &[&str], new_value: Value) {
    let (first, rest) = path.split_first().expect("config path must not be empty");
    if rest.is_empty() {
        root.insert((*first).to_string(), new_value);
        return;
    }

    let entry = root
        .entry((*first).to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if !entry.is_object() {
        *entry = Value::Object(serde_json::Map::new());
    }
    let map = entry.as_object_mut().expect("object inserted");
    set_nested_value(map, rest, new_value);
}

fn iso8601_timestamp() -> String {
    if let Ok(output) = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
    {
        if output.status.success() {
            return decode_console_output(&output.stdout).trim().to_string();
        }
    }
    iso8601_now()
}

/// 本进程的命令搜索目录集合（等价于改前 `find_command_path` 里那次
/// `std::env::var_os("PATH")` 读取）。
///
/// P-08/I6：只把「PATH 从哪里来」收敛成一个函数 —— 生产入口仍读本进程环境，
/// 测试改为向 [`find_command_path_in`] 显式传入路径集合。
fn process_command_search_paths() -> Option<Vec<PathBuf>> {
    std::env::var_os("PATH").map(|path| std::env::split_paths(&path).collect())
}

#[allow(clippy::needless_pass_by_value)]
fn execute_powershell(input: PowerShellInput) -> std::io::Result<runtime::BashCommandOutput> {
    // 生产入口：搜索范围 = 本进程 PATH，行为与改前一致。
    let shell = detect_powershell_shell()?;
    run_powershell_command(input, shell)
}

/// PowerShell 执行的公共实现（两个入口共用）：只负责 timeout 语义与 spawn。
fn run_powershell_command(
    input: PowerShellInput,
    shell: String,
) -> std::io::Result<runtime::BashCommandOutput> {
    let _ = &input.description;
    // timeout 语义为「秒」（schema 已标 SECONDS），底层 execute_shell_command 按毫秒判定，故 ×1000；
    // 上限 3600 秒避免误填超大值。历史 bug：曾把秒值直接当毫秒，模型传 30/60 即在几十毫秒内超时（runtime-timeout）。
    let timeout_ms = input
        .timeout
        .map(|secs| secs.min(3600).saturating_mul(1000));
    execute_shell_command(
        &shell,
        &input.command,
        input.cwd.as_deref(),
        timeout_ms,
        input.run_in_background,
    )
}

/// P-08/I6 **测试接缝**：与 [`execute_powershell`] 走同一个执行实现，
/// 只是"用哪个 shell"由**显式路径集合**查得 —— 不读本进程 PATH，于是用例不必再改父进程环境
/// （也就没有改进程环境的污染面与互斥需求）。
#[cfg(test)]
fn execute_powershell_with_search_paths(
    input: PowerShellInput,
    search_paths: Option<&[PathBuf]>,
) -> std::io::Result<runtime::BashCommandOutput> {
    let shell = detect_powershell_shell_in(search_paths)?;
    run_powershell_command(input, shell)
}

/// 生产入口：搜索范围 = 本进程 PATH（先委托公开入口 `find_command_path` 读环境）。
fn detect_powershell_shell() -> std::io::Result<String> {
    detect_powershell_shell_with(find_command_path)
}

/// 在**显式给出**的搜索目录集合里定位 PowerShell 可执行文件（P-08/I6 注入接缝，见
/// [`execute_powershell_with_search_paths`]）。
#[cfg(test)]
fn detect_powershell_shell_in(search_paths: Option<&[PathBuf]>) -> std::io::Result<String> {
    detect_powershell_shell_with(|command| find_command_path_in(command, search_paths))
}

/// 「`pwsh` 优先、再 `powershell`」这一查找顺序与错误文案的**单一份**实现，
/// 生产入口与注入接缝共用（避免两处顺序/文案各自漂移）。
fn detect_powershell_shell_with(
    lookup: impl Fn(&str) -> Option<String>,
) -> std::io::Result<String> {
    if let Some(shell) = lookup("pwsh") {
        Ok(shell)
    } else if let Some(shell) = lookup("powershell") {
        Ok(shell)
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "PowerShell executable not found (expected `pwsh` or `powershell` in PATH)",
        ))
    }
}

/// 命令解析入口（公开入口）：搜索范围取自本进程环境，随后**原样委托**给纯查找函数。
/// 行为与改前一致：`find_command_path(c) == find_command_path_in(c, 本进程 PATH)`。
fn find_command_path(command: &str) -> Option<String> {
    find_command_path_in(command, process_command_search_paths().as_deref())
}

/// 纯查找（P-08/I6 注入接缝）：只在 `search_paths` 给出的目录集合里查找 `command`，
/// **不读也不改**本进程 PATH；`search_paths == None` 表示"本进程环境里没有 PATH"。
///
/// 命令解析政策与改前**逐字一致**（本次不借机调整）：
/// - Windows：`command` 自带路径分隔符 → 直接判该路径是否为文件；否则按 PATHEXT 扩展名
///   逐目录检索（`command` 已带扩展名则不再追加）。
/// - 其它平台：仍委托 `sh -lc "command -v <command>"` 判定（保留 shell 自己的解析政策，
///   含 alias / function / 内建），只是把这份显式路径集合作为**子进程** PATH 传给它
///   （`None` 时不覆写子进程环境，维持旧语义）。
fn find_command_path_in(command: &str, search_paths: Option<&[PathBuf]>) -> Option<String> {
    #[cfg(windows)]
    {
        let command_path = Path::new(command);
        if command_path.components().count() > 1 {
            return command_path
                .is_file()
                .then(|| command_path.display().to_string());
        }

        let has_extension = command_path.extension().is_some();
        let extensions = if has_extension {
            vec![String::new()]
        } else {
            std::env::var("PATHEXT")
                .unwrap_or_else(|_| String::from(".COM;.EXE;.BAT;.CMD"))
                .split(';')
                .filter(|extension| !extension.trim().is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        };

        for dir in search_paths.unwrap_or(&[]) {
            for extension in &extensions {
                let candidate = dir.join(format!("{command}{extension}"));
                if candidate.is_file() {
                    return Some(candidate.display().to_string());
                }
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        let mut probe = std::process::Command::new("sh");
        probe
            .arg("-lc")
            .arg(format!("command -v {command} 2>/dev/null"))
            .stdin(std::process::Stdio::null());
        if let Some(paths) = search_paths {
            if let Ok(joined) = std::env::join_paths(paths) {
                probe.env("PATH", joined);
            }
        }
        let output = probe.output().ok()?;
        if !output.status.success() {
            return None;
        }
        // shell 自己报告的解析结果：是文件就用它（避免执行侧再隐式读一遍父进程 PATH，
        // 也让"显式路径集合"对查找与执行两侧同时生效）；否则（alias / function / 内建）
        // 维持改前行为，把命令名原样交给执行侧。
        let resolved = decode_console_output(&output.stdout).trim().to_string();
        if resolved.is_empty() || !Path::new(&resolved).is_file() {
            return Some(command.to_string());
        }
        Some(resolved)
    }
}

#[allow(clippy::too_many_lines)]
fn execute_shell_command(
    shell: &str,
    command: &str,
    cwd: Option<&str>,
    timeout: Option<u64>,
    run_in_background: Option<bool>,
) -> std::io::Result<runtime::BashCommandOutput> {
    if run_in_background.unwrap_or(false) {
        let mut process = std::process::Command::new(shell);
        process
            .arg("-NoProfile")
            .arg("-NonInteractive")
            .arg("-Command")
            .arg(command)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(cwd) = cwd {
            process.current_dir(cwd);
        }
        let child = process.spawn()?;
        return Ok(runtime::BashCommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            raw_output_path: None,
            interrupted: false,
            is_image: None,
            background_task_id: Some(child.id().to_string()),
            backgrounded_by_user: Some(true),
            assistant_auto_backgrounded: Some(false),
            dangerously_disable_sandbox: None,
            return_code_interpretation: None,
            no_output_expected: Some(true),
            structured_content: None,
            persisted_output_path: None,
            persisted_output_size: None,
            sandbox_status: None,
        });
    }

    let mut process = std::process::Command::new(shell);
    process.arg("-NoProfile").arg("-NonInteractive").arg("-Command").arg(command);
    if let Some(cwd) = cwd { process.current_dir(cwd); }
    let managed = runtime::managed_process::output(&mut process, timeout.map(Duration::from_millis))?;
    let output = managed.output;
    let mut stderr = decode_console_output(&output.stderr);
    if let Some(reason) = managed.interruption {
        if !stderr.is_empty() { stderr.push('\n'); }
        stderr.push_str(&match reason {
            runtime::managed_process::Interruption::TimedOut => format!("Command exceeded timeout of {} ms", timeout.unwrap_or(0)),
            runtime::managed_process::Interruption::Cancelled => "Command cancelled; managed process exited".to_string(),
        });
    }
    let return_code_interpretation = managed.interruption.map(|reason| match reason {
        runtime::managed_process::Interruption::TimedOut => "timeout".to_string(),
        runtime::managed_process::Interruption::Cancelled => "cancelled".to_string(),
    }).or_else(|| output.status.code().filter(|code| *code != 0).map(|code| format!("exit_code:{code}")));
    Ok(runtime::BashCommandOutput {
        stdout: decode_console_output(&output.stdout),
        stderr,
        raw_output_path: None,
        interrupted: managed.interruption.is_some(),
        is_image: None,
        background_task_id: None,
        backgrounded_by_user: None,
        assistant_auto_backgrounded: None,
        dangerously_disable_sandbox: None,
        return_code_interpretation,
        no_output_expected: Some(output.stdout.is_empty() && output.stderr.is_empty()),
        structured_content: None,
        persisted_output_path: None,
        persisted_output_size: None,
        sandbox_status: None,
    })
}

fn resolve_cell_index(
    cells: &[serde_json::Value],
    cell_id: Option<&str>,
    edit_mode: NotebookEditMode,
) -> Result<usize, String> {
    if cells.is_empty()
        && matches!(
            edit_mode,
            NotebookEditMode::Replace | NotebookEditMode::Delete
        )
    {
        return Err(String::from("Notebook has no cells to edit"));
    }
    if let Some(cell_id) = cell_id {
        cells
            .iter()
            .position(|cell| cell.get("id").and_then(serde_json::Value::as_str) == Some(cell_id))
            .ok_or_else(|| format!("Cell id not found: {cell_id}"))
    } else {
        Ok(cells.len().saturating_sub(1))
    }
}

fn source_lines(source: &str) -> Vec<serde_json::Value> {
    if source.is_empty() {
        return vec![serde_json::Value::String(String::new())];
    }
    source
        .split_inclusive('\n')
        .map(|line| serde_json::Value::String(line.to_string()))
        .collect()
}

fn format_notebook_edit_mode(mode: NotebookEditMode) -> String {
    match mode {
        NotebookEditMode::Replace => String::from("replace"),
        NotebookEditMode::Insert => String::from("insert"),
        NotebookEditMode::Delete => String::from("delete"),
    }
}

fn make_cell_id(index: usize) -> String {
    format!("cell-{}", index + 1)
}

fn parse_skill_description(contents: &str) -> Option<String> {
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("description:") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;
    use std::ffi::OsString;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread;
    use std::time::Duration;

    use super::{
        agent_permission_policy, allowed_tools_for_subagent, base64url_decode, decode_bing_redirect,
        detect_powershell_shell_in, execute_agent_with_spawn, execute_powershell_with_search_paths,
        execute_tool, final_assistant_text, find_command_path, find_command_path_in,
        mvp_tool_specs, persist_agent_terminal_state, push_output_block, AgentInput, AgentJob,
        PowerShellInput, SubagentToolExecutor, WebSearchConfig, DEFAULT_SEARCH_TOTAL_BUDGET_MS,
        DEFAULT_WEB_SEARCH_BASE_URL, MAX_SEARCH_ATTEMPT_TIMEOUT_MS, MAX_SEARCH_BUDGET_MS,
        MAX_SEARCH_CONNECT_TIMEOUT_MS,
    };
    use api::OutputContentBlock;
    use runtime::{ApiRequest, AssistantEvent, ConversationRuntime, RuntimeError, Session};
    use serde_json::json;

    fn process_state_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    /// `env_lock` 与 [`process_state_lock`] 是**同一把**进程状态锁的两个名字（RPR-01b 裁决第 1 条：
    /// 不得每个模块各建一把锁）。改 cwd / 改进程环境的用例都必须先持有它。
    fn env_lock() -> &'static Mutex<()> {
        process_state_lock()
    }

    /// 打印一行**不被 libtest 捕获**的输出（本 crate 自己的一份，各 crate 不共享 util）。
    ///
    /// 环境例外必须对"通过的运行"也可见：libtest 会捕获通过用例的 `println!`，
    /// 只写进被捕获的缓冲就等于**静默通过**。直接写 `std::io::stdout()` 不经过捕获通道。
    fn announce_env(line: &str) {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        let _ = writeln!(stdout, "{line}");
        let _ = stdout.flush();
    }

    /// **显式环境例外**：需要真实 PowerShell 的用例必须先调用本函数。
    ///
    /// ## 本 crate 测试已声明的环境要求（除本项外都为"环境无关"）
    ///
    /// 1. `std::env::temp_dir()`（Windows 上是 `TEMP`/`TMP`）必须可写：各用例的临时工作区、
    ///    桩目录与夹具都建在它下面；
    /// 2. 一般用例**不需要**真实 PowerShell —— `powershell_runs_via_stub_shell` 与
    ///    `powershell_errors_when_shell_is_missing` 走**显式注入的搜索路径集合**（P-08/I6 接缝），
    ///    只有 `powershell_timeout_treated_as_seconds` 需要真实 shell（它验证的是超时口径本身）；
    /// 3. 需要 `git` 的用例：`discover_with_git_*` 不在本 crate（见 core-runtime）。
    ///
    /// ## 为什么是"声明式跳过"（RPR-01b 裁决 §5.2）
    ///
    /// 改前这里是裸 `if detect_powershell_shell().is_err() { return; }`：libtest 把
    /// "什么都没做就返回"报成 **ok**，门禁看不到"本机未验证"这个事实。PowerShell 属
    /// **平台/外部可执行依赖**（非 Windows 默认没有），裁决 §5.2 表允许
    /// "可选外部依赖缺失且**前置有真实探测** ⇒ 明确跳过并**独立统计**"，因此这里保留跳过，
    /// 但补齐三件事：① 前置是真实探测；② 打印绕过捕获的 `[env-skip]` 行，写明原因、
    /// 被探测的对象与"本机不验证任何行为、**不是通过**"；③ 独立统计口径 = 数 `[env-skip]` 行。
    /// PowerShell 属**执行环境依赖**，不是可选依赖：产品的原生输入 helper 本身就经
    /// `powershell.exe -Command` 内联执行，缺它意味着**本机无法执行 CU**。
    ///
    /// 因此本机缺 PowerShell 时**必须失败**（第八轮裁决 §5：PowerShell → fail），
    /// 不得记成"跳过"——那会让发布门禁在一台根本跑不了 CU 的机器上变绿。
    fn require_powershell_or_fail(test_name: &str) {
        if let Err(error) = super::detect_powershell_shell() {
            panic!(
                "[env-missing] {test_name}: 找不到可用的 PowerShell 可执行文件（{error}）。本用例需要真实 shell 才能验证超时口径；缺前置时它不验证任何行为，因此不是通过，而是验收未完成（不是产品缺陷，是执行环境无效）。Windows 上请确认 PATH 里的 pwsh/powershell 可用。"
            );
        }
    }

    // ==== RPR-01b：进程环境变量的「统一锁 + RAII guard」 ====

    /// 恢复环境变量失败时的全局留痕（裁决第 5 条：不得被 poison-tolerant 取锁静默吞掉）。
    static ENV_RESTORE_FAILED: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    fn env_restore_failures() -> &'static Mutex<Vec<String>> {
        static FAILURES: OnceLock<Mutex<Vec<String>>> = OnceLock::new();
        FAILURES.get_or_init(|| Mutex::new(Vec::new()))
    }

    fn record_env_restore_failure(details: &[String]) {
        ENV_RESTORE_FAILED.store(true, std::sync::atomic::Ordering::SeqCst);
        env_restore_failures()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .extend(details.iter().cloned());
        // 正确措辞（裁决第 6 条）：污染的是**当前测试进程的后续执行**，
        // 直到恢复（本进程内无法恢复时即进程结束）。不是"用户系统环境变量被永久修改"。
        eprintln!(
            "RPR-01b：恢复进程环境变量失败（本进程后续执行可能仍读到被写入的值，直到恢复或进程结束）：{}",
            details.join("; ")
        );
    }

    /// 前序有恢复失败时**响亮地**失败：拒绝在可能被污染的环境上继续跑依赖进程级环境解析的用例。
    fn assert_no_env_restore_failure() {
        if !ENV_RESTORE_FAILED.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let details = env_restore_failures()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .join("; ");
        panic!(
            "RPR-01b：前序用例恢复环境变量失败（{details}），本进程环境可能仍被污染；\
             拒绝在脏环境下继续（poison-tolerant 取锁不得把该信号吞掉）。"
        );
    }

    /// 测试专用：清除「恢复失败」标志。调用方必须**已持有**统一锁令牌 ——
    /// 此时其它线程都阻塞在锁上（标志检查在真锁内进行），故清标志不存在竞争窗口。
    fn clear_env_restore_failure_for_test() {
        ENV_RESTORE_FAILED.store(false, std::sync::atomic::Ordering::SeqCst);
        env_restore_failures()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    fn env_restore_failure_recorded() -> bool {
        ENV_RESTORE_FAILED.load(std::sync::atomic::Ordering::SeqCst)
    }

    thread_local! {
        /// 当前线程是否已持有统一锁（用于识别嵌套 guard，避免二次加锁自锁）。
        static ENV_LOCK_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    /// 统一锁的持有令牌（裁决第 4 条：嵌套 guard 复用已持有的锁令牌）。
    ///
    /// `owned == None` 表示本 guard 复用了**本线程**外层已持有的锁，不负责释放也不再加锁 ——
    /// `std::sync::Mutex` 不可重入，嵌套时第二次加锁会直接自锁。
    ///
    /// 注意：嵌套识别靠的是令牌自己的线程局部标记，**裸 `env_lock().lock()` 不会被识别**。
    /// 因此同一线程里不要"令牌 + 裸取锁"混用（会自锁）；只需要互斥、不改进程环境时用裸取锁，
    /// 需要改环境时用本令牌 / [`ScopedEnv`]，二者不要叠在同一线程（本次迁移时就踩过一次：
    /// `powershell_runs_via_stub_shell` 残留了一行裸 `env_lock()`，直接自锁挂住整个用例）。
    struct EnvLockToken {
        owned: Option<std::sync::MutexGuard<'static, ()>>,
    }

    impl EnvLockToken {
        fn acquire() -> Self {
            if ENV_LOCK_HELD.with(std::cell::Cell::get) {
                // 嵌套：复用外层令牌。仍需检查恢复失败标志 —— 此时本线程持有真锁，
                // 其它线程都阻塞在锁上，故不存在"并发观察到标志"的竞争窗口。
                assert_no_env_restore_failure();
                return Self { owned: None };
            }
            // 先拿锁，再检查标志：panic 时线程局部状态（HELD）尚未置位，保持干净。
            let guard = env_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert_no_env_restore_failure();
            ENV_LOCK_HELD.with(|held| held.set(true));
            Self { owned: Some(guard) }
        }

        /// 是否复用了本线程外层已持有的锁令牌（嵌套场景为 `true`）。
        fn is_reentrant(&self) -> bool {
            self.owned.is_none()
        }
    }

    impl Drop for EnvLockToken {
        fn drop(&mut self) {
            if self.owned.is_some() {
                ENV_LOCK_HELD.with(|held| held.set(false));
            }
            // `owned` 的 MutexGuard 随后自动析构释放锁：poison 语义与旧用例保持一致
            // （持锁用例 panic → 锁被毒化 → 后续用例仍可用 poison-tolerant 方式取锁，
            //  但真正的"环境恢复失败"改由上面的标志响亮暴露，不再被吞掉）。
        }
    }

    /// 设置/移除一个环境变量，把 std 在非法名字（空 / 含 '=' / 含 NUL）、值含 NUL 时的 panic
    /// 收敛成 `Err`：恢复路径运行在 `Drop` 里，绝不能让 std 的 panic 直接逃逸出去
    /// （若此刻正 unwind，二次 panic 会直接 abort 整个测试进程）。
    fn apply_env(name: &str, target: Option<&OsString>) -> Result<(), String> {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match target {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }));
        result.map_err(|_| format!("{name}: set/remove 环境变量时 panic（名字非法或值含 NUL）"))
    }

    fn env_set(
        name: &'static str,
        value: impl AsRef<std::ffi::OsStr>,
    ) -> (&'static str, Option<OsString>) {
        (name, Some(value.as_ref().to_os_string()))
    }

    /// 目标状态 = 「不存在」（注意与「存在但为空串」不同，后者用 `env_set(name, "")`）。
    fn env_unset(name: &'static str) -> (&'static str, Option<OsString>) {
        (name, None)
    }

    /// RAII 版**进程级**环境变量作用域。
    ///
    /// 用法：
    /// ```ignore
    /// let _env = ScopedEnv::new(vec![env_set("HOME", &home), env_unset("CLAW_CONFIG_HOME")]);
    /// ```
    ///
    /// 保证：
    /// 1. **持锁区间**：从「读原值之前」到「全部恢复之后」一直持有统一锁令牌（Drop 体先恢复、字段后释放锁）；
    /// 2. **三态无损**（裁决第 2 条）：原值用 `Option<OsString>` 保存，区分「原来不存在」(`None`)、
    ///    「原来为空串」(`Some(OsString::new())`)、「原来有值」（不用空串冒充"不存在"）；
    /// 3. **部分失败回滚**（裁决第 3 条）：多变量构造时先把"原值"登记进 `self`，再改值；
    ///    中途失败（panic）时 `self` 已是构造函数的局部变量，析构会恢复**已改动**的部分；
    /// 4. **嵌套复用令牌**（裁决第 4 条）：见 [`EnvLockToken`]；
    /// 5. **恢复失败不静默**（裁决第 5 条）：见 [`record_env_restore_failure`]。
    ///
    /// 边界（裁决第 7 条）：RAII 只覆盖**栈展开（unwind）**路径。`std::process::abort`、
    /// `libc::_exit` / `pthread_exit` 等不展开栈的终止方式**不会**运行析构函数，
    /// 因此本 guard 不承诺"任何情况下都必定恢复"；这类终止方式下进程本身已结束或环境已被放弃。
    struct ScopedEnv {
        /// 先声明锁令牌：`ScopedEnv::drop` 体（恢复）跑完之后，字段才按声明顺序析构（释放锁）。
        lock: EnvLockToken,
        originals: Vec<(&'static str, Option<OsString>)>,
    }

    impl ScopedEnv {
        fn new(targets: Vec<(&'static str, Option<OsString>)>) -> Self {
            // 先登记原值再改值：登记与改值之间没有可 panic 的语句，
            // 因此"已登记但未改值"的条目恢复起来只是把原值写回一遍，幂等无害。
            let mut scope = Self {
                lock: EnvLockToken::acquire(),
                originals: Vec::with_capacity(targets.len()),
            };
            for (name, target) in targets {
                let original = std::env::var_os(name);
                scope.originals.push((name, original));
                if let Err(error) = apply_env(name, target.as_ref()) {
                    // apply 失败时该变量状态未变（std 在真正改动前就 panic 了），
                    // 撤销刚登记的恢复项，避免对非法名字做一次注定失败的"恢复"。
                    scope.originals.pop();
                    panic!("ScopedEnv 设置环境变量失败：{error}");
                }
            }
            scope
        }

        /// 测试专用：直接注入「原值」而不做任何 set —— 用于构造"恢复必然失败"的路径
        /// （std 的 `set_var`/`remove_var` 对合法名字几乎不会失败，只能注入非法名字来复现该分支）。
        fn with_injected_originals(originals: Vec<(&'static str, Option<OsString>)>) -> Self {
            Self {
                lock: EnvLockToken::acquire(),
                originals,
            }
        }

        /// 本 guard 是否嵌套复用了外层锁令牌。
        fn is_reentrant(&self) -> bool {
            self.lock.is_reentrant()
        }

        /// 已保存的原值（`None` = 该变量没有登记；`Some(None)` = 原来不存在）。
        fn original_of(&self, name: &str) -> Option<&Option<OsString>> {
            self.originals
                .iter()
                .find(|(candidate, _)| *candidate == name)
                .map(|(_, original)| original)
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            let mut failures = Vec::new();
            // 逆序恢复：后设的先回退，覆盖同一变量被登记两次的情形。
            for (name, original) in self.originals.iter().rev() {
                if let Err(error) = apply_env(name, original.as_ref()) {
                    failures.push(error);
                }
            }
            if failures.is_empty() {
                return;
            }
            record_env_restore_failure(&failures);
            // 非 unwind 路径（正常返回时的析构）可以安全 panic：立刻让用例失败，绝不静默；
            // unwind 路径不能二次 panic（会 abort 整个测试进程），改为上面"留痕 + 标志 + 阻塞后续"。
            if !std::thread::panicking() {
                panic!(
                    "ScopedEnv 恢复环境变量失败：{}（进程环境与本进程后续执行可能仍被污染）",
                    failures.join("; ")
                );
            }
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("claw-tools-{unique}-{name}"))
    }

    fn normalize_path(value: &str) -> String {
        value.replace('\\', "/")
    }

    fn web_search_config(base_url: String, transport: Option<&str>) -> String {
        let mut config = format!("[web_search]\nbase_url = \"{base_url}\"\n");
        if let Some(transport) = transport {
            config.push_str(&format!("transport = \"{transport}\"\n"));
        }
        config
    }

    /// RAII 版的进程 cwd 切换：进入 `path`，离开作用域时恢复原 cwd 并清理临时目录。
    ///
    /// 历史写法是每个用例手写 `let original = current_dir(); set_current_dir(root); ...;
    /// set_current_dir(original)`。一旦用例中途 panic（断言/expect 失败），末尾的恢复语句根本执行不到：
    /// 进程 cwd 永久停在一个临时目录里。又因为 env_lock 是 poison-tolerant 的，后续用例不会报"锁被毒化"，
    /// 而是**静默**在错误 cwd 下运行（凡从 cwd 推导路径的代码都会拿到错误结果）。
    /// 交给 Drop 恢复，unwind 路径同样收口。
    struct ScopedCurrentDir {
        original: PathBuf,
        /// 作用域结束时删除的目录。通常是 `path` 本身；cwd 只是临时目录子目录时传外层根，
        /// 避免"用例末尾的 remove_dir_all 被 panic 跳过"导致临时目录残留。
        cleanup: PathBuf,
        path: PathBuf,
    }

    impl ScopedCurrentDir {
        fn enter(path: PathBuf) -> Self {
            Self::enter_in(path.clone(), path)
        }

        /// 进入 `path`，但作用域结束时只清理 `cleanup`（`cleanup` 应包含 `path`）。
        fn enter_in(path: PathBuf, cleanup: PathBuf) -> Self {
            let original = std::env::current_dir().expect("current dir");
            if let Err(error) = std::env::set_current_dir(&path) {
                panic!("set current dir to {}: {error}", path.display());
            }
            Self {
                original,
                cleanup,
                path,
            }
        }
    }

    impl Drop for ScopedCurrentDir {
        fn drop(&mut self) {
            // 这里绝不能 panic：用例正在 unwind 时二次 panic 会直接 abort 整个测试进程，
            // 那时连"把 cwd 还回去"都做不到（其它用例一起陪葬）。恢复失败只能尽力而为 + 留痕。
            if let Err(error) = std::env::set_current_dir(&self.original) {
                eprintln!(
                    "ScopedCurrentDir 恢复进程 cwd 失败：{} -> {}：{error}",
                    self.path.display(),
                    self.original.display()
                );
            }
            let _ = fs::remove_dir_all(&self.cleanup);
        }
    }

    /// 回归（RPR-01）：作用域内 panic（unwind）时也必须恢复 cwd 并清理临时目录。
    ///
    /// 对应缺陷：用例手写 `set_current_dir(root) ... set_current_dir(original)`，
    /// panic 会跳过恢复语句，把进程 cwd 留在临时目录里（已用临时用例复现过）。
    #[test]
    fn scoped_current_dir_restores_cwd_when_scope_panics() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = std::env::current_dir().expect("cwd before");
        let root = temp_path("rpr01-panic-restore");
        fs::create_dir_all(&root).expect("create root");

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _cwd = ScopedCurrentDir::enter(root.clone());
            let inside = std::env::current_dir().expect("cwd inside");
            assert_eq!(
                inside.canonicalize().expect("canonical inside"),
                root.canonicalize().expect("canonical root"),
                "作用域内 cwd 应指向临时目录"
            );
            panic!("模拟用例中途断言失败");
        }));
        assert!(panicked.is_err(), "作用域内的 panic 必须继续向外传播");

        assert_eq!(
            std::env::current_dir().expect("cwd after panic"),
            before,
            "panic（unwind）后进程 cwd 必须回到进入前的目录"
        );
        assert!(!root.exists(), "panic 后临时目录也必须被清理");
    }

    /// 回归（RPR-01）：Drop 不得 panic —— 原目录已被删（或压根不可达）时只能尽力而为。
    ///
    /// 对应缺陷：`Drop` 里 `set_current_dir(..).expect("restore current dir")` 在 unwind 过程中
    /// 二次 panic 会 abort 整个测试进程。改前本用例必失败（panic: restore current dir / NotFound）。
    #[test]
    fn scoped_current_dir_drop_never_panics_when_original_is_gone() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = std::env::current_dir().expect("cwd before");
        let cleanup = temp_path("rpr01-drop-survives");
        fs::create_dir_all(&cleanup).expect("create cleanup");

        // 直接构造"原目录不存在"的作用域，模拟恢复失败的路径。
        {
            let _cwd = ScopedCurrentDir {
                original: temp_path("rpr01-missing-original"),
                cleanup: cleanup.clone(),
                path: cleanup.clone(),
            };
        }

        assert_eq!(
            std::env::current_dir().expect("cwd after drop"),
            before,
            "恢复失败时不得改动 cwd"
        );
        assert!(!cleanup.exists(), "清理仍然要执行");
    }

    /// 回归（RPR-01）：持锁用例 panic（锁被毒化）后，其它用例仍必须能取到锁，
    /// 且不会看到残留的临时 cwd —— 即"poison 不级联 + cwd 无脏状态"。
    ///
    /// 注意：本用例会**故意毒化** process_state_lock，断言 poison-tolerant 取锁有效后，
    /// 用 `clear_poison` 复原，不给其它用例留下额外全局状态。
    #[test]
    fn poisoned_process_state_lock_does_not_cascade_or_leak_cwd() {
        // RPR-01b 修正：原实现在**未持锁**时快照 `before`、也在未持锁时做最终断言，
        // 会与并发的"改 cwd 用例"互相看见对方的临时目录，从而随机失败
        // （实测第一遍 `cargo test -p coolzhu-tool-registry` 就在这里挂过一次）。
        // 现在两次读 cwd 都在持有统一锁令牌时进行：
        //   - 基线快照前必须持锁，否则 `before` 可能是别的用例的临时目录；
        //   - 最终断言同样持锁，否则断言之间可能被并发用例改掉 cwd。
        let before = {
            let token = EnvLockToken::acquire();
            let before = std::env::current_dir().expect("cwd before");
            drop(token);
            before
        };
        let root = temp_path("rpr01-poison");
        fs::create_dir_all(&root).expect("create root");

        // 故意在**持锁**状态下 panic（这里用裸 env_lock 而非令牌：本线程此刻不持锁，
        // 令牌会自动判为嵌套/复用而不真正加锁，反而构造不出"持锁用例失败"）。
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = env_lock()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let _cwd = ScopedCurrentDir::enter(root.clone());
            panic!("模拟持锁用例失败");
        }));
        assert!(panicked.is_err(), "panic 必须向外传播");
        assert!(
            env_lock().is_poisoned(),
            "持锁用例 panic 后锁必须真的被毒化，否则本用例对 poison 行为没有判别力"
        );

        // 锁被毒化后仍可获取（否则后续用例会连锁失败）；下面的断言都在持锁状态下进行。
        {
            let _token = EnvLockToken::acquire();
            assert_eq!(
                std::env::current_dir().expect("cwd after poisoned scope"),
                before,
                "持锁用例 panic 后不得残留临时 cwd"
            );
            assert!(!root.exists(), "临时目录必须已清理");
        }
        env_lock().clear_poison();
    }

    /// 回归（RPR-01b）：正常返回（作用域自然结束）时，三态原值全部无损恢复。
    ///
    /// 判别性：改前那种 `set_var(...)` + 末尾手写 restore 的写法区分不出
    /// 「原来不存在」和「原来为空串」（旧 Config 用例用 `var().ok()` 读原值，空串被当成不存在），
    /// 本用例直接断言保存下来的原值三态各不相同。
    #[test]
    fn scoped_env_restores_all_three_original_states_on_normal_drop() {
        // 外层 guard：把"人肉构造"的三种原状态在用例结束时清干净（顺便验证自身恢复）。
        let outer = ScopedEnv::new(vec![
            env_unset("CLAW_RPR01B_MISSING"),
            env_set("CLAW_RPR01B_EMPTY", ""),
            env_set("CLAW_RPR01B_VALUE", "original"),
        ]);
        assert!(!outer.is_reentrant(), "最外层 guard 必须真正持有统一锁");
        assert_eq!(std::env::var_os("CLAW_RPR01B_MISSING"), None);
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_EMPTY"),
            Some(OsString::from(""))
        );
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_VALUE"),
            Some(OsString::from("original"))
        );

        {
            // 内层 guard：三种原状态分别是「不存在」「空串」「有值」，改写后 Drop 必须逐一还原。
            let inner = ScopedEnv::new(vec![
                env_set("CLAW_RPR01B_MISSING", "now-set"),
                env_unset("CLAW_RPR01B_EMPTY"),
                env_set("CLAW_RPR01B_VALUE", "changed"),
            ]);
            assert!(inner.is_reentrant(), "嵌套 guard 必须复用外层令牌（不再加锁）");
            assert_eq!(
                inner.original_of("CLAW_RPR01B_MISSING"),
                Some(&None),
                "原来不存在必须记为 None"
            );
            assert_eq!(
                inner.original_of("CLAW_RPR01B_EMPTY"),
                Some(&Some(OsString::from(""))),
                "原来为空串必须记为 Some(\"\")，不得退化成 None"
            );
            assert_eq!(
                inner.original_of("CLAW_RPR01B_VALUE"),
                Some(&Some(OsString::from("original")))
            );
            assert_eq!(
                std::env::var_os("CLAW_RPR01B_MISSING"),
                Some(OsString::from("now-set"))
            );
            assert_eq!(std::env::var_os("CLAW_RPR01B_EMPTY"), None);
            assert_eq!(
                std::env::var_os("CLAW_RPR01B_VALUE"),
                Some(OsString::from("changed"))
            );
        }

        assert_eq!(std::env::var_os("CLAW_RPR01B_MISSING"), None, "回到「不存在」");
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_EMPTY"),
            Some(OsString::from("")),
            "回到「存在但为空串」"
        );
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_VALUE"),
            Some(OsString::from("original")),
            "回到「原来有值」"
        );
    }

    /// 回归（RPR-01b）：作用域内**提前返回**（`return` / `?` 传播）同样必须恢复。
    /// 判别性：改前的手写 restore 写在函数末尾，提前返回根本走不到。
    #[test]
    fn scoped_env_restores_on_early_return() {
        fn probe() -> Result<(), String> {
            let _env = ScopedEnv::new(vec![env_set("CLAW_RPR01B_EARLY", "in-scope")]);
            assert_eq!(
                std::env::var_os("CLAW_RPR01B_EARLY"),
                Some(OsString::from("in-scope"))
            );
            return Err(String::from("提前返回"));
        }

        let _holder = ScopedEnv::new(vec![env_unset("CLAW_RPR01B_EARLY")]);
        assert_eq!(std::env::var_os("CLAW_RPR01B_EARLY"), None);

        assert_eq!(probe(), Err(String::from("提前返回")));

        assert_eq!(
            std::env::var_os("CLAW_RPR01B_EARLY"),
            None,
            "提前返回后必须恢复到作用域前的状态"
        );
    }

    /// 回归（RPR-01b）：受控 panic / 栈展开后必须恢复，且**后续执行**（含生产代码读取路径）
    /// 看到的环境与进入作用域前完全一致。
    /// 判别性：改前 panic 会跳过末尾手写 restore，进程环境里会残留作用域内的值。
    #[test]
    fn scoped_env_restores_on_panic_and_keeps_later_readers_consistent() {
        let _holder = ScopedEnv::new(vec![env_unset("CLAW_TODO_STORE")]);
        let before = std::env::var_os("CLAW_TODO_STORE");
        assert_eq!(before, None);

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _env = ScopedEnv::new(vec![env_set("CLAW_TODO_STORE", "rpr01b-dirty-todos.json")]);
            assert_eq!(
                std::env::var_os("CLAW_TODO_STORE"),
                Some(OsString::from("rpr01b-dirty-todos.json"))
            );
            panic!("模拟作用域内断言失败");
        }));
        assert!(panicked.is_err(), "作用域内的 panic 必须继续向外传播");

        assert_eq!(
            std::env::var_os("CLAW_TODO_STORE"),
            before,
            "panic（unwind）后必须恢复到进入作用域前的三态"
        );
        // 生产读取路径（todo_store_path 会优先读 CLAW_TODO_STORE）不得再看到作用域内的临时值。
        assert_ne!(
            super::todo_store_path().expect("todo store path"),
            PathBuf::from("rpr01b-dirty-todos.json"),
            "panic 后生产读取路径不得仍读到作用域内写入的值"
        );
        assert!(
            !env_restore_failure_recorded(),
            "正常 unwind 恢复不应留下任何'恢复失败'记录"
        );
    }

    /// 判别性对照（RPR-01b）：**旧写法**（裸 `set_var` + 末尾手写 restore）在 panic 后确实残留，
    /// 而 [`ScopedEnv`] 同样的 panic 场景下不残留 —— 本用例把这个差异做成可执行的断言，
    /// 而不是只在注释里声称"改前会残留"。
    ///
    /// 说明：对照实验需要自己收尾（旧写法没有守卫），所以结尾手动 remove_var。
    #[test]
    fn naive_manual_restore_leaks_on_panic_which_scoped_env_prevents() {
        let _env = ScopedEnv::new(vec![
            env_unset("CLAW_RPR01B_NAIVE"),
            env_unset("CLAW_RPR01B_GUARDED"),
        ]);

        // (1) 旧写法：set 之后 panic → 末尾的 restore 永远执行不到 → 测试值留在进程环境里。
        let naive = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            std::env::set_var("CLAW_RPR01B_NAIVE", "leaked");
            panic!("模拟断言失败：后面的手写 restore 被跳过");
        }));
        assert!(naive.is_err());
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_NAIVE"),
            Some(OsString::from("leaked")),
            "旧写法在 panic 后确实把测试值留在了本进程环境里（这正是要消除的缺陷）"
        );
        std::env::remove_var("CLAW_RPR01B_NAIVE"); // 旧写法没有守卫，只能手动收尾

        // (2) 同样的 panic 场景，guard 收口的变量不留残留。
        let guarded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _inner = ScopedEnv::new(vec![env_set("CLAW_RPR01B_GUARDED", "leaked")]);
            panic!("模拟断言失败");
        }));
        assert!(guarded.is_err());
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_GUARDED"),
            None,
            "guard 在同样的 panic 下必须已经恢复（与上面的旧写法形成对照）"
        );
    }

    /// 回归（RPR-01b 裁决第 3 条）：多变量构造**中途失败**时，已经修改的部分也必须恢复。
    /// 判别性：改前是裸 `set_var(A); set_var(BAD);`，panic 后 A 会永久留在本进程环境里
    /// （直到进程结束），后续用例就会读到 A 的测试值。
    #[test]
    fn scoped_env_rolls_back_applied_vars_when_later_construction_fails() {
        let _holder = ScopedEnv::new(vec![
            env_unset("CLAW_RPR01B_PARTIAL_A"),
            env_unset("CLAW_RPR01B_PARTIAL_B"),
        ]);
        let before_a = std::env::var_os("CLAW_RPR01B_PARTIAL_A");
        let before_b = std::env::var_os("CLAW_RPR01B_PARTIAL_B");
        assert_eq!(before_a, None);
        assert_eq!(before_b, None);

        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _env = ScopedEnv::new(vec![
                env_set("CLAW_RPR01B_PARTIAL_A", "applied-before-failure"),
                env_set("CLAW_RPR01B_PARTIAL_B", "also-applied"),
                // 非法名字（含 '='）：std 会在真正改动前 panic，构造在此中断。
                env_set("CLAW_RPR01B=BAD", "invalid"),
            ]);
        }));
        assert!(panicked.is_err(), "非法名字必须让构造 panic（不得静默跳过）");

        assert_eq!(
            std::env::var_os("CLAW_RPR01B_PARTIAL_A"),
            before_a,
            "构造失败前已应用的第一个变量必须被回滚"
        );
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_PARTIAL_B"),
            before_b,
            "构造失败前已应用的第二个变量必须被回滚"
        );
    }

    /// 回归（RPR-01b 裁决第 4 条）：嵌套 guard 复用已持有的锁令牌 —— 内层不再加锁，
    /// 因此不会自锁（若回归成"每个 guard 各加一次锁"，本用例会**死锁**而不是失败）。
    /// 同时验证恢复顺序：内层先回到内层写入前的值，外层最后回到最初的原值。
    #[test]
    fn scoped_env_nested_guards_reuse_lock_token_and_restore_inner_first() {
        let outer = ScopedEnv::new(vec![env_set("CLAW_RPR01B_NEST", "outer")]);
        assert!(!outer.is_reentrant(), "最外层 guard 必须持有真锁");
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_NEST"),
            Some(OsString::from("outer"))
        );

        {
            let inner = ScopedEnv::new(vec![env_set("CLAW_RPR01B_NEST", "inner")]);
            assert!(inner.is_reentrant(), "嵌套 guard 必须复用外层锁令牌");
            assert_eq!(
                std::env::var_os("CLAW_RPR01B_NEST"),
                Some(OsString::from("inner"))
            );
        }
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_NEST"),
            Some(OsString::from("outer")),
            "内层先恢复到自己进入前的值"
        );

        drop(outer);
        assert_eq!(
            std::env::var_os("CLAW_RPR01B_NEST"),
            None,
            "外层最后恢复到最初的原值（不存在）"
        );
    }

    /// 回归（RPR-01b 裁决第 5 条）：恢复失败**不得**被 poison-tolerant 取锁静默吞掉。
    ///
    /// 做法：注入一个"原值无法恢复"的作用域（名字非法 → set/remove 都 panic），
    /// 断言 (1) 非 unwind 路径下 Drop 立刻响亮失败、(2) 全局留痕置位、
    /// (3) **下一次取锁（含嵌套复用路径）拒绝在脏环境上继续**。
    /// 这是"改前"做不到的：旧写法既不认识"恢复失败"，poison-tolerant 取锁又会把唯一信号吞掉。
    #[test]
    fn env_restore_failure_is_not_swallowed_by_poison_tolerant_lock() {
        // 全程持有令牌：标志的置位/清除都在锁内，其它线程此时阻塞在锁上，不存在竞争窗口。
        let token = EnvLockToken::acquire();
        clear_env_restore_failure_for_test();
        assert!(!env_restore_failure_recorded());

        let dropped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // 注入非法名字当"原值"：Drop 恢复时必然失败。
            let _broken =
                ScopedEnv::with_injected_originals(vec![("CLAW_RPR01B=BAD", Some(OsString::from("x")))]);
        }));
        assert!(
            dropped.is_err(),
            "非 unwind 路径下恢复失败必须立刻让用例失败（不得静默）"
        );
        assert!(
            env_restore_failure_recorded(),
            "恢复失败必须留下全局记录，供后续用例拒绝在脏环境上继续"
        );

        // 下一次取锁（这里走嵌套复用路径，最坏情况）必须响亮 panic，而不是静默返回。
        let reused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| EnvLockToken::acquire()));
        assert!(
            reused.is_err(),
            "存在未处理的恢复失败时，取锁必须响亮失败（poison-tolerant 也不得吞掉）"
        );

        // 复原全局状态（仍在持锁状态），不给其它用例留下"恢复失败"记录。
        clear_env_restore_failure_for_test();
        assert!(!env_restore_failure_recorded());
        drop(token);
    }

    /// 回归：`timeout` 必须按**秒**解释（改前被当毫秒，命令必超时）。
    ///
    /// ## 时间预算为什么是 10 秒（RPR-01b：30 轮重复运行暴露的真实 flake）
    ///
    /// 改前用 `timeout: Some(2)` 跑 `Start-Sleep -Milliseconds 500; Write-Output ok`：
    /// 这 2 秒要同时覆盖 **pwsh 启动 + 500ms 睡眠 + 输出**。本机重载（兄弟工单并行编译）时
    /// pwsh 启动就能吃掉 1.5 秒以上 —— 实测 30 轮里 **2 轮**以
    /// `stdout="" stderr="Command exceeded timeout of 2000 ms"` 失败，属**负载导致的假失败**
    /// （同一断言、同一原因，重跑即绿）。
    ///
    /// 这里只放宽**墙钟预算**，不放宽判据：本用例防的回归是"秒被当成毫秒"，
    /// 10 秒 vs 10 毫秒仍然判然有别（误读成 10ms 时连 pwsh 启动都过不去，必然超时 ⇒ 仍红），
    /// 因此**判别力不减**；命令真挂住时最坏多等 10 秒（有界），随后仍由断言判失败。
    #[test]
    fn powershell_timeout_treated_as_seconds() {
        use super::execute_powershell;
        // 显式环境例外：需要真实 PowerShell（缺前置 ⇒ 声明式跳过并独立统计，绝不静默通过）。
        require_powershell_or_fail("powershell_timeout_treated_as_seconds");
        // RPR-01：cwd: None 意味着子进程继承**进程 cwd**，等价于读一次进程全局状态；
        // 必须与改动 cwd 的用例互斥，否则可能继承到正被删除的临时目录（os error 267）。
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let out = execute_powershell(PowerShellInput {
            command: "Start-Sleep -Milliseconds 500; Write-Output ok".to_string(),
            cwd: None,
            timeout: Some(10), // 10 秒；改前被当 10ms（更早是 2ms），命令必超时
            description: None,
            run_in_background: None,
        })
        .expect("powershell run");
        assert!(
            out.stdout.contains("ok"),
            "stdout={:?} stderr={:?}",
            out.stdout,
            out.stderr
        );
        assert!(
            !out.stderr.contains("exceeded timeout"),
            "应在 10 秒内完成、不超时（超时说明 timeout 被按毫秒解释，或本机异常缓慢）: {:?}",
            out.stderr
        );
    }

    #[test]
    fn exposes_mvp_tools() {
        let names = mvp_tool_specs()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>();
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"WebFetch"));
        assert!(names.contains(&"WebSearch"));
        assert!(names.contains(&"TodoWrite"));
        assert!(names.contains(&"Skill"));
        assert!(names.contains(&"Agent"));
        assert!(names.contains(&"ToolSearch"));
        assert!(names.contains(&"NotebookEdit"));
        assert!(names.contains(&"Sleep"));
        assert!(names.contains(&"SendUserMessage"));
        assert!(names.contains(&"Config"));
        assert!(names.contains(&"StructuredOutput"));
        assert!(names.contains(&"REPL"));
        assert!(names.contains(&"PowerShell"));
    }

    #[test]
    fn rejects_unknown_tool_names() {
        let error = execute_tool("nope", &json!({})).expect_err("tool should be rejected");
        assert!(error.contains("unsupported tool"));
    }

    #[test]
    fn web_fetch_returns_prompt_aware_summary() {
        let server = TestServer::spawn(Arc::new(|request_line: &str| {
            assert!(request_line.starts_with("GET /page "));
            HttpResponse::html(
                200,
                "OK",
                "<html><head><title>Ignored</title></head><body><h1>Test Page</h1><p>Hello <b>world</b> from local server.</p></body></html>",
            )
        }));

        let result = execute_tool(
            "WebFetch",
            &json!({
                "url": format!("http://{}/page", server.addr()),
                "prompt": "Summarize this page"
            }),
        )
        .expect("WebFetch should succeed");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        assert_eq!(output["code"], 200);
        let summary = output["result"].as_str().expect("result string");
        assert!(summary.contains("Fetched"));
        assert!(summary.contains("Test Page"));
        assert!(summary.contains("Hello world from local server"));

        let titled = execute_tool(
            "WebFetch",
            &json!({
                "url": format!("http://{}/page", server.addr()),
                "prompt": "What is the page title?"
            }),
        )
        .expect("WebFetch title query should succeed");
        let titled_output: serde_json::Value = serde_json::from_str(&titled).expect("valid json");
        let titled_summary = titled_output["result"].as_str().expect("result string");
        assert!(titled_summary.contains("Title: Ignored"));
    }

    #[test]
    fn web_fetch_supports_plain_text_and_rejects_invalid_url() {
        let server = TestServer::spawn(Arc::new(|request_line: &str| {
            assert!(request_line.starts_with("GET /plain "));
            HttpResponse::text(200, "OK", "plain text response")
        }));

        let result = execute_tool(
            "WebFetch",
            &json!({
                "url": format!("http://{}/plain", server.addr()),
                "prompt": "Show me the content"
            }),
        )
        .expect("WebFetch should succeed for text content");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        assert_eq!(output["url"], format!("http://{}/plain", server.addr()));
        assert!(output["result"]
            .as_str()
            .expect("result")
            .contains("plain text response"));

        let error = execute_tool(
            "WebFetch",
            &json!({
                "url": "not a url",
                "prompt": "Summarize"
            }),
        )
        .expect_err("invalid URL should fail");
        assert!(error.contains("relative URL without a base") || error.contains("invalid"));
    }

    #[test]
    fn web_search_extracts_and_filters_results() {
        let _guard = process_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let server = TestServer::spawn(Arc::new(|request_line: &str| {
            assert!(request_line.contains("GET /search?q=rust+web+search "));
            HttpResponse::html(
                200,
                "OK",
                r#"
                <html><body>
                  <a class="result__a" href="https://docs.rs/reqwest">Reqwest docs</a>
                  <a class="result__a" href="https://example.com/blocked">Blocked result</a>
                </body></html>
                "#,
            )
        }));
        let workspace = temp_path("web-search-filter-workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            workspace.join("coolzhu.toml"),
            web_search_config(format!("http://{}/search", server.addr()), None),
        )
        .expect("write coolzhu.toml");
        let _cwd = ScopedCurrentDir::enter(workspace);
        let result = execute_tool(
            "WebSearch",
            &json!({
                "query": "rust web search",
                "allowed_domains": ["https://DOCS.rs/"],
                "blocked_domains": ["HTTPS://EXAMPLE.COM"]
            }),
        )
        .expect("WebSearch should succeed");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        assert_eq!(output["query"], "rust web search");
        let results = output["results"].as_array().expect("results array");
        let search_result = results
            .iter()
            .find(|item| item.get("content").is_some())
            .expect("search result block present");
        let content = search_result["content"].as_array().expect("content array");
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["title"], "Reqwest docs");
        assert_eq!(content[0]["url"], "https://docs.rs/reqwest");
    }

    #[test]
    fn web_search_handles_generic_links_and_invalid_base_url() {
        let _guard = process_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let server = TestServer::spawn(Arc::new(|request_line: &str| {
            assert!(request_line.contains("GET /fallback?q=generic+links "));
            HttpResponse::html(
                200,
                "OK",
                r#"
                <html><body>
                  <a href="https://example.com/one">Example One</a>
                  <a href="https://example.com/one">Duplicate Example One</a>
                  <a href="https://docs.rs/tokio">Tokio Docs</a>
                </body></html>
                "#,
            )
        }));
        let workspace = temp_path("web-search-generic-workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            workspace.join("coolzhu.toml"),
            web_search_config(format!("http://{}/fallback", server.addr()), None),
        )
        .expect("write coolzhu.toml");
        let _cwd = ScopedCurrentDir::enter(workspace.clone());
        let result = execute_tool(
            "WebSearch",
            &json!({
                "query": "generic links"
            }),
        )
        .expect("WebSearch fallback parsing should succeed");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        let results = output["results"].as_array().expect("results array");
        let search_result = results
            .iter()
            .find(|item| item.get("content").is_some())
            .expect("search result block present");
        let content = search_result["content"].as_array().expect("content array");
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["url"], "https://example.com/one");
        assert_eq!(content[1]["url"], "https://docs.rs/tokio");

        fs::write(
            workspace.join("coolzhu.toml"),
            web_search_config("://bad-base-url".to_string(), None),
        )
        .expect("write invalid coolzhu.toml");
        let error = execute_tool("WebSearch", &json!({ "query": "generic links" }))
            .expect_err("invalid base URL should fail");
        assert!(error.contains("relative URL without a base") || error.contains("empty host"));
    }

    /// 默认后端已从 DuckDuckGo 换成 Bing（前者在部分网络下 TCP 不通），
    /// 解析器必须能读 Bing 的结果页，否则换后端后只会抓到一堆导航链接。
    #[test]
    fn web_search_parses_bing_result_pages() {
        let _guard = process_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let server = TestServer::spawn(Arc::new(|request_line: &str| {
            assert!(request_line.contains("GET /search?q=bing+layout "));
            HttpResponse::html(
                200,
                "OK",
                r##"
                <html><body>
                  <a href="https://cn.bing.com/images/search?q=bing">Images</a>
                  <li class="b_algo" data-id iid=SERP.1>
                    <div class="b_tpcn"><a class="tilk" href="https://rust-lang.org/zh-CN/">rust-lang.org</a></div>
                    <h2 class=""><a target="_blank" href="https://rust-lang.org/zh-CN/">Rust 程序设计语言</a></h2>
                  </li>
                  <li class="b_algo" data-id iid=SERP.2>
                    <div class="b_tpcn"><a class="tilk" href="https://www.bing.com/ck/a?u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS9kb2M&amp;ntb=1">example.com</a></div>
                    <h2 class=""><a target="_blank" href="https://www.bing.com/ck/a?u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS9kb2M&amp;ntb=1">Example Doc</a></h2>
                  </li>
                </body></html>
                "##,
            )
        }));
        let workspace = temp_path("web-search-bing-workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            workspace.join("coolzhu.toml"),
            web_search_config(format!("http://{}/search", server.addr()), None),
        )
        .expect("write coolzhu.toml");
        let _cwd = ScopedCurrentDir::enter(workspace);

        let result = execute_tool("WebSearch", &json!({ "query": "bing layout" }))
            .expect("Bing result page should parse");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        let results = output["results"].as_array().expect("results array");
        let search_result = results
            .iter()
            .find(|item| item.get("content").is_some())
            .expect("search result block present");
        let content = search_result["content"].as_array().expect("content array");
        assert_eq!(content.len(), 2, "engine-internal nav link must be dropped");
        assert_eq!(content[0]["title"], "Rust 程序设计语言");
        assert_eq!(content[0]["url"], "https://rust-lang.org/zh-CN/");
        // Bing 的 ck/a 跳转包裹必须还原成真实目标地址，否则模型拿到的是 bing.com 链接。
        assert_eq!(content[1]["title"], "Example Doc");
        assert_eq!(content[1]["url"], "https://example.com/doc");
    }

    /// 所有后端都不可达时，必须返回带逐后端原因的终态失败，
    /// 而不是挂到调用方 deadline 被强杀（那样模型只看得到 timeout）。
    #[test]
    fn web_search_reports_unreachable_backends_instead_of_hanging() {
        let _guard = process_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let workspace = temp_path("web-search-unreachable-workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            workspace.join("coolzhu.toml"),
            concat!(
                "[web_search]\n",
                "base_url = \"http://127.0.0.1:9/search\"\n",
                "transport = \"reqwest\"\n",
                "connect_timeout_ms = 1000\n",
                "attempt_timeout_ms = 1500\n",
                "total_budget_ms = 3000\n",
            ),
        )
        .expect("write coolzhu.toml");
        let _cwd = ScopedCurrentDir::enter(workspace);

        let started = std::time::Instant::now();
        let error = execute_tool("WebSearch", &json!({ "query": "unreachable backend" }))
            .expect_err("unreachable backend must fail");
        let elapsed = started.elapsed();

        assert!(
            error.contains("could not reach any configured search backend"),
            "unexpected error: {error}"
        );
        assert!(error.contains("127.0.0.1:9"), "error must name the backend: {error}");
        assert!(
            error.contains("coolzhu.toml"),
            "error must point at the config knob: {error}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(15),
            "unreachable backends must fail inside the internal budget, took {elapsed:?}"
        );
    }

    #[test]
    fn web_search_default_backends_are_reachable_ordering_and_clamped() {
        let config = WebSearchConfig::default();
        let candidates = config.candidate_urls();
        assert_eq!(candidates[0], DEFAULT_WEB_SEARCH_BASE_URL);
        // 默认不带降级后端：默认降级会隐式依赖另一个站点，网络可达性因环境而异，
        // 也会让"只配了 base_url"的调用意外多打一次网络。
        assert_eq!(candidates.len(), 1);
        assert_eq!(config.effective_total_budget_ms(), DEFAULT_SEARCH_TOTAL_BUDGET_MS);
        // 预算/超时都必须被夹到安全区间，避免用户填出会被外层 deadline 强杀的值。
        let huge = WebSearchConfig {
            attempt_timeout_ms: u64::MAX,
            connect_timeout_ms: u64::MAX,
            total_budget_ms: u64::MAX,
            ..WebSearchConfig::default()
        };
        assert_eq!(huge.effective_total_budget_ms(), MAX_SEARCH_BUDGET_MS);
        assert_eq!(huge.effective_attempt_timeout_ms(), MAX_SEARCH_ATTEMPT_TIMEOUT_MS);
        assert_eq!(huge.effective_connect_timeout_ms(), MAX_SEARCH_CONNECT_TIMEOUT_MS);
        // 重复配置的降级后端只保留一次。
        // 显式配置降级后端时：与主后端重复的项去重，其余按顺序保留。
        let with_fallbacks = WebSearchConfig {
            fallback_urls: vec![
                DEFAULT_WEB_SEARCH_BASE_URL.to_string(),
                "https://html.duckduckgo.com/html/".to_string(),
            ],
            ..WebSearchConfig::default()
        };
        let candidates = with_fallbacks.candidate_urls();
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[1], "https://html.duckduckgo.com/html/");
    }

    #[test]
    fn bing_redirect_payload_decodes_to_target_url() {
        assert_eq!(
            decode_bing_redirect("https://www.bing.com/ck/a?u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS9kb2M&ntb=1"),
            Some(String::from("https://example.com/doc"))
        );
        // 非跳转链接与损坏的载荷都必须被拒绝，不能把 bing.com 内部地址当结果返回。
        assert_eq!(decode_bing_redirect("https://example.com/doc"), None);
        assert_eq!(
            decode_bing_redirect("https://www.bing.com/ck/a?u=not-base64!!"),
            None
        );
        assert_eq!(base64url_decode("aHR0cHM6Ly9leGFtcGxlLmNvbS9kb2M"), {
            Some(String::from("https://example.com/doc").into_bytes())
        });
        assert_eq!(base64url_decode("!!!"), None);
    }

    #[cfg(windows)]
    #[test]
    fn web_search_reads_base_url_and_transport_from_coolzhu_toml() {
        let _guard = process_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let server = TestServer::spawn(Arc::new(|request: &str| {
            assert!(request.contains("GET /configured?q=configured+search "));
            if request.contains("claw-rust-tools/0.1") {
                HttpResponse::html(200, "OK", "<html><body>No reqwest hits</body></html>")
            } else {
                HttpResponse::html(
                    200,
                    "OK",
                    r#"
                    <html><body>
                      <a class="result__a" href="https://example.com/configured">Configured result</a>
                    </body></html>
                    "#,
                )
            }
        }));
        let workspace = temp_path("web-search-config-workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            workspace.join("coolzhu.toml"),
            web_search_config(
                format!("http://{}/configured", server.addr()),
                Some("powershell"),
            ),
        )
        .expect("write coolzhu.toml");
        let _cwd = ScopedCurrentDir::enter(workspace);

        let result = execute_tool("WebSearch", &json!({ "query": "configured search" }))
            .expect("WebSearch should read coolzhu.toml");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        let results = output["results"].as_array().expect("results array");
        let search_result = results
            .iter()
            .find(|item| item.get("content").is_some())
            .expect("search result block present");
        let content = search_result["content"].as_array().expect("content array");
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["title"], "Configured result");
    }

    #[cfg(windows)]
    #[test]
    fn web_search_can_use_powershell_fallback_transport() {
        let _guard = process_state_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let server = TestServer::spawn(Arc::new(|request: &str| {
            assert!(request.contains("GET /fallback?q=codex+tutorial "));
            if request.contains("claw-rust-tools/0.1") {
                HttpResponse::html(200, "OK", "<html><body>No rust hits</body></html>")
            } else {
                HttpResponse::html(
                    200,
                    "OK",
                    r#"
                    <html><body>
                      <a class="result__a" href="https://openai.com/codex">Codex tutorial</a>
                    </body></html>
                    "#,
                )
            }
        }));
        let workspace = temp_path("web-search-powershell-workspace");
        fs::create_dir_all(&workspace).expect("workspace");
        fs::write(
            workspace.join("coolzhu.toml"),
            format!(
                "[web_search]\nbase_url = \"http://{}/fallback\"\nforce_powershell_fallback = true\n",
                server.addr()
            ),
        )
        .expect("write coolzhu.toml");
        let _cwd = ScopedCurrentDir::enter(workspace);
        let result = execute_tool("WebSearch", &json!({ "query": "codex tutorial" }))
            .expect("PowerShell fallback should succeed");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        let results = output["results"].as_array().expect("results array");
        let search_result = results
            .iter()
            .find(|item| item.get("content").is_some())
            .expect("search result block present");
        let content = search_result["content"].as_array().expect("content array");
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["title"], "Codex tutorial");
    }

    #[test]
    fn pending_tools_preserve_multiple_streaming_tool_calls_by_index() {
        let mut events = Vec::new();
        let mut pending_tools = BTreeMap::new();

        push_output_block(
            OutputContentBlock::ToolUse {
                id: "tool-1".to_string(),
                name: "read_file".to_string(),
                input: json!({}),
            },
            1,
            &mut events,
            &mut pending_tools,
            true,
        );
        push_output_block(
            OutputContentBlock::ToolUse {
                id: "tool-2".to_string(),
                name: "grep_search".to_string(),
                input: json!({}),
            },
            2,
            &mut events,
            &mut pending_tools,
            true,
        );

        pending_tools
            .get_mut(&1)
            .expect("first tool pending")
            .2
            .push_str("{\"path\":\"src/main.rs\"}");
        pending_tools
            .get_mut(&2)
            .expect("second tool pending")
            .2
            .push_str("{\"pattern\":\"TODO\"}");

        assert_eq!(
            pending_tools.remove(&1),
            Some((
                "tool-1".to_string(),
                "read_file".to_string(),
                "{\"path\":\"src/main.rs\"}".to_string(),
            ))
        );
        assert_eq!(
            pending_tools.remove(&2),
            Some((
                "tool-2".to_string(),
                "grep_search".to_string(),
                "{\"pattern\":\"TODO\"}".to_string(),
            ))
        );
    }

    #[test]
    fn provider_thinking_blocks_become_reasoning_events() {
        let mut events = Vec::new();
        let mut pending_tools = BTreeMap::new();

        push_output_block(
            OutputContentBlock::Thinking {
                thinking: "检查工具输入".to_string(),
                signature: None,
            },
            0,
            &mut events,
            &mut pending_tools,
            true,
        );
        push_output_block(
            OutputContentBlock::RedactedThinking {
                data: json!({"sealed": true}),
            },
            1,
            &mut events,
            &mut pending_tools,
            true,
        );

        assert!(matches!(
            events.first(),
            Some(AssistantEvent::ReasoningDelta { text, redacted: false })
                if text == "检查工具输入"
        ));
        assert!(matches!(
            events.get(1),
            Some(AssistantEvent::ReasoningDelta { text, redacted: true }) if text.is_empty()
        ));
    }

    #[test]
    fn todo_write_persists_and_returns_previous_state() {
        let path = temp_path("todos.json");
        // RPR-01b：CLAW_TODO_STORE 是**进程级解析**（生产代码 todo_store_path 在本进程读它），
        // 故用统一锁 + RAII guard 收口，而不是手写 set→restore（panic 会跳过 restore）。
        let _env = ScopedEnv::new(vec![env_set("CLAW_TODO_STORE", &path)]);

        let first = execute_tool(
            "TodoWrite",
            &json!({
                "todos": [
                    {"content": "Add tool", "activeForm": "Adding tool", "status": "in_progress"},
                    {"content": "Run tests", "activeForm": "Running tests", "status": "pending"}
                ]
            }),
        )
        .expect("TodoWrite should succeed");
        let first_output: serde_json::Value = serde_json::from_str(&first).expect("valid json");
        assert_eq!(first_output["oldTodos"].as_array().expect("array").len(), 0);

        let second = execute_tool(
            "TodoWrite",
            &json!({
                "todos": [
                    {"content": "Add tool", "activeForm": "Adding tool", "status": "completed"},
                    {"content": "Run tests", "activeForm": "Running tests", "status": "completed"},
                    {"content": "Verify", "activeForm": "Verifying", "status": "completed"}
                ]
            }),
        )
        .expect("TodoWrite should succeed");
        let _ = std::fs::remove_file(path);

        let second_output: serde_json::Value = serde_json::from_str(&second).expect("valid json");
        assert_eq!(
            second_output["oldTodos"].as_array().expect("array").len(),
            2
        );
        assert_eq!(
            second_output["newTodos"].as_array().expect("array").len(),
            3
        );
        assert!(second_output["verificationNudgeNeeded"].is_null());
    }

    #[test]
    fn todo_write_rejects_invalid_payloads_and_sets_verification_nudge() {
        let path = temp_path("todos-errors.json");
        // RPR-01b：同上，进程级环境变量改用统一锁 + RAII guard。
        let _env = ScopedEnv::new(vec![env_set("CLAW_TODO_STORE", &path)]);

        let empty = execute_tool("TodoWrite", &json!({ "todos": [] }))
            .expect_err("empty todos should fail");
        assert!(empty.contains("todos must not be empty"));

        // Multiple in_progress items are now allowed for parallel workflows
        let _multi_active = execute_tool(
            "TodoWrite",
            &json!({
                "todos": [
                    {"content": "One", "activeForm": "Doing one", "status": "in_progress"},
                    {"content": "Two", "activeForm": "Doing two", "status": "in_progress"}
                ]
            }),
        )
        .expect("multiple in-progress todos should succeed");

        let blank_content = execute_tool(
            "TodoWrite",
            &json!({
                "todos": [
                    {"content": "   ", "activeForm": "Doing it", "status": "pending"}
                ]
            }),
        )
        .expect_err("blank content should fail");
        assert!(blank_content.contains("todo content must not be empty"));

        let nudge = execute_tool(
            "TodoWrite",
            &json!({
                "todos": [
                    {"content": "Write tests", "activeForm": "Writing tests", "status": "completed"},
                    {"content": "Fix errors", "activeForm": "Fixing errors", "status": "completed"},
                    {"content": "Ship branch", "activeForm": "Shipping branch", "status": "completed"}
                ]
            }),
        )
        .expect("completed todos should succeed");
        let _ = fs::remove_file(path);

        let output: serde_json::Value = serde_json::from_str(&nudge).expect("valid json");
        assert_eq!(output["verificationNudgeNeeded"], true);
    }

    #[test]
    fn skill_loads_local_skill_prompt() {
        let codex_home = temp_path("skill-home");
        let skill_dir = codex_home.join("skills").join("help");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            "Guide on using oh-my-codex plugin\n",
        )
        .expect("write skill");
        // RPR-01b：CODEX_HOME 由生产代码 resolve_skill_path 在**本进程**读取（进程级解析），
        // 原值改用 guard 的 Option<OsString> 无损保存（改前手写分支区分不出"空串"和"不存在"）。
        let _env = ScopedEnv::new(vec![env_set("CODEX_HOME", &codex_home)]);

        let result = execute_tool(
            "Skill",
            &json!({
                "skill": "help",
                "args": "overview"
            }),
        )
        .expect("Skill should succeed");

        let output: serde_json::Value = serde_json::from_str(&result).expect("valid json");
        assert_eq!(output["skill"], "help");
        assert!(normalize_path(output["path"].as_str().expect("path")).ends_with("/help/SKILL.md"));
        assert!(output["prompt"]
            .as_str()
            .expect("prompt")
            .contains("Guide on using oh-my-codex plugin"));

        let dollar_result = execute_tool(
            "Skill",
            &json!({
                "skill": "$help"
            }),
        )
        .expect("Skill should accept $skill invocation form");
        let dollar_output: serde_json::Value =
            serde_json::from_str(&dollar_result).expect("valid json");
        assert_eq!(dollar_output["skill"], "$help");
        assert!(
            normalize_path(dollar_output["path"].as_str().expect("path"))
                .ends_with("/help/SKILL.md")
        );
        let _ = fs::remove_dir_all(codex_home);
    }

    #[test]
    fn tool_search_supports_keyword_and_select_queries() {
        let keyword = execute_tool(
            "ToolSearch",
            &json!({"query": "web current", "max_results": 3}),
        )
        .expect("ToolSearch should succeed");
        let keyword_output: serde_json::Value = serde_json::from_str(&keyword).expect("valid json");
        let matches = keyword_output["matches"].as_array().expect("matches");
        assert!(matches.iter().any(|value| value == "WebSearch"));

        let selected = execute_tool("ToolSearch", &json!({"query": "select:Agent,Skill"}))
            .expect("ToolSearch should succeed");
        let selected_output: serde_json::Value =
            serde_json::from_str(&selected).expect("valid json");
        assert_eq!(selected_output["matches"][0], "Agent");
        assert_eq!(selected_output["matches"][1], "Skill");

        let aliased = execute_tool("ToolSearch", &json!({"query": "AgentTool"}))
            .expect("ToolSearch should support tool aliases");
        let aliased_output: serde_json::Value = serde_json::from_str(&aliased).expect("valid json");
        assert_eq!(aliased_output["matches"][0], "Agent");
        assert_eq!(aliased_output["normalized_query"], "agent");

        let selected_with_alias =
            execute_tool("ToolSearch", &json!({"query": "select:AgentTool,Skill"}))
                .expect("ToolSearch alias select should succeed");
        let selected_with_alias_output: serde_json::Value =
            serde_json::from_str(&selected_with_alias).expect("valid json");
        assert_eq!(selected_with_alias_output["matches"][0], "Agent");
        assert_eq!(selected_with_alias_output["matches"][1], "Skill");
    }

    #[test]
    fn agent_persists_handoff_metadata() {
        let dir = temp_path("agent-store");
        // RPR-01b：CLAW_AGENT_STORE 由 agent_store_dir 在本进程读取（进程级解析）→ 统一锁 + guard。
        let _env = ScopedEnv::new(vec![env_set("CLAW_AGENT_STORE", &dir)]);
        let captured = Arc::new(Mutex::new(None::<AgentJob>));
        let captured_for_spawn = Arc::clone(&captured);

        let manifest = execute_agent_with_spawn(
            AgentInput {
                description: "Audit the branch".to_string(),
                prompt: "Check tests and outstanding work.".to_string(),
                subagent_type: Some("Explore".to_string()),
                name: Some("ship-audit".to_string()),
                model: None,
            },
            move |job| {
                *captured_for_spawn
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(job);
                Ok(())
            },
        )
        .expect("Agent should succeed");

        assert_eq!(manifest.name, "ship-audit");
        assert_eq!(manifest.subagent_type.as_deref(), Some("Explore"));
        assert_eq!(manifest.status, "running");
        assert!(!manifest.created_at.is_empty());
        assert!(manifest.started_at.is_some());
        assert!(manifest.completed_at.is_none());
        let contents = std::fs::read_to_string(&manifest.output_file).expect("agent file exists");
        let manifest_contents =
            std::fs::read_to_string(&manifest.manifest_file).expect("manifest file exists");
        assert!(contents.contains("Audit the branch"));
        assert!(contents.contains("Check tests and outstanding work."));
        assert!(manifest_contents.contains("\"subagentType\": \"Explore\""));
        assert!(manifest_contents.contains("\"status\": \"running\""));
        let captured_job = captured
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect("spawn job should be captured");
        assert_eq!(captured_job.prompt, "Check tests and outstanding work.");
        assert!(captured_job.allowed_tools.contains("read_file"));
        assert!(!captured_job.allowed_tools.contains("Agent"));

        let normalized = execute_tool(
            "Agent",
            &json!({
                "description": "Verify the branch",
                "prompt": "Check tests.",
                "subagent_type": "explorer"
            }),
        )
        .expect("Agent should normalize built-in aliases");
        let normalized_output: serde_json::Value =
            serde_json::from_str(&normalized).expect("valid json");
        assert_eq!(normalized_output["subagentType"], "Explore");

        let named = execute_tool(
            "Agent",
            &json!({
                "description": "Review the branch",
                "prompt": "Inspect diff.",
                "name": "Ship Audit!!!"
            }),
        )
        .expect("Agent should normalize explicit names");
        let named_output: serde_json::Value = serde_json::from_str(&named).expect("valid json");
        assert_eq!(named_output["name"], "ship-audit");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_fake_runner_can_persist_completion_and_failure() {
        let dir = temp_path("agent-runner");
        // RPR-01b：同上，进程级环境变量改用统一锁 + RAII guard（原来手写 set→remove）。
        let _env = ScopedEnv::new(vec![env_set("CLAW_AGENT_STORE", &dir)]);

        let completed = execute_agent_with_spawn(
            AgentInput {
                description: "Complete the task".to_string(),
                prompt: "Do the work".to_string(),
                subagent_type: Some("Explore".to_string()),
                name: Some("complete-task".to_string()),
                model: Some("claude-sonnet-4-6".to_string()),
            },
            |job| {
                persist_agent_terminal_state(
                    &job.manifest,
                    "completed",
                    Some("Finished successfully"),
                    None,
                )
            },
        )
        .expect("completed agent should succeed");

        let completed_manifest = std::fs::read_to_string(&completed.manifest_file)
            .expect("completed manifest should exist");
        let completed_output =
            std::fs::read_to_string(&completed.output_file).expect("completed output should exist");
        assert!(completed_manifest.contains("\"status\": \"completed\""));
        assert!(completed_output.contains("Finished successfully"));

        let failed = execute_agent_with_spawn(
            AgentInput {
                description: "Fail the task".to_string(),
                prompt: "Do the failing work".to_string(),
                subagent_type: Some("Verification".to_string()),
                name: Some("fail-task".to_string()),
                model: None,
            },
            |job| {
                persist_agent_terminal_state(
                    &job.manifest,
                    "failed",
                    None,
                    Some(String::from("simulated failure")),
                )
            },
        )
        .expect("failed agent should still spawn");

        let failed_manifest =
            std::fs::read_to_string(&failed.manifest_file).expect("failed manifest should exist");
        let failed_output =
            std::fs::read_to_string(&failed.output_file).expect("failed output should exist");
        assert!(failed_manifest.contains("\"status\": \"failed\""));
        assert!(failed_manifest.contains("simulated failure"));
        assert!(failed_output.contains("simulated failure"));

        let spawn_error = execute_agent_with_spawn(
            AgentInput {
                description: "Spawn error task".to_string(),
                prompt: "Never starts".to_string(),
                subagent_type: None,
                name: Some("spawn-error".to_string()),
                model: None,
            },
            |_| Err(String::from("thread creation failed")),
        )
        .expect_err("spawn errors should surface");
        assert!(spawn_error.contains("failed to spawn sub-agent"));
        let spawn_error_manifest = std::fs::read_dir(&dir)
            .expect("agent dir should exist")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
            .find_map(|path| {
                let contents = std::fs::read_to_string(&path).ok()?;
                contents
                    .contains("\"name\": \"spawn-error\"")
                    .then_some(contents)
            })
            .expect("failed manifest should still be written");
        assert!(spawn_error_manifest.contains("\"status\": \"failed\""));
        assert!(spawn_error_manifest.contains("thread creation failed"));

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn agent_tool_subset_mapping_is_expected() {
        let general = allowed_tools_for_subagent("general-purpose");
        assert!(general.contains("bash"));
        assert!(general.contains("write_file"));
        assert!(!general.contains("Agent"));

        let explore = allowed_tools_for_subagent("Explore");
        assert!(explore.contains("read_file"));
        assert!(explore.contains("grep_search"));
        assert!(!explore.contains("bash"));

        let plan = allowed_tools_for_subagent("Plan");
        assert!(plan.contains("TodoWrite"));
        assert!(plan.contains("StructuredOutput"));
        assert!(!plan.contains("Agent"));

        let verification = allowed_tools_for_subagent("Verification");
        assert!(verification.contains("bash"));
        assert!(verification.contains("PowerShell"));
        assert!(!verification.contains("write_file"));
    }

    #[derive(Debug)]
    struct MockSubagentApiClient {
        calls: usize,
        input_path: String,
    }

    impl runtime::ApiClient for MockSubagentApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            self.calls += 1;
            match self.calls {
                1 => {
                    assert_eq!(request.messages.len(), 1);
                    Ok(vec![
                        AssistantEvent::ToolUse {
                            id: "tool-1".to_string(),
                            name: "read_file".to_string(),
                            input: json!({ "path": self.input_path }).to_string(),
                        },
                        AssistantEvent::MessageStop,
                    ])
                }
                2 => {
                    assert!(request.messages.len() >= 3);
                    Ok(vec![
                        AssistantEvent::TextDelta("Scope: completed mock review".to_string()),
                        AssistantEvent::MessageStop,
                    ])
                }
                _ => panic!("unexpected mock stream call"),
            }
        }
    }

    #[test]
    fn subagent_runtime_executes_tool_loop_with_isolated_session() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let path = temp_path("subagent-input.txt");
        std::fs::write(&path, "hello from child").expect("write input file");

        let mut runtime = ConversationRuntime::new(
            Session::new(),
            MockSubagentApiClient {
                calls: 0,
                input_path: path.display().to_string(),
            },
            SubagentToolExecutor::new(BTreeSet::from([String::from("read_file")])),
            agent_permission_policy(),
            vec![String::from("system prompt")],
        );

        let summary = runtime
            .run_turn("Inspect the delegated file", None)
            .expect("subagent loop should succeed");

        assert_eq!(
            final_assistant_text(&summary),
            "Scope: completed mock review"
        );
        assert!(runtime
            .session()
            .messages
            .iter()
            .flat_map(|message| message.blocks.iter())
            .any(|block| matches!(
                block,
                runtime::ContentBlock::ToolResult { output, .. }
                    if output.contains("hello from child")
            )));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn agent_rejects_blank_required_fields() {
        let missing_description = execute_tool(
            "Agent",
            &json!({
                "description": "  ",
                "prompt": "Inspect"
            }),
        )
        .expect_err("blank description should fail");
        assert!(missing_description.contains("description must not be empty"));

        let missing_prompt = execute_tool(
            "Agent",
            &json!({
                "description": "Inspect branch",
                "prompt": " "
            }),
        )
        .expect_err("blank prompt should fail");
        assert!(missing_prompt.contains("prompt must not be empty"));
    }

    #[test]
    fn notebook_edit_replaces_inserts_and_deletes_cells() {
        let path = temp_path("notebook.ipynb");
        std::fs::write(
            &path,
            r#"{
  "cells": [
    {"cell_type": "code", "id": "cell-a", "metadata": {}, "source": ["print(1)\n"], "outputs": [], "execution_count": null}
  ],
  "metadata": {"kernelspec": {"language": "python"}},
  "nbformat": 4,
  "nbformat_minor": 5
}"#,
        )
        .expect("write notebook");

        let replaced = execute_tool(
            "NotebookEdit",
            &json!({
                "notebook_path": path.display().to_string(),
                "cell_id": "cell-a",
                "new_source": "print(2)\n",
                "edit_mode": "replace"
            }),
        )
        .expect("NotebookEdit replace should succeed");
        let replaced_output: serde_json::Value = serde_json::from_str(&replaced).expect("json");
        assert_eq!(replaced_output["cell_id"], "cell-a");
        assert_eq!(replaced_output["cell_type"], "code");

        let inserted = execute_tool(
            "NotebookEdit",
            &json!({
                "notebook_path": path.display().to_string(),
                "cell_id": "cell-a",
                "new_source": "# heading\n",
                "cell_type": "markdown",
                "edit_mode": "insert"
            }),
        )
        .expect("NotebookEdit insert should succeed");
        let inserted_output: serde_json::Value = serde_json::from_str(&inserted).expect("json");
        assert_eq!(inserted_output["cell_type"], "markdown");
        let appended = execute_tool(
            "NotebookEdit",
            &json!({
                "notebook_path": path.display().to_string(),
                "new_source": "print(3)\n",
                "edit_mode": "insert"
            }),
        )
        .expect("NotebookEdit append should succeed");
        let appended_output: serde_json::Value = serde_json::from_str(&appended).expect("json");
        assert_eq!(appended_output["cell_type"], "code");

        let deleted = execute_tool(
            "NotebookEdit",
            &json!({
                "notebook_path": path.display().to_string(),
                "cell_id": "cell-a",
                "edit_mode": "delete"
            }),
        )
        .expect("NotebookEdit delete should succeed without new_source");
        let deleted_output: serde_json::Value = serde_json::from_str(&deleted).expect("json");
        assert!(deleted_output["cell_type"].is_null());
        assert_eq!(deleted_output["new_source"], "");

        let final_notebook: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read notebook"))
                .expect("valid notebook json");
        let cells = final_notebook["cells"].as_array().expect("cells array");
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0]["cell_type"], "markdown");
        assert!(cells[0].get("outputs").is_none());
        assert_eq!(cells[1]["cell_type"], "code");
        assert_eq!(cells[1]["source"][0], "print(3)\n");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn notebook_edit_rejects_invalid_inputs() {
        let text_path = temp_path("notebook.txt");
        fs::write(&text_path, "not a notebook").expect("write text file");
        let wrong_extension = execute_tool(
            "NotebookEdit",
            &json!({
                "notebook_path": text_path.display().to_string(),
                "new_source": "print(1)\n"
            }),
        )
        .expect_err("non-ipynb file should fail");
        assert!(wrong_extension.contains("Jupyter notebook"));
        let _ = fs::remove_file(&text_path);

        let empty_notebook = temp_path("empty.ipynb");
        fs::write(
            &empty_notebook,
            r#"{"cells":[],"metadata":{"kernelspec":{"language":"python"}},"nbformat":4,"nbformat_minor":5}"#,
        )
        .expect("write empty notebook");

        let missing_source = execute_tool(
            "NotebookEdit",
            &json!({
                "notebook_path": empty_notebook.display().to_string(),
                "edit_mode": "insert"
            }),
        )
        .expect_err("insert without source should fail");
        assert!(missing_source.contains("new_source is required"));

        let missing_cell = execute_tool(
            "NotebookEdit",
            &json!({
                "notebook_path": empty_notebook.display().to_string(),
                "edit_mode": "delete"
            }),
        )
        .expect_err("delete on empty notebook should fail");
        assert!(missing_cell.contains("Notebook has no cells to edit"));
        let _ = fs::remove_file(empty_notebook);
    }

    #[test]
    fn bash_tool_reports_success_exit_failure_timeout_and_background() {
        // 本用例会以进程 cwd 启动子 shell：必须与所有改动 cwd/env 的用例互斥，
        // 否则并发的 set_current_dir + remove_dir_all 会让 cwd 短暂指向已删除目录，
        // 子进程启动报 "目录名称无效 (os error 267)"。
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let success = execute_tool("bash", &json!({ "command": shell_success_command() }))
            .expect("bash should succeed");
        let success_output: serde_json::Value = serde_json::from_str(&success).expect("json");
        assert_eq!(
            success_output["stdout"].as_str().expect("stdout").trim(),
            "hello"
        );
        assert_eq!(success_output["interrupted"], false);

        let failure = execute_tool("bash", &json!({ "command": shell_failure_command() }))
            .expect("bash failure should still return structured output");
        let failure_output: serde_json::Value = serde_json::from_str(&failure).expect("json");
        assert_eq!(failure_output["returnCodeInterpretation"], "exit_code:7");
        assert!(failure_output["stderr"]
            .as_str()
            .expect("stderr")
            .contains("oops"));

        let timeout = execute_tool(
            "bash",
            &json!({ "command": shell_sleep_command(), "timeout": 1 }),
        )
        .expect("bash timeout should return output");
        let timeout_output: serde_json::Value = serde_json::from_str(&timeout).expect("json");
        assert_eq!(timeout_output["interrupted"], true);
        assert_eq!(timeout_output["returnCodeInterpretation"], "timeout");
        assert!(timeout_output["stderr"]
            .as_str()
            .expect("stderr")
            .contains("Command exceeded timeout"));

        let background = execute_tool(
            "bash",
            &json!({ "command": shell_sleep_command(), "run_in_background": true }),
        )
        .expect("bash background should succeed");
        let background_output: serde_json::Value = serde_json::from_str(&background).expect("json");
        assert!(background_output["backgroundTaskId"].as_str().is_some());
        assert_eq!(background_output["noOutputExpected"], true);
    }

    #[cfg(windows)]
    fn shell_success_command() -> &'static str {
        "echo hello"
    }

    #[cfg(not(windows))]
    fn shell_success_command() -> &'static str {
        "printf 'hello'"
    }

    #[cfg(windows)]
    fn shell_failure_command() -> &'static str {
        "echo oops 1>&2 && exit /b 7"
    }

    #[cfg(not(windows))]
    fn shell_failure_command() -> &'static str {
        "printf 'oops' >&2; exit 7"
    }

    #[cfg(windows)]
    fn shell_sleep_command() -> &'static str {
        "ping -n 4 127.0.0.1 >NUL"
    }

    #[cfg(not(windows))]
    fn shell_sleep_command() -> &'static str {
        "sleep 3"
    }

    #[test]
    fn file_tools_cover_read_write_and_edit_behaviors() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = temp_path("fs-suite");
        fs::create_dir_all(&root).expect("create root");
        // RPR-01：cwd 改用 RAII 收口（手写 save/restore 会被中途 panic 跳过，把进程 cwd 留在临时目录里）。
        let _cwd = ScopedCurrentDir::enter(root.clone());

        let write_create = execute_tool(
            "write_file",
            &json!({ "path": "nested/demo.txt", "content": "alpha\nbeta\nalpha\n" }),
        )
        .expect("write create should succeed");
        let write_create_output: serde_json::Value =
            serde_json::from_str(&write_create).expect("json");
        assert_eq!(write_create_output["type"], "create");
        assert!(root.join("nested/demo.txt").exists());

        let write_update = execute_tool(
            "write_file",
            &json!({ "path": "nested/demo.txt", "content": "alpha\nbeta\ngamma\n", "expected_version": write_create_output["version"]["sha256"] }),
        )
        .expect("write update should succeed");
        let write_update_output: serde_json::Value =
            serde_json::from_str(&write_update).expect("json");
        assert_eq!(write_update_output["type"], "update");
        assert_eq!(write_update_output["originalFile"], "alpha\nbeta\nalpha\n");

        let read_full = execute_tool("read_file", &json!({ "path": "nested/demo.txt" }))
            .expect("read full should succeed");
        let read_full_output: serde_json::Value = serde_json::from_str(&read_full).expect("json");
        assert_eq!(read_full_output["file"]["content"], "alpha\nbeta\ngamma");
        assert_eq!(read_full_output["file"]["startLine"], 1);

        let read_slice = execute_tool(
            "read_file",
            &json!({ "path": "nested/demo.txt", "offset": 1, "limit": 1 }),
        )
        .expect("read slice should succeed");
        let read_slice_output: serde_json::Value = serde_json::from_str(&read_slice).expect("json");
        assert_eq!(read_slice_output["file"]["content"], "beta");
        assert_eq!(read_slice_output["file"]["startLine"], 2);

        let read_past_end = execute_tool(
            "read_file",
            &json!({ "path": "nested/demo.txt", "offset": 50 }),
        )
        .expect("read past EOF should succeed");
        let read_past_end_output: serde_json::Value =
            serde_json::from_str(&read_past_end).expect("json");
        assert_eq!(read_past_end_output["file"]["content"], "");
        assert_eq!(read_past_end_output["file"]["startLine"], 4);

        let read_error = execute_tool("read_file", &json!({ "path": "missing.txt" }))
            .expect_err("missing file should fail");
        assert!(!read_error.is_empty());

        let edit_once = execute_tool(
            "edit_file",
            &json!({ "path": "nested/demo.txt", "old_string": "alpha", "new_string": "omega", "expected_version": read_full_output["version"]["sha256"] }),
        )
        .expect("single edit should succeed");
        let edit_once_output: serde_json::Value = serde_json::from_str(&edit_once).expect("json");
        assert_eq!(edit_once_output["replaceAll"], false);
        assert_eq!(
            fs::read_to_string(root.join("nested/demo.txt")).expect("read file"),
            "omega\nbeta\ngamma\n"
        );

        let reset = execute_tool(
            "write_file",
            &json!({ "path": "nested/demo.txt", "content": "alpha\nbeta\nalpha\n", "expected_version": edit_once_output["version"]["sha256"] }),
        ).expect("reset file");
        let reset: serde_json::Value = serde_json::from_str(&reset).unwrap();
        let edit_all = execute_tool(
            "edit_file",
            &json!({
                "path": "nested/demo.txt",
                "old_string": "alpha",
                "new_string": "omega",
                "replace_all": true,
                "expected_version": reset["version"]["sha256"]
            }),
        )
        .expect("replace all should succeed");
        let edit_all_output: serde_json::Value = serde_json::from_str(&edit_all).expect("json");
        assert_eq!(edit_all_output["replaceAll"], true);
        assert_eq!(
            fs::read_to_string(root.join("nested/demo.txt")).expect("read file"),
            "omega\nbeta\nomega\n"
        );

        let edit_same = execute_tool(
            "edit_file",
            &json!({ "path": "nested/demo.txt", "old_string": "omega", "new_string": "omega", "expected_version": edit_all_output["version"]["sha256"] }),
        )
        .expect_err("identical old/new should fail");
        assert!(edit_same.contains("must differ"));

        let edit_missing = execute_tool(
            "edit_file",
            &json!({ "path": "nested/demo.txt", "old_string": "missing", "new_string": "omega", "expected_version": edit_all_output["version"]["sha256"] }),
        )
        .expect_err("missing substring should fail");
        assert!(edit_missing.contains("old_string not found"));
    }

    #[test]
    fn glob_and_grep_tools_cover_success_and_errors() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let root = temp_path("search-suite");
        fs::create_dir_all(root.join("nested")).expect("create root");
        // RPR-01：cwd 改用 RAII 收口（手写 save/restore 会被中途 panic 跳过）。
        let _cwd = ScopedCurrentDir::enter(root.clone());

        fs::write(
            root.join("nested/lib.rs"),
            "fn main() {}\nlet alpha = 1;\nlet alpha = 2;\n",
        )
        .expect("write rust file");
        fs::write(root.join("nested/notes.txt"), "alpha\nbeta\n").expect("write txt file");

        let globbed = execute_tool("glob_search", &json!({ "pattern": "nested/*.rs" }))
            .expect("glob should succeed");
        let globbed_output: serde_json::Value = serde_json::from_str(&globbed).expect("json");
        assert_eq!(globbed_output["numFiles"], 1);
        assert!(
            normalize_path(globbed_output["filenames"][0].as_str().expect("filename"))
                .ends_with("nested/lib.rs")
        );

        let glob_error = execute_tool("glob_search", &json!({ "pattern": "[" }))
            .expect_err("invalid glob should fail");
        assert!(!glob_error.is_empty());

        let grep_content = execute_tool(
            "grep_search",
            &json!({
                "pattern": "alpha",
                "path": "nested",
                "glob": "*.rs",
                "output_mode": "content",
                "-n": true,
                "head_limit": 1,
                "offset": 1
            }),
        )
        .expect("grep content should succeed");
        let grep_content_output: serde_json::Value =
            serde_json::from_str(&grep_content).expect("json");
        assert_eq!(grep_content_output["numFiles"], 0);
        assert!(grep_content_output["appliedLimit"].is_null());
        assert_eq!(grep_content_output["appliedOffset"], 1);
        assert!(grep_content_output["content"]
            .as_str()
            .expect("content")
            .contains("let alpha = 2;"));

        let grep_count = execute_tool(
            "grep_search",
            &json!({ "pattern": "alpha", "path": "nested", "output_mode": "count" }),
        )
        .expect("grep count should succeed");
        let grep_count_output: serde_json::Value = serde_json::from_str(&grep_count).expect("json");
        assert_eq!(grep_count_output["numMatches"], 3);

        let grep_error = execute_tool(
            "grep_search",
            &json!({ "pattern": "(alpha", "path": "nested" }),
        )
        .expect_err("invalid regex should fail");
        assert!(!grep_error.is_empty());
    }

    #[test]
    fn sleep_waits_and_reports_duration() {
        let started = std::time::Instant::now();
        let result =
            execute_tool("Sleep", &json!({"duration_ms": 20})).expect("Sleep should succeed");
        let elapsed = started.elapsed();
        let output: serde_json::Value = serde_json::from_str(&result).expect("json");
        assert_eq!(output["duration_ms"], 20);
        assert!(output["message"]
            .as_str()
            .expect("message")
            .contains("Slept for 20ms"));
        assert!(elapsed >= Duration::from_millis(15));
    }

    #[test]
    fn brief_returns_sent_message_and_attachment_metadata() {
        let attachment = std::env::temp_dir().join(format!(
            "claw-brief-{}.png",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::write(&attachment, b"png-data").expect("write attachment");

        let result = execute_tool(
            "SendUserMessage",
            &json!({
                "message": "hello user",
                "attachments": [attachment.display().to_string()],
                "status": "normal"
            }),
        )
        .expect("SendUserMessage should succeed");

        let output: serde_json::Value = serde_json::from_str(&result).expect("json");
        assert_eq!(output["message"], "hello user");
        assert!(output["sentAt"].as_str().is_some());
        assert_eq!(output["attachments"][0]["isImage"], true);
        let _ = std::fs::remove_file(attachment);
    }

    #[test]
    fn config_reads_and_writes_supported_values() {
        let root = std::env::temp_dir().join(format!(
            "claw-config-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let home = root.join("home");
        let cwd = root.join("cwd");
        std::fs::create_dir_all(home.join(".claw")).expect("home dir");
        std::fs::create_dir_all(cwd.join(".claw")).expect("cwd dir");
        std::fs::write(
            home.join(".claw").join("settings.json"),
            r#"{"verbose":false}"#,
        )
        .expect("write global settings");

        // RPR-01b：HOME / CLAW_CONFIG_HOME 由 config_home_dir 在**本进程**读取（进程级解析），
        // 改用统一锁 + RAII guard。改前用 `var().ok()` 读原值，把「原来为空串」误当成
        // 「原来不存在」，恢复时会写成 remove_var；guard 用 Option<OsString> 三态无损。
        // 「原来不存在」这一态正是这里要的：清掉 CLAW_CONFIG_HOME 让全局配置落到 HOME/.claw。
        let _env = ScopedEnv::new(vec![
            env_set("HOME", &home),
            env_unset("CLAW_CONFIG_HOME"),
        ]);
        // RPR-01：cwd 用 RAII 收口（cwd 只是 root 的子目录，故连 root 一起交给 guard 清理），
        // 避免用例中途 panic 时把进程 cwd 留在临时目录、且临时目录残留。
        let _cwd = ScopedCurrentDir::enter_in(cwd.clone(), root.clone());

        let get = execute_tool("Config", &json!({"setting": "verbose"})).expect("get config");
        let get_output: serde_json::Value = serde_json::from_str(&get).expect("json");
        assert_eq!(get_output["value"], false);

        let set = execute_tool(
            "Config",
            &json!({"setting": "permissions.defaultMode", "value": "plan"}),
        )
        .expect("set config");
        let set_output: serde_json::Value = serde_json::from_str(&set).expect("json");
        assert_eq!(set_output["operation"], "set");
        assert_eq!(set_output["newValue"], "plan");

        let invalid = execute_tool(
            "Config",
            &json!({"setting": "permissions.defaultMode", "value": "bogus"}),
        )
        .expect_err("invalid config value should error");
        assert!(invalid.contains("Invalid value"));

        let unknown =
            execute_tool("Config", &json!({"setting": "nope"})).expect("unknown setting result");
        let unknown_output: serde_json::Value = serde_json::from_str(&unknown).expect("json");
        assert_eq!(unknown_output["success"], false);

        // 环境变量与 root 目录的清理都交给 _env / _cwd 两个 guard 的 Drop。
    }

    #[test]
    fn structured_output_echoes_input_payload() {
        let result = execute_tool("StructuredOutput", &json!({"ok": true, "items": [1, 2, 3]}))
            .expect("StructuredOutput should succeed");
        let output: serde_json::Value = serde_json::from_str(&result).expect("json");
        assert_eq!(output["data"], "Structured output provided successfully");
        assert_eq!(output["structured_output"]["ok"], true);
        assert_eq!(output["structured_output"]["items"][1], 2);
    }

    #[test]
    fn repl_executes_python_code() {
        // RPR-01：REPL 子进程继承**进程 cwd**（等价于读一次进程全局状态），
        // 必须与改动 cwd 的用例互斥。
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = execute_tool(
            "REPL",
            &json!({"language": "python", "code": "print(1 + 1)", "timeout_ms": 500}),
        )
        .expect("REPL should succeed");
        let output: serde_json::Value = serde_json::from_str(&result).expect("json");
        assert_eq!(output["language"], "python");
        assert_eq!(output["exitCode"], 0);
        assert!(output["stdout"].as_str().expect("stdout").contains('2'));
    }

    #[test]
    fn repl_timeout_is_enforced_after_the_program_really_started() {
        let _guard = env_lock().lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let started = std::time::Instant::now();
        let result = execute_tool("REPL", &json!({"language":"python",
            "code":"import time; print('started', flush=True); time.sleep(4); print('late', flush=True)",
            "timeout_ms":1000})).expect("超时应返回真实退出回执");
        let output: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(output["interrupted"], true);
        assert_eq!(output["exitCode"], 124);
        assert!(output["stdout"].as_str().unwrap().contains("started"));
        assert!(!output["stdout"].as_str().unwrap().contains("late"));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    /// 建一个"桩 PowerShell"：把 `-Command` 之后的第一个参数原样回显为 `pwsh:<参数>`。
    /// 返回脚本路径（Windows: `pwsh.cmd`；其它平台: `pwsh`，已置可执行位）。
    fn write_stub_pwsh(dir: &std::path::Path) -> PathBuf {
        std::fs::create_dir_all(dir).expect("create dir");
        #[cfg(windows)]
        let script = dir.join("pwsh.cmd");
        #[cfg(not(windows))]
        let script = dir.join("pwsh");

        #[cfg(windows)]
        std::fs::write(
            &script,
            r#"@echo off
:loop
if "%~1"=="-Command" goto found
if "%~1"=="" goto done
shift
goto loop
:found
shift
<nul set /p="pwsh:%~1"
:done
"#,
        )
        .expect("write script");

        #[cfg(not(windows))]
        {
            std::fs::write(
                &script,
                r#"#!/bin/sh
while [ "$1" != "-Command" ] && [ $# -gt 0 ]; do shift; done
shift
printf 'pwsh:%s' "$1"
"#,
            )
            .expect("write script");
            std::process::Command::new("/bin/chmod")
                .arg("+x")
                .arg(&script)
                .status()
                .expect("chmod");
        }
        script
    }

    fn temp_bin_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    /// P-08/I6：**注入显式路径集合**，不再改父进程 PATH。
    ///
    /// 改前（RPR-01b）把桩目录 prepend 到本进程 PATH，再用 guard 恢复：本质仍是"改进程环境"，
    /// 需要与所有读/写进程环境的用例互斥，且恢复失败时会污染本进程后续执行。
    /// 现在搜索范围由 `execute_powershell_with_search_paths` 显式传入，桩目录**只**出现在这份集合里，
    /// 父进程 PATH 全程不变（用例结尾直接断言）。
    #[test]
    fn powershell_runs_via_stub_shell() {
        let dir = temp_bin_dir("claw-pwsh-bin");
        let _script = write_stub_pwsh(&dir);

        let path_before = std::env::var_os("PATH");
        let search_paths = vec![dir.clone()];
        let run = |input: &serde_json::Value| {
            // 保留原先经 JSON 构造入参的覆盖（`from_value::<PowerShellInput>` 的语义不因本用例改变）。
            let input: PowerShellInput =
                serde_json::from_value(input.clone()).expect("parse PowerShell input");
            execute_powershell_with_search_paths(input, Some(&search_paths))
        };

        let result = run(&json!({"command": "Write-Output hello", "timeout": 1000}))
            .expect("PowerShell should succeed");
        let background = run(&json!({"command": "Write-Output hello", "run_in_background": true}))
            .expect("PowerShell background should succeed");

        let _ = std::fs::remove_dir_all(&dir);

        // 断言与改前逐条对齐（原来断言的是 execute_tool 返回的 JSON，这里断言同一结构的序列化结果）。
        let output = serde_json::to_value(&result).expect("json");
        assert_eq!(output["stdout"], "pwsh:Write-Output hello");
        assert!(output["stderr"].as_str().expect("stderr").is_empty());

        let background_output = serde_json::to_value(&background).expect("json");
        assert!(background_output["backgroundTaskId"].as_str().is_some());
        assert_eq!(background_output["backgroundedByUser"], true);
        assert_eq!(background_output["assistantAutoBackgrounded"], false);

        // 回归（P-08/I6）：用例执行前后父进程 PATH 值必须一致 —— 不再需要（也不允许）改父进程环境。
        assert_eq!(
            std::env::var_os("PATH"),
            path_before,
            "I6：注入路径集合后，本用例不得改动父进程 PATH"
        );
        assert!(
            !std::env::var_os("PATH")
                .unwrap_or_default()
                .to_string_lossy()
                .contains("claw-pwsh-bin"),
            "I6：桩目录不得出现在父进程 PATH 里（命中必须来自注入的路径集合）"
        );
    }

    /// P-08/I6：注入**只有一个空目录**的路径集合 → 必须报"找不到 PowerShell"。
    ///
    /// 判别性：即使本机装了真 PowerShell，本用例也必须失败于"找不到"，
    /// 因为搜索范围完全由注入集合决定；改前只能靠改成进程 PATH 来构造同样的场景。
    #[test]
    fn powershell_errors_when_shell_is_missing() {
        let empty_dir = temp_bin_dir("claw-empty-bin");
        std::fs::create_dir_all(&empty_dir).expect("create empty dir");

        let path_before = std::env::var_os("PATH");
        let search_paths = vec![empty_dir.clone()];
        let input: PowerShellInput =
            serde_json::from_value(json!({"command": "Write-Output hello"})).expect("parse input");

        let err = execute_powershell_with_search_paths(input, Some(&search_paths))
            .expect_err("PowerShell should fail when shell is missing");

        let _ = std::fs::remove_dir_all(&empty_dir);

        let message = err.to_string();
        assert!(
            message.contains("PowerShell executable not found"),
            "err={message}"
        );
        assert_eq!(
            std::env::var_os("PATH"),
            path_before,
            "I6：注入路径集合后，本用例不得改动父进程 PATH"
        );
    }

    /// 回归（P-08/I6）：查找接缝是**纯查找** —— 搜索范围完全由参数决定，
    /// 用例不需要、也不得改动父进程环境（PATH / PATHEXT 全程不变）。
    ///
    /// 判别性：把桩目录写进注入集合（该目录并不在进程 PATH 里）后查找必须命中；
    /// 空集合必须查不到；生产入口 `find_command_path` 与接缝是同一实现（同一个"委托人"）。
    #[test]
    fn command_lookup_seam_never_touches_process_env() {
        let path_before = std::env::var_os("PATH");
        let pathext_before = std::env::var_os("PATHEXT");

        let dir = temp_bin_dir("claw-pwsh-seam");
        let script = write_stub_pwsh(&dir);
        let injected = vec![dir.clone()];

        // (1) 命中：只可能来自注入集合（桩目录不在进程 PATH 中）。
        // 注意扩展名大小写由 PATHEXT 决定（Windows 下命中 `pwsh.CMD`），故按"所在目录 + 文件名主体"断言。
        let found = find_command_path_in("pwsh", Some(&injected)).expect("stub pwsh must be found");
        let found_path = std::path::Path::new(&found);
        assert!(found_path.is_file(), "命中的必须是文件：{found}");
        assert_eq!(
            found_path.parent(),
            script.parent(),
            "命中的必须是注入集合里的那个桩：{found}"
        );
        assert_eq!(
            found_path.file_stem().and_then(|stem| stem.to_str()),
            Some("pwsh")
        );
        // 注入集合里没有的命令 → 查不到（不改环境也能构造"缺失"场景）。
        assert_eq!(
            find_command_path_in("claw-rpr01b-absent-cmd", Some(&injected)),
            None
        );
        // 空集合 → 查不到（Windows: 无目录可扫；其它平台: 子进程 PATH 为空）。
        assert_eq!(find_command_path_in("pwsh", Some(&[])), None);

        // (2) PowerShell 探测：空集合 → 与改前同一句错误文案。
        let err = detect_powershell_shell_in(Some(&[])).expect_err("empty search set must fail");
        assert!(
            err.to_string().contains("PowerShell executable not found"),
            "err={err}"
        );

        // (3) 生产入口的委托关系：同一个查找语义（命令名不存在 → None，读数取自进程 PATH）。
        assert_eq!(find_command_path("claw-rpr01b-absent-cmd"), None);

        let _ = std::fs::remove_dir_all(&dir);

        // (4) 全程未动父进程环境。
        assert_eq!(std::env::var_os("PATH"), path_before, "I6：不得改动 PATH");
        assert_eq!(
            std::env::var_os("PATHEXT"),
            pathext_before,
            "I6：不得改动 PATHEXT"
        );
    }

    struct TestServer {
        addr: SocketAddr,
        shutdown: Option<std::sync::mpsc::Sender<()>>,
        handle: Option<thread::JoinHandle<()>>,
    }

    impl TestServer {
        fn spawn(handler: Arc<dyn Fn(&str) -> HttpResponse + Send + Sync + 'static>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            listener
                .set_nonblocking(true)
                .expect("set nonblocking listener");
            let addr = listener.local_addr().expect("local addr");
            let (tx, rx) = std::sync::mpsc::channel::<()>();

            let handle = thread::spawn(move || loop {
                if rx.try_recv().is_ok() {
                    break;
                }

                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // 同上：非阻塞 listener 的 accepted socket 会继承非阻塞模式，
                        // 必须显式恢复阻塞读，否则请求未到就 WouldBlock 而 panic。
                        stream.set_nonblocking(false).expect("restore blocking read");
                        let mut buffer = [0_u8; 4096];
                        let size = stream.read(&mut buffer).expect("read request");
                        let request = String::from_utf8_lossy(&buffer[..size]).into_owned();
                        let response = handler(&request);
                        stream
                            .write_all(response.to_bytes().as_slice())
                            .expect("write response");
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("server accept failed: {error}"),
                }
            });

            Self {
                addr,
                shutdown: Some(tx),
                handle: Some(handle),
            }
        }

        fn addr(&self) -> SocketAddr {
            self.addr
        }
    }

    impl Drop for TestServer {
        fn drop(&mut self) {
            if let Some(tx) = self.shutdown.take() {
                let _ = tx.send(());
            }
            if let Some(handle) = self.handle.take() {
                handle.join().expect("join test server");
            }
        }
    }

    struct HttpResponse {
        status: u16,
        reason: &'static str,
        content_type: &'static str,
        body: String,
    }

    impl HttpResponse {
        fn html(status: u16, reason: &'static str, body: &str) -> Self {
            Self {
                status,
                reason,
                content_type: "text/html; charset=utf-8",
                body: body.to_string(),
            }
        }

        fn text(status: u16, reason: &'static str, body: &str) -> Self {
            Self {
                status,
                reason,
                content_type: "text/plain; charset=utf-8",
                body: body.to_string(),
            }
        }

        fn to_bytes(&self) -> Vec<u8> {
            format!(
                "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                self.status,
                self.reason,
                self.content_type,
                self.body.len(),
                self.body
            )
            .into_bytes()
        }
    }
}
