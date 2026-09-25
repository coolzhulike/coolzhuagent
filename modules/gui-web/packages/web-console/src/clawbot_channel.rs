use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::wechat_command::{
    command_registry, command_spec_by_id, match_command_spec, render_command_help,
    render_help_pages, WechatCapability, WechatCommandAvailability,
};

pub const CLAWBOT_CHANNEL_ID: &str = "weixin-clawbot";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClawbotCommand {
    SelectRoom {
        room: String,
    },
    UseSessionOrModel {
        selector: String,
    },
    SelectTargets {
        target_agent_ids: Vec<String>,
    },
    ListRooms,
    ListSessions,
    ListTasks,
    ContinueTask {
        task_id: String,
    },
    Status,
    NewTurn,
    Help {
        topic: Option<String>,
    },
    Registered {
        command_id: String,
        arguments: String,
    },
    Unknown {
        input: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotMessageKind {
    Text,
    Image,
    Video,
    File,
    Voice,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotMediaRef {
    pub media_id: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotInboundMessage {
    pub account_id: String,
    pub peer_id: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
    pub peer_name: Option<String>,
    #[serde(default)]
    pub sender_id: Option<String>,
    #[serde(default)]
    pub sender_name: Option<String>,
    #[serde(default)]
    pub is_group: bool,
    #[serde(default)]
    pub group_event: Option<crate::wechat_group::WechatGroupEventKind>,
    #[serde(default)]
    pub mentioned_bot: bool,
    #[serde(default)]
    pub mentions: Vec<String>,
    #[serde(default)]
    pub raw_payload_summary: Option<String>,
    pub context_token: Option<String>,
    pub external_msg_id: String,
    pub kind: ClawbotMessageKind,
    pub text: Option<String>,
    pub media_refs: Vec<ClawbotMediaRef>,
    pub received_at_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotInboundSource {
    WeixinUser,
    GatewayEcho,
    CoolzhuOutbox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClawbotIngressGuardAction {
    Accept,
    RejectRecursive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotInboundEnvelope {
    pub source: ClawbotInboundSource,
    pub hop_count: u32,
    pub message: ClawbotInboundMessage,
}

impl ClawbotInboundEnvelope {
    pub fn weixin_user(message: ClawbotInboundMessage) -> Self {
        Self {
            source: ClawbotInboundSource::WeixinUser,
            hop_count: 0,
            message,
        }
    }

    pub fn guard_action(&self) -> ClawbotIngressGuardAction {
        if self.source == ClawbotInboundSource::WeixinUser && self.hop_count == 0 {
            ClawbotIngressGuardAction::Accept
        } else {
            ClawbotIngressGuardAction::RejectRecursive
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotConversationBinding {
    pub account_id: String,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub chat_room_id: Option<String>,
    pub default_session_id: Option<String>,
    pub target_agent_ids: Vec<String>,
    pub workspace_id: String,
    pub last_context_token: Option<String>,
    pub allowlisted: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotDispatchAction {
    DispatchToCoolzhu,
    ApplyCommand,
    RejectNotAllowlisted,
    RejectDisabledBinding,
    NeedBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotRuntimeStatus {
    Disabled,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotCommandDescriptor {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub syntax: String,
    pub capability: String,
    pub availability: String,
    pub disabled_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotStatus {
    pub channel: String,
    pub status: ClawbotRuntimeStatus,
    pub capabilities: Vec<String>,
    pub commands: Vec<ClawbotCommandDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotInboundPreview {
    pub action: ClawbotDispatchAction,
    pub command: Option<ClawbotCommand>,
    pub account_id: String,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub chat_room_id: Option<String>,
    pub session_id: Option<String>,
    pub target_agent_ids: Vec<String>,
    pub workspace_id: Option<String>,
    pub context_token: Option<String>,
    pub external_msg_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClawbotCommandApplyStatus {
    Applied,
    Noop,
    MissingBinding,
    RejectedNotAllowlisted,
    RejectedDisabledBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotCommandApplyResult {
    pub status: ClawbotCommandApplyStatus,
    pub message: String,
    pub binding: Option<ClawbotConversationBinding>,
    pub pending_task_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClawbotApplyCommandRequest {
    pub account_id: String,
    pub peer_id: String,
    pub command: ClawbotCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ClawbotPersistedState {
    version: u32,
    bindings: Vec<ClawbotConversationBinding>,
}

#[derive(Debug, Clone, Default)]
pub struct ClawbotChannelState {
    bindings: HashMap<(String, String), ClawbotConversationBinding>,
}

impl ClawbotChannelState {
    pub fn status(&self) -> ClawbotStatus {
        ClawbotStatus {
            channel: CLAWBOT_CHANNEL_ID.to_string(),
            status: ClawbotRuntimeStatus::Disabled,
            capabilities: vec!["text".to_string()],
            commands: command_registry()
                .iter()
                .map(|command| ClawbotCommandDescriptor {
                    id: command.id.to_string(),
                    name: format!("/{}", command.path.join(" ")),
                    description: command.summary.to_string(),
                    category: command.category.label().to_string(),
                    syntax: command.syntax.to_string(),
                    capability: command.capability.as_str().to_string(),
                    availability: command.availability.label().to_string(),
                    disabled_reason: command
                        .availability
                        .disabled_reason()
                        .map(ToOwned::to_owned),
                })
                .collect(),
        }
    }

    #[cfg(test)]
    pub fn with_binding(mut self, binding: ClawbotConversationBinding) -> Self {
        self.bindings.insert(
            (binding.account_id.clone(), binding.peer_id.clone()),
            binding,
        );
        self
    }

    pub fn binding(&self, account_id: &str, peer_id: &str) -> Option<&ClawbotConversationBinding> {
        self.bindings
            .get(&(account_id.to_string(), peer_id.to_string()))
    }

    pub fn bindings(&self) -> Vec<ClawbotConversationBinding> {
        self.bindings.values().cloned().collect()
    }

    pub fn upsert_binding(
        &mut self,
        binding: ClawbotConversationBinding,
    ) -> ClawbotConversationBinding {
        self.bindings.insert(
            (binding.account_id.clone(), binding.peer_id.clone()),
            binding.clone(),
        );
        binding
    }

    pub fn remove_binding(
        &mut self,
        account_id: &str,
        peer_id: &str,
    ) -> Option<ClawbotConversationBinding> {
        self.bindings
            .remove(&(account_id.to_string(), peer_id.to_string()))
    }

    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("读取 ClawBot 状态文件失败：{} ({error})", path.display()))?;
        let persisted: ClawbotPersistedState = serde_json::from_str(&content)
            .map_err(|error| format!("解析 ClawBot 状态文件失败：{} ({error})", path.display()))?;
        let mut state = Self::default();
        for binding in persisted.bindings {
            state.upsert_binding(binding);
        }
        Ok(state)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!("创建 ClawBot 状态目录失败：{} ({error})", parent.display())
            })?;
        }
        let mut bindings = self.bindings();
        bindings.sort_by(|left, right| {
            left.account_id
                .cmp(&right.account_id)
                .then_with(|| left.peer_id.cmp(&right.peer_id))
        });
        let persisted = ClawbotPersistedState {
            version: 1,
            bindings,
        };
        let content = serde_json::to_string_pretty(&persisted)
            .map_err(|error| format!("序列化 ClawBot 状态失败：{error}"))?;
        std::fs::write(path, content)
            .map_err(|error| format!("写入 ClawBot 状态文件失败：{} ({error})", path.display()))
    }

    pub fn apply_command(
        &mut self,
        account_id: &str,
        peer_id: &str,
        command: ClawbotCommand,
    ) -> ClawbotCommandApplyResult {
        let key = (account_id.to_string(), peer_id.to_string());
        let Some(binding) = self.bindings.get_mut(&key) else {
            return ClawbotCommandApplyResult {
                status: ClawbotCommandApplyStatus::MissingBinding,
                message: "未找到微信联系人绑定，请先在 coolzhu 控制台完成 allowlist 与聊天室绑定。"
                    .to_string(),
                binding: None,
                pending_task_id: None,
            };
        };
        if !binding.allowlisted {
            return ClawbotCommandApplyResult {
                status: ClawbotCommandApplyStatus::RejectedNotAllowlisted,
                message: "微信联系人未在 allowlist 中，拒绝应用命令。".to_string(),
                binding: Some(binding.clone()),
                pending_task_id: None,
            };
        }
        if !binding.enabled {
            return ClawbotCommandApplyResult {
                status: ClawbotCommandApplyStatus::RejectedDisabledBinding,
                message: "微信联系人绑定已停用，拒绝应用命令。".to_string(),
                binding: Some(binding.clone()),
                pending_task_id: None,
            };
        }

        let mut pending_task_id = None;
        let message = match command {
            ClawbotCommand::SelectRoom { room } => {
                binding.chat_room_id = Some(room.clone());
                format!("已选择 coolzhu 聊天室：{room}")
            }
            ClawbotCommand::UseSessionOrModel { selector } => {
                binding.default_session_id = Some(selector.clone());
                format!("已选择会话/模型：{selector}")
            }
            ClawbotCommand::SelectTargets { target_agent_ids } => {
                binding.target_agent_ids = target_agent_ids.clone();
                format!("已选择目标 Agent：{}", target_agent_ids.join(", "))
            }
            ClawbotCommand::ContinueTask { task_id } => {
                pending_task_id = Some(task_id.clone());
                format!("已准备在当前聊天室继续任务：{task_id}")
            }
            ClawbotCommand::ListRooms
            | ClawbotCommand::ListSessions
            | ClawbotCommand::ListTasks
            | ClawbotCommand::Status
            | ClawbotCommand::NewTurn => {
                "该命令需要由 web-console/sidecar 查询实时数据后回复。".to_string()
            }
            ClawbotCommand::Help { topic } => match topic {
                Some(topic) => render_command_help(&topic).unwrap_or_else(|| {
                    format!("没有找到命令主题：{topic}\n请发送 /help 查看完整目录。")
                }),
                None => render_help_pages(command_registry(), 1_800).join("\n\n"),
            },
            ClawbotCommand::Registered {
                command_id,
                arguments: _,
            } => command_registry()
                .iter()
                .find(|command| command.id == command_id)
                .map(|command| match command.availability {
                    WechatCommandAvailability::Available => {
                        format!("命令 {} 已注册，等待实时执行器处理。", command.syntax)
                    }
                    WechatCommandAvailability::FeatureGated(reason) => {
                        format!("命令 {} 当前不可用：{reason}", command.syntax)
                    }
                })
                .unwrap_or_else(|| format!("命令注册信息不存在：{command_id}")),
            ClawbotCommand::Unknown { input } => {
                format!("未识别微信命令：{input}\n请发送 /help 查看完整目录。")
            }
        };

        ClawbotCommandApplyResult {
            status: ClawbotCommandApplyStatus::Applied,
            message,
            binding: Some(binding.clone()),
            pending_task_id,
        }
    }

    pub fn preview_inbound(&self, message: ClawbotInboundMessage) -> ClawbotInboundPreview {
        let binding = self
            .bindings
            .get(&(message.account_id.clone(), message.peer_id.clone()));
        match binding {
            Some(binding) => self.preview_inbound_with_binding(message, Some(binding)),
            None => self.preview_inbound_with_binding(message, None),
        }
    }

    /// 使用调用方提供的权威绑定生成预览。微信群绑定来自 SQLite 生命周期记录时，
    /// 不应再回退到可能滞后的 JSON 联系人绑定。
    pub fn preview_inbound_with_binding(
        &self,
        message: ClawbotInboundMessage,
        binding: Option<&ClawbotConversationBinding>,
    ) -> ClawbotInboundPreview {
        let command = message.text.as_deref().and_then(parse_clawbot_command);
        let Some(binding) = binding else {
            return ClawbotInboundPreview {
                action: ClawbotDispatchAction::NeedBinding,
                command,
                account_id: message.account_id,
                peer_id: message.peer_id,
                peer_name: message.peer_name,
                chat_room_id: None,
                session_id: None,
                target_agent_ids: Vec::new(),
                workspace_id: None,
                context_token: message.context_token,
                external_msg_id: message.external_msg_id,
                reason: Some("未找到微信联系人到 coolzhu 聊天室/会话的绑定".to_string()),
            };
        };
        if !binding.allowlisted {
            return self.preview_for_binding(
                &message,
                binding,
                ClawbotDispatchAction::RejectNotAllowlisted,
                command,
                Some("微信联系人未在 coolzhu 控制台 allowlist 中，拒绝调度".to_string()),
            );
        }
        if !binding.enabled {
            return self.preview_for_binding(
                &message,
                binding,
                ClawbotDispatchAction::RejectDisabledBinding,
                command,
                Some("当前微信联系人绑定已停用，拒绝调度".to_string()),
            );
        }
        if command.is_some() {
            return self.preview_for_binding(
                &message,
                binding,
                ClawbotDispatchAction::ApplyCommand,
                command,
                None,
            );
        }
        self.preview_for_binding(
            &message,
            binding,
            ClawbotDispatchAction::DispatchToCoolzhu,
            None,
            None,
        )
    }

    fn preview_for_binding(
        &self,
        message: &ClawbotInboundMessage,
        binding: &ClawbotConversationBinding,
        action: ClawbotDispatchAction,
        command: Option<ClawbotCommand>,
        reason: Option<String>,
    ) -> ClawbotInboundPreview {
        ClawbotInboundPreview {
            action,
            command,
            account_id: message.account_id.clone(),
            peer_id: message.peer_id.clone(),
            peer_name: message
                .peer_name
                .clone()
                .or_else(|| binding.peer_name.clone()),
            chat_room_id: binding.chat_room_id.clone(),
            session_id: binding.default_session_id.clone(),
            target_agent_ids: binding.target_agent_ids.clone(),
            workspace_id: Some(binding.workspace_id.clone()),
            context_token: message
                .context_token
                .clone()
                .or_else(|| binding.last_context_token.clone()),
            external_msg_id: message.external_msg_id.clone(),
            reason,
        }
    }
}

pub fn parse_clawbot_command(input: &str) -> Option<ClawbotCommand> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let Some((spec, rest)) = match_command_spec(trimmed) else {
        return Some(ClawbotCommand::Unknown {
            input: trimmed.to_string(),
        });
    };
    match spec.id {
        "help" => Some(ClawbotCommand::Help {
            topic: (!rest.is_empty()).then_some(rest),
        }),
        "commands" => Some(ClawbotCommand::Help { topic: None }),
        "room.select" if !rest.is_empty() => Some(ClawbotCommand::SelectRoom { room: rest }),
        "use" if !rest.is_empty() => Some(ClawbotCommand::UseSessionOrModel { selector: rest }),
        "target.select" if !rest.is_empty() => {
            let target_agent_ids = rest
                .split(|ch: char| ch == ',' || ch.is_whitespace())
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            if target_agent_ids.is_empty() {
                None
            } else {
                Some(ClawbotCommand::SelectTargets { target_agent_ids })
            }
        }
        "continue" if !rest.is_empty() => Some(ClawbotCommand::ContinueTask { task_id: rest }),
        "rooms" => Some(ClawbotCommand::ListRooms),
        "sessions" => Some(ClawbotCommand::ListSessions),
        "tasks" => Some(ClawbotCommand::ListTasks),
        "status" => Some(ClawbotCommand::Status),
        "new" => Some(ClawbotCommand::Registered {
            command_id: spec.id.to_string(),
            arguments: rest,
        }),
        _ => Some(ClawbotCommand::Registered {
            command_id: spec.id.to_string(),
            arguments: rest,
        }),
    }
}

pub fn command_reply_pages(command: &ClawbotCommand, max_chars: usize) -> Option<Vec<String>> {
    match command {
        ClawbotCommand::Help { topic: None } => {
            Some(render_help_pages(command_registry(), max_chars))
        }
        ClawbotCommand::Help { topic: Some(topic) } => Some(vec![render_command_help(topic)
            .unwrap_or_else(|| format!("没有找到命令主题：{topic}\n请发送 /help 查看完整目录。"))]),
        _ => None,
    }
}

pub fn command_required_capability(command: Option<&ClawbotCommand>) -> WechatCapability {
    let Some(command) = command else {
        return WechatCapability::Chat;
    };
    let id = match command {
        ClawbotCommand::SelectRoom { .. } => "room.select",
        ClawbotCommand::UseSessionOrModel { .. } => "use",
        ClawbotCommand::SelectTargets { .. } => "target.select",
        ClawbotCommand::ListRooms => "rooms",
        ClawbotCommand::ListSessions => "sessions",
        ClawbotCommand::ListTasks => "tasks",
        ClawbotCommand::ContinueTask { .. } => "continue",
        ClawbotCommand::Status => "status",
        ClawbotCommand::NewTurn => "new",
        ClawbotCommand::Help { .. } | ClawbotCommand::Unknown { .. } => "help",
        ClawbotCommand::Registered { command_id, .. } => command_id,
    };
    command_spec_by_id(id)
        .map(|spec| spec.capability)
        .unwrap_or(WechatCapability::Status)
}

pub fn command_id(command: &ClawbotCommand) -> &str {
    match command {
        ClawbotCommand::SelectRoom { .. } => "room.select",
        ClawbotCommand::UseSessionOrModel { .. } => "use",
        ClawbotCommand::SelectTargets { .. } => "target.select",
        ClawbotCommand::ListRooms => "rooms",
        ClawbotCommand::ListSessions => "sessions",
        ClawbotCommand::ListTasks => "tasks",
        ClawbotCommand::ContinueTask { .. } => "continue",
        ClawbotCommand::Status => "status",
        ClawbotCommand::NewTurn => "new",
        ClawbotCommand::Help { .. } | ClawbotCommand::Unknown { .. } => "help",
        ClawbotCommand::Registered { command_id, .. } => command_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inbound_message() -> ClawbotInboundMessage {
        ClawbotInboundMessage {
            account_id: "wx-a".to_string(),
            peer_id: "peer-1".to_string(),
            conversation_id: None,
            peer_name: Some("测试联系人".to_string()),
            sender_id: None,
            sender_name: None,
            is_group: false,
            group_event: None,
            mentioned_bot: false,
            mentions: vec![],
            raw_payload_summary: None,
            context_token: Some("ctx-1".to_string()),
            external_msg_id: "msg-guard-1".to_string(),
            kind: ClawbotMessageKind::Text,
            text: Some("继续任务".to_string()),
            media_refs: vec![],
            received_at_ms: 1,
        }
    }

    #[test]
    fn ingress_guard_accepts_only_first_hop_weixin_user_messages() {
        let accepted = ClawbotInboundEnvelope::weixin_user(inbound_message());
        let gateway_echo = ClawbotInboundEnvelope {
            source: ClawbotInboundSource::GatewayEcho,
            hop_count: 1,
            message: inbound_message(),
        };
        let replayed_user = ClawbotInboundEnvelope {
            source: ClawbotInboundSource::WeixinUser,
            hop_count: 1,
            message: inbound_message(),
        };

        assert_eq!(accepted.guard_action(), ClawbotIngressGuardAction::Accept);
        assert_eq!(
            gateway_echo.guard_action(),
            ClawbotIngressGuardAction::RejectRecursive
        );
        assert_eq!(
            replayed_user.guard_action(),
            ClawbotIngressGuardAction::RejectRecursive
        );
    }

    #[test]
    fn parse_room_and_use_commands() {
        assert_eq!(
            parse_clawbot_command("/room 项目A"),
            Some(ClawbotCommand::SelectRoom {
                room: "项目A".to_string()
            })
        );
        assert_eq!(
            parse_clawbot_command("/use GLM5.2"),
            Some(ClawbotCommand::UseSessionOrModel {
                selector: "GLM5.2".to_string()
            })
        );
    }

    #[test]
    fn parse_help_alias_registered_and_unknown_commands_without_model_fallback() {
        assert_eq!(
            parse_clawbot_command("/help file get"),
            Some(ClawbotCommand::Help {
                topic: Some("file get".to_string())
            })
        );
        assert_eq!(
            parse_clawbot_command("/commands"),
            Some(ClawbotCommand::Help { topic: None })
        );
        assert_eq!(
            parse_clawbot_command("/file get docs/report.md"),
            Some(ClawbotCommand::Registered {
                command_id: "file.get".to_string(),
                arguments: "docs/report.md".to_string(),
            })
        );
        assert_eq!(
            parse_clawbot_command("/definitely-unknown value"),
            Some(ClawbotCommand::Unknown {
                input: "/definitely-unknown value".to_string(),
            })
        );
        assert_eq!(parse_clawbot_command("普通聊天消息"), None);
    }

    #[test]
    fn parse_new_command_preserves_existing_session_selector() {
        assert_eq!(
            parse_clawbot_command("/new session-openai"),
            Some(ClawbotCommand::Registered {
                command_id: "new".to_string(),
                arguments: "session-openai".to_string(),
            })
        );
        assert_eq!(
            parse_clawbot_command("/new"),
            Some(ClawbotCommand::Registered {
                command_id: "new".to_string(),
                arguments: String::new(),
            })
        );
    }

    #[test]
    fn status_commands_are_generated_from_the_single_registry() {
        let status = ClawbotChannelState::default().status();

        assert_eq!(
            status.commands.len(),
            crate::wechat_command::command_registry().len()
        );
        assert!(status.commands.iter().any(|command| {
            command.id == "file.get"
                && command.syntax == "/file get <路径>"
                && command.capability == "files.read"
                && command.availability == "可用"
                && command.disabled_reason.is_none()
        }));
        assert!(status.commands.iter().any(|command| {
            command.id == "file.put"
                && command.syntax == "/file put <路径>"
                && command.capability == "files.write"
                && command.availability == "当前不可用"
                && command.disabled_reason.is_some()
        }));
    }

    #[test]
    fn help_command_produces_ordered_pages_instead_of_one_truncated_message() {
        let pages =
            command_reply_pages(&ClawbotCommand::Help { topic: None }, 420).expect("help pages");

        assert!(pages.len() > 1);
        assert!(pages[0].starts_with(&format!("微信连接命令 第 1/{} 页", pages.len())));
        assert!(pages.last().is_some_and(|page| page.starts_with(&format!(
            "微信连接命令 第 {}/{} 页",
            pages.len(),
            pages.len()
        ))));
    }

    #[test]
    fn inbound_message_keeps_group_sender_and_mention_identity_with_legacy_defaults() {
        let legacy: ClawbotInboundMessage = serde_json::from_value(serde_json::json!({
            "account_id": "wx-main",
            "peer_id": "peer-1",
            "peer_name": "联系人",
            "context_token": null,
            "external_msg_id": "msg-1",
            "kind": "text",
            "text": "hello",
            "media_refs": [],
            "received_at_ms": 1
        }))
        .expect("legacy inbound");
        assert!(!legacy.is_group);
        assert!(!legacy.mentioned_bot);
        assert!(legacy.sender_id.is_none());
        assert!(legacy.group_event.is_none());
        assert!(legacy.raw_payload_summary.is_none());

        let group: ClawbotInboundMessage = serde_json::from_value(serde_json::json!({
            "account_id": "wx-main",
            "peer_id": "group-1",
            "conversation_id": "group-1",
            "sender_id": "member-1",
            "sender_name": "测试成员",
            "is_group": true,
            "mentioned_bot": true,
            "mentions": ["wx-main"],
            "group_event": "bot_added",
            "raw_payload_summary": "keys=conversation_id,sender_id",
            "external_msg_id": "msg-2",
            "kind": "text",
            "text": "@机器人 /status",
            "media_refs": [],
            "received_at_ms": 2
        }))
        .expect("group inbound");
        assert_eq!(group.conversation_id.as_deref(), Some("group-1"));
        assert_eq!(group.sender_id.as_deref(), Some("member-1"));
        assert_eq!(group.sender_name.as_deref(), Some("测试成员"));
        assert!(group.is_group);
        assert!(group.mentioned_bot);
        assert_eq!(group.mentions, vec!["wx-main"]);
        assert_eq!(
            group.group_event,
            Some(crate::wechat_group::WechatGroupEventKind::BotAdded)
        );
        assert_eq!(
            group.raw_payload_summary.as_deref(),
            Some("keys=conversation_id,sender_id")
        );
    }

    #[test]
    fn required_capability_is_derived_from_the_same_command_registry() {
        assert_eq!(
            command_required_capability(Some(&ClawbotCommand::Registered {
                command_id: "file.get".to_string(),
                arguments: "docs/report.md".to_string(),
            })),
            crate::wechat_command::WechatCapability::FilesRead
        );
        assert_eq!(
            command_required_capability(Some(&ClawbotCommand::SelectRoom {
                room: "main-room".to_string(),
            })),
            crate::wechat_command::WechatCapability::BindingsAdmin
        );
        assert_eq!(
            command_required_capability(None),
            crate::wechat_command::WechatCapability::Chat
        );
    }

    #[test]
    fn bind_inbound_text_to_existing_conversation() {
        let state = ClawbotChannelState::default().with_binding(ClawbotConversationBinding {
            account_id: "wx-a".to_string(),
            peer_id: "peer-1".to_string(),
            peer_name: Some("测试联系人".to_string()),
            chat_room_id: Some("room-project-a".to_string()),
            default_session_id: Some("glm-session".to_string()),
            target_agent_ids: vec!["agent-test001".to_string()],
            workspace_id: "default".to_string(),
            last_context_token: Some("ctx-1".to_string()),
            allowlisted: true,
            enabled: true,
        });

        let preview = state.preview_inbound(ClawbotInboundMessage {
            account_id: "wx-a".to_string(),
            peer_id: "peer-1".to_string(),
            conversation_id: None,
            peer_name: Some("测试联系人".to_string()),
            sender_id: None,
            sender_name: None,
            is_group: false,
            group_event: None,
            mentioned_bot: false,
            mentions: vec![],
            raw_payload_summary: None,
            context_token: Some("ctx-2".to_string()),
            external_msg_id: "msg-1".to_string(),
            kind: ClawbotMessageKind::Text,
            text: Some("继续上个任务".to_string()),
            media_refs: vec![],
            received_at_ms: 1,
        });

        assert_eq!(preview.action, ClawbotDispatchAction::DispatchToCoolzhu);
        assert_eq!(preview.chat_room_id.as_deref(), Some("room-project-a"));
        assert_eq!(preview.session_id.as_deref(), Some("glm-session"));
        assert_eq!(preview.target_agent_ids, vec!["agent-test001"]);
    }

    #[test]
    fn authoritative_group_binding_overrides_and_can_remove_stale_json_binding() {
        let stale = ClawbotConversationBinding {
            account_id: "wx-a".to_string(),
            peer_id: "group-1".to_string(),
            peer_name: Some("旧群名".to_string()),
            chat_room_id: Some("stale-room".to_string()),
            default_session_id: Some("stale-session".to_string()),
            target_agent_ids: vec![],
            workspace_id: "default".to_string(),
            last_context_token: None,
            allowlisted: true,
            enabled: true,
        };
        let authoritative = ClawbotConversationBinding {
            chat_room_id: Some("main-room".to_string()),
            default_session_id: Some("glm-5.2".to_string()),
            ..stale.clone()
        };
        let mut state = ClawbotChannelState::default().with_binding(stale);
        let mut message = inbound_message();
        message.peer_id = "group-1".to_string();
        message.is_group = true;

        let preview = state.preview_inbound_with_binding(message.clone(), Some(&authoritative));
        assert_eq!(preview.chat_room_id.as_deref(), Some("main-room"));
        assert_eq!(preview.session_id.as_deref(), Some("glm-5.2"));

        assert!(state.remove_binding("wx-a", "group-1").is_some());
        assert_eq!(
            state.preview_inbound(message).action,
            ClawbotDispatchAction::NeedBinding
        );
    }

    #[test]
    fn default_status_exposes_disabled_text_only_capabilities() {
        let status = ClawbotChannelState::default().status();

        assert_eq!(status.channel, "weixin-clawbot");
        assert_eq!(status.status, ClawbotRuntimeStatus::Disabled);
        assert!(status.capabilities.contains(&"text".to_string()));
        assert!(status
            .commands
            .iter()
            .any(|command| command.name == "/room"));
    }

    #[test]
    fn apply_room_and_use_commands_updates_binding_route() {
        let mut state = ClawbotChannelState::default().with_binding(ClawbotConversationBinding {
            account_id: "wx-a".to_string(),
            peer_id: "peer-1".to_string(),
            peer_name: Some("测试联系人".to_string()),
            chat_room_id: Some("old-room".to_string()),
            default_session_id: Some("old-session".to_string()),
            target_agent_ids: vec![],
            workspace_id: "default".to_string(),
            last_context_token: None,
            allowlisted: true,
            enabled: true,
        });

        let room_result = state.apply_command(
            "wx-a",
            "peer-1",
            ClawbotCommand::SelectRoom {
                room: "project-room".to_string(),
            },
        );
        let use_result = state.apply_command(
            "wx-a",
            "peer-1",
            ClawbotCommand::UseSessionOrModel {
                selector: "GLM5.2".to_string(),
            },
        );
        let binding = state.binding("wx-a", "peer-1").expect("binding present");

        assert_eq!(room_result.status, ClawbotCommandApplyStatus::Applied);
        assert_eq!(use_result.status, ClawbotCommandApplyStatus::Applied);
        assert_eq!(binding.chat_room_id.as_deref(), Some("project-room"));
        assert_eq!(binding.default_session_id.as_deref(), Some("GLM5.2"));
    }

    #[test]
    fn persists_bindings_to_json_and_loads_them_back() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("clawbot-channel.json");
        let state = ClawbotChannelState::default().with_binding(ClawbotConversationBinding {
            account_id: "wx-a".to_string(),
            peer_id: "peer-1".to_string(),
            peer_name: Some("测试联系人".to_string()),
            chat_room_id: Some("room-project-a".to_string()),
            default_session_id: Some("glm-session".to_string()),
            target_agent_ids: vec!["agent-test001".to_string()],
            workspace_id: "default".to_string(),
            last_context_token: Some("ctx-1".to_string()),
            allowlisted: true,
            enabled: true,
        });

        state.save_to_path(&path).expect("save state");
        let loaded = ClawbotChannelState::load_from_path(&path).expect("load state");
        let binding = loaded.binding("wx-a", "peer-1").expect("binding present");

        assert_eq!(binding.chat_room_id.as_deref(), Some("room-project-a"));
        assert_eq!(binding.default_session_id.as_deref(), Some("glm-session"));
        assert_eq!(binding.target_agent_ids, vec!["agent-test001"]);
    }

    #[test]
    fn missing_persistence_file_loads_empty_state() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("missing-clawbot-channel.json");
        let loaded = ClawbotChannelState::load_from_path(&path).expect("load empty");

        assert!(loaded.bindings().is_empty());
    }
}
