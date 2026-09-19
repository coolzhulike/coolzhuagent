//! 会话级 reasoning 能力、解析与 provider wire 映射。
//!
//! 这里是 Web、会话存储和 provider payload builder 共用的唯一事实来源。
//! `MessageRequest.reasoning_effort` 仍保留为字符串是为了兼容旧 JSON/TOML，
//! 但任何出站请求都必须先经过本模块的解析和能力分辨率。

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::providers::{
    canonical_claude_model_id, ProviderKind, CLAUDE_HAIKU_45_LEGACY_MODEL_ID,
    CLAUDE_HAIKU_45_MODEL_ID,
};
use crate::resolver::ProviderProtocol;

/// 跨 provider 的 canonical 请求值。
///
/// `Auto` 表示不主动覆盖供应商默认，不等于某个固定档位；本项目不引入
/// Codex 私有的 `ultra` 或 `service-tier` 语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Auto,
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl Default for ReasoningEffort {
    fn default() -> Self {
        Self::Auto
    }
}

impl ReasoningEffort {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Auto => "自动",
            Self::None => "关闭",
            Self::Minimal => "极简",
            Self::Low => "低",
            Self::Medium => "中",
            Self::High => "高",
            Self::XHigh => "超高",
            Self::Max => "最大",
        }
    }

    /// 解析新 API 的 canonical 值。未知值必须显式失败，不能静默变成 medium。
    pub fn parse(value: &str) -> Result<Self, ReasoningParseError> {
        let raw = value.trim();
        let effort = match raw.to_ascii_lowercase().as_str() {
            "auto" => Self::Auto,
            "none" => Self::None,
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "xhigh" => Self::XHigh,
            "max" => Self::Max,
            _ => {
                return Err(ReasoningParseError {
                    raw: raw.to_string(),
                });
            }
        };
        Ok(effort)
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 解析错误面，便于 Web API 转成 400 并返回允许值。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningParseError {
    pub raw: String,
}

impl fmt::Display for ReasoningParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "未知 reasoning_effort 值 {:?}", self.raw)
    }
}

impl std::error::Error for ReasoningParseError {}

/// 解析可选的新请求值；缺省值是 `auto`。
pub fn parse_reasoning_effort(value: Option<&str>) -> Result<ReasoningEffort, ReasoningParseError> {
    match value {
        None => Ok(ReasoningEffort::Auto),
        Some(raw) if raw.trim().is_empty() => Err(ReasoningParseError { raw: raw.into() }),
        Some(raw) => ReasoningEffort::parse(raw),
    }
}

/// 旧 SQLite/JSON 的读取结果。旧值必须可读，但不能丢掉“发生过迁移”的证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyReasoningValue {
    pub value: ReasoningEffort,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<String>,
    pub used_fallback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// 读取旧数据时保守迁移：已知旧别名正常 canonicalize，未知值回退 auto 并标记。
#[must_use]
pub fn parse_legacy_reasoning_effort(value: Option<&str>) -> LegacyReasoningValue {
    let raw = value.map(str::trim).filter(|v| !v.is_empty());
    let Some(raw) = raw else {
        return LegacyReasoningValue {
            value: ReasoningEffort::Auto,
            raw: None,
            used_fallback: false,
            note: None,
        };
    };

    let normalized = raw.to_ascii_lowercase();
    let legacy_alias = match normalized.as_str() {
        "extra_high" | "extra-high" | "超高" => Some(ReasoningEffort::XHigh),
        "maximum" | "最" | "最高" => Some(ReasoningEffort::Max),
        "default" | "unset" => Some(ReasoningEffort::Auto),
        "关闭思考" | "关闭" | "off" => Some(ReasoningEffort::None),
        _ => None,
    };
    if let Some(value) = legacy_alias.or_else(|| ReasoningEffort::parse(raw).ok()) {
        return LegacyReasoningValue {
            value,
            raw: Some(raw.to_string()),
            used_fallback: false,
            note: None,
        };
    }

    LegacyReasoningValue {
        value: ReasoningEffort::Auto,
        raw: Some(raw.to_string()),
        used_fallback: true,
        note: Some(format!(
            "旧配置值 {:?} 未知，已安全回退 auto；未向 provider 添加 reasoning 字段",
            raw
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningCapabilityStatus {
    Verified,
    Unknown,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningStrategy {
    ProviderDefault,
    OpenAiReasoningEffort,
    AnthropicAdaptiveThinking,
    DeepSeekThinking,
    ZhipuThinking,
    QwenThinking,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningOption {
    pub value: ReasoningEffort,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl ReasoningOption {
    fn new(value: ReasoningEffort) -> Self {
        Self {
            value,
            label: value.label().to_string(),
            note: None,
        }
    }
}

/// provider + model 的 reasoning manifest/descriptor。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningCapability {
    pub provider_id: String,
    pub provider_label: String,
    pub model_id: String,
    pub model_label: String,
    pub api_model_id: String,
    pub supported_options: Vec<ReasoningOption>,
    #[serde(rename = "default")]
    pub default_reasoning: ReasoningEffort,
    pub strategy: ReasoningStrategy,
    pub protocol: ProviderProtocol,
    pub status: ReasoningCapabilityStatus,
    pub deprecated: bool,
    pub note: String,
}

impl ReasoningCapability {
    #[must_use]
    pub fn supported_values(&self) -> Vec<ReasoningEffort> {
        self.supported_options.iter().map(|item| item.value).collect()
    }

    #[must_use]
    pub fn supports(&self, effort: ReasoningEffort) -> bool {
        self.supported_options
            .iter()
            .any(|item| item.value == effort)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningResolutionStatus {
    Default,
    Exact,
    Downgraded,
    Unsupported,
    LegacyFallback,
}

/// 请求值到当前 model/provider 的 effective/preflight wire 结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningResolution {
    pub requested: ReasoningEffort,
    pub effective: ReasoningEffort,
    pub status: ReasoningResolutionStatus,
    pub strategy: ReasoningStrategy,
    pub protocol: ProviderProtocol,
    pub preflight_wire: ReasoningWire,
    pub reason: String,
    pub supported_options: Vec<ReasoningOption>,
}

impl ReasoningResolution {
    #[must_use]
    pub fn wire(&self) -> &ReasoningWire {
        &self.preflight_wire
    }
}

/// provider payload 中 reasoning 的原生表示。`Omit` 是有意的安全结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReasoningWire {
    Omit,
    OpenAi {
        reasoning_effort: String,
    },
    AnthropicAdaptive {
        thinking_type: String,
        output_effort: String,
    },
    AnthropicDisabled,
    DeepSeek {
        thinking_type: String,
        reasoning_effort: Option<String>,
    },
    Zhipu {
        thinking_type: String,
        reasoning_effort: Option<String>,
    },
    Qwen {
        enable_thinking: bool,
        reasoning_effort: Option<String>,
    },
}

impl ReasoningWire {
    /// 将原生映射应用到 JSON payload；调用方负责先构造其它通用字段。
    pub fn apply_to_payload(&self, payload: &mut Value) {
        match self {
            Self::Omit => {}
            Self::OpenAi { reasoning_effort } => {
                payload["reasoning_effort"] = json!(reasoning_effort);
            }
            Self::AnthropicAdaptive {
                thinking_type,
                output_effort,
            } => {
                payload["thinking"] = json!({ "type": thinking_type });
                payload["output_config"] = json!({ "effort": output_effort });
            }
            Self::AnthropicDisabled => {
                payload["thinking"] = json!({ "type": "disabled" });
            }
            Self::DeepSeek {
                thinking_type,
                reasoning_effort,
            }
            | Self::Zhipu {
                thinking_type,
                reasoning_effort,
            } => {
                payload["thinking"] = json!({ "type": thinking_type });
                if let Some(effort) = reasoning_effort {
                    payload["reasoning_effort"] = json!(effort);
                }
            }
            Self::Qwen { enable_thinking, reasoning_effort } => {
                // Qwen3.8 的 effort 与 budget 互斥，也不接受通用 thinking 对象。
                if let Some(object) = payload.as_object_mut() {
                    object.remove("thinking");
                    object.remove("thinking_budget");
                    object.remove("output_config");
                    object.remove("reasoning_effort");
                }
                payload["enable_thinking"] = json!(enable_thinking);
                if let Some(effort) = reasoning_effort {
                    payload["reasoning_effort"] = json!(effort);
                }
            }
        }
    }
}

/// 只匹配已核验的 Qwen3.8 模型，Omni 和未知后缀不继承此协议。
/// 来源：https://www.alibabacloud.com/help/en/model-studio/qwen-api-via-openai-chat-completions
pub(crate) fn is_qwen38_reasoning_model(model: &str) -> bool {
    matches!(model.trim().to_ascii_lowercase().as_str(),
        "qwen3.8-flash" | "qwen3.8-max" | "qwen3.8-max-0902"
        | "qwen3.8-2.4t-a95b" | "qwen3.8-27b")
}

fn normalized_provider_id(provider: &str) -> Option<ProviderKind> {
    let normalized = provider
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "");
    match normalized.as_str() {
        "clawapi" | "claw" | "anthropic" => Some(ProviderKind::ClawApi),
        "openai" => Some(ProviderKind::OpenAi),
        "xai" | "grok" => Some(ProviderKind::Xai),
        "zhipuai" | "zhipu" | "zai" | "bigmodel" | "智谱ai" | "智谱" => {
            Some(ProviderKind::ZhipuAi)
        }
        "alibababailian" | "alibaba" | "aliyun" | "bailian" | "dashscope" => {
            Some(ProviderKind::AlibabaBailian)
        }
        "baidu" | "baiduqianfan" | "qianfan" | "wenxin" => {
            Some(ProviderKind::BaiduQianfan)
        }
        "bytedance" | "bytedanceark" | "ark" | "volcengine" | "doubao" => {
            Some(ProviderKind::ByteDanceArk)
        }
        "deepseek" => Some(ProviderKind::DeepSeek),
        "custom" | "customopenai" | "customopenaicompatible" | "ollama" => {
            Some(ProviderKind::Custom)
        }
        _ => None,
    }
}

fn canonical_provider_id(kind: Option<ProviderKind>) -> String {
    match kind {
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic) => "clawapi".to_string(),
        Some(kind) => kind.id(),
        None => "unknown".to_string(),
    }
}

fn provider_label(kind: Option<ProviderKind>) -> String {
    match kind {
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic) => "Anthropic / ClawAPI".to_string(),
        Some(ProviderKind::OpenAi) => "OpenAI".to_string(),
        Some(ProviderKind::Xai) => "xAI".to_string(),
        Some(ProviderKind::ZhipuAi) => "Zhipu AI".to_string(),
        Some(ProviderKind::AlibabaBailian) => "Alibaba Bailian".to_string(),
        Some(ProviderKind::BaiduQianfan) => "Baidu Qianfan".to_string(),
        Some(ProviderKind::ByteDanceArk) => "ByteDance Ark".to_string(),
        Some(ProviderKind::DeepSeek) => "DeepSeek".to_string(),
        Some(ProviderKind::Custom) => "Custom OpenAI-compatible".to_string(),
        None => "未知 provider".to_string(),
    }
}

fn model_label(model: &str) -> String {
    match model.to_ascii_lowercase().as_str() {
        "claude-opus-4-6" => "Claude Opus 4.6".to_string(),
        "claude-sonnet-4-6" => "Claude Sonnet 4.6".to_string(),
        "claude-haiku-4-5-20251001" => "Claude Haiku 4.5".to_string(),
        "gpt-4.1" => "GPT-4.1".to_string(),
        "gpt-4.1-mini" => "GPT-4.1 Mini".to_string(),
        "gpt-4o-mini" => "GPT-4o Mini".to_string(),
        "grok-3" => "Grok 3".to_string(),
        "grok-3-mini" => "Grok 3 Mini".to_string(),
        "grok-2" => "Grok 2".to_string(),
        "grok-4.6" => "Grok 4.6".to_string(),
        "grok-4.5" => "Grok 4.5".to_string(),
        "grok-4.3" => "Grok 4.3".to_string(),
        "glm-4.7" => "GLM-4.7".to_string(),
        "glm-4.7-flash" => "GLM-4.7 Flash".to_string(),
        "glm-4.6v-flash" => "GLM-4.6V Flash".to_string(),
        "glm-free" => "GLM Free".to_string(),
        "glm-5" => "GLM-5".to_string(),
        "glm-5.2" => "GLM-5.2".to_string(),
        "glm-5.3" => "GLM-5.3".to_string(),
        "qwen-plus" => "Qwen Plus".to_string(),
        "qwen-turbo" => "Qwen Turbo".to_string(),
        "qwen-max" => "Qwen Max".to_string(),
        "qwen3.7-max" => "Qwen3.7 Max".to_string(),
        "ernie-4.5-turbo-128k" => "ERNIE 4.5 Turbo 128K".to_string(),
        "ernie-x1-turbo-32k" => "ERNIE X1 Turbo 32K".to_string(),
        "doubao-1-5-pro-32k-250115" => "Doubao 1.5 Pro 32K".to_string(),
        "doubao-1-5-lite-32k-250115" => "Doubao 1.5 Lite 32K".to_string(),
        "deepseek-v4-flash" => "DeepSeek V4 Flash".to_string(),
        "deepseek-v4-pro" => "DeepSeek V4 Pro".to_string(),
        "deepseek-chat" => "DeepSeek Chat".to_string(),
        "deepseek-reasoner" => "DeepSeek Reasoner".to_string(),
        other => other.to_string(),
    }
}

fn api_model_id_for(model: &str) -> String {
    if model.trim().eq_ignore_ascii_case("glm-free") {
        "glm-4-flash".to_string()
    } else {
        model.trim().to_string()
    }
}

fn options(values: &[ReasoningEffort]) -> Vec<ReasoningOption> {
    values.iter().copied().map(ReasoningOption::new).collect()
}

/// 返回 provider + model 的 bundled descriptor；未核验模型严格使用 auto/omit。
#[must_use]
pub fn reasoning_capability_for_provider(
    provider: &str,
    model: &str,
) -> ReasoningCapability {
    let kind = normalized_provider_id(provider);
    let canonical_provider = canonical_provider_id(kind);
    let canonical_model = match kind {
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic) => canonical_claude_model_id(model),
        _ => model.trim().to_string(),
    };
    let lower_model = canonical_model.to_ascii_lowercase();
    let protocol = match kind {
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic) => ProviderProtocol::AnthropicMessages,
        _ => ProviderProtocol::OpenAiChatCompletions,
    };

    let (supported, default_reasoning, strategy, status, deprecated, note) = match kind {
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic)
            if lower_model == "claude-opus-4-6" || lower_model == "claude-sonnet-4-6" => (
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::None,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ],
            ReasoningEffort::Auto,
            ReasoningStrategy::AnthropicAdaptiveThinking,
            ReasoningCapabilityStatus::Verified,
            false,
            "Anthropic Messages：auto 不覆盖；low/medium/high/max 使用 adaptive thinking + output_config.effort；none 使用原生 disabled。".to_string(),
        ),
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic)
            if lower_model == "claude-haiku-4-5-20251001" => (
            vec![ReasoningEffort::Auto, ReasoningEffort::None],
            ReasoningEffort::Auto,
            ReasoningStrategy::AnthropicAdaptiveThinking,
            ReasoningCapabilityStatus::Verified,
            false,
            "Haiku 4.5 本切片不伪造 budget_tokens；仅允许供应商默认或安全关闭。".to_string(),
        ),
        Some(ProviderKind::OpenAi)
            if matches!(lower_model.as_str(), "gpt-4.1" | "gpt-4.1-mini" | "gpt-4o-mini") => (
            vec![ReasoningEffort::Auto, ReasoningEffort::None],
            ReasoningEffort::Auto,
            ReasoningStrategy::ProviderDefault,
            ReasoningCapabilityStatus::Verified,
            false,
            "当前内置 GPT-4.1/GPT-4.1 Mini/GPT-4o Mini 按 non-reasoning 处理；auto/none 均不发送 reasoning 字段。".to_string(),
        ),
        Some(ProviderKind::Xai) if lower_model == "grok-4.6" => (
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
                ReasoningEffort::XHigh,
            ],
            ReasoningEffort::Auto,
            ReasoningStrategy::OpenAiReasoningEffort,
            ReasoningCapabilityStatus::Verified,
            false,
            "xAI Grok 4.6：thinking 强制开启，仅发送明确支持的 low/medium/high/xhigh；不提供关闭或 max。".to_string(),
        ),
        Some(ProviderKind::Xai) if lower_model == "grok-4.5" => (
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
            ],
            ReasoningEffort::Auto,
            ReasoningStrategy::OpenAiReasoningEffort,
            ReasoningCapabilityStatus::Verified,
            false,
            "xAI Grok 4.5：仅发送明确支持的 low/medium/high；旧 xhigh 会在 preflight 显式降为 high。".to_string(),
        ),
        Some(ProviderKind::Xai) if lower_model == "grok-4.3" => (
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::None,
                ReasoningEffort::Low,
                ReasoningEffort::Medium,
                ReasoningEffort::High,
            ],
            ReasoningEffort::Auto,
            ReasoningStrategy::OpenAiReasoningEffort,
            ReasoningCapabilityStatus::Verified,
            false,
            "xAI Grok 4.3：支持 auto/关闭及 low/medium/high，wire 使用顶层 reasoning_effort。".to_string(),
        ),
        _ if protocol != ProviderProtocol::AnthropicMessages && is_qwen38_reasoning_model(model) => (
            vec![ReasoningEffort::Auto, ReasoningEffort::None, ReasoningEffort::Low,
                ReasoningEffort::Medium, ReasoningEffort::XHigh],
            ReasoningEffort::Auto,
            ReasoningStrategy::QwenThinking,
            ReasoningCapabilityStatus::Verified,
            false,
            "Qwen3.8：auto 保留默认；none 发送 enable_thinking=false；low/medium/xhigh 使用 enable_thinking=true；minimal→low，high/max→xhigh。effort 与 thinking_budget 互斥。".to_string(),
        ),
        Some(ProviderKind::DeepSeek)
            if matches!(lower_model.as_str(), "deepseek-chat" | "deepseek-reasoner") => (
            vec![ReasoningEffort::Auto],
            ReasoningEffort::Auto,
            ReasoningStrategy::ProviderDefault,
            ReasoningCapabilityStatus::Deprecated,
            true,
            "该 DeepSeek 模型已退役，仅为旧会话兼容保留；不再发送 reasoning 字段。".to_string(),
        ),
        Some(ProviderKind::DeepSeek) if lower_model.starts_with("deepseek-v4") => (
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::None,
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ],
            ReasoningEffort::Auto,
            ReasoningStrategy::DeepSeekThinking,
            ReasoningCapabilityStatus::Verified,
            false,
            "DeepSeek V4：auto 不覆盖供应商默认；none 使用 thinking.disabled；low/high/max 使用 enabled + reasoning_effort。medium/xhigh 预解析为 high。".to_string(),
        ),
        Some(ProviderKind::ZhipuAi) if lower_model.starts_with("glm-5.2") => (
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::None,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ],
            ReasoningEffort::Auto,
            ReasoningStrategy::ZhipuThinking,
            ReasoningCapabilityStatus::Verified,
            false,
            "GLM-5.2：none/minimal 关闭；low/medium→high；xhigh→max。".to_string(),
        ),
        Some(ProviderKind::ZhipuAi) if lower_model.starts_with("glm-5.3") => (
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ],
            ReasoningEffort::Auto,
            ReasoningStrategy::ZhipuThinking,
            ReasoningCapabilityStatus::Verified,
            false,
            "GLM-5.3 强制 thinking 开启：仅声明 auto/low/high/max，不支持关闭。".to_string(),
        ),
        Some(ProviderKind::ZhipuAi)
            if lower_model == "glm-5"
                || lower_model == "glm-4.6"
                || lower_model == "glm-4.7"
                || lower_model == "glm-4.7-flash"
                || lower_model == "glm-free" => (
            vec![ReasoningEffort::Auto, ReasoningEffort::None],
            ReasoningEffort::Auto,
            ReasoningStrategy::ZhipuThinking,
            ReasoningCapabilityStatus::Verified,
            false,
            "该 Zhipu 模型本切片只声明 auto/关闭 thinking，不宣称可调 effort。".to_string(),
        ),
        Some(ProviderKind::Xai)
            if matches!(lower_model.as_str(), "grok-3" | "grok-3-mini" | "grok-2") => (
            vec![ReasoningEffort::Auto, ReasoningEffort::None],
            ReasoningEffort::Auto,
            ReasoningStrategy::ProviderDefault,
            ReasoningCapabilityStatus::Deprecated,
            true,
            "旧 grok slug 已 deprecated，按供应商新模型/redirect 处理；本切片不宣称 max 或其它 effort。".to_string(),
        ),
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic) => (
            vec![ReasoningEffort::Auto],
            ReasoningEffort::Auto,
            ReasoningStrategy::ProviderDefault,
            ReasoningCapabilityStatus::Unknown,
            false,
            "未核验的 Anthropic/Claw 模型只使用供应商默认，未知值不发送 reasoning 字段。".to_string(),
        ),
        _ => (
            vec![ReasoningEffort::Auto],
            ReasoningEffort::Auto,
            ReasoningStrategy::ProviderDefault,
            ReasoningCapabilityStatus::Unknown,
            false,
            "当前没有该 provider/model 的精确 reasoning 协议证据，仅使用供应商默认并省略字段。".to_string(),
        ),
    };

    ReasoningCapability {
        provider_id: canonical_provider,
        provider_label: provider_label(kind),
        model_id: model.trim().to_string(),
        model_label: model_label(&canonical_model),
        api_model_id: api_model_id_for(&canonical_model),
        supported_options: options(&supported),
        default_reasoning,
        strategy,
        protocol,
        status,
        deprecated,
        note,
    }
}

/// 返回当前能力目录条目的显式历史模型别名。
///
/// 别名只用于让旧会话继续在能力设置中显示其原始 model id；实际 provider
/// registry 与出站 payload 仍统一使用 canonical/API model id。这里不把别名
/// 反向加入 39 条当前目录，避免把历史标识误当成新模型。
#[must_use]
pub fn reasoning_model_aliases(provider: &str, model: &str) -> Vec<String> {
    let kind = normalized_provider_id(provider);
    let canonical_model = match kind {
        Some(ProviderKind::ClawApi | ProviderKind::Anthropic) => canonical_claude_model_id(model),
        _ => model.trim().to_string(),
    };
    if matches!(kind, Some(ProviderKind::ClawApi | ProviderKind::Anthropic))
        && canonical_model.eq_ignore_ascii_case(CLAUDE_HAIKU_45_MODEL_ID)
    {
        vec![CLAUDE_HAIKU_45_LEGACY_MODEL_ID.to_string()]
    } else {
        Vec::new()
    }
}

/// provider id 已 canonical/别名均可传入的 resolver 入口。
pub fn resolve_reasoning(
    provider: &str,
    model: &str,
    requested: Option<&str>,
) -> Result<ReasoningResolution, ReasoningParseError> {
    let requested = parse_reasoning_effort(requested)?;
    Ok(resolve_parsed_reasoning(provider, model, requested, false))
}

/// 旧数据解析入口：未知值安全回退 auto，并让返回结果保留 legacy 原因。
#[must_use]
pub fn resolve_legacy_reasoning(
    provider: &str,
    model: &str,
    requested: Option<&str>,
) -> ReasoningResolution {
    let parsed = parse_legacy_reasoning_effort(requested);
    let mut resolution = resolve_parsed_reasoning(provider, model, parsed.value, parsed.used_fallback);
    if parsed.used_fallback {
        resolution.status = ReasoningResolutionStatus::LegacyFallback;
        resolution.reason = parsed.note.unwrap_or_else(|| resolution.reason.clone());
    }
    resolution
}

fn resolve_parsed_reasoning(
    provider: &str,
    model: &str,
    requested: ReasoningEffort,
    legacy_fallback: bool,
) -> ReasoningResolution {
    let capability = reasoning_capability_for_provider(provider, model);
    let mut effective = requested;
    let mut status = if requested == ReasoningEffort::Auto {
        ReasoningResolutionStatus::Default
    } else {
        ReasoningResolutionStatus::Exact
    };
    let mut reason = if requested == ReasoningEffort::Auto {
        "requested=auto：不主动覆盖供应商默认".to_string()
    } else {
        "requested 值由当前 provider/model manifest 原样支持".to_string()
    };

    let wire = match capability.strategy {
        ReasoningStrategy::AnthropicAdaptiveThinking => match requested {
            ReasoningEffort::Auto => ReasoningWire::Omit,
            ReasoningEffort::None => ReasoningWire::AnthropicDisabled,
            ReasoningEffort::Low
            | ReasoningEffort::Medium
            | ReasoningEffort::High
            | ReasoningEffort::Max => ReasoningWire::AnthropicAdaptive {
                thinking_type: "adaptive".to_string(),
                output_effort: requested.as_str().to_string(),
            },
            ReasoningEffort::Minimal | ReasoningEffort::XHigh => {
                effective = ReasoningEffort::Auto;
                status = ReasoningResolutionStatus::Downgraded;
                reason = format!(
                    "当前 Anthropic manifest 不支持 {}，已回退 auto；未发送通用 reasoning_effort",
                    requested.as_str()
                );
                ReasoningWire::Omit
            }
        },
        ReasoningStrategy::DeepSeekThinking => match requested {
            ReasoningEffort::Auto => ReasoningWire::Omit,
            ReasoningEffort::None => ReasoningWire::DeepSeek {
                thinking_type: "disabled".to_string(),
                reasoning_effort: None,
            },
            ReasoningEffort::Low | ReasoningEffort::High | ReasoningEffort::Max => {
                ReasoningWire::DeepSeek {
                    thinking_type: "enabled".to_string(),
                    reasoning_effort: Some(requested.as_str().to_string()),
                }
            }
            ReasoningEffort::Medium | ReasoningEffort::XHigh => {
                effective = ReasoningEffort::High;
                status = ReasoningResolutionStatus::Downgraded;
                reason = format!(
                    "DeepSeek V4 原生仅接受 low/high/max，{} 已映射 high",
                    requested.as_str()
                );
                ReasoningWire::DeepSeek {
                    thinking_type: "enabled".to_string(),
                    reasoning_effort: Some("high".to_string()),
                }
            }
            ReasoningEffort::Minimal => {
                effective = ReasoningEffort::Auto;
                status = ReasoningResolutionStatus::Unsupported;
                reason = "DeepSeek V4 未声明 minimal，安全回退 auto".to_string();
                ReasoningWire::Omit
            }
        },
        ReasoningStrategy::ZhipuThinking => {
            let model_lower = model.trim().to_ascii_lowercase();
            if model_lower.starts_with("glm-5.2") {
                match requested {
                    ReasoningEffort::Auto => ReasoningWire::Omit,
                    ReasoningEffort::None | ReasoningEffort::Minimal => {
                        if requested == ReasoningEffort::Minimal {
                            status = ReasoningResolutionStatus::Downgraded;
                            reason = "GLM-5.2 的 minimal 按官方兼容语义映射为 none（关闭）".to_string();
                        }
                        effective = ReasoningEffort::None;
                        ReasoningWire::Zhipu {
                            thinking_type: "disabled".to_string(),
                            reasoning_effort: None,
                        }
                    }
                    ReasoningEffort::Low | ReasoningEffort::Medium => {
                        effective = ReasoningEffort::High;
                        status = ReasoningResolutionStatus::Downgraded;
                        reason = format!(
                            "GLM-5.2 的 {} 按官方兼容语义映射 high",
                            requested.as_str()
                        );
                        ReasoningWire::Zhipu {
                            thinking_type: "enabled".to_string(),
                            reasoning_effort: Some("high".to_string()),
                        }
                    }
                    ReasoningEffort::High => ReasoningWire::Zhipu {
                        thinking_type: "enabled".to_string(),
                        reasoning_effort: Some("high".to_string()),
                    },
                    ReasoningEffort::XHigh | ReasoningEffort::Max => {
                        if requested == ReasoningEffort::XHigh {
                            status = ReasoningResolutionStatus::Downgraded;
                            reason = "GLM-5.2 的 xhigh 按官方兼容语义映射 max".to_string();
                        }
                        effective = ReasoningEffort::Max;
                        ReasoningWire::Zhipu {
                            thinking_type: "enabled".to_string(),
                            reasoning_effort: Some("max".to_string()),
                        }
                    }
                }
            } else if model_lower.starts_with("glm-5.3") {
                match requested {
                    ReasoningEffort::Auto => ReasoningWire::Omit,
                    ReasoningEffort::Low | ReasoningEffort::High | ReasoningEffort::Max => {
                        ReasoningWire::Zhipu {
                            thinking_type: "enabled".to_string(),
                            reasoning_effort: Some(requested.as_str().to_string()),
                        }
                    }
                    _ => {
                        effective = ReasoningEffort::Auto;
                        status = ReasoningResolutionStatus::Unsupported;
                        reason = format!(
                            "GLM-5.3 未声明 {}，安全回退 auto",
                            requested.as_str()
                        );
                        ReasoningWire::Omit
                    }
                }
            } else {
                match requested {
                    ReasoningEffort::Auto => ReasoningWire::Omit,
                    ReasoningEffort::None => ReasoningWire::Zhipu {
                        thinking_type: "disabled".to_string(),
                        reasoning_effort: None,
                    },
                    _ => {
                        effective = ReasoningEffort::Auto;
                        status = ReasoningResolutionStatus::Unsupported;
                        reason = format!(
                            "当前 Zhipu 模型只声明 auto/关闭 thinking，{} 不发送",
                            requested.as_str()
                        );
                        ReasoningWire::Omit
                    }
                }
            }
        }
        ReasoningStrategy::OpenAiReasoningEffort => {
            let model_lower = model.trim().to_ascii_lowercase();
            if requested == ReasoningEffort::Auto {
                ReasoningWire::Omit
            } else if requested == ReasoningEffort::None && capability.supports(requested) {
                // 对明确支持关闭的型号发送原生 none；省略会让 provider 回到默认 thinking。
                ReasoningWire::OpenAi {
                    reasoning_effort: "none".to_string(),
                }
            } else if capability.supports(requested) {
                ReasoningWire::OpenAi {
                    reasoning_effort: requested.as_str().to_string(),
                }
            } else if model_lower == "grok-4.5" && requested == ReasoningEffort::XHigh {
                effective = ReasoningEffort::High;
                status = ReasoningResolutionStatus::Downgraded;
                reason = "Grok 4.5 的 legacy xhigh 已明确降为 high".to_string();
                ReasoningWire::OpenAi {
                    reasoning_effort: "high".to_string(),
                }
            } else {
                effective = ReasoningEffort::Auto;
                status = ReasoningResolutionStatus::Unsupported;
                reason = format!(
                    "当前 xAI 模型未声明 {}，安全回退 auto 并省略字段",
                    requested.as_str()
                );
                ReasoningWire::Omit
            }
        }
        ReasoningStrategy::QwenThinking => {
            match requested {
                ReasoningEffort::Auto => ReasoningWire::Omit,
                ReasoningEffort::None => ReasoningWire::Qwen {
                    enable_thinking: false, reasoning_effort: None,
                },
                _ => {
                    effective = match requested {
                        ReasoningEffort::Minimal => ReasoningEffort::Low,
                        ReasoningEffort::High | ReasoningEffort::Max => ReasoningEffort::XHigh,
                        value => value,
                    };
                    if effective != requested {
                        status = ReasoningResolutionStatus::Downgraded;
                        reason = format!("Qwen3.8 原生支持 low/medium/xhigh，{} 已兼容映射为 {}",
                            requested.as_str(), effective.as_str());
                    }
                    ReasoningWire::Qwen {
                        enable_thinking: true,
                        reasoning_effort: Some(effective.as_str().to_string()),
                    }
                }
            }
        }
        ReasoningStrategy::ProviderDefault => {
            // 本切片只对有明确原生协议证据的分支发字段。OpenAI 内置 GPT、
            // xAI 旧 slug、Alibaba/Baidu/Ark/Custom 与未知模型全部走 omission。
            if requested == ReasoningEffort::Auto
                || (requested == ReasoningEffort::None && capability.supports(requested))
            {
                ReasoningWire::Omit
            } else {
                effective = ReasoningEffort::Auto;
                status = ReasoningResolutionStatus::Unsupported;
                reason = format!(
                    "当前 provider/model 未声明可调 reasoning，{} 已安全回退 auto 并省略字段",
                    requested.as_str()
                );
                ReasoningWire::Omit
            }
        }
    };

    if legacy_fallback {
        status = ReasoningResolutionStatus::LegacyFallback;
    }

    ReasoningResolution {
        requested,
        effective,
        status,
        strategy: capability.strategy,
        protocol: capability.protocol,
        preflight_wire: wire,
        reason,
        supported_options: capability.supported_options,
    }
}

/// 按固定顺序返回 Web/API 使用的 bundled capability catalog。
#[must_use]
pub fn reasoning_capability_catalog() -> Vec<ReasoningCapability> {
    let models: &[(&str, &str)] = &[
        ("clawapi", "claude-opus-4-6"),
        ("clawapi", "claude-sonnet-4-6"),
        ("clawapi", "claude-haiku-4-5-20251001"),
        ("openai", "gpt-4.1"),
        ("openai", "gpt-4.1-mini"),
        ("openai", "gpt-4o-mini"),
        ("xai", "grok-3"),
        ("xai", "grok-3-mini"),
        ("xai", "grok-2"),
        ("xai", "grok-4.6"),
        ("xai", "grok-4.5"),
        ("xai", "grok-4.3"),
        ("zhipuai", "glm-4.7"),
        ("zhipuai", "glm-4.6"),
        ("zhipuai", "glm-4.7-flash"),
        ("zhipuai", "glm-4.6v-flash"),
        ("zhipuai", "glm-free"),
        ("zhipuai", "glm-5"),
        ("zhipuai", "glm-5.2"),
        ("zhipuai", "glm-5.3"),
        ("alibaba-bailian", "qwen3.7-max"),
        ("alibaba-bailian", "qwen-plus"),
        ("alibaba-bailian", "qwen-turbo"),
        ("alibaba-bailian", "qwen-max"),
        ("alibaba-bailian", "glm-5.2"),
        ("alibaba-bailian", "glm-5.1"),
        ("alibaba-bailian", "glm-5"),
        ("baidu", "ernie-4.5-turbo-128k"),
        ("baidu", "ernie-x1-turbo-32k"),
        ("bytedance", "doubao-1-5-pro-32k-250115"),
        ("bytedance", "doubao-1-5-lite-32k-250115"),
        ("deepseek", "deepseek-v4-flash"),
        ("deepseek", "deepseek-v4-pro"),
        ("deepseek", "deepseek-chat"),
        ("deepseek", "deepseek-reasoner"),
        ("custom", "custom-model"),
        ("custom", "qwen2.5-vl-3b"),
        ("custom", "showui"),
        ("custom", "llama3.1"),
    ];
    models
        .iter()
        .map(|(provider, model)| {
            let mut capability = reasoning_capability_for_provider(provider, model);
            capability.model_label = model_label(model);
            capability
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_and_legacy_parsing_are_distinct() {
        assert_eq!(ReasoningEffort::parse("auto"), Ok(ReasoningEffort::Auto));
        assert_eq!(ReasoningEffort::parse("none"), Ok(ReasoningEffort::None));
        assert!(ReasoningEffort::parse("future-level").is_err());
        let legacy = parse_legacy_reasoning_effort(Some("future-level"));
        assert_eq!(legacy.value, ReasoningEffort::Auto);
        assert!(legacy.used_fallback);
        assert_eq!(
            parse_legacy_reasoning_effort(Some("off")).value,
            ReasoningEffort::None
        );
        assert_eq!(
            parse_legacy_reasoning_effort(Some("关闭思考")).value,
            ReasoningEffort::None
        );
        assert_eq!(
            parse_legacy_reasoning_effort(Some("default")).value,
            ReasoningEffort::Auto
        );
        assert_eq!(parse_legacy_reasoning_effort(None).value, ReasoningEffort::Auto);
    }

    #[test]
    fn anthropic_46_uses_native_adaptive_and_disabled_wire() {
        let adaptive = resolve_reasoning("clawapi", "claude-opus-4-6", Some("high")).unwrap();
        assert_eq!(adaptive.effective, ReasoningEffort::High);
        assert_eq!(
            adaptive.preflight_wire,
            ReasoningWire::AnthropicAdaptive {
                thinking_type: "adaptive".into(),
                output_effort: "high".into(),
            }
        );
        let disabled = resolve_reasoning("anthropic", "claude-sonnet-4-6", Some("none")).unwrap();
        assert_eq!(disabled.preflight_wire, ReasoningWire::AnthropicDisabled);
    }

    #[test]
    fn openai_and_unknown_models_omit_reasoning() {
        let openai = resolve_reasoning("openai", "gpt-4.1", Some("medium")).unwrap();
        assert_eq!(openai.effective, ReasoningEffort::Auto);
        assert_eq!(openai.preflight_wire, ReasoningWire::Omit);
        let custom = resolve_reasoning("custom", "my-local-model", Some("max")).unwrap();
        assert_eq!(custom.preflight_wire, ReasoningWire::Omit);
    }

    #[test]
    fn deepseek_and_zhipu_compatibility_mapping_is_explicit() {
        let deepseek = resolve_reasoning("deepseek", "deepseek-v4-pro", Some("medium")).unwrap();
        assert_eq!(deepseek.effective, ReasoningEffort::High);
        assert_eq!(deepseek.status, ReasoningResolutionStatus::Downgraded);
        assert_eq!(
            deepseek.preflight_wire,
            ReasoningWire::DeepSeek {
                thinking_type: "enabled".into(),
                reasoning_effort: Some("high".into()),
            }
        );
        let zhipu = resolve_reasoning("zhipuai", "glm-5.2", Some("xhigh")).unwrap();
        assert_eq!(zhipu.effective, ReasoningEffort::Max);
        assert_eq!(zhipu.status, ReasoningResolutionStatus::Downgraded);
    }

    #[test]
    fn glm_53_is_forced_thinking_and_cannot_be_disabled() {
        let capability = reasoning_capability_for_provider("zhipuai", "glm-5.3");
        let supported = capability.supported_values();
        assert_eq!(
            supported,
            vec![
                ReasoningEffort::Auto,
                ReasoningEffort::Low,
                ReasoningEffort::High,
                ReasoningEffort::Max,
            ]
        );
        let none = resolve_reasoning("zhipuai", "glm-5.3", Some("none")).unwrap();
        assert_eq!(none.effective, ReasoningEffort::Auto);
        assert_eq!(none.status, ReasoningResolutionStatus::Unsupported);
        assert_eq!(none.preflight_wire, ReasoningWire::Omit);
        let minimal = resolve_reasoning("zhipuai", "glm-5.3", Some("minimal")).unwrap();
        assert_eq!(minimal.preflight_wire, ReasoningWire::Omit);
    }

    #[test]
    fn current_xai_models_have_explicit_effort_mapping() {
        let grok46 = reasoning_capability_for_provider("xai", "grok-4.6");
        assert_eq!(grok46.strategy, ReasoningStrategy::OpenAiReasoningEffort);
        assert!(!grok46.supports(ReasoningEffort::None));
        let xhigh = resolve_reasoning("xai", "grok-4.6", Some("xhigh")).unwrap();
        assert_eq!(xhigh.effective, ReasoningEffort::XHigh);
        assert_eq!(
            xhigh.preflight_wire,
            ReasoningWire::OpenAi {
                reasoning_effort: "xhigh".into()
            }
        );

        let grok45 = resolve_reasoning("xai", "grok-4.5", Some("xhigh")).unwrap();
        assert_eq!(grok45.effective, ReasoningEffort::High);
        assert_eq!(grok45.status, ReasoningResolutionStatus::Downgraded);
        assert_eq!(
            grok45.preflight_wire,
            ReasoningWire::OpenAi {
                reasoning_effort: "high".into()
            }
        );

        let grok43 = resolve_reasoning("xai", "grok-4.3", Some("none")).unwrap();
        assert_eq!(grok43.effective, ReasoningEffort::None);
        assert_eq!(
            grok43.preflight_wire,
            ReasoningWire::OpenAi {
                reasoning_effort: "none".into()
            }
        );
        let old = reasoning_capability_for_provider("xai", "grok-3");
        assert_eq!(old.status, ReasoningCapabilityStatus::Deprecated);
        assert!(old.deprecated);
        assert_eq!(old.supported_values(), vec![ReasoningEffort::Auto, ReasoningEffort::None]);
    }

    #[test]
    fn retired_deepseek_models_are_explicitly_deprecated() {
        for model in ["deepseek-chat", "deepseek-reasoner"] {
            let capability = reasoning_capability_for_provider("deepseek", model);
            assert_eq!(capability.status, ReasoningCapabilityStatus::Deprecated);
            assert!(capability.deprecated);
            assert_eq!(capability.supported_values(), vec![ReasoningEffort::Auto]);
            let resolution = resolve_reasoning("deepseek", model, Some("high")).unwrap();
            assert_eq!(resolution.effective, ReasoningEffort::Auto);
            assert_eq!(resolution.preflight_wire, ReasoningWire::Omit);
        }
    }

    #[test]
    fn haiku_catalog_uses_official_id_and_legacy_reasoning_semantics() {
        let catalog = reasoning_capability_catalog();
        let haiku = catalog
            .iter()
            .find(|item| item.model_label == "Claude Haiku 4.5")
            .expect("Haiku 4.5 must be in the bundled catalog");
        assert_eq!(haiku.model_id, "claude-haiku-4-5-20251001");
        assert_eq!(haiku.api_model_id, "claude-haiku-4-5-20251001");
        assert_eq!(
            catalog
                .iter()
                .filter(|item| item.model_id == "claude-haiku-4-5-20251001")
                .count(),
            1
        );
        assert!(!catalog
            .iter()
            .any(|item| item.model_id == "claude-haiku-4-5-20251213"));
        assert_eq!(
            reasoning_model_aliases("clawapi", "claude-haiku-4-5-20251001"),
            vec!["claude-haiku-4-5-20251213"]
        );
        assert!(reasoning_model_aliases("openai", "claude-haiku-4-5-20251001").is_empty());

        let auto = resolve_reasoning(
            "clawapi",
            "claude-haiku-4-5-20251213",
            None,
        )
        .expect("legacy Haiku id should resolve");
        assert_eq!(auto.effective, ReasoningEffort::Auto);
        assert_eq!(auto.preflight_wire, ReasoningWire::Omit);
        let none = resolve_reasoning(
            "clawapi",
            "claude-haiku-4-5-20251213",
            Some("none"),
        )
        .expect("legacy Haiku id should preserve none");
        assert_eq!(none.effective, ReasoningEffort::None);
        assert_eq!(none.preflight_wire, ReasoningWire::AnthropicDisabled);
    }

    #[test]
    fn catalog_is_deterministic_and_has_claude_without_moonshot() {
        let catalog = reasoning_capability_catalog();
        assert_eq!(catalog[0].provider_id, "clawapi");
        assert!(catalog.iter().any(|item| item.model_id == "claude-opus-4-6"));
        assert!(catalog.iter().any(|item| item.model_id == "glm-4.6"));
        assert!(catalog.iter().any(|item| item.model_id == "grok-4.6"));
        assert!(!catalog.iter().any(|item| item.model_id.contains("kimi")));
    }
}
