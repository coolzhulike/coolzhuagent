//! 模型工具派发服务：负责身份接纳、冻结预算/取消和持久收尾。
//! 复用既有事实/执行模块，不拥有第二份状态，不另起任务改变task-local作用域。
use std::sync::Arc;
use axum::http::StatusCode;
use super::{
    api_error, computer_use_context_incomplete_response, default_session_sqlite_path,
    devin_acp, dsh_execution, host_child_agent, random_hex_identifier,
    register_tool_call_at_dispatch, root_execution_budget, run_model_tool_dispatch_within_root,
    tool_invocation_identity, tool_loop_coordinator, tool_turn_cancellation_registry,
    truncate_tool_result_for_context, ApiResult, ChatTurnCancellation, FrozenParentContext,
    JsonValue, ToolDispatchResponse, ToolTurnCancellationScope, CHAT_CANCELLATION,
};

pub(super) struct DispatchRequest<'a> {
    pub name: &'a str,
    pub input: &'a JsonValue,
    pub caller_session_id: Option<&'a str>,
    pub provider_tool_call_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    pub chat_room_id: Option<&'a str>,
    pub parent: Option<&'a FrozenParentContext>,
    pub host_scope: Option<&'a host_child_agent::HostToolScope>,
}

pub(super) struct ToolDispatchService;
impl ToolDispatchService {
    pub(super) async fn dispatch(request: DispatchRequest<'_>) -> ApiResult<ToolDispatchResponse> {
        let DispatchRequest { name, input, caller_session_id, provider_tool_call_id,
            turn_id, chat_room_id, parent, host_scope } = request;
    // 缺必需 CU 来源时先拒绝，不访问默认数据库或登记匿名工具调用。
    if name.starts_with(dsh_execution::PREFIX) && (parent.is_none()
        || provider_tool_call_id.is_none_or(|id| id.trim().is_empty())
        || caller_session_id.is_none_or(|id| id.trim().is_empty())
        || chat_room_id.is_none_or(|id| id.trim().is_empty())
        || turn_id.is_none_or(|id| id.trim().is_empty())) {
        return Err(api_error(StatusCode::CONFLICT, "DSH缺少真实父轮/provider/会话/聊天室/轮次身份，未登记匿名调用"));
    }
    let normalized = name.replace('-', "_");
    if normalized == "computer_use.perform" || normalized == "computer_use_perform" {
        let missing_call = provider_tool_call_id.is_none_or(|value| value.trim().is_empty());
        let missing_session = caller_session_id.is_none_or(|value| value.trim().is_empty());
        let missing_turn = turn_id.is_none_or(|value| value.trim().is_empty());
        if missing_call || missing_session || missing_turn {
            return Ok(computer_use_context_incomplete_response(missing_call, missing_session, missing_turn));
        }
    }
    // 冻结父上下文和当前作用域都可能缩短预算，必须沿用较早的截止。
    let budget = parent.and_then(|parent| parent.root_budget.clone()).into_iter()
        .chain(root_execution_budget::current()).min_by_key(|budget| budget.remaining());
    if budget.as_ref().is_some_and(|budget| budget.is_expired()) {
        return Err(api_error(StatusCode::REQUEST_TIMEOUT, root_execution_budget::EXPIRED_REASON));
    }
    let existing = match host_scope {
        Some(host_child_agent::HostToolScope::Child(child)) => Some(Arc::clone(&child.cancellation)),
        _ => turn_id.and_then(|trace| tool_turn_cancellation_registry().lock().ok().and_then(|entries| entries.get(trace).cloned()))
            .or_else(|| CHAT_CANCELLATION.try_with(Arc::clone).ok()),
    };
    let cancellation = existing.clone().unwrap_or_else(|| Arc::new(ChatTurnCancellation::new()));
    if cancellation.is_requested() { return Err(api_error(StatusCode::CONFLICT, "本轮已停止，未派发新工具")); }
    // 宿主/冻结权限预检先于身份登记与一切 hook：宿主拒绝时登记/hook/工具全部零调用。
    super::model_tool_host_preflight(name, input, caller_session_id, turn_id, chat_room_id, parent, host_scope)?;
    let registry_path = parent.and_then(|parent| parent.goal_phase.as_ref()).map(|goal| goal.db_path().to_path_buf())
        .or_else(|| parent.and_then(|parent| parent.runtime_db_path.clone())).unwrap_or_else(default_session_sqlite_path);
    // provider 编号可跨模型请求复用；宿主身份包含真实run与回复作用域，参数变化不能换执行资格。
    let source = match provider_tool_call_id.filter(|id| !id.trim().is_empty()) {
        Some(raw_id) => {
            let request_key = tool_invocation_identity::current()
                .or_else(|| turn_id.map(|trace| format!("{trace}/direct-dispatch")))
                .ok_or_else(|| api_error(StatusCode::CONFLICT, "模型工具缺少明确来源请求，未执行"))?;
            Some(tool_invocation_identity::ModelToolIdentity::from_source(
                parent.and_then(|parent| parent.parent_run_id.as_deref()), &request_key, raw_id)
                .map_err(|error| api_error(StatusCode::CONFLICT, &error))?)
        },
        None => None,
    };
    let registration_id = match &source {
        Some(source) => source.execution_id.clone(),
        None => format!("host-tool-{}", random_hex_identifier(24, "tool invocation")
            .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR, &error))?),
    };
    let mut settlement = register_tool_call_at_dispatch(&registry_path, &registration_id, name, input,
        parent.and_then(|parent| parent.parent_run_id.as_deref()), budget.clone(), source.as_ref())?;
    let _scope = if existing.is_none() { turn_id.map(|trace| ToolTurnCancellationScope::install(trace, cancellation.clone())) } else { None };
    // CU/事实层的既有 provider_tool_call_id 字段兼容保存宿主执行id；raw值只作关联并已单独持久化。
    let execution_call_id = source.as_ref().map(|source| source.execution_id.as_str());
    // §7.3 hook 接线：置于身份接纳/重复登记拒绝之后、目标执行之前——宿主拒绝、取消或重复身份
    // 在登记处已返回，hook 与工具零调用；configured 不等于 authorized，缺独立授权的 hook 不运行且不得当作放行凭据。
    let hooks = dispatch_hook_runner(parent);
    let hook_input = input.to_string();
    let mut outcome = {
        match run_dispatch_hook(hooks.clone(), name, &hook_input, None, budget.as_ref(), &cancellation).await {
        Err(error) => Err(error),
        Ok(pre) if pre.is_denied() => {
            Err(api_error(StatusCode::FORBIDDEN, &format!(
                "pre_tool_use hook 拒绝 `{name}`，目标未执行：{}", pre.messages().join("；"))))
        }
        Ok(_) => {
            // pre 若真正运行过（executed_commands>0），旧批准不复用：目标仍须原样通过
            // within_root 的冻结快照/宿主范围/语义写扩展门禁重判。
            // 巨型分发future放到堆上，避免多层冻结上下文在默认线程栈上溢出；仍沿用同一预算和取消。
            let run = Box::pin(run_model_tool_dispatch_within_root(name, input, caller_session_id, execution_call_id, turn_id, chat_room_id, parent, host_scope,
                source.as_ref().map(|source| source.provider_tool_call_id.as_str()), &mut settlement));
            let controlled = async {
                super::await_chat_turn(&cancellation, run).await
                    .map_err(|_|api_error(StatusCode::CONFLICT,"执行已停止等待，可能发生的效果须核查轨迹"))?
            };
            if let Some(budget) = &budget {
                match root_execution_budget::scope(budget.clone(), budget.run_cancellable(controlled, || { cancellation.request_timeout(); })).await {
                    Ok(result) => result,
                    Err(error) => Err(api_error(StatusCode::REQUEST_TIMEOUT, &error.to_string())),
                }
            } else { controlled.await }
        }
        }
    };
    // post-hook 仅目标派发产生回执后运行；pre 拒绝/门禁拒绝/取消均无回执，不触发。
    if let Ok(result) = &outcome {
        let post = run_dispatch_hook(hooks, name, &hook_input,
            Some((result.tool_result_text.clone().unwrap_or_default(),
                tool_loop_coordinator::terminal_status_is_error(&result.status))),
            budget.as_ref(), &cancellation).await;
        match post {
            Err(error) => outcome = Err(error),
            Ok(post) if post.is_denied() => {
                outcome = Err(api_error(StatusCode::FORBIDDEN, &format!(
                    "post_tool_use hook 拒绝 `{name}`，目标回执已生成，不可自动重试：{}", post.messages().join("；"))));
            }
            Ok(_) => {}
        }
    }
    let status = match &outcome {
        Ok(result) if name.starts_with(dsh_execution::PREFIX)
            && result.route == "runtime-dry-run"
            && result.dispatch_plan.as_ref().is_some_and(|plan| plan.audit.requires_human_confirmation) => "awaiting_approval",
        Ok(result) if !tool_loop_coordinator::terminal_status_is_error(&result.status) => "completed",
        _ => "failed",
    };
    if !settlement.is_settled() {
        settlement.finish(status).map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,
            &format!("工具已结束等待，但终态记录未确认：{error}；不可自动重试")))?;
    }
    // ACP 由外层桥对完整结构化回执投影一次，避免内层先截断或重复保存带提示的原文。
    if devin_acp::bridge::current().is_none() {
        if let Ok(response) = outcome.as_mut() {
            if let Some(text) = response.tool_result_text.take() {
                // 此入口没有本轮实际声明的读取能力证明，不能默认交回未开放的续读指针。
                response.tool_result_text = Some(truncate_tool_result_for_context(text, parent, false));
            }
        }
    }
    outcome

    }
}

struct HookWaitGuard(runtime::managed_process::ExecutionControl);
impl Drop for HookWaitGuard {
    fn drop(&mut self) { self.0.cancel(); }
}

/// 同步 hook 在受控 worker 中执行；不阻塞异步取消，也不新建根预算或继承目标授权。
/// 正常返回前等待真实进程回收；外层等待被丢弃时，Drop 发取消，收尾任务继续接回执。
async fn run_dispatch_hook(
    hooks: runtime::HookRunner, name: &str, input: &str, output: Option<(String, bool)>,
    budget: Option<&root_execution_budget::RootExecutionBudget>, cancellation: &Arc<ChatTurnCancellation>,
) -> ApiResult<runtime::HookRunResult> {
    let stage = if output.is_some() { "后置 hook（目标回执已生成，不可自动重试）" } else { "前置 hook（目标未执行）" };
    let stopped = || {
        if budget.is_some_and(|budget| budget.is_expired()) {
            cancellation.request_timeout();
            Some(api_error(StatusCode::REQUEST_TIMEOUT, &format!("{stage}：{}", root_execution_budget::EXPIRED_REASON)))
        } else if cancellation.is_requested() {
            Some(api_error(StatusCode::CONFLICT, &format!("{stage}：本轮已停止")))
        } else { None }
    };
    if let Some(error) = stopped() { return Err(error); }
    // 生产默认没有配置或授权，不为两个空 hook 新建 blocking worker。
    if hooks == runtime::HookRunner::default() { return Ok(runtime::HookRunResult::allow(Vec::new())); }
    let token = cancellation.clone();
    let control = runtime::managed_process::ExecutionControl::new(
        budget.map(|budget| budget.remaining()), Some(Arc::new(move || token.is_requested())));
    let _guard = HookWaitGuard(control.clone());
    let name = name.to_owned();
    let input = input.to_owned();
    let worker = tokio::task::spawn_blocking(move || {
        runtime::managed_process::with_execution_control(control, || match output {
            Some((output, is_error)) => hooks.run_post_tool_use(&name, &input, &output, is_error),
            None => hooks.run_pre_tool_use(&name, &input),
        })
    });
    let (sender, receiver) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let result = worker.await.map_err(|error| error.to_string());
        if let Err(late) = sender.send(result) {
            tracing::info!(worker_joined = late.is_ok(), stage, "hook 等待结束后的真实收尾已收取");
        }
    });
    let result = receiver.await.map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,
        &format!("{stage} 监督回执丢失，结果未知：{error}")))?
        .map_err(|error| api_error(StatusCode::INTERNAL_SERVER_ERROR,
            &format!("{stage} worker 异常退出：{error}")))?;
    if let Some(error) = stopped() { return Err(error); }
    Ok(result)
}

/// 正式配置在服务启动时由宿主冻结；父工作区不匹配时不会借用当前工程的授权。
fn dispatch_hook_runner(parent: Option<&FrozenParentContext>) -> runtime::HookRunner {
    dispatch_hook_runner_override().unwrap_or_else(|| {
        let workspace_id = parent.map(|parent| parent.workspace_id.as_str().to_owned())
            .unwrap_or_else(|| super::workspace_identity(&super::active_workspace_path()));
        super::dispatch_hooks::runner(&workspace_id)
    })
}

#[cfg(test)]
fn dispatch_hook_runner_override() -> Option<runtime::HookRunner> {
    DISPATCH_HOOK_RUNNER_OVERRIDE.lock().ok().and_then(|slot| slot.clone())
}

#[cfg(not(test))]
fn dispatch_hook_runner_override() -> Option<runtime::HookRunner> {
    None
}

/// 仅测试用：注入具独立授权的 runner，以判别性验收服务级 hook 语义。
#[cfg(test)]
pub(super) static DISPATCH_HOOK_RUNNER_OVERRIDE: std::sync::Mutex<Option<runtime::HookRunner>> =
    std::sync::Mutex::new(None);
