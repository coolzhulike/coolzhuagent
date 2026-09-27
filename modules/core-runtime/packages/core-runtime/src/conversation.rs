use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

use crate::agent_event::{AgentEvent, AgentEventKind, ContextSnapshot, ItemId, ThreadId, TurnId};
use crate::compact::{
    compact_session, estimate_session_tokens, CompactionConfig, CompactionResult,
};
use crate::config::RuntimeFeatureConfig;
use crate::hooks::{HookRunResult, HookRunner};
use crate::permissions::{PermissionOutcome, PermissionPolicy, PermissionPrompter};
use crate::session::{ContentBlock, ConversationMessage, Session};
use crate::usage::{TokenUsage, UsageTracker};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest {
    pub system_prompt: Vec<String>,
    pub messages: Vec<ConversationMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantEvent {
    TextDelta(String),
    /// 可展示的模型思考摘要增量。`redacted` 用于标记 provider 只提供了
    /// 脱敏/加密思考块，调用方不应把它当作普通可见文本展示。
    ReasoningDelta {
        text: String,
        redacted: bool,
    },
    ToolUse {
        id: String,
        name: String,
        input: String,
    },
    Usage(TokenUsage),
    MessageStop,
}

pub trait ApiClient {
    fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError>;
}

pub trait ToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    message: String,
}

impl ToolError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for ToolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ToolError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    message: String,
}

impl RuntimeError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for RuntimeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RuntimeError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnSummary {
    pub thread_id: ThreadId,
    pub turn_id: TurnId,
    pub context_snapshot: ContextSnapshot,
    pub events: Vec<AgentEvent>,
    pub assistant_messages: Vec<ConversationMessage>,
    pub tool_results: Vec<ConversationMessage>,
    pub iterations: usize,
    pub usage: TokenUsage,
    /// 本轮模型返回的 reasoning summary；不与最终 assistant 文本混合。
    pub reasoning_text: String,
}

pub struct ConversationRuntime<C, T> {
    session: Session,
    api_client: C,
    tool_executor: T,
    permission_policy: PermissionPolicy,
    system_prompt: Vec<String>,
    max_iterations: usize,
    usage_tracker: UsageTracker,
    hook_runner: HookRunner,
    thread_id: ThreadId,
    next_turn_number: u64,
}

impl<C, T> ConversationRuntime<C, T>
where
    C: ApiClient,
    T: ToolExecutor,
{
    #[must_use]
    pub fn new(
        session: Session,
        api_client: C,
        tool_executor: T,
        permission_policy: PermissionPolicy,
        system_prompt: Vec<String>,
    ) -> Self {
        Self::new_with_features(
            session,
            api_client,
            tool_executor,
            permission_policy,
            system_prompt,
            RuntimeFeatureConfig::default(),
        )
    }

    #[must_use]
    pub fn new_with_features(
        session: Session,
        api_client: C,
        tool_executor: T,
        permission_policy: PermissionPolicy,
        system_prompt: Vec<String>,
        feature_config: RuntimeFeatureConfig,
    ) -> Self {
        let usage_tracker = UsageTracker::from_session(&session);
        Self {
            session,
            api_client,
            tool_executor,
            permission_policy,
            system_prompt,
            max_iterations: usize::MAX,
            usage_tracker,
            hook_runner: HookRunner::from_feature_config(&feature_config),
            thread_id: ThreadId::default(),
            next_turn_number: 0,
        }
    }

    #[must_use]
    pub fn with_thread_id(mut self, thread_id: impl Into<String>) -> Self {
        self.thread_id = ThreadId::new(thread_id);
        self
    }

    #[must_use]
    pub fn with_max_iterations(mut self, max_iterations: usize) -> Self {
        self.max_iterations = max_iterations;
        self
    }

    /// 授予 hook 自身的执行授权（§7.3）。
    ///
    /// 首期缺省为**未授权**：没有独立执行授权的 hook 一律不运行——其"未运行"
    /// 事实仍会记入工具结果。hook 是任意 shell，宿主必须显式声明该能力的最低
    /// 权限，例如
    /// `PermissionPolicy::new(PermissionMode::DangerFullAccess)
    /// .with_tool_requirement(HOOK_CAPABILITY_PRE_TOOL_USE, PermissionMode::DangerFullAccess)`。
    #[must_use]
    pub fn with_hook_authorization(mut self, authorization: PermissionPolicy) -> Self {
        self.hook_runner = self.hook_runner.with_authorization(authorization);
        self
    }

    pub fn run_turn(
        &mut self,
        user_input: impl Into<String>,
        mut prompter: Option<&mut dyn PermissionPrompter>,
    ) -> Result<TurnSummary, RuntimeError> {
        self.session
            .messages
            .push(ConversationMessage::user_text(user_input.into()));

        self.next_turn_number = self.next_turn_number.saturating_add(1);
        let turn_id = TurnId::new(format!("turn-{}", self.next_turn_number));
        let context_snapshot = ContextSnapshot::from_session(&self.thread_id, &self.session);
        let mut event_log = vec![
            AgentEvent::new(
                1,
                self.thread_id.clone(),
                turn_id.clone(),
                None,
                AgentEventKind::TurnStarted,
                serde_json::json!({"user_message_count": context_snapshot.user_message_count}),
            ),
            AgentEvent::new(
                2,
                self.thread_id.clone(),
                turn_id.clone(),
                None,
                AgentEventKind::ContextSnapshot,
                context_snapshot.event_payload(),
            ),
        ];
        let mut next_event_sequence = 3;

        let mut assistant_messages = Vec::new();
        let mut tool_results = Vec::new();
        let mut iterations = 0;
        let mut reasoning_text = String::new();

        loop {
            iterations += 1;
            if iterations > self.max_iterations {
                return Err(RuntimeError::new(
                    "conversation loop exceeded the maximum number of iterations",
                ));
            }

            let request = ApiRequest {
                system_prompt: self.system_prompt.clone(),
                messages: self.session.messages.clone(),
            };
            let stream_events = self.api_client.stream(request)?;
            let (assistant_message, usage, iteration_reasoning) =
                build_assistant_message(stream_events)?;
            reasoning_text.push_str(&iteration_reasoning);
            if !iteration_reasoning.is_empty() {
                event_log.push(AgentEvent::new(
                    next_event_sequence,
                    self.thread_id.clone(),
                    turn_id.clone(),
                    None,
                    AgentEventKind::ReasoningDelta,
                    serde_json::json!({"text": iteration_reasoning, "redacted": false}),
                ));
                next_event_sequence = next_event_sequence.saturating_add(1);
            }
            for block in &assistant_message.blocks {
                if let ContentBlock::ToolUse { id, name, input } = block {
                    event_log.push(AgentEvent::new(
                        next_event_sequence,
                        self.thread_id.clone(),
                        turn_id.clone(),
                        Some(ItemId::new(id.clone())),
                        AgentEventKind::ToolCall,
                        serde_json::json!({"name": name, "input": input}),
                    ));
                    next_event_sequence = next_event_sequence.saturating_add(1);
                }
            }
            event_log.push(AgentEvent::new(
                next_event_sequence,
                self.thread_id.clone(),
                turn_id.clone(),
                None,
                AgentEventKind::MessageDone,
                serde_json::json!({"block_count": assistant_message.blocks.len()}),
            ));
            next_event_sequence = next_event_sequence.saturating_add(1);
            if let Some(usage) = usage {
                self.usage_tracker.record(usage);
            }
            let pending_tool_uses = assistant_message
                .blocks
                .iter()
                .filter_map(|block| match block {
                    ContentBlock::ToolUse { id, name, input } => {
                        Some((id.clone(), name.clone(), input.clone()))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();

            self.session.messages.push(assistant_message.clone());
            assistant_messages.push(assistant_message);

            if pending_tool_uses.is_empty() {
                break;
            }

            for (tool_use_id, tool_name, input) in pending_tool_uses {
                // 顺序硬约束（§7.3 保证表）：
                //   宿主预检查（权限闸门）→ pre-tool hook → 目标执行器 → post-tool hook。
                // 闸门**必须先于任何 hook**：已明确拒绝时既不运行 pre-tool shell hook，
                // 也不运行目标执行器，hook 无法把 deny 反转成执行。
                let permission_outcome =
                    authorize_call(&self.permission_policy, &tool_name, &input, &mut prompter);

                let result_message = match permission_outcome {
                    PermissionOutcome::Deny { reason } => {
                        // 宿主 deny 生效：目标执行器零调用，hook 也不运行。
                        ConversationMessage::tool_result(tool_use_id, tool_name, reason, true)
                    }
                    PermissionOutcome::Allow => {
                        // 没有独立执行授权的 hook 不会被运行（其中的事实仍被记录）。
                        let pre_hook_result = self.hook_runner.run_pre_tool_use(&tool_name, &input);
                        if pre_hook_result.is_denied() {
                            // 获授权的 hook deny ⇒ 目标执行器零调用；hook 自身的输出
                            // 仍然记录在结果里，不因为拒绝而丢失。
                            let deny_message = format!("PreToolUse hook denied tool `{tool_name}`");
                            ConversationMessage::tool_result(
                                tool_use_id,
                                tool_name,
                                format_hook_message(&pre_hook_result, &deny_message),
                                true,
                            )
                        } else {
                            // hook 真的跑过 ⇒ 它可能在闸门之后改变了世界（输入/目标/
                            // 路径状态），因此**原审批不得沿用**，必须重新判定；原
                            // 判定是来自用户的批准，本次重判同样会回到该批准通道。
                            // 没有任何 hook 运行时不做重判：中间没有别的东西改变过。
                            let rejudged = (pre_hook_result.executed_commands() > 0)
                                .then(|| {
                                    authorize_call(
                                        &self.permission_policy,
                                        &tool_name,
                                        &input,
                                        &mut prompter,
                                    )
                                });
                            match rejudged {
                                Some(PermissionOutcome::Deny { reason }) => {
                                    ConversationMessage::tool_result(
                                        tool_use_id,
                                        tool_name,
                                        merge_hook_feedback(
                                            pre_hook_result.messages(),
                                            format!(
                                                "PreToolUse hook ran and the previous approval does not carry over; \
                                                 the re-evaluated host decision is deny: {reason}"
                                            ),
                                            true,
                                        ),
                                        true,
                                    )
                                }
                                _ => {
                                    let (mut output, mut is_error) =
                                        match self.tool_executor.execute(&tool_name, &input) {
                                            Ok(output) => (output, false),
                                            Err(error) => (error.to_string(), true),
                                        };
                                    output =
                                        merge_hook_feedback(pre_hook_result.messages(), output, false);

                                    // post-tool hook 只在目标执行器真正跑过之后运行。
                                    let post_hook_result = self.hook_runner.run_post_tool_use(
                                        &tool_name, &input, &output, is_error,
                                    );
                                    if post_hook_result.is_denied() {
                                        is_error = true;
                                    }
                                    output = merge_hook_feedback(
                                        post_hook_result.messages(),
                                        output,
                                        post_hook_result.is_denied(),
                                    );

                                    ConversationMessage::tool_result(
                                        tool_use_id,
                                        tool_name,
                                        output,
                                        is_error,
                                    )
                                }
                            }
                        }
                    }
                };
                self.session.messages.push(result_message.clone());
                tool_results.push(result_message);
            }
        }

        event_log.push(AgentEvent::new(
            next_event_sequence,
            self.thread_id.clone(),
            turn_id.clone(),
            None,
            AgentEventKind::TurnCompleted,
            serde_json::json!({
                "iterations": iterations,
                "reasoning_chars": reasoning_text.chars().count(),
            }),
        ));

        Ok(TurnSummary {
            thread_id: self.thread_id.clone(),
            turn_id,
            context_snapshot,
            events: event_log,
            assistant_messages,
            tool_results,
            iterations,
            usage: self.usage_tracker.cumulative_usage(),
            reasoning_text,
        })
    }

    #[must_use]
    pub fn compact(&self, config: CompactionConfig) -> CompactionResult {
        compact_session(&self.session, config)
    }

    #[must_use]
    pub fn estimated_tokens(&self) -> usize {
        estimate_session_tokens(&self.session)
    }

    #[must_use]
    pub fn usage(&self) -> &UsageTracker {
        &self.usage_tracker
    }

    #[must_use]
    pub fn session(&self) -> &Session {
        &self.session
    }

    #[must_use]
    pub fn into_session(self) -> Session {
        self.session
    }
}

fn build_assistant_message(
    events: Vec<AssistantEvent>,
) -> Result<(ConversationMessage, Option<TokenUsage>, String), RuntimeError> {
    let mut text = String::new();
    let mut reasoning_text = String::new();
    let mut blocks = Vec::new();
    let mut finished = false;
    let mut usage = None;

    for event in events {
        match event {
            AssistantEvent::TextDelta(delta) => text.push_str(&delta),
            AssistantEvent::ReasoningDelta { text, redacted } => {
                if !redacted {
                    reasoning_text.push_str(&text);
                }
            }
            AssistantEvent::ToolUse { id, name, input } => {
                flush_text_block(&mut text, &mut blocks);
                blocks.push(ContentBlock::ToolUse { id, name, input });
            }
            AssistantEvent::Usage(value) => usage = Some(value),
            AssistantEvent::MessageStop => {
                finished = true;
            }
        }
    }

    flush_text_block(&mut text, &mut blocks);

    if !finished {
        return Err(RuntimeError::new(
            "assistant stream ended without a message stop event",
        ));
    }
    if blocks.is_empty() {
        return Err(RuntimeError::new("assistant stream produced no content"));
    }

    Ok((
        ConversationMessage::assistant_with_usage(blocks, usage),
        usage,
        reasoning_text,
    ))
}

fn flush_text_block(text: &mut String, blocks: &mut Vec<ContentBlock>) {
    if !text.is_empty() {
        blocks.push(ContentBlock::Text {
            text: std::mem::take(text),
        });
    }
}

/// 单一授权入口。
///
/// 宿主预检查与 hook 之后的重判都必须经由这里，避免两处判定逻辑各自漂移
/// （重判若走另一条路径，就可能比原判定更宽松）。
fn authorize_call(
    policy: &PermissionPolicy,
    tool_name: &str,
    input: &str,
    prompter: &mut Option<&mut dyn PermissionPrompter>,
) -> PermissionOutcome {
    if let Some(prompt) = prompter.as_mut() {
        policy.authorize(tool_name, input, Some(*prompt))
    } else {
        policy.authorize(tool_name, input, None)
    }
}

fn format_hook_message(result: &HookRunResult, fallback: &str) -> String {
    if result.messages().is_empty() {
        fallback.to_string()
    } else {
        result.messages().join("\n")
    }
}

fn merge_hook_feedback(messages: &[String], output: String, denied: bool) -> String {
    if messages.is_empty() {
        return output;
    }

    let mut sections = Vec::new();
    if !output.trim().is_empty() {
        sections.push(output);
    }
    let label = if denied {
        "Hook feedback (denied)"
    } else {
        "Hook feedback"
    };
    sections.push(format!("{label}:\n{}", messages.join("\n")));
    sections.join("\n\n")
}

type ToolHandler = Box<dyn FnMut(&str) -> Result<String, ToolError>>;

#[derive(Default)]
pub struct StaticToolExecutor {
    handlers: BTreeMap<String, ToolHandler>,
}

impl StaticToolExecutor {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn register(
        mut self,
        tool_name: impl Into<String>,
        handler: impl FnMut(&str) -> Result<String, ToolError> + 'static,
    ) -> Self {
        self.handlers.insert(tool_name.into(), Box::new(handler));
        self
    }
}

impl ToolExecutor for StaticToolExecutor {
    fn execute(&mut self, tool_name: &str, input: &str) -> Result<String, ToolError> {
        self.handlers
            .get_mut(tool_name)
            .ok_or_else(|| ToolError::new(format!("unknown tool: {tool_name}")))?(input)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApiClient, ApiRequest, AssistantEvent, ConversationRuntime, RuntimeError,
        StaticToolExecutor, ToolError, TurnSummary,
    };
    use crate::compact::CompactionConfig;
    use crate::config::{RuntimeFeatureConfig, RuntimeHookConfig};
    use crate::hooks::test_support::CountingHook;
    use crate::hooks::{HOOK_CAPABILITY_POST_TOOL_USE, HOOK_CAPABILITY_PRE_TOOL_USE};
    use crate::permissions::{
        PermissionMode, PermissionPolicy, PermissionPromptDecision, PermissionPrompter,
        PermissionRequest,
    };
    use crate::prompt::{ProjectContext, SystemPromptBuilder};
    use crate::session::{ContentBlock, MessageRole, Session};
    use crate::usage::TokenUsage;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// hook 自身的独立执行授权（§7.3）。
    ///
    /// hook 是任意 shell，最低权限按 `DangerFullAccess` 声明；两个能力分开声明，
    /// 便于验证"授权 pre 不等于授权 post"。
    fn hook_authorization() -> PermissionPolicy {
        PermissionPolicy::new(PermissionMode::DangerFullAccess)
            .with_tool_requirement(HOOK_CAPABILITY_PRE_TOOL_USE, PermissionMode::DangerFullAccess)
            .with_tool_requirement(HOOK_CAPABILITY_POST_TOOL_USE, PermissionMode::DangerFullAccess)
    }

    /// 记录调用次数的工具执行器（"目标执行器零调用"的可断言证据）。
    #[derive(Clone, Default)]
    struct CallCounter {
        calls: Arc<AtomicUsize>,
    }

    impl CallCounter {
        fn new() -> Self {
            Self::default()
        }

        fn count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        fn handler(
            &self,
            output: &'static str,
        ) -> impl FnMut(&str) -> Result<String, ToolError> + 'static {
            let calls = Arc::clone(&self.calls);
            move |_input| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(output.to_string())
            }
        }
    }

    /// 脚本化审批应答器：按顺序给出预设决定，并记录被询问次数。
    ///
    /// 询问次数是"审批是否被沿用/是否重新判定"的直接证据。
    struct ScriptedPrompter {
        decisions: std::collections::VecDeque<PermissionPromptDecision>,
        seen: usize,
    }

    impl ScriptedPrompter {
        fn new(decisions: Vec<PermissionPromptDecision>) -> Self {
            Self {
                decisions: decisions.into(),
                seen: 0,
            }
        }

        fn seen(&self) -> usize {
            self.seen
        }
    }

    impl PermissionPrompter for ScriptedPrompter {
        fn decide(&mut self, _request: &PermissionRequest) -> PermissionPromptDecision {
            self.seen += 1;
            self.decisions
                .pop_front()
                .expect("宿主询问审批的次数多于脚本预设")
        }
    }

    /// 一次工具调用的模型客户端：先返回 ToolUse，看到工具结果后返回纯文本。
    struct OneToolUseApiClient {
        tool_name: &'static str,
        tool_input: &'static str,
    }

    impl ApiClient for OneToolUseApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            if request
                .messages
                .iter()
                .any(|message| message.role == MessageRole::Tool)
            {
                return Ok(vec![
                    AssistantEvent::TextDelta("done".to_string()),
                    AssistantEvent::MessageStop,
                ]);
            }
            Ok(vec![
                AssistantEvent::ToolUse {
                    id: "tool-1".to_string(),
                    name: self.tool_name.to_string(),
                    input: self.tool_input.to_string(),
                },
                AssistantEvent::MessageStop,
            ])
        }
    }

    /// 取唯一的工具结果（output, is_error）。
    fn single_tool_result(summary: &TurnSummary) -> (String, bool) {
        assert_eq!(summary.tool_results.len(), 1, "expected exactly one tool result");
        let ContentBlock::ToolResult {
            output, is_error, ..
        } = &summary.tool_results[0].blocks[0]
        else {
            panic!("expected tool result block");
        };
        (output.clone(), *is_error)
    }

    struct ScriptedApiClient {
        call_count: usize,
    }

    impl ApiClient for ScriptedApiClient {
        fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
            self.call_count += 1;
            match self.call_count {
                1 => {
                    assert!(request
                        .messages
                        .iter()
                        .any(|message| message.role == MessageRole::User));
                    Ok(vec![
                        AssistantEvent::TextDelta("Let me calculate that.".to_string()),
                        AssistantEvent::ToolUse {
                            id: "tool-1".to_string(),
                            name: "add".to_string(),
                            input: "2,2".to_string(),
                        },
                        AssistantEvent::Usage(TokenUsage {
                            input_tokens: 20,
                            output_tokens: 6,
                            cache_creation_input_tokens: 1,
                            cache_read_input_tokens: 2,
                        }),
                        AssistantEvent::MessageStop,
                    ])
                }
                2 => {
                    let last_message = request
                        .messages
                        .last()
                        .expect("tool result should be present");
                    assert_eq!(last_message.role, MessageRole::Tool);
                    Ok(vec![
                        AssistantEvent::TextDelta("The answer is 4.".to_string()),
                        AssistantEvent::Usage(TokenUsage {
                            input_tokens: 24,
                            output_tokens: 4,
                            cache_creation_input_tokens: 1,
                            cache_read_input_tokens: 3,
                        }),
                        AssistantEvent::MessageStop,
                    ])
                }
                _ => Err(RuntimeError::new("unexpected extra API call")),
            }
        }
    }

    struct PromptAllowOnce;

    impl PermissionPrompter for PromptAllowOnce {
        fn decide(&mut self, request: &PermissionRequest) -> PermissionPromptDecision {
            assert_eq!(request.tool_name, "add");
            PermissionPromptDecision::Allow
        }
    }

    #[test]
    fn runs_user_to_tool_to_result_loop_end_to_end_and_tracks_usage() {
        let api_client = ScriptedApiClient { call_count: 0 };
        let tool_executor = StaticToolExecutor::new().register("add", |input| {
            let total = input
                .split(',')
                .map(|part| part.parse::<i32>().expect("input must be valid integer"))
                .sum::<i32>();
            Ok(total.to_string())
        });
        // 工具必须显式声明最低权限：未声明的工具按配置错误 fail-closed。
        let permission_policy = PermissionPolicy::new(PermissionMode::WorkspaceWrite)
            .with_tool_requirement("add", PermissionMode::ReadOnly);
        let system_prompt = SystemPromptBuilder::new()
            .with_project_context(ProjectContext {
                cwd: PathBuf::from("/tmp/project"),
                current_date: "2026-03-31".to_string(),
                git_status: None,
                git_diff: None,
                instruction_files: Vec::new(),
            })
            .with_os("linux", "6.8")
            .build();
        let mut runtime = ConversationRuntime::new(
            Session::new(),
            api_client,
            tool_executor,
            permission_policy,
            system_prompt,
        );

        let summary = runtime
            .run_turn("what is 2 + 2?", Some(&mut PromptAllowOnce))
            .expect("conversation loop should succeed");

        assert_eq!(summary.iterations, 2);
        assert_eq!(summary.assistant_messages.len(), 2);
        assert_eq!(summary.tool_results.len(), 1);
        assert_eq!(runtime.session().messages.len(), 4);
        assert_eq!(summary.usage.output_tokens, 10);
        assert!(matches!(
            runtime.session().messages[1].blocks[1],
            ContentBlock::ToolUse { .. }
        ));
        assert!(matches!(
            runtime.session().messages[2].blocks[0],
            ContentBlock::ToolResult {
                is_error: false,
                ..
            }
        ));
    }

    #[test]
    fn preserves_reasoning_summary_separately_from_final_answer() {
        struct ReasoningApi;

        impl ApiClient for ReasoningApi {
            fn stream(
                &mut self,
                _request: ApiRequest,
            ) -> Result<Vec<AssistantEvent>, RuntimeError> {
                Ok(vec![
                    AssistantEvent::ReasoningDelta {
                        text: "先检查输入，再给出结论。".to_string(),
                        redacted: false,
                    },
                    AssistantEvent::ReasoningDelta {
                        text: "[provider redacted]".to_string(),
                        redacted: true,
                    },
                    AssistantEvent::TextDelta("结论已就绪。".to_string()),
                    AssistantEvent::MessageStop,
                ])
            }
        }

        let mut runtime = ConversationRuntime::new(
            Session::new(),
            ReasoningApi,
            StaticToolExecutor::new(),
            PermissionPolicy::new(PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
        );

        let summary = runtime
            .run_turn("请分析", None)
            .expect("reasoning turn should succeed");

        assert_eq!(summary.reasoning_text, "先检查输入，再给出结论。");
        assert_eq!(
            summary.assistant_messages[0].blocks,
            vec![ContentBlock::Text {
                text: "结论已就绪。".to_string()
            }]
        );
        assert_eq!(summary.thread_id.as_str(), "thread-default");
        assert_eq!(summary.turn_id.as_str(), "turn-1");
        assert_eq!(summary.context_snapshot.user_message_count, 1);
        assert!(summary
            .events
            .iter()
            .any(|event| event.kind == crate::agent_event::AgentEventKind::ReasoningDelta));
        assert_eq!(
            summary.events.last().map(|event| event.kind),
            Some(crate::agent_event::AgentEventKind::TurnCompleted)
        );
    }

    #[test]
    fn records_denied_tool_results_when_prompt_rejects() {
        struct RejectPrompter;
        impl PermissionPrompter for RejectPrompter {
            fn decide(&mut self, _request: &PermissionRequest) -> PermissionPromptDecision {
                PermissionPromptDecision::Deny {
                    reason: "not now".to_string(),
                }
            }
        }

        struct SingleCallApiClient;
        impl ApiClient for SingleCallApiClient {
            fn stream(&mut self, request: ApiRequest) -> Result<Vec<AssistantEvent>, RuntimeError> {
                if request
                    .messages
                    .iter()
                    .any(|message| message.role == MessageRole::Tool)
                {
                    return Ok(vec![
                        AssistantEvent::TextDelta("I could not use the tool.".to_string()),
                        AssistantEvent::MessageStop,
                    ]);
                }
                Ok(vec![
                    AssistantEvent::ToolUse {
                        id: "tool-1".to_string(),
                        name: "blocked".to_string(),
                        input: "secret".to_string(),
                    },
                    AssistantEvent::MessageStop,
                ])
            }
        }

        let mut runtime = ConversationRuntime::new(
            Session::new(),
            SingleCallApiClient,
            StaticToolExecutor::new(),
            // 显式声明为需要 DangerFullAccess，让 WorkspaceWrite 走审批提示路径；
            // 未声明的工具会按配置错误直接拒绝，不再回退成高权限。
            PermissionPolicy::new(PermissionMode::WorkspaceWrite)
                .with_tool_requirement("blocked", PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
        );

        let summary = runtime
            .run_turn("use the tool", Some(&mut RejectPrompter))
            .expect("conversation should continue after denied tool");

        assert_eq!(summary.tool_results.len(), 1);
        assert!(matches!(
            &summary.tool_results[0].blocks[0],
            ContentBlock::ToolResult { is_error: true, output, .. } if output == "not now"
        ));
    }

    #[test]
    fn denies_tool_use_when_pre_tool_hook_blocks() {
        // 获授权的 hook 返回 deny ⇒ 目标执行器零调用，hook 自身事实仍然被记录。
        let pre = CountingHook::new("conv-hook-deny-pre");
        let post = CountingHook::new("conv-hook-deny-post");
        let counter = CallCounter::new();

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "blocked",
                tool_input: r#"{"path":"secret.txt"}"#,
            },
            StaticToolExecutor::new().register("blocked", counter.handler("tool-output")),
            // 权限闸门必须放行，被拒绝的原因才是 hook——所以此处显式声明。
            PermissionPolicy::new(PermissionMode::DangerFullAccess)
                .with_tool_requirement("blocked", PermissionMode::ReadOnly),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
                vec![pre.deny_command()],
                vec![post.allow_command()],
            )),
        )
        .with_hook_authorization(hook_authorization());

        let summary = runtime
            .run_turn("use the tool", None)
            .expect("conversation should continue after hook denial");

        let (output, is_error) = single_tool_result(&summary);
        // 正对照：授权之后同一个 hook 确实跑过（标记文件存在），
        // 因此下面"没跑"的断言不是恒真。
        assert!(pre.ran(), "an authorized pre hook must have run");
        assert_eq!(counter.count(), 0, "the target executor must not run after a hook denial");
        assert!(!post.ran(), "post hook must not run when the tool never executed");
        assert!(is_error, "hook denial should produce an error result: {output}");
        assert!(
            output.contains(&pre.transcript_marker()),
            "hook's own fact must be recorded: {output:?}"
        );
        pre.cleanup();
        post.cleanup();
    }

    /// 保证表第 1 行：宿主预检查已明确拒绝 ⇒ 不运行 pre-tool shell hook，
    /// 也不运行目标执行器。这里连"带 deny 意见的 hook"也不运行。
    #[test]
    fn host_denial_runs_neither_the_pre_hook_nor_the_executor() {
        let pre = CountingHook::new("gate-deny-pre");
        let post = CountingHook::new("gate-deny-post");
        let counter = CallCounter::new();
        let mut prompter = ScriptedPrompter::new(vec![PermissionPromptDecision::Deny {
            reason: "host says no".to_string(),
        }]);

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "blocked",
                tool_input: r#"{"path":"secret.txt"}"#,
            },
            StaticToolExecutor::new().register("blocked", counter.handler("tool-output")),
            // 工具需要升级到 DangerFullAccess ⇒ 走审批通道；prompter 拒绝即宿主明确拒绝。
            PermissionPolicy::new(PermissionMode::WorkspaceWrite)
                .with_tool_requirement("blocked", PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
                vec![pre.deny_command()],
                vec![post.allow_command()],
            )),
        )
        .with_hook_authorization(hook_authorization());

        let summary = runtime
            .run_turn("use the tool", Some(&mut prompter))
            .expect("conversation should continue after host denial");

        let (output, is_error) = single_tool_result(&summary);
        assert_eq!(prompter.seen(), 1);
        assert_eq!(counter.count(), 0, "target executor must not run after a host denial");
        assert!(!pre.ran(), "pre-tool hook must not run after a host denial");
        assert!(!post.ran(), "post-tool hook must not run after a host denial");
        assert!(output.contains("host says no"), "{output}");
        assert!(
            !output.contains(&pre.transcript_marker()),
            "hook must not have produced any fact: {output:?}"
        );
        assert!(is_error);
        pre.cleanup();
        post.cleanup();
    }

    /// 保证表第 4 行：hook 返回 allow + 宿主 deny ⇒ 宿主 deny 生效、执行器零调用。
    #[test]
    fn authorized_hook_allow_cannot_override_host_denial() {
        let pre = CountingHook::new("gate-allow-pre");
        let post = CountingHook::new("gate-allow-post");
        let counter = CallCounter::new();
        let mut prompter = ScriptedPrompter::new(vec![PermissionPromptDecision::Deny {
            reason: "host says no".to_string(),
        }]);

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "blocked",
                tool_input: r#"{"path":"secret.txt"}"#,
            },
            StaticToolExecutor::new().register("blocked", counter.handler("tool-output")),
            PermissionPolicy::new(PermissionMode::WorkspaceWrite)
                .with_tool_requirement("blocked", PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
                // hook 若运行会返回 allow（退出码 0）
                vec![pre.allow_command()],
                vec![post.allow_command()],
            )),
        )
        .with_hook_authorization(hook_authorization());

        let summary = runtime
            .run_turn("use the tool", Some(&mut prompter))
            .expect("conversation should continue after host denial");

        let (output, is_error) = single_tool_result(&summary);
        assert_eq!(prompter.seen(), 1, "the host decision must be reached exactly once");
        assert_eq!(counter.count(), 0, "hook allow must not revive a host denial");
        assert!(
            !pre.ran() && !post.ran(),
            "no hook may run after a host denial, not even an authorized one"
        );
        assert!(output.contains("host says no"), "host denial is final: {output}");
        assert!(
            !output.contains(&pre.transcript_marker()),
            "hook must not have produced any fact: {output:?}"
        );
        assert!(is_error);
        pre.cleanup();
        post.cleanup();
    }

    /// 保证表第 2 行：hook 没有独立执行授权 ⇒ 不运行该 hook；
    /// 但"hook 未授权"是宿主配置事实，**不得**变成对目标工具的拒绝。
    #[test]
    fn hook_without_independent_authorization_is_not_run_but_the_tool_still_runs() {
        let pre = CountingHook::new("noauth-pre");
        let post = CountingHook::new("noauth-post");
        let counter = CallCounter::new();

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "add",
                tool_input: r#"{"lhs":2,"rhs":2}"#,
            },
            StaticToolExecutor::new().register("add", counter.handler("4")),
            PermissionPolicy::new(PermissionMode::DangerFullAccess)
                .with_tool_requirement("add", PermissionMode::ReadOnly),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
                vec![pre.allow_command()],
                vec![post.allow_command()],
            )),
        );
        // 刻意不调用 with_hook_authorization()：首期没有任何 hook 持有独立授权。

        let summary = runtime.run_turn("use add", None).expect("tool loop succeeds");

        let (output, is_error) = single_tool_result(&summary);
        assert!(!is_error, "missing hook authorization must not fail the tool: {output:?}");
        assert_eq!(counter.count(), 1, "the target tool must still run");
        assert!(!pre.ran(), "unauthorized pre hook must not run");
        assert!(!post.ran(), "unauthorized post hook must not run");
        assert!(output.contains('4'), "{output:?}");
        assert!(
            output.contains("not run") && output.contains(HOOK_CAPABILITY_PRE_TOOL_USE),
            "the skip fact must be recorded for audit: {output:?}"
        );
        pre.cleanup();
        post.cleanup();
    }

    /// hook 真的运行过 ⇒ 它可能在闸门之后改变了世界，原审批不得沿用，必须重新判定。
    /// 判别性：重判为 deny 时目标执行器零调用，且宿主被询问了第二次。
    #[test]
    fn hook_execution_voids_the_previous_approval_and_the_call_is_rejudged() {
        let pre = CountingHook::new("rejudge-pre");
        let post = CountingHook::new("rejudge-post");
        let counter = CallCounter::new();
        let mut prompter = ScriptedPrompter::new(vec![
            PermissionPromptDecision::Allow,
            PermissionPromptDecision::Deny {
                reason: "approval revoked after hook".to_string(),
            },
        ]);

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "escalate",
                tool_input: r#"{"path":"target.txt"}"#,
            },
            StaticToolExecutor::new().register("escalate", counter.handler("tool-output")),
            // 需要升级审批 ⇒ 第一次判定来自用户批准，重判仍回到同一审批对象。
            PermissionPolicy::new(PermissionMode::WorkspaceWrite)
                .with_tool_requirement("escalate", PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
                vec![pre.allow_command()],
                vec![post.allow_command()],
            )),
        )
        .with_hook_authorization(hook_authorization());

        let summary = runtime
            .run_turn("use the tool", Some(&mut prompter))
            .expect("conversation should continue after re-judged denial");

        let (output, is_error) = single_tool_result(&summary);
        assert!(pre.ran(), "the authorized pre hook must have run");
        assert_eq!(
            prompter.seen(),
            2,
            "the approval must be re-judged after a hook actually ran"
        );
        assert_eq!(counter.count(), 0, "re-judged denial must keep the executor at zero calls");
        assert!(!post.ran(), "post hook must not run when the tool never executed");
        assert!(output.contains("approval revoked after hook"), "{output:?}");
        assert!(
            output.contains(&pre.transcript_marker()),
            "hook fact must survive the re-judged denial: {output:?}"
        );
        assert!(is_error);
        pre.cleanup();
        post.cleanup();
    }

    /// 重判不是"一律拒绝"：重判仍放行时目标执行器正常执行一次。
    #[test]
    fn rejudged_approval_that_still_allows_executes_the_tool_once() {
        let pre = CountingHook::new("rejudge-allow-pre");
        let counter = CallCounter::new();
        let mut prompter = ScriptedPrompter::new(vec![
            PermissionPromptDecision::Allow,
            PermissionPromptDecision::Allow,
        ]);

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "escalate",
                tool_input: r#"{"path":"target.txt"}"#,
            },
            StaticToolExecutor::new().register("escalate", counter.handler("tool-output")),
            PermissionPolicy::new(PermissionMode::WorkspaceWrite)
                .with_tool_requirement("escalate", PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default()
                .with_hooks(RuntimeHookConfig::new(vec![pre.allow_command()], Vec::new())),
        )
        .with_hook_authorization(hook_authorization());

        let summary = runtime
            .run_turn("use the tool", Some(&mut prompter))
            .expect("tool loop succeeds");

        let (output, is_error) = single_tool_result(&summary);
        assert!(pre.ran());
        assert_eq!(prompter.seen(), 2);
        assert_eq!(counter.count(), 1, "a re-judged allow still executes exactly once");
        assert!(!is_error, "{output:?}");
        assert!(output.contains("tool-output"), "{output:?}");
        pre.cleanup();
    }

    /// 边界：没有任何 hook 真正运行时不重判，也不会多问用户一次。
    #[test]
    fn no_hook_run_means_no_rejudgment_and_no_extra_prompt() {
        let pre = CountingHook::new("inert-pre");
        let counter = CallCounter::new();
        let mut prompter = ScriptedPrompter::new(vec![PermissionPromptDecision::Allow]);

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "escalate",
                tool_input: r#"{"path":"target.txt"}"#,
            },
            StaticToolExecutor::new().register("escalate", counter.handler("tool-output")),
            PermissionPolicy::new(PermissionMode::WorkspaceWrite)
                .with_tool_requirement("escalate", PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default()
                .with_hooks(RuntimeHookConfig::new(vec![pre.allow_command()], Vec::new())),
        );
        // 未授权 ⇒ hook 不运行 ⇒ 两次判定之间没有任何东西改变，原审批仍然成立。

        let summary = runtime
            .run_turn("use the tool", Some(&mut prompter))
            .expect("tool loop succeeds");

        let (output, is_error) = single_tool_result(&summary);
        assert!(!pre.ran(), "unauthorized hook must not run");
        assert_eq!(prompter.seen(), 1, "no extra approval prompt when no hook ran");
        assert_eq!(counter.count(), 1, "the tool still executes");
        assert!(!is_error, "{output:?}");
        pre.cleanup();
    }

    #[test]
    fn appends_post_tool_hook_feedback_to_tool_result() {
        // 正对照（判别性证据的"探测器可用"证明）：授权之后 pre/post hook 都确实
        // 运行过（标记文件 = 外部证据），且它们的事实都进了工具结果。
        let pre = CountingHook::new("conv-hook-ok-pre");
        let post = CountingHook::new("conv-hook-ok-post");
        let counter = CallCounter::new();

        let mut runtime = ConversationRuntime::new_with_features(
            Session::new(),
            OneToolUseApiClient {
                tool_name: "add",
                tool_input: r#"{"lhs":2,"rhs":2}"#,
            },
            StaticToolExecutor::new().register("add", counter.handler("4")),
            PermissionPolicy::new(PermissionMode::DangerFullAccess)
                .with_tool_requirement("add", PermissionMode::ReadOnly),
            vec!["system".to_string()],
            RuntimeFeatureConfig::default().with_hooks(RuntimeHookConfig::new(
                vec![pre.allow_command()],
                vec![post.allow_command()],
            )),
        )
        .with_hook_authorization(hook_authorization());

        let summary = runtime
            .run_turn("use add", None)
            .expect("tool loop succeeds");

        let (output, is_error) = single_tool_result(&summary);
        assert!(pre.ran() && post.ran(), "authorized pre/post hooks must have run");
        assert_eq!(counter.count(), 1);
        assert!(!is_error, "post hook should preserve non-error result: {output:?}");
        assert!(output.contains('4'), "tool output missing value: {output:?}");
        assert!(
            output.contains(&pre.transcript_marker()),
            "tool output missing pre hook feedback: {output:?}"
        );
        assert!(
            output.contains(&post.transcript_marker()),
            "tool output missing post hook feedback: {output:?}"
        );
        pre.cleanup();
        post.cleanup();
    }

    #[test]
    fn reconstructs_usage_tracker_from_restored_session() {
        struct SimpleApi;
        impl ApiClient for SimpleApi {
            fn stream(
                &mut self,
                _request: ApiRequest,
            ) -> Result<Vec<AssistantEvent>, RuntimeError> {
                Ok(vec![
                    AssistantEvent::TextDelta("done".to_string()),
                    AssistantEvent::MessageStop,
                ])
            }
        }

        let mut session = Session::new();
        session
            .messages
            .push(crate::session::ConversationMessage::assistant_with_usage(
                vec![ContentBlock::Text {
                    text: "earlier".to_string(),
                }],
                Some(TokenUsage {
                    input_tokens: 11,
                    output_tokens: 7,
                    cache_creation_input_tokens: 2,
                    cache_read_input_tokens: 1,
                }),
            ));

        let runtime = ConversationRuntime::new(
            session,
            SimpleApi,
            StaticToolExecutor::new(),
            PermissionPolicy::new(PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
        );

        assert_eq!(runtime.usage().turns(), 1);
        assert_eq!(runtime.usage().cumulative_usage().total_tokens(), 21);
    }

    #[test]
    fn compacts_session_after_turns() {
        struct SimpleApi;
        impl ApiClient for SimpleApi {
            fn stream(
                &mut self,
                _request: ApiRequest,
            ) -> Result<Vec<AssistantEvent>, RuntimeError> {
                Ok(vec![
                    AssistantEvent::TextDelta("done".to_string()),
                    AssistantEvent::MessageStop,
                ])
            }
        }

        let mut runtime = ConversationRuntime::new(
            Session::new(),
            SimpleApi,
            StaticToolExecutor::new(),
            PermissionPolicy::new(PermissionMode::DangerFullAccess),
            vec!["system".to_string()],
        );
        runtime.run_turn("a", None).expect("turn a");
        runtime.run_turn("b", None).expect("turn b");
        runtime.run_turn("c", None).expect("turn c");

        let result = runtime.compact(CompactionConfig {
            preserve_recent_messages: 2,
            max_estimated_tokens: 1,
        });
        assert!(result.summary.contains("Conversation summary"));
        assert_eq!(
            result.compacted_session.messages[0].role,
            MessageRole::System
        );
    }
}
