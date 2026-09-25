use std::env;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use api::{
    max_tokens_for_model, resolve_model_alias, InputContentBlock, InputMessage, MessageRequest,
    OutputContentBlock, ProviderClient, ToolResultContentBlock,
};
use runtime::{
    ContentBlock as SessionContentBlock, ConversationMessage as SessionConversationMessage,
    MessageRole as SessionMessageRole, Session,
};
use serde_json::json;
use vision::{analyze_latest_desktop_with_backend, LocalOpenAiVisionBackend, ZhipuVisionBackend};

use crate::config::{GuiConfig, GuiConfigStore};
use crate::desktop_agent::is_desktop_automation_prompt;
use crate::desktop_capture::capture_latest_desktop_snapshot_now;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuiRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuiMessage {
    pub role: GuiRole,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRoute {
    Chat,
    DesktopVision,
    DesktopAutomation,
}

#[derive(Debug, Clone)]
pub struct GuiAgentService {
    config: GuiConfig,
    cwd: PathBuf,
    os_name: String,
    os_version: String,
}

#[derive(Debug, Clone)]
pub struct GuiReply {
    pub text: String,
    pub model: String,
    pub request_id: Option<String>,
    pub total_tokens: u32,
}

#[derive(Debug, Clone)]
pub struct SmokeTestResult {
    pub mode: String,
    pub model: String,
    pub reply: String,
    pub request_id: Option<String>,
    pub total_tokens: u32,
}

impl GuiAgentService {
    #[must_use]
    pub fn from_config(config: GuiConfig) -> Self {
        let config = config.sanitized();
        config.apply_process_env();
        let cwd = config.workspace_path();
        Self {
            config,
            cwd,
            os_name: env::consts::OS.to_string(),
            os_version: env::var("OS").unwrap_or_else(|_| "未知".to_string()),
        }
    }

    #[must_use]
    pub fn model(&self) -> &str {
        &self.config.chat_model
    }

    #[must_use]
    pub fn vision_model(&self) -> &str {
        if self.config.vision_backend == "local-openai" {
            &self.config.local_vision_model
        } else {
            &self.config.vision_model
        }
    }

    #[must_use]
    pub fn vision_backend(&self) -> &str {
        &self.config.vision_backend
    }

    #[must_use]
    pub fn profile(&self) -> &str {
        &self.config.agent_profile
    }

    #[must_use]
    pub fn context_engine(&self) -> &str {
        &self.config.context_engine
    }

    #[must_use]
    pub const fn fast_mode(&self) -> bool {
        self.config.fast_mode
    }

    #[must_use]
    pub fn workspace(&self) -> &Path {
        &self.cwd
    }

    #[must_use]
    pub const fn chat_timeout_seconds(&self) -> u64 {
        self.config.chat_agent_timeout_seconds
    }

    #[must_use]
    pub const fn vision_timeout_seconds(&self) -> u64 {
        self.config.vision_agent_timeout_seconds
    }

    #[must_use]
    pub fn live_mode_enabled(&self) -> bool {
        has_non_empty_env("ZAI_API_KEY")
            || has_non_empty_env("BIGMODEL_API_KEY")
            || has_non_empty_env("OPENAI_API_KEY")
    }

    pub fn send_chat(&self, transcript: &Session, prompt: &str) -> Result<GuiReply, String> {
        self.config.apply_process_env();

        if classify_prompt_route(prompt) == AgentRoute::DesktopVision {
            return self.send_latest_desktop_vision_local_first(prompt);
        }

        if !self.live_mode_enabled() {
            thread::sleep(Duration::from_millis(220));
            return Ok(GuiReply {
                text: demo_response(prompt, &self.config),
                model: "demo-mode".to_string(),
                request_id: None,
                total_tokens: 0,
            });
        }

        let resolved_model = resolve_model_alias(&self.config.chat_model);
        let messages = session_to_request_messages(transcript, prompt);
        let system = build_gui_system_prompt(self, &resolved_model);

        let client = ProviderClient::from_model(&resolved_model)
            .map_err(|error| format!("failed to build provider client: {error}"))?;
        let request = MessageRequest {
            model: resolved_model,
            max_tokens: max_tokens_for_model(&self.config.chat_model),
            messages,
            system: Some(system),
            tools: None,
            tool_choice: None,
            stream: false,
            reasoning_effort: None,
        };
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|error| format!("failed to create async runtime: {error}"))?;
        let timeout_seconds = self.chat_timeout_seconds();
        let response = run_with_timeout("主代理", timeout_seconds, move || {
            runtime
                .block_on(client.send_message(&request))
                .map_err(|error| format!("live request failed: {error}"))
        })?;

        Ok(GuiReply {
            text: extract_assistant_text(&response.content),
            model: response.model,
            request_id: response.request_id,
            total_tokens: response.usage.total_tokens(),
        })
    }

    #[allow(dead_code)]
    fn send_latest_desktop_vision(&self, prompt: &str) -> Result<GuiReply, String> {
        let latest_capture = capture_latest_desktop_snapshot_now()
            .map_err(|error| format!("failed to capture latest desktop before vision: {error}"))?;
        if !latest_capture.is_file() {
            return Ok(GuiReply {
                text: format!(
                    "我还没有拿到可分析的最新桌面截图。请先在界面里启用“内视觉采集”，然后再问我当前桌面相关问题。\n\n期望截图路径：{}",
                    latest_capture.display()
                ),
                model: self.config.vision_model.clone(),
                request_id: None,
                total_tokens: 0,
            });
        }

        if !self.live_mode_enabled() {
            thread::sleep(Duration::from_millis(120));
            return Ok(GuiReply {
                text: format!(
                    "已经检测到最新桌面截图：{}\n但当前没有可用的在线视觉模型 API Key，所以还不能自动分析截图。",
                    latest_capture.display()
                ),
                model: "desktop-vision-demo".to_string(),
                request_id: None,
                total_tokens: 0,
            });
        }

        let backend = ZhipuVisionBackend::default().with_model(&self.config.vision_model);
        let vision_prompt = build_latest_desktop_prompt(prompt, &latest_capture);
        let timeout_seconds = self.vision_timeout_seconds();
        let response = run_with_timeout("视觉代理", timeout_seconds, move || {
            analyze_latest_desktop_with_backend(&backend, &vision_prompt)
                .map_err(|error| format!("latest desktop vision failed: {error}"))
        })?;

        Ok(GuiReply {
            text: response.text,
            model: response.model,
            request_id: response.request_id,
            total_tokens: response.total_tokens,
        })
    }

    fn send_latest_desktop_vision_local_first(&self, prompt: &str) -> Result<GuiReply, String> {
        let latest_capture = capture_latest_desktop_snapshot_now()
            .map_err(|error| format!("failed to capture latest desktop before vision: {error}"))?;
        if !latest_capture.is_file() {
            return Ok(GuiReply {
                text: format!(
                    "还没有拿到可分析的最新桌面截图。\n\n期望截图路径：{}",
                    latest_capture.display()
                ),
                model: self.vision_model().to_string(),
                request_id: None,
                total_tokens: 0,
            });
        }

        let vision_prompt = build_latest_desktop_prompt(prompt, &latest_capture);
        let timeout_seconds = self.vision_timeout_seconds();
        let response = if self.config.vision_backend == "zhipu" {
            self.run_cloud_vision(vision_prompt, timeout_seconds)?
        } else {
            match self.run_local_vision(vision_prompt.clone(), timeout_seconds) {
                Ok(response) => response,
                Err(local_error)
                    if self.config.cloud_vision_fallback_enabled && self.live_mode_enabled() =>
                {
                    let mut response = self.run_cloud_vision(vision_prompt, timeout_seconds)?;
                    response.text = format!(
                        "本地视觉暂不可用，已临时切换远端视觉兜底。\n本地错误：{local_error}\n\n{}",
                        response.text
                    );
                    response
                }
                Err(local_error) => {
                    return Err(format!(
                        "本地视觉模型不可用：{local_error}\n请先启动本地 OpenAI-compatible 视觉服务，例如 vLLM/LM Studio/Ollama，并确认地址为 {}，模型为 {}。",
                        self.config.local_vision_base_url,
                        self.config.local_vision_model
                    ));
                }
            }
        };

        Ok(GuiReply {
            text: response.text,
            model: response.model,
            request_id: response.request_id,
            total_tokens: response.total_tokens,
        })
    }

    fn run_local_vision(
        &self,
        vision_prompt: String,
        timeout_seconds: u64,
    ) -> Result<vision::VisionResponse, String> {
        let backend = LocalOpenAiVisionBackend::new(
            &self.config.local_vision_base_url,
            &self.config.local_vision_model,
        )
        .with_api_key(&self.config.local_vision_api_key)
        .with_timeout_seconds(timeout_seconds);
        diagnostics::info(
            "gui.vision",
            "local_request",
            "starting local desktop vision request",
            &[
                ("model", self.config.local_vision_model.clone()),
                ("base_url", self.config.local_vision_base_url.clone()),
                ("timeout_seconds", timeout_seconds.to_string()),
            ],
        );
        run_with_timeout("本地视觉代理", timeout_seconds, move || {
            analyze_latest_desktop_with_backend(&backend, &vision_prompt)
                .map_err(|error| format!("latest desktop local vision failed: {error}"))
        })
    }

    fn run_cloud_vision(
        &self,
        vision_prompt: String,
        timeout_seconds: u64,
    ) -> Result<vision::VisionResponse, String> {
        if !self.live_mode_enabled() {
            return Err("远端视觉兜底未启用或缺少 API Key".to_string());
        }
        let backend = ZhipuVisionBackend::default().with_model(&self.config.vision_model);
        run_with_timeout("远端视觉代理", timeout_seconds, move || {
            analyze_latest_desktop_with_backend(&backend, &vision_prompt)
                .map_err(|error| format!("latest desktop cloud vision failed: {error}"))
        })
    }
}

fn build_gui_system_prompt(service: &GuiAgentService, model: &str) -> String {
    format!(
        "You are the backend for the Claw GUI desktop demo.\n\
Respond in concise Chinese unless the user asks otherwise.\n\
The interface includes a central reply panel, a left control sidebar, and a right vision/tools sidebar.\n\
Do not claim to reveal hidden chain-of-thought. If asked about reasoning, summarize observable runtime steps instead.\n\
Keep answers practical, short, and easy to scan.\n\
Runtime hints:\n\
- chat model: {model}\n\
- vision backend: {}\n\
- vision model: {}\n\
- profile: {}\n\
- context engine: {}\n\
- fast mode: {}\n\
- cwd: {}\n\
- os: {} {}\n",
        service.vision_backend(),
        service.vision_model(),
        service.profile(),
        service.context_engine(),
        if service.fast_mode() { "on" } else { "off" },
        service.workspace().display(),
        service.os_name,
        service.os_version,
    )
}

pub fn run_smoke_test(prompt: &str) -> Result<SmokeTestResult, String> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let store =
        GuiConfigStore::load_or_create(&cwd).unwrap_or_else(|_| GuiConfigStore::fallback(&cwd));
    let service = GuiAgentService::from_config(store.config().clone());
    let reply = service.send_chat(&Session::new(), prompt)?;
    Ok(SmokeTestResult {
        mode: if service.live_mode_enabled() {
            "live".to_string()
        } else {
            "demo".to_string()
        },
        model: reply.model,
        reply: reply.text,
        request_id: reply.request_id,
        total_tokens: reply.total_tokens,
    })
}

pub fn run_desktop_vision_test(prompt: &str) -> Result<SmokeTestResult, String> {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let store =
        GuiConfigStore::load_or_create(&cwd).unwrap_or_else(|_| GuiConfigStore::fallback(&cwd));
    let service = GuiAgentService::from_config(store.config().clone());
    let reply = service.send_latest_desktop_vision_local_first(prompt)?;
    Ok(SmokeTestResult {
        mode: service.vision_backend().to_string(),
        model: reply.model,
        reply: reply.text,
        request_id: reply.request_id,
        total_tokens: reply.total_tokens,
    })
}

#[must_use]
pub fn classify_prompt_route(prompt: &str) -> AgentRoute {
    if is_desktop_automation_prompt(prompt) {
        AgentRoute::DesktopAutomation
    } else if should_route_to_latest_desktop_vision(prompt) {
        AgentRoute::DesktopVision
    } else {
        AgentRoute::Chat
    }
}

fn session_to_request_messages(session: &Session, prompt: &str) -> Vec<InputMessage> {
    let mut messages = Vec::new();
    for message in &session.messages {
        match message.role {
            SessionMessageRole::System => {}
            SessionMessageRole::User => {
                let content = message
                    .blocks
                    .iter()
                    .filter_map(session_block_to_input_block)
                    .collect::<Vec<_>>();
                if !content.is_empty() {
                    messages.push(InputMessage {
                        role: "user".to_string(),
                        content,
                    });
                }
            }
            SessionMessageRole::Assistant => {
                let content = message
                    .blocks
                    .iter()
                    .filter_map(session_block_to_input_block)
                    .collect::<Vec<_>>();
                if !content.is_empty() {
                    messages.push(InputMessage {
                        role: "assistant".to_string(),
                        content,
                    });
                }
            }
            SessionMessageRole::Tool => {
                let content = message
                    .blocks
                    .iter()
                    .filter_map(tool_result_block_to_input_block)
                    .collect::<Vec<_>>();
                if !content.is_empty() {
                    messages.push(InputMessage {
                        role: "user".to_string(),
                        content,
                    });
                }
            }
        }
    }
    if !session_already_has_prompt_tail(session, prompt) && !prompt.trim().is_empty() {
        messages.push(InputMessage {
            role: "user".to_string(),
            content: vec![InputContentBlock::Text {
                text: prompt.trim().to_string(),
            }],
        });
    }
    messages
}

fn session_already_has_prompt_tail(session: &Session, prompt: &str) -> bool {
    let Some(last_message) = session.messages.last() else {
        return false;
    };
    if last_message.role != SessionMessageRole::User {
        return false;
    }

    let last_text = collect_text_blocks(&last_message.blocks);
    !last_text.is_empty() && last_text.trim() == prompt.trim()
}

fn session_block_to_input_block(block: &SessionContentBlock) -> Option<InputContentBlock> {
    match block {
        SessionContentBlock::Text { text } => Some(InputContentBlock::Text { text: text.clone() }),
        SessionContentBlock::ToolUse { id, name, input } => Some(InputContentBlock::ToolUse {
            id: id.clone(),
            name: name.clone(),
            input: parse_json_or_string(input),
        }),
        SessionContentBlock::ToolResult { .. } => None,
    }
}

fn tool_result_block_to_input_block(block: &SessionContentBlock) -> Option<InputContentBlock> {
    match block {
        SessionContentBlock::ToolResult {
            tool_use_id,
            output,
            is_error,
            ..
        } => Some(InputContentBlock::ToolResult {
            tool_use_id: tool_use_id.clone(),
            content: vec![ToolResultContentBlock::Text {
                text: output.clone(),
            }],
            is_error: *is_error,
        }),
        _ => None,
    }
}

fn parse_json_or_string(input: &str) -> serde_json::Value {
    serde_json::from_str(input).unwrap_or_else(|_| serde_json::Value::String(input.to_string()))
}

pub fn session_to_gui_messages(session: &Session) -> Vec<GuiMessage> {
    session
        .messages
        .iter()
        .flat_map(session_message_to_gui_messages)
        .collect()
}

fn session_message_to_gui_messages(message: &SessionConversationMessage) -> Vec<GuiMessage> {
    match message.role {
        SessionMessageRole::System => Vec::new(),
        SessionMessageRole::User => {
            let text = collect_text_blocks(&message.blocks);
            if text.is_empty() {
                Vec::new()
            } else {
                vec![GuiMessage {
                    role: GuiRole::User,
                    text,
                }]
            }
        }
        SessionMessageRole::Assistant => {
            let mut rendered = Vec::new();
            let text = collect_text_blocks(&message.blocks);
            if !text.is_empty() {
                rendered.push(GuiMessage {
                    role: GuiRole::Assistant,
                    text,
                });
            }
            for block in &message.blocks {
                if let SessionContentBlock::ToolUse { name, input, .. } = block {
                    rendered.push(GuiMessage {
                        role: GuiRole::Assistant,
                        text: format!("[工具调用] {name}\n{input}"),
                    });
                }
            }
            rendered
        }
        SessionMessageRole::Tool => message
            .blocks
            .iter()
            .filter_map(|block| match block {
                SessionContentBlock::ToolResult {
                    tool_name,
                    output,
                    is_error,
                    ..
                } => Some(GuiMessage {
                    role: GuiRole::Assistant,
                    text: if *is_error {
                        format!("[工具结果:{tool_name}] 错误\n{output}")
                    } else {
                        format!("[工具结果:{tool_name}]\n{output}")
                    },
                }),
                _ => None,
            })
            .collect(),
    }
}

fn collect_text_blocks(blocks: &[SessionContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|block| match block {
            SessionContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_assistant_text(blocks: &[OutputContentBlock]) -> String {
    let mut text = blocks
        .iter()
        .filter_map(|block| match block {
            OutputContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if text.trim().is_empty() {
        text = json!({
            "notice": "模型没有返回文本块，可能改用了工具调用或结构化输出。"
        })
        .to_string();
    }
    text
}

fn demo_response(prompt: &str, config: &GuiConfig) -> String {
    format!(
        "COOLZHU AGENT 控制台已经启动，但当前没有检测到可用的在线 API Key，所以先进入演示模式。\n\n\
当前聊天模型：{}\n\
当前视觉模型：{}\n\
工作目录：{}\n\
桌面采集频率：{} ms\n\
桌面缓存上限：{} 帧\n\n\
你刚才输入的是：{prompt}\n\n\
如果你已经准备好 key，只需要在界面的“配置中心”里确认 `API Key 文件路径`，然后点击“保存并应用”。",
        config.chat_model,
        config.vision_model,
        config.workspace_path().display(),
        config.desktop_capture_interval_ms,
        config.desktop_cache_limit,
    )
}

fn has_non_empty_env(key: &str) -> bool {
    env::var(key).is_ok_and(|value| !value.trim().is_empty())
}

fn run_with_timeout<T, F>(agent_name: &str, timeout_seconds: u64, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(operation());
    });

    rx.recv_timeout(Duration::from_secs(timeout_seconds))
        .map_err(|_| format!("{agent_name} 执行超时，已超过 {timeout_seconds} 秒"))?
}

fn should_route_to_latest_desktop_vision(prompt: &str) -> bool {
    let normalized = prompt.trim().to_ascii_lowercase();
    let desktop_keywords = [
        "当前桌面",
        "当前屏幕",
        "桌面上",
        "屏幕上",
        "你看到",
        "你能看到",
        "看一下桌面",
        "看下桌面",
        "看看桌面",
        "看一下屏幕",
        "看下屏幕",
        "看看屏幕",
        "最新截图",
        "当前截图",
    ];

    desktop_keywords
        .iter()
        .any(|keyword| prompt.contains(keyword))
        || ((prompt.contains("看一下") || prompt.contains("看下") || prompt.contains("看看"))
            && (prompt.contains("桌面") || prompt.contains("屏幕") || prompt.contains("截图")))
        || normalized.contains("what do you see")
        || normalized.contains("look at the desktop")
        || normalized.contains("look at the screen")
        || normalized.contains("current desktop")
        || normalized.contains("current screen")
}

fn build_latest_desktop_prompt(prompt: &str, latest_capture: &Path) -> String {
    format!(
        "以下问题需要根据最新桌面截图回答。只依据截图中可见内容作答；看不清就明确说看不清，不要猜测。\n最新截图路径：{}\n用户问题：{}",
        latest_capture.display(),
        prompt.trim()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_latest_desktop_prompt, session_to_request_messages,
        should_route_to_latest_desktop_vision,
    };
    use api::InputContentBlock;
    use runtime::{ConversationMessage, Session};
    use std::path::Path;

    #[test]
    fn request_messages_append_prompt_when_session_does_not_include_it() {
        let session = Session::new();
        let messages = session_to_request_messages(&session, "hello from smoke test");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
        assert_eq!(
            messages[0].content,
            vec![InputContentBlock::Text {
                text: "hello from smoke test".to_string(),
            }]
        );
    }

    #[test]
    fn request_messages_do_not_duplicate_prompt_already_at_tail() {
        let mut session = Session::new();
        session
            .messages
            .push(ConversationMessage::user_text("same prompt"));
        let messages = session_to_request_messages(&session, "same prompt");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "user");
    }

    #[test]
    fn desktop_keywords_trigger_latest_desktop_vision() {
        assert!(should_route_to_latest_desktop_vision("帮我看一下当前桌面"));
        assert!(should_route_to_latest_desktop_vision("你看到什么？"));
        assert!(should_route_to_latest_desktop_vision("look at the screen"));
    }

    #[test]
    fn unrelated_requests_do_not_trigger_latest_desktop_vision() {
        assert!(!should_route_to_latest_desktop_vision(
            "看一下这个方案是否可行"
        ));
        assert!(!should_route_to_latest_desktop_vision("总结一下刚才的对话"));
    }

    #[test]
    fn latest_desktop_prompt_mentions_capture_path() {
        let prompt = build_latest_desktop_prompt("当前桌面有什么", Path::new("C:/demo/latest.png"));
        assert!(prompt.contains("C:/demo/latest.png"));
        assert!(prompt.contains("当前桌面有什么"));
    }
}
