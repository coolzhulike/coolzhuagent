use std::env;
use std::future::Future;
use std::pin::Pin;

use crate::error::ApiError;
use crate::inflight::EndpointIdentity;
use crate::types::{MessageRequest, MessageResponse};

pub mod claw_provider;
pub mod openai_compat;

pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, ApiError>> + Send + 'a>>;

pub trait Provider {
    type Stream;

    /// 连接 / 服务身份查询（RPR-11a）。
    ///
    /// 默认实现返回 [`EndpointIdentity::unsupported`]（"身份未知 / 不支持"）。
    /// 该默认值在排空判定里**必须导致该连接不能自动切换**，
    /// **不得**被解释成"没有在途请求"（见 `crate::inflight::DrainVerdict::IdentityUnknown`）。
    ///
    /// 有真实已解析配置的 provider 应覆盖本方法，返回不含密钥的端点身份。
    fn endpoint_identity(&self) -> EndpointIdentity {
        EndpointIdentity::unsupported()
    }

    fn send_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, MessageResponse>;

    fn stream_message<'a>(
        &'a self,
        request: &'a MessageRequest,
    ) -> ProviderFuture<'a, Self::Stream>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    ClawApi,
    Anthropic,
    Xai,
    OpenAi,
    ZhipuAi,
    AlibabaBailian,
    BaiduQianfan,
    ByteDanceArk,
    DeepSeek,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderMetadata {
    pub provider: ProviderKind,
    pub auth_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderOption {
    pub slug: &'static str,
    pub label: &'static str,
    pub provider: ProviderKind,
    pub auth_env: &'static str,
    pub base_url_env: &'static str,
    pub default_base_url: &'static str,
    pub recommended_models: &'static [&'static str],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelTokenLimit {
    pub model: &'static str,
    pub context_tokens: u32,
    pub max_output_tokens: u32,
}

pub(crate) const CLAUDE_HAIKU_45_MODEL_ID: &str = "claude-haiku-4-5-20251001";
pub(crate) const CLAUDE_HAIKU_45_LEGACY_MODEL_ID: &str = "claude-haiku-4-5-20251213";

#[must_use]
pub(crate) fn canonical_claude_model_id(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.eq_ignore_ascii_case("haiku")
        || trimmed.eq_ignore_ascii_case(CLAUDE_HAIKU_45_LEGACY_MODEL_ID)
    {
        CLAUDE_HAIKU_45_MODEL_ID.to_string()
    } else {
        trimmed.to_string()
    }
}

const CLAW_API_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::ClawApi,
    auth_env: "ANTHROPIC_API_KEY",
    base_url_env: "ANTHROPIC_BASE_URL",
    default_base_url: claw_provider::DEFAULT_BASE_URL,
};

const XAI_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::Xai,
    auth_env: "XAI_API_KEY",
    base_url_env: "XAI_BASE_URL",
    default_base_url: openai_compat::DEFAULT_XAI_BASE_URL,
};

const ZHIPU_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::ZhipuAi,
    auth_env: "ZAI_API_KEY",
    base_url_env: "ZAI_BASE_URL",
    default_base_url: openai_compat::DEFAULT_ZHIPU_BASE_URL,
};

const ALIBABA_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::AlibabaBailian,
    auth_env: "DASHSCOPE_API_KEY",
    base_url_env: "DASHSCOPE_BASE_URL",
    default_base_url: openai_compat::DEFAULT_ALIBABA_BASE_URL,
};

const BAIDU_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::BaiduQianfan,
    auth_env: "QIANFAN_API_KEY",
    base_url_env: "QIANFAN_BASE_URL",
    default_base_url: openai_compat::DEFAULT_BAIDU_BASE_URL,
};

const BYTEDANCE_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::ByteDanceArk,
    auth_env: "ARK_API_KEY",
    base_url_env: "ARK_BASE_URL",
    default_base_url: openai_compat::DEFAULT_BYTEDANCE_BASE_URL,
};

const DEEPSEEK_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::DeepSeek,
    auth_env: "DEEPSEEK_API_KEY",
    base_url_env: "DEEPSEEK_BASE_URL",
    default_base_url: openai_compat::DEFAULT_DEEPSEEK_BASE_URL,
};

const CUSTOM_METADATA: ProviderMetadata = ProviderMetadata {
    provider: ProviderKind::Custom,
    auth_env: "CUSTOM_API_KEY",
    base_url_env: "CUSTOM_BASE_URL",
    default_base_url: openai_compat::DEFAULT_CUSTOM_BASE_URL,
};

const ZHIPU_MODELS: &[&str] = &["glm-free", "glm-4.7-flash", "glm-4.7", "glm-5"];
const ALIBABA_MODELS: &[&str] = &[
    "qwen3.7-max",
    "qwen-plus",
    "qwen-turbo",
    "qwen-max",
    "glm-5.2",
    "glm-5.1",
    "glm-5",
];
const BAIDU_MODELS: &[&str] = &["ernie-4.5-turbo-128k", "ernie-x1-turbo-32k"];
const BYTEDANCE_MODELS: &[&str] = &["doubao-1-5-pro-32k-250115", "doubao-1-5-lite-32k-250115"];
const DEEPSEEK_MODELS: &[&str] = &[
    "deepseek-v4-flash",
    "deepseek-v4-pro",
    "deepseek-chat",
    "deepseek-reasoner",
];
const OPENAI_MODELS: &[&str] = &["gpt-4.1", "gpt-4.1-mini", "gpt-4o-mini"];
const XAI_MODELS: &[&str] = &["grok-3", "grok-3-mini", "grok-2"];
const CUSTOM_MODELS: &[&str] = &["qwen2.5-vl-3b", "showui", "llama3.1", "custom-model"];

const PROVIDER_CATALOG: &[ProviderOption] = &[
    ProviderOption {
        slug: "zhipu",
        label: "Zhipu AI",
        provider: ProviderKind::ZhipuAi,
        auth_env: ZHIPU_METADATA.auth_env,
        base_url_env: ZHIPU_METADATA.base_url_env,
        default_base_url: ZHIPU_METADATA.default_base_url,
        recommended_models: ZHIPU_MODELS,
    },
    ProviderOption {
        slug: "aliyun",
        label: "Alibaba DashScope",
        provider: ProviderKind::AlibabaBailian,
        auth_env: ALIBABA_METADATA.auth_env,
        base_url_env: ALIBABA_METADATA.base_url_env,
        default_base_url: ALIBABA_METADATA.default_base_url,
        recommended_models: ALIBABA_MODELS,
    },
    ProviderOption {
        slug: "baidu",
        label: "Baidu Qianfan",
        provider: ProviderKind::BaiduQianfan,
        auth_env: BAIDU_METADATA.auth_env,
        base_url_env: BAIDU_METADATA.base_url_env,
        default_base_url: BAIDU_METADATA.default_base_url,
        recommended_models: BAIDU_MODELS,
    },
    ProviderOption {
        slug: "bytedance",
        label: "ByteDance Ark",
        provider: ProviderKind::ByteDanceArk,
        auth_env: BYTEDANCE_METADATA.auth_env,
        base_url_env: BYTEDANCE_METADATA.base_url_env,
        default_base_url: BYTEDANCE_METADATA.default_base_url,
        recommended_models: BYTEDANCE_MODELS,
    },
    ProviderOption {
        slug: "deepseek",
        label: "DeepSeek",
        provider: ProviderKind::DeepSeek,
        auth_env: DEEPSEEK_METADATA.auth_env,
        base_url_env: DEEPSEEK_METADATA.base_url_env,
        default_base_url: DEEPSEEK_METADATA.default_base_url,
        recommended_models: DEEPSEEK_MODELS,
    },
    ProviderOption {
        slug: "openai",
        label: "OpenAI",
        provider: ProviderKind::OpenAi,
        auth_env: "OPENAI_API_KEY",
        base_url_env: "OPENAI_BASE_URL",
        default_base_url: openai_compat::DEFAULT_OPENAI_BASE_URL,
        recommended_models: OPENAI_MODELS,
    },
    ProviderOption {
        slug: "xai",
        label: "xAI",
        provider: ProviderKind::Xai,
        auth_env: XAI_METADATA.auth_env,
        base_url_env: XAI_METADATA.base_url_env,
        default_base_url: XAI_METADATA.default_base_url,
        recommended_models: XAI_MODELS,
    },
    ProviderOption {
        slug: "custom-openai",
        label: "Custom OpenAI-compatible",
        provider: ProviderKind::Custom,
        auth_env: CUSTOM_METADATA.auth_env,
        base_url_env: CUSTOM_METADATA.base_url_env,
        default_base_url: CUSTOM_METADATA.default_base_url,
        recommended_models: CUSTOM_MODELS,
    },
];

const MODEL_REGISTRY: &[(&str, ProviderMetadata)] = &[
    ("opus", CLAW_API_METADATA),
    ("sonnet", CLAW_API_METADATA),
    ("haiku", CLAW_API_METADATA),
    ("claude-opus-4-6", CLAW_API_METADATA),
    ("claude-sonnet-4-6", CLAW_API_METADATA),
    ("claude-haiku-4-5-20251001", CLAW_API_METADATA),
    ("grok", XAI_METADATA),
    ("grok-3", XAI_METADATA),
    ("grok-mini", XAI_METADATA),
    ("grok-3-mini", XAI_METADATA),
    ("grok-2", XAI_METADATA),
    ("glm", ZHIPU_METADATA),
    ("glm-free", ZHIPU_METADATA),
    ("zhipu-free", ZHIPU_METADATA),
    ("glm-4", ZHIPU_METADATA),
    ("glm-4-flash", ZHIPU_METADATA),
    ("glm-4.5", ZHIPU_METADATA),
    ("glm-4.5-flash", ZHIPU_METADATA),
    ("glm-4.6v-flash", ZHIPU_METADATA),
    ("glm-vision", ZHIPU_METADATA),
    ("glm-vision-free", ZHIPU_METADATA),
    ("glm-4.7", ZHIPU_METADATA),
    ("glm-4.7-flash", ZHIPU_METADATA),
    ("glm-5", ZHIPU_METADATA),
    ("glm-5.1", ALIBABA_METADATA),
    ("zhipu/glm-5", ALIBABA_METADATA),
    ("zhipu/glm-5.1", ALIBABA_METADATA),
    ("qwen", ALIBABA_METADATA),
    ("qwen-plus", ALIBABA_METADATA),
    ("qwen-turbo", ALIBABA_METADATA),
    ("qwen-max", ALIBABA_METADATA),
    ("qwen3.7-max", ALIBABA_METADATA),
    ("glm-5.2", ALIBABA_METADATA),
    ("aliyun", ALIBABA_METADATA),
    ("dashscope", ALIBABA_METADATA),
    ("ernie", BAIDU_METADATA),
    ("ernie-4.5-turbo-128k", BAIDU_METADATA),
    ("ernie-x1-turbo-32k", BAIDU_METADATA),
    ("baidu", BAIDU_METADATA),
    ("qianfan", BAIDU_METADATA),
    ("doubao", BYTEDANCE_METADATA),
    ("doubao-1-5-pro-32k-250115", BYTEDANCE_METADATA),
    ("doubao-1-5-lite-32k-250115", BYTEDANCE_METADATA),
    ("bytedance", BYTEDANCE_METADATA),
    ("ark", BYTEDANCE_METADATA),
    ("deepseek", DEEPSEEK_METADATA),
    ("deepseek-v4-flash", DEEPSEEK_METADATA),
    ("deepseek-v4-pro", DEEPSEEK_METADATA),
    ("deepseek-chat", DEEPSEEK_METADATA),
    ("deepseek-reasoner", DEEPSEEK_METADATA),
];

const DEFAULT_MODEL_TOKEN_LIMIT: ModelTokenLimit = ModelTokenLimit {
    model: "default",
    context_tokens: 64_000,
    max_output_tokens: 64_000,
};

const MODEL_TOKEN_LIMITS: &[ModelTokenLimit] = &[
    ModelTokenLimit {
        // Claude Code 计划下 Opus 4.x 支持 1M 上下文（官网核实 2026-05-31）。
        model: "claude-opus-4-6",
        context_tokens: 1_000_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        model: "claude-sonnet-4-6",
        context_tokens: 1_000_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        model: "claude-haiku-4-5-20251001",
        context_tokens: 200_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        // Grok 3 上下文 1M（x.ai 官网核实 2026-05-31）。
        model: "grok-3",
        context_tokens: 1_000_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        model: "grok-3-mini",
        context_tokens: 1_000_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        model: "grok-2",
        context_tokens: 128_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        // GLM-4.x/5 系列上下文 200K（智谱官网核实 2026-05-31）。
        model: "glm-4.6v-flash",
        context_tokens: 200_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        model: "glm-4.7",
        context_tokens: 200_000,
        max_output_tokens: 128_000,
    },
    ModelTokenLimit {
        model: "glm-4.7-flash",
        context_tokens: 200_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        model: "glm-5",
        context_tokens: 200_000,
        max_output_tokens: 128_000,
    },
    ModelTokenLimit {
        model: "glm-5.1",
        context_tokens: 200_000,
        max_output_tokens: 128_000,
    },
    ModelTokenLimit {
        // GLM-5.2：MoE 744B/40B 激活，上下文升至 1M，纯文本+代码（智谱 2026-06 核实）。
        model: "glm-5.2",
        context_tokens: 1_000_000,
        max_output_tokens: 128_000,
    },
    ModelTokenLimit {
        // Qwen3.7-Max：阿里云百炼旗舰，万亿 MoE，1M 上下文，多模态（文/图/代码）（2026-05 核实）。
        model: "qwen3.7-max",
        context_tokens: 1_000_000,
        max_output_tokens: 65_536,
    },
    ModelTokenLimit {
        // Qwen3.x-Plus 上下文 1M（Qwen 官网核实 2026-05-31）。
        model: "qwen-plus",
        context_tokens: 1_000_000,
        max_output_tokens: 65_536,
    },
    ModelTokenLimit {
        model: "qwen-turbo",
        context_tokens: 128_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        // Qwen3.x-Max 上下文 1M / 最大输出 65536（Qwen 官网核实 2026-05-31）。
        model: "qwen-max",
        context_tokens: 1_000_000,
        max_output_tokens: 65_536,
    },
    ModelTokenLimit {
        model: "ernie-4.5-turbo-128k",
        context_tokens: 128_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        model: "ernie-x1-turbo-32k",
        context_tokens: 32_000,
        max_output_tokens: 32_000,
    },
    ModelTokenLimit {
        model: "doubao-1-5-pro-32k-250115",
        context_tokens: 32_000,
        max_output_tokens: 32_000,
    },
    ModelTokenLimit {
        model: "doubao-1-5-lite-32k-250115",
        context_tokens: 32_000,
        max_output_tokens: 32_000,
    },
    ModelTokenLimit {
        // DeepSeek V4 系列上下文 1M（用户确认，官网值）。max_output 暂保守 64k，待 test5 联网核对。
        model: "deepseek-v4-flash",
        context_tokens: 1_000_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        // DeepSeek V4 系列上下文 1M（用户确认，官网值）。max_output 暂保守 64k，待 test5 联网核对。
        model: "deepseek-v4-pro",
        context_tokens: 1_000_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        // DeepSeek V4 系列上下文 1M（官网核实 2026-05-31）。
        model: "deepseek-chat",
        context_tokens: 1_000_000,
        max_output_tokens: 64_000,
    },
    ModelTokenLimit {
        // DeepSeek Reasoner（Think Max）：上下文 1M、最大输出 384K（官网核实 2026-05-31）。
        model: "deepseek-reasoner",
        context_tokens: 1_000_000,
        max_output_tokens: 384_000,
    },
    ModelTokenLimit {
        model: "gpt-4.1",
        context_tokens: 1_000_000,
        max_output_tokens: 32_768,
    },
    ModelTokenLimit {
        model: "gpt-4.1-mini",
        context_tokens: 1_000_000,
        max_output_tokens: 32_768,
    },
    ModelTokenLimit {
        model: "gpt-4o-mini",
        context_tokens: 128_000,
        max_output_tokens: 16_384,
    },
];

#[must_use]
pub const fn provider_catalog() -> &'static [ProviderOption] {
    PROVIDER_CATALOG
}

#[must_use]
pub fn provider_option(slug_or_label: &str) -> Option<ProviderOption> {
    let normalized = normalize_provider_name(slug_or_label);
    PROVIDER_CATALOG
        .iter()
        .find(|option| {
            option.slug == normalized || normalize_provider_name(option.label) == normalized
        })
        .copied()
}

#[must_use]
pub fn provider_kind_from_name(slug_or_label: &str) -> Option<ProviderKind> {
    let normalized = normalize_provider_name(slug_or_label);
    provider_option(slug_or_label)
        .map(|option| option.provider)
        .or_else(|| match normalized.as_str() {
            "zhipuai" | "zai" | "bigmodel" | "zhupuai" | "智谱ai" | "智谱ai(z.ai)" | "智谱" => {
                Some(ProviderKind::ZhipuAi)
            }
            "阿里百炼" | "阿里云百炼" | "alibababailian" | "bailian" | "dashscope"
            | "aliyunbailian" => Some(ProviderKind::AlibabaBailian),
            "火山方舟" | "volcengine" | "doubao" | "bytedanceark" => {
                Some(ProviderKind::ByteDanceArk)
            }
            "百度千帆" | "baiduqianfan" | "wenxin" => Some(ProviderKind::BaiduQianfan),
            "deepseek" | "深度求索" => Some(ProviderKind::DeepSeek),
            "openai" => Some(ProviderKind::OpenAi),
            "xai" | "grok" => Some(ProviderKind::Xai),
            "anthropic" | "clawapi" | "claw" => Some(ProviderKind::ClawApi),
            "custom"
            | "customopenai"
            | "customopenaicompatible"
            | "openai兼容"
            | "openai-compatible"
            | "openai兼容(自定义)"
            | "ollama"
            | "ollama(本地)"
            | "本地"
            | "本地模型" => Some(ProviderKind::Custom),
            _ => None,
        })
}

#[must_use]
pub fn resolve_model_alias(model: &str) -> String {
    let trimmed = model.trim();
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        "opus" => "claude-opus-4-6".to_string(),
        "sonnet" => "claude-sonnet-4-6".to_string(),
        "haiku" => CLAUDE_HAIKU_45_MODEL_ID.to_string(),
        CLAUDE_HAIKU_45_LEGACY_MODEL_ID => CLAUDE_HAIKU_45_MODEL_ID.to_string(),
        "grok" | "grok-3" => "grok-3".to_string(),
        "grok-mini" | "grok-3-mini" => "grok-3-mini".to_string(),
        "grok-2" => "grok-2".to_string(),
        "glm" | "glm-4" | "glm-4.5" | "glm-4.7" => "glm-4.7".to_string(),
        "glm-vision" | "glm-vision-free" | "glm-4.6v-flash" => "glm-4.6v-flash".to_string(),
        "glm-free" | "zhipu-free" | "glm-4-flash" | "glm-4.5-flash" | "glm-4.7-flash" => {
            "glm-4.7-flash".to_string()
        }
        "glm-5" => "glm-5".to_string(),
        "glm-5.1" => "glm-5.1".to_string(),
        "zhipu/glm-5" => "ZHIPU/GLM-5".to_string(),
        "zhipu/glm-5.1" => "glm-5.1".to_string(),
        "qwen" | "aliyun" | "dashscope" => "qwen-plus".to_string(),
        "ernie" | "baidu" | "qianfan" => "ernie-4.5-turbo-128k".to_string(),
        "doubao" | "bytedance" | "ark" => "doubao-1-5-pro-32k-250115".to_string(),
        "deepseek" => "deepseek-v4-flash".to_string(),
        "deepseek-pro" => "deepseek-v4-pro".to_string(),
        "deepseek-r1" | "deepseek-reasoner" => "deepseek-reasoner".to_string(),
        _ => trimmed.to_string(),
    }
}

#[must_use]
pub fn metadata_for_model(model: &str) -> Option<ProviderMetadata> {
    let canonical = resolve_model_alias(model);
    let lower = canonical.to_ascii_lowercase();
    if let Some((_, metadata)) = MODEL_REGISTRY.iter().find(|(alias, _)| *alias == lower) {
        return Some(*metadata);
    }
    if lower.starts_with("grok") {
        return Some(XAI_METADATA);
    }
    if lower.starts_with("glm") {
        return Some(ZHIPU_METADATA);
    }
    if lower.starts_with("qwen") {
        return Some(ALIBABA_METADATA);
    }
    if lower.starts_with("ernie") {
        return Some(BAIDU_METADATA);
    }
    if lower.starts_with("doubao") {
        return Some(BYTEDANCE_METADATA);
    }
    if lower.starts_with("deepseek") {
        return Some(DEEPSEEK_METADATA);
    }
    None
}

#[must_use]
pub fn detect_provider_kind(model: &str) -> ProviderKind {
    if let Some(metadata) = metadata_for_model(model) {
        return metadata.provider;
    }
    if let Ok(provider_name) = env::var("PROVIDER") {
        if let Some(option) = provider_option(&provider_name) {
            return option.provider;
        }
    }
    if claw_provider::has_auth_from_env_or_saved().unwrap_or(false) {
        return ProviderKind::ClawApi;
    }
    if openai_compat::has_api_key("ZAI_API_KEY") || openai_compat::has_api_key("BIGMODEL_API_KEY") {
        return ProviderKind::ZhipuAi;
    }
    if openai_compat::has_api_key("OPENAI_API_KEY") {
        return ProviderKind::OpenAi;
    }
    if openai_compat::has_api_key("XAI_API_KEY") {
        return ProviderKind::Xai;
    }
    if openai_compat::has_api_key("DASHSCOPE_API_KEY") {
        return ProviderKind::AlibabaBailian;
    }
    if openai_compat::has_api_key("QIANFAN_API_KEY") {
        return ProviderKind::BaiduQianfan;
    }
    if openai_compat::has_api_key("ARK_API_KEY") {
        return ProviderKind::ByteDanceArk;
    }
    if openai_compat::has_api_key("DEEPSEEK_API_KEY") {
        return ProviderKind::DeepSeek;
    }
    ProviderKind::ClawApi
}

fn normalize_provider_name(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '_', '-'], "")
}

#[must_use]
pub fn max_tokens_for_model(model: &str) -> u32 {
    model_token_limit(model).max_output_tokens
}

#[must_use]
pub fn context_tokens_for_model(model: &str) -> u32 {
    model_token_limit(model).context_tokens
}

#[must_use]
pub fn model_token_limit(model: &str) -> ModelTokenLimit {
    let canonical = resolve_model_alias(model).to_ascii_lowercase();
    if let Some(limit) = MODEL_TOKEN_LIMITS
        .iter()
        .find(|limit| limit.model == canonical)
        .copied()
    {
        return limit;
    }
    if canonical.starts_with("claude-opus") {
        return MODEL_TOKEN_LIMITS[0];
    }
    if canonical.starts_with("claude-") {
        // Claude Code 计划下 Opus/Sonnet 4.x 支持 1M 上下文（官网核实 2026-05-31）。
        return ModelTokenLimit {
            model: "claude-default",
            context_tokens: 1_000_000,
            max_output_tokens: 64_000,
        };
    }
    if canonical.starts_with("grok") {
        // Grok 3 上下文 1M（x.ai 官网核实 2026-05-31）。
        return ModelTokenLimit {
            model: "grok-default",
            context_tokens: 1_000_000,
            max_output_tokens: 64_000,
        };
    }
    if canonical.starts_with("deepseek") {
        // DeepSeek V4 系列上下文 1M（官网核实 2026-05-31）。
        return ModelTokenLimit {
            model: "deepseek-default",
            context_tokens: 1_000_000,
            max_output_tokens: 64_000,
        };
    }
    if canonical.starts_with("glm-5") || canonical.starts_with("glm-4") {
        // GLM-4.x/5 系列上下文 200K（智谱官网核实 2026-05-31）。
        return ModelTokenLimit {
            model: "glm-default",
            context_tokens: 200_000,
            max_output_tokens: 128_000,
        };
    }
    if canonical.starts_with("qwen") {
        // Qwen3.x 系列上下文 1M（Qwen 官网核实 2026-05-31）。
        return ModelTokenLimit {
            model: "qwen-default",
            context_tokens: 1_000_000,
            max_output_tokens: 65_536,
        };
    }
    if canonical.starts_with("ernie-4.5") {
        return ModelTokenLimit {
            model: "128k-default",
            context_tokens: 128_000,
            max_output_tokens: 64_000,
        };
    }
    if canonical.starts_with("doubao")
        || canonical.starts_with("ernie-x1")
        || canonical.contains("-32k")
    {
        return ModelTokenLimit {
            model: "32k-default",
            context_tokens: 32_000,
            max_output_tokens: 32_000,
        };
    }
    DEFAULT_MODEL_TOKEN_LIMIT
}

#[cfg(test)]
mod tests {
    use super::{
        context_tokens_for_model, detect_provider_kind, max_tokens_for_model, model_token_limit,
        resolve_model_alias, Provider, ProviderFuture, ProviderKind,
    };
    use crate::error::ApiError;
    use crate::inflight::{drain_verdict_for, local_endpoint_inflight, DrainVerdict,
        EndpointIdentitySource};
    use crate::types::{MessageRequest, MessageResponse};

    /// 未覆盖身份查询的 provider（模拟"身份未知/不支持"的连接）。
    struct IdentitylessProvider;

    impl Provider for IdentitylessProvider {
        type Stream = ();

        fn send_message<'a>(
            &'a self,
            _request: &'a MessageRequest,
        ) -> ProviderFuture<'a, MessageResponse> {
            Box::pin(async { Err(ApiError::UnsupportedCapability { capability: "test".into() }) })
        }

        fn stream_message<'a>(&'a self, _request: &'a MessageRequest) -> ProviderFuture<'a, ()> {
            Box::pin(async { Err(ApiError::UnsupportedCapability { capability: "test".into() }) })
        }
    }

    /// RPR-11a 约束 3：trait 默认身份是"未知/不支持"，
    /// 该默认值必须导致**不能自动切换**，不得被解释成"没有在途请求"。
    #[test]
    fn provider_default_identity_is_unknown_and_blocks_automatic_switch() {
        let identity = IdentitylessProvider.endpoint_identity();
        assert!(!identity.is_known());
        assert_eq!(identity.source(), EndpointIdentitySource::Unsupported);
        assert_eq!(identity.key(), None);

        // 用确定性的**空快照**表达"零在途 + 身份未知"：仍必须拒绝自动切换。
        let empty = crate::inflight::EndpointInFlightStatus {
            identity: identity.clone(),
            in_flight: 0,
            remote_result_unknown: 0,
            settled: 0,
            in_flight_attempts: Vec::new(),
            recent_settled: Vec::new(),
        };
        let verdict = drain_verdict_for(&identity, &empty);
        assert_eq!(
            verdict,
            DrainVerdict::IdentityUnknown,
            "零在途 + 未知身份 ≠ 可自动切换"
        );
        assert!(!verdict.permits_automatic_switch());
        assert_ne!(verdict, DrainVerdict::ClientSettled);

        let report = crate::inflight::EndpointDrainReport {
            status: empty,
            verdict,
            waited: std::time::Duration::ZERO,
        };
        assert!(!report.client_drained());
        assert!(!report.permits_automatic_switch());
        // 不依赖进程级注册表的具体计数：只断言该身份下的判定语义。
        let _ = local_endpoint_inflight(&identity);
    }

    #[test]
    fn resolves_grok_aliases() {
        assert_eq!(resolve_model_alias("grok"), "grok-3");
        assert_eq!(resolve_model_alias("grok-mini"), "grok-3-mini");
        assert_eq!(resolve_model_alias("grok-2"), "grok-2");
    }

    #[test]
    fn resolves_zhipu_aliases_to_current_models() {
        assert_eq!(resolve_model_alias("glm"), "glm-4.7");
        assert_eq!(resolve_model_alias("glm-4"), "glm-4.7");
        assert_eq!(resolve_model_alias("glm-free"), "glm-4.7-flash");
        assert_eq!(resolve_model_alias("glm-4.5-flash"), "glm-4.7-flash");
        assert_eq!(resolve_model_alias("glm-vision"), "glm-4.6v-flash");
        assert_eq!(resolve_model_alias("glm-vision-free"), "glm-4.6v-flash");
        assert_eq!(resolve_model_alias("GLM-5.1"), "glm-5.1");
        assert_eq!(resolve_model_alias("ZHIPU/GLM-5.1"), "glm-5.1");
    }

    #[test]
    fn resolves_new_provider_aliases() {
        assert_eq!(resolve_model_alias("qwen"), "qwen-plus");
        assert_eq!(resolve_model_alias("ernie"), "ernie-4.5-turbo-128k");
        assert_eq!(resolve_model_alias("doubao"), "doubao-1-5-pro-32k-250115");
        assert_eq!(resolve_model_alias("deepseek"), "deepseek-v4-flash");
        assert_eq!(resolve_model_alias("deepseek-pro"), "deepseek-v4-pro");
        assert_eq!(resolve_model_alias("deepseek-r1"), "deepseek-reasoner");
    }

    #[test]
    fn detects_provider_from_model_name_first() {
        assert_eq!(detect_provider_kind("grok"), ProviderKind::Xai);
        assert_eq!(
            detect_provider_kind("claude-sonnet-4-6"),
            ProviderKind::ClawApi
        );
        assert_eq!(detect_provider_kind("glm-4.7-flash"), ProviderKind::ZhipuAi);
        assert_eq!(
            detect_provider_kind("qwen-plus"),
            ProviderKind::AlibabaBailian
        );
        assert_eq!(
            detect_provider_kind("glm-5.1"),
            ProviderKind::AlibabaBailian
        );
        assert_eq!(
            detect_provider_kind("ernie-4.5-turbo-128k"),
            ProviderKind::BaiduQianfan
        );
        assert_eq!(
            detect_provider_kind("doubao-1-5-pro-32k-250115"),
            ProviderKind::ByteDanceArk
        );
        assert_eq!(
            detect_provider_kind("deepseek-chat"),
            ProviderKind::DeepSeek
        );
    }

    #[test]
    fn keeps_existing_max_token_heuristic() {
        // 2026-05-31 官网核实更正：opus 4.x 最大输出 64K。
        assert_eq!(max_tokens_for_model("opus"), 64_000);
        assert_eq!(max_tokens_for_model("grok-3"), 64_000);
        assert_eq!(max_tokens_for_model("glm-4.7-flash"), 64_000);
    }

    #[test]
    fn resolves_model_token_limits_from_global_table() {
        // 2026-05-31 官网核实更正：GLM-5.x 上下文 200K、DeepSeek V4/Qwen3.x 1M、Grok3 1M。
        assert_eq!(context_tokens_for_model("GLM-5.1"), 200_000);
        assert_eq!(context_tokens_for_model("deepseek-v4-pro"), 1_000_000);
        assert_eq!(context_tokens_for_model("qwen-max"), 1_000_000);
        assert_eq!(context_tokens_for_model("grok-3"), 1_000_000);
        assert_eq!(context_tokens_for_model("ernie-x1-turbo-32k"), 32_000);
        assert_eq!(model_token_limit("unknown-model").context_tokens, 64_000);
    }
}
