//! Web 聊天插件工具接线：目录、定义和执行均按冻结工程与已启用清单核对。
use super::*;
use plugins::PluginTool;

const PREFIX: &str = "plugin__";

fn exposed_name(name: &str) -> Result<String, String> {
    let exposed = format!("{PREFIX}{name}");
    if exposed.len() > 64 || !exposed.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-') {
        return Err(format!("插件工具名 `{name}` 不符合模型函数名称规则（字母、数字、下划线或连字符，至多 {} 字符）", 64 - PREFIX.len()));
    }
    Ok(exposed)
}

fn loaded_tools(workspace: &Path) -> Result<Vec<PluginTool>, String> {
    extension_market::manager(workspace).map_err(|error| format!("{error:?}"))?
        .aggregated_tools().map_err(|error| format!("插件工具加载失败：{error}"))
}

pub(super) fn definitions(workspace: &Path, existing: &[ToolDefinition]) -> Result<Vec<ToolDefinition>, String> {
    let mut seen = existing.iter().map(|tool| tool.name.clone()).collect::<HashSet<_>>();
    let mut result = Vec::new();
    for tool in loaded_tools(workspace)? {
        let name = exposed_name(&tool.definition().name)?;
        if name.starts_with("plugin__devin_") {
            return Err("外部插件使用了内置 Devin 工具保留名称，当前插件工具未加载".into());
        }
        if !seen.insert(name.clone()) {
            return Err(format!("插件工具 `{name}` 与已有模型工具重名，当前插件工具未加载"));
        }
        result.push(ToolDefinition { name, description: tool.definition().description.clone(),
            input_schema: tool.definition().input_schema.clone() });
    }
    for tool in devin_plugin::definitions(workspace) {
        if !seen.insert(tool.name.clone()) { return Err("Devin 工具与已有工具重名，未加载".into()); }
        result.push(tool);
    }
    let dsh = dsh_web::model_bindings(workspace).unwrap_or_else(|error| {
        diag_log(&format!("[DSH-TOOLS] 当前工具未加载：{error}")); BTreeMap::new()
    });
    for binding in dsh.into_values() {
        if !seen.insert(binding.definition.name.clone()) { return Err("DSH工具与现有工具重名，未加载".into()); }
        result.push(binding.definition);
    }
    Ok(result)
}

pub(super) fn is_plugin_name(name: &str) -> bool { name.starts_with(PREFIX) || name.starts_with(dsh_execution::PREFIX) }

pub(super) fn executor(workspace: &Path, exposed: &str) -> Result<Option<Arc<dyn ToolInvocationExecutor>>, String> {
    if exposed.starts_with("plugin__devin_") { return devin_plugin::executor(workspace, exposed); }
    let Some(raw) = exposed.strip_prefix(PREFIX) else { return Ok(None); };
    let selected = loaded_tools(workspace)?.into_iter().filter(|tool| tool.definition().name == raw)
        .collect::<Vec<_>>();
    let selected = match selected.as_slice() {
        [only] => only,
        [] => return Err(format!("插件工具 `{exposed}` 在当前工程未启用，调用已拒绝")),
        _ => return Err(format!("插件工具 `{exposed}` 在当前工程重名，调用已拒绝")),
    };
    if exposed_name(&selected.definition().name)? != exposed {
        return Err(format!("插件工具 `{exposed}` 名称无效，调用已拒绝"));
    }
    Ok(Some(Arc::new(PluginExecutor { workspace: workspace.to_path_buf(),
        name: exposed.to_string(), plugin_id: selected.plugin_id().to_string() })))
}

struct PluginExecutor { workspace: PathBuf, name: String, plugin_id: String }

impl ToolInvocationExecutor for PluginExecutor {
    fn handles(&self, name: &str) -> bool { name == self.name }

    fn execute(&self, invoke: &ToolInvoke) -> ToolOutcome {
        let start = Instant::now();
        let mut interruption = None;
        let result = (|| -> Result<String, String> {
            if invoke.workspace_id != workspace_identity(&self.workspace) {
                return Err("插件调用的工程身份已变化，未执行".into());
            }
            // 权限闸门放行后再次读当前工程启用状态，停用与卸载立即拒绝新执行。
            let raw = self.name.strip_prefix(PREFIX).ok_or("插件工具名称无效")?;
            let matching = loaded_tools(&self.workspace)?.into_iter()
                .filter(|tool| tool.definition().name == raw).collect::<Vec<_>>();
            let tool = match matching.as_slice() {
                [only] if only.plugin_id() == self.plugin_id => only,
                [] => return Err("插件已停用或卸载，未执行".into()),
                _ => return Err("插件工具重名或来源变更，未执行".into()),
            };
            let input = invoke.input.to_string();
            let mut command = tool.process_command(&invoke.input);
            let result = runtime::managed_process::output_with_input(&mut command,
                Some(Duration::from_secs(60)), Some(input.into_bytes()))
                .map_err(|error| format!("插件进程执行失败：{error}"))?;
            if let Some(reason) = result.interruption {
                interruption = Some(reason);
                return Err(format!("插件进程已中止：{reason:?}"));
            }
            if !result.output.status.success() {
                let message = runtime::managed_process::decode_console_output(&result.output.stderr);
                return Err(format!("插件退出码 {}：{}", result.output.status,
                    message.trim().chars().take(640).collect::<String>()));
            }
            Ok(runtime::managed_process::decode_console_output(&result.output.stdout).trim().to_string())
        })();
        let (status, output, summary_text) = match result {
            Ok(text) => (ToolOutcomeStatus::Ok, JsonValue::String(text), format!("插件工具 {} 已完成", self.name)),
            Err(error) => {
                let (status, code) = match interruption {
                    Some(runtime::managed_process::Interruption::TimedOut) => (ToolOutcomeStatus::Timeout, "timed_out"),
                    Some(runtime::managed_process::Interruption::Cancelled) => (ToolOutcomeStatus::Failed, "cancelled"),
                    None => (ToolOutcomeStatus::Failed, "execution_failed"),
                };
                (status, json!({"code": code, "error": error}), format!("插件工具 {} 未完成：{error}", self.name))
            },
        };
        ToolOutcome { call_id: invoke.call_id.clone(), tool_name: invoke.tool_name.clone(), status,
            output, summary_text, elapsed_ms: start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            permission_gate: runtime::PermissionGateReport::deny(PermissionMode::DangerFullAccess,
                "placeholder-overwritten-by-runtime"), evidence: None }
    }
}
