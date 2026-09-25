use std::collections::HashMap;
use std::sync::OnceLock;

use crate::config::{load_config, AdapterConfig, ModelConfig};
use crate::model_info::{ModelInfo, ProviderInfo};
use crate::providers::{ProviderKind, CLAUDE_HAIKU_45_MODEL_ID, CLAUDE_HAIKU_45_LEGACY_MODEL_ID};

static DEFAULT_REGISTRY: OnceLock<ModelRegistry> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct ResolvedModel {
    pub canonical_id: String,
    pub provider: ProviderKind,
    pub api_model_id: String,
    pub base_url: String,
    pub env_keys: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ModelRegistry {
    providers: HashMap<ProviderKind, ProviderInfo>,
    models: HashMap<String, ModelInfo>,
    aliases: HashMap<String, String>,
    config: AdapterConfig,
}

impl ModelRegistry {
    pub fn global() -> &'static Self {
        DEFAULT_REGISTRY.get_or_init(Self::from_config_or_default)
    }

    pub fn from_config_or_default() -> Self {
        load_config().map_or_else(|_| Self::default_registry(), Self::with_config)
    }

    pub fn default_registry() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            models: HashMap::new(),
            aliases: HashMap::new(),
            config: AdapterConfig::default(),
        };
        registry.register_builtin_providers();
        registry.register_builtin_models();
        registry
    }

    pub fn with_config(config: AdapterConfig) -> Self {
        let mut registry = Self::default_registry();
        registry.config = config;
        registry
    }

    fn register_builtin_providers(&mut self) {
        self.register_provider(
            ProviderKind::ClawApi,
            ProviderInfo {
                id: "clawapi".to_string(),
                name: "Anthropic ClawAPI".to_string(),
                env_keys: vec!["ANTHROPIC_API_KEY".to_string()],
                base_url_env: vec!["ANTHROPIC_BASE_URL".to_string()],
                default_base_url: "https://api.anthropic.com/v1".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::Anthropic,
            ProviderInfo {
                id: "anthropic".to_string(),
                name: "Anthropic".to_string(),
                env_keys: vec!["ANTHROPIC_API_KEY".to_string()],
                base_url_env: vec!["ANTHROPIC_BASE_URL".to_string()],
                default_base_url: "https://api.anthropic.com/v1".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::OpenAi,
            ProviderInfo {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                env_keys: vec!["OPENAI_API_KEY".to_string()],
                base_url_env: vec!["OPENAI_BASE_URL".to_string()],
                default_base_url: "https://api.openai.com/v1".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::Xai,
            ProviderInfo {
                id: "xai".to_string(),
                name: "xAI".to_string(),
                env_keys: vec!["XAI_API_KEY".to_string()],
                base_url_env: vec!["XAI_BASE_URL".to_string()],
                default_base_url: "https://api.x.ai/v1".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::ZhipuAi,
            ProviderInfo {
                id: "zhipuai".to_string(),
                name: "Zhipu AI".to_string(),
                env_keys: vec!["ZAI_API_KEY".to_string(), "BIGMODEL_API_KEY".to_string()],
                base_url_env: vec!["ZAI_BASE_URL".to_string(), "BIGMODEL_BASE_URL".to_string()],
                default_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::AlibabaBailian,
            ProviderInfo {
                id: "alibaba-bailian".to_string(),
                name: "Alibaba Bailian".to_string(),
                env_keys: vec!["DASHSCOPE_API_KEY".to_string()],
                base_url_env: vec!["DASHSCOPE_BASE_URL".to_string()],
                default_base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::BaiduQianfan,
            ProviderInfo {
                id: "baidu".to_string(),
                name: "Baidu Qianfan".to_string(),
                env_keys: vec!["QIANFAN_API_KEY".to_string()],
                base_url_env: vec!["QIANFAN_BASE_URL".to_string()],
                default_base_url: "https://qianfan.baidubce.com/v2".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::ByteDanceArk,
            ProviderInfo {
                id: "bytedance".to_string(),
                name: "ByteDance Ark".to_string(),
                env_keys: vec!["ARK_API_KEY".to_string()],
                base_url_env: vec!["ARK_BASE_URL".to_string()],
                default_base_url: "https://ark.cn-beijing.volces.com/api/v3".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::DeepSeek,
            ProviderInfo {
                id: "deepseek".to_string(),
                name: "DeepSeek".to_string(),
                env_keys: vec!["DEEPSEEK_API_KEY".to_string()],
                base_url_env: vec!["DEEPSEEK_BASE_URL".to_string()],
                default_base_url: "https://api.deepseek.com".to_string(),
            },
        );
        self.register_provider(
            ProviderKind::Custom,
            ProviderInfo {
                id: "custom".to_string(),
                name: "Custom OpenAI-compatible".to_string(),
                env_keys: vec!["CUSTOM_API_KEY".to_string(), "OPENAI_API_KEY".to_string()],
                base_url_env: vec![
                    "CUSTOM_BASE_URL".to_string(),
                    "OPENAI_COMPATIBLE_BASE_URL".to_string(),
                    "OPENAI_BASE_URL".to_string(),
                ],
                default_base_url: "http://127.0.0.1:11434/v1".to_string(),
            },
        );
    }

    fn register_builtin_models(&mut self) {
        self.register_clawapi_models();
        self.register_xai_models();
        self.register_zhipu_models();
        self.register_alibaba_models();
        self.register_baidu_models();
        self.register_bytedance_models();
        self.register_deepseek_models();
        self.register_openai_models();
        self.register_aliases();
    }

    fn register_clawapi_models(&mut self) {
        self.register_model(
            ModelInfo::new("claude-opus-4-6", "clawapi")
                .with_name("Claude Opus 4.6")
                .with_base_url("https://api.anthropic.com/v1")
                .with_limit(crate::model_info::ModelLimit {
                    context: 200_000,
                    input: None,
                    output: 32_000,
                }),
        );
        self.register_model(
            ModelInfo::new("claude-sonnet-4-6", "clawapi")
                .with_name("Claude Sonnet 4.6")
                .with_base_url("https://api.anthropic.com/v1")
                .with_limit(crate::model_info::ModelLimit {
                    context: 200_000,
                    input: None,
                    output: 64_000,
                }),
        );
        self.register_model(
            ModelInfo::new(CLAUDE_HAIKU_45_MODEL_ID, "clawapi")
                .with_name("Claude Haiku 4.5")
                .with_base_url("https://api.anthropic.com/v1")
                .with_limit(crate::model_info::ModelLimit {
                    context: 200_000,
                    input: None,
                    output: 64_000,
                }),
        );
    }

    fn register_xai_models(&mut self) {
        self.register_model(
            ModelInfo::new("grok-3", "xai")
                .with_name("Grok 3")
                .with_base_url("https://api.x.ai/v1"),
        );
        self.register_model(
            ModelInfo::new("grok-3-mini", "xai")
                .with_name("Grok 3 Mini")
                .with_base_url("https://api.x.ai/v1"),
        );
        self.register_model(
            ModelInfo::new("grok-2", "xai")
                .with_name("Grok 2")
                .with_base_url("https://api.x.ai/v1"),
        );
    }

    fn register_zhipu_models(&mut self) {
        self.register_model(
            ModelInfo::new("glm-4.7", "zhipuai")
                .with_name("GLM-4.7")
                .with_base_url("https://open.bigmodel.cn/api/paas/v4"),
        );
        self.register_model(
            ModelInfo::new("glm-4.7-flash", "zhipuai")
                .with_name("GLM-4.7 Flash")
                .with_base_url("https://open.bigmodel.cn/api/paas/v4"),
        );
        self.register_model(
            ModelInfo::new("glm-4.6v-flash", "zhipuai")
                .with_name("GLM-4.6V Flash (Vision)")
                .with_base_url("https://open.bigmodel.cn/api/paas/v4"),
        );
        self.register_model(
            ModelInfo::new("glm-free", "zhipuai")
                .with_api_model_id("glm-4-flash")
                .with_name("GLM Free")
                .with_base_url("https://open.bigmodel.cn/api/paas/v4"),
        );
        self.register_model(
            ModelInfo::new("glm-5", "zhipuai")
                .with_name("GLM-5")
                .with_base_url("https://open.bigmodel.cn/api/paas/v4"),
        );
    }

    fn register_alibaba_models(&mut self) {
        self.register_model(
            ModelInfo::new("qwen-plus", "alibaba-bailian")
                .with_name("Qwen Plus")
                .with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        );
        self.register_model(
            ModelInfo::new("qwen-turbo", "alibaba-bailian")
                .with_name("Qwen Turbo")
                .with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        );
        self.register_model(
            ModelInfo::new("qwen-max", "alibaba-bailian")
                .with_name("Qwen Max")
                .with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        );
        self.register_model(
            ModelInfo::new("glm-zhipu-5", "alibaba-bailian")
                .with_name("GLM-5 (via Bailian)")
                .with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        );
        self.register_model(
            ModelInfo::new("ZHIPU/GLM-5", "alibaba-bailian")
                .with_name("GLM-5 (Zhipu direct via Bailian)")
                .with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        );
        self.register_model(
            ModelInfo::new("glm-5.1", "alibaba-bailian")
                .with_name("GLM-5.1")
                .with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        );
        // GLM-5.2 经阿里百炼 OpenAI 兼容端（compatible-mode/v1）；与 providers/mod.rs
        // 的 MODEL_REGISTRY/MODEL_TOKEN_LIMITS 保持一致，消除双注册表不一致。
        self.register_model(
            ModelInfo::new("glm-5.2", "alibaba-bailian")
                .with_name("GLM-5.2")
                .with_base_url("https://dashscope.aliyuncs.com/compatible-mode/v1"),
        );
    }

    fn register_baidu_models(&mut self) {
        self.register_model(
            ModelInfo::new("ernie-4.5-turbo-128k", "baidu")
                .with_name("ERNIE 4.5 Turbo 128K")
                .with_base_url("https://qianfan.baidubce.com/v2"),
        );
        self.register_model(
            ModelInfo::new("ernie-x1-turbo-32k", "baidu")
                .with_name("ERNIE X1 Turbo 32K")
                .with_base_url("https://qianfan.baidubce.com/v2"),
        );
    }

    fn register_bytedance_models(&mut self) {
        self.register_model(
            ModelInfo::new("doubao-1-5-pro-32k-250115", "bytedance")
                .with_name("Doubao 1.5 Pro 32K")
                .with_base_url("https://ark.cn-beijing.volces.com/api/v3"),
        );
        self.register_model(
            ModelInfo::new("doubao-1-5-lite-32k-250115", "bytedance")
                .with_name("Doubao 1.5 Lite 32K")
                .with_base_url("https://ark.cn-beijing.volces.com/api/v3"),
        );
    }

    fn register_deepseek_models(&mut self) {
        self.register_model(
            ModelInfo::new("deepseek-v4-flash", "deepseek")
                .with_name("DeepSeek V4 Flash")
                .with_base_url("https://api.deepseek.com"),
        );
        self.register_model(
            ModelInfo::new("deepseek-v4-pro", "deepseek")
                .with_name("DeepSeek V4 Pro")
                .with_base_url("https://api.deepseek.com"),
        );
        self.register_model(
            ModelInfo::new("deepseek-chat", "deepseek")
                .with_name("DeepSeek Chat")
                .with_base_url("https://api.deepseek.com"),
        );
        self.register_model(
            ModelInfo::new("deepseek-reasoner", "deepseek")
                .with_name("DeepSeek Reasoner")
                .with_base_url("https://api.deepseek.com"),
        );
    }

    fn register_openai_models(&mut self) {
        self.register_model(
            ModelInfo::new("gpt-4.1", "openai")
                .with_name("GPT-4.1")
                .with_base_url("https://api.openai.com/v1"),
        );
        self.register_model(
            ModelInfo::new("gpt-4.1-mini", "openai")
                .with_name("GPT-4.1 Mini")
                .with_base_url("https://api.openai.com/v1"),
        );
        self.register_model(
            ModelInfo::new("gpt-4o-mini", "openai")
                .with_name("GPT-4o Mini")
                .with_base_url("https://api.openai.com/v1"),
        );
    }

    fn register_aliases(&mut self) {
        self.add_alias("opus", "claude-opus-4-6");
        self.add_alias("sonnet", "claude-sonnet-4-6");
        self.add_alias("haiku", CLAUDE_HAIKU_45_MODEL_ID);
        self.add_alias(CLAUDE_HAIKU_45_LEGACY_MODEL_ID, CLAUDE_HAIKU_45_MODEL_ID);
        self.add_alias("grok", "grok-3");
        self.add_alias("grok-mini", "grok-3-mini");
        self.add_alias("glm", "glm-4.7");
        self.add_alias("glm-4", "glm-4.7");
        self.add_alias("glm-4-flash", "glm-4.7-flash");
        self.add_alias("glm-4.5-flash", "glm-4.7-flash");
        self.add_alias("zhipu-free", "glm-free");
        self.add_alias("glm-vision", "glm-4.6v-flash");
        self.add_alias("glm-vision-free", "glm-4.6v-flash");
        self.add_alias("zhipu/glm-5", "ZHIPU/GLM-5");
        self.add_alias("glm-5.1", "glm-5.1");
        self.add_alias("zhipu/glm-5.1", "glm-5.1");
        self.add_alias("glm-5.2", "glm-5.2");
        self.add_alias("zhipu/glm-5.2", "glm-5.2");
        self.add_alias("qwen", "qwen-plus");
        self.add_alias("aliyun", "qwen-plus");
        self.add_alias("dashscope", "qwen-plus");
        self.add_alias("ernie", "ernie-4.5-turbo-128k");
        self.add_alias("baidu", "ernie-4.5-turbo-128k");
        self.add_alias("qianfan", "ernie-4.5-turbo-128k");
        self.add_alias("doubao", "doubao-1-5-pro-32k-250115");
        self.add_alias("bytedance", "doubao-1-5-pro-32k-250115");
        self.add_alias("ark", "doubao-1-5-pro-32k-250115");
        self.add_alias("deepseek", "deepseek-v4-flash");
        self.add_alias("deepseek-pro", "deepseek-v4-pro");
        self.add_alias("deepseek-r1", "deepseek-reasoner");
    }

    fn register_provider(&mut self, kind: ProviderKind, info: ProviderInfo) {
        self.providers.insert(kind, info);
    }

    fn register_model(&mut self, info: ModelInfo) {
        self.models.insert(info.id.clone(), info);
    }

    fn add_alias(&mut self, alias: &str, canonical: &str) {
        self.aliases
            .insert(alias.to_string(), canonical.to_string());
    }

    pub fn resolve_model(&self, input: &str) -> ResolvedModel {
        let trimmed = input.trim();
        let lower = trimmed.to_ascii_lowercase();

        let canonical_id = self
            .aliases
            .get(&lower)
            .cloned()
            .unwrap_or_else(|| trimmed.to_string());

        let model_config = self.config.models.get(&canonical_id);
        let provider_override = model_config
            .as_ref()
            .and_then(|c| c.provider.as_ref())
            .and_then(|p| parse_provider_kind(p));

        let model_info = self.models.get(&canonical_id);
        let default_provider = model_info
            .as_ref()
            .map(|m| parse_provider_kind(&m.provider_id))
            .flatten()
            .unwrap_or_else(|| self.detect_provider_from_env(&canonical_id));

        let provider = provider_override.unwrap_or(default_provider);

        let api_model_id = model_config
            .as_ref()
            .and_then(|c| c.api_model_id.as_ref())
            .cloned()
            .unwrap_or_else(|| {
                model_info
                    .as_ref()
                    .map(|m| m.api_model_id.clone())
                    .unwrap_or_else(|| canonical_id.clone())
            });

        let base_url = self.resolve_base_url(&canonical_id, provider, model_config, model_info);

        let env_keys = self
            .providers
            .get(&provider)
            .map(|p| p.env_keys.clone())
            .unwrap_or_default();

        ResolvedModel {
            canonical_id,
            provider,
            api_model_id,
            base_url,
            env_keys,
        }
    }

    pub fn resolve_model_for_provider(&self, input: &str, provider: ProviderKind) -> ResolvedModel {
        let mut resolved = self.resolve_model(input);
        if resolved.provider != provider {
            resolved.provider = provider;
            resolved.base_url = self.resolve_base_url(&resolved.canonical_id, provider, None, None);
            resolved.env_keys = self
                .providers
                .get(&provider)
                .map(|info| info.env_keys.clone())
                .unwrap_or_default();
        }
        resolved
    }

    fn resolve_base_url(
        &self,
        _model_id: &str,
        provider: ProviderKind,
        model_config: Option<&ModelConfig>,
        model_info: Option<&ModelInfo>,
    ) -> String {
        if let Some(url) = model_config.as_ref().and_then(|c| c.base_url.as_ref()) {
            return url.clone();
        }
        if let Some(url) = self
            .config
            .providers
            .get(&provider.id())
            .and_then(|p| p.base_url.as_ref())
        {
            return url.clone();
        }
        if let Some(info) = self.providers.get(&provider) {
            for env in &info.base_url_env {
                if let Ok(url) = std::env::var(env) {
                    if !url.is_empty() {
                        return url;
                    }
                }
            }
            if let Ok(url) = std::env::var("apiBaseUrl") {
                if !url.is_empty() {
                    return url;
                }
            }
            if let Ok(url) = std::env::var("API_BASE_URL") {
                if !url.is_empty() {
                    return url;
                }
            }
            return info.default_base_url.clone();
        }
        model_info
            .as_ref()
            .map(|m| m.default_base_url.clone())
            .unwrap_or_default()
    }

    fn detect_provider_from_env(&self, model_id: &str) -> ProviderKind {
        if model_id.starts_with("grok") {
            return ProviderKind::Xai;
        }
        if model_id.starts_with("claude") {
            return ProviderKind::ClawApi;
        }
        if model_id.starts_with("glm") {
            if has_api_key("DASHSCOPE_API_KEY") {
                return ProviderKind::AlibabaBailian;
            }
            if has_api_key("ZAI_API_KEY") || has_api_key("BIGMODEL_API_KEY") {
                return ProviderKind::ZhipuAi;
            }
        }
        if model_id.starts_with("qwen") {
            return ProviderKind::AlibabaBailian;
        }
        if model_id.starts_with("ernie") {
            return ProviderKind::BaiduQianfan;
        }
        if model_id.starts_with("doubao") {
            return ProviderKind::ByteDanceArk;
        }
        if model_id.starts_with("deepseek") {
            return ProviderKind::DeepSeek;
        }
        if model_id.starts_with("gpt") {
            return ProviderKind::OpenAi;
        }
        if let Ok(provider_name) = std::env::var("PROVIDER") {
            if let Some(kind) = parse_provider_kind(&provider_name) {
                return kind;
            }
        }
        for (kind, info) in &self.providers {
            for env in &info.env_keys {
                if has_api_key(env) {
                    return *kind;
                }
            }
        }
        ProviderKind::ClawApi
    }

    pub fn get_provider(&self, kind: ProviderKind) -> Option<&ProviderInfo> {
        self.providers.get(&kind)
    }

    pub fn get_model(&self, id: &str) -> Option<&ModelInfo> {
        let canonical = self
            .aliases
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_string());
        self.models.get(&canonical)
    }

    pub fn all_providers(&self) -> Vec<&ProviderInfo> {
        self.providers.values().collect()
    }

    pub fn all_models(&self) -> Vec<&ModelInfo> {
        self.models.values().collect()
    }
}

fn parse_provider_kind(s: &str) -> Option<ProviderKind> {
    let normalized = s.trim().to_ascii_lowercase().replace([' ', '_', '-'], "");
    match normalized.as_str() {
        "clawapi" | "claw" | "anthropic" => Some(ProviderKind::ClawApi),
        "openai" => Some(ProviderKind::OpenAi),
        "xai" => Some(ProviderKind::Xai),
        "zhipuai" | "zhipu" => Some(ProviderKind::ZhipuAi),
        "alibababailian" | "bailian" | "dashscope" | "alibaba" => {
            Some(ProviderKind::AlibabaBailian)
        }
        "baidu" | "qianfan" | "baiduqianfan" => Some(ProviderKind::BaiduQianfan),
        "bytedance" | "ark" | "bytedanceark" => Some(ProviderKind::ByteDanceArk),
        "deepseek" => Some(ProviderKind::DeepSeek),
        "custom" | "customopenai" | "customopenaicompatible" | "ollama" => {
            Some(ProviderKind::Custom)
        }
        _ => None,
    }
}

fn has_api_key(env: &str) -> bool {
    std::env::var(env).ok().filter(|v| !v.is_empty()).is_some()
}

impl ProviderKind {
    pub fn id(&self) -> String {
        match self {
            ProviderKind::ClawApi => "clawapi".to_string(),
            ProviderKind::Anthropic => "anthropic".to_string(),
            ProviderKind::OpenAi => "openai".to_string(),
            ProviderKind::Xai => "xai".to_string(),
            ProviderKind::ZhipuAi => "zhipuai".to_string(),
            ProviderKind::AlibabaBailian => "alibaba-bailian".to_string(),
            ProviderKind::BaiduQianfan => "baidu".to_string(),
            ProviderKind::ByteDanceArk => "bytedance".to_string(),
            ProviderKind::DeepSeek => "deepseek".to_string(),
            ProviderKind::Custom => "custom".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_resolves_alias() {
        let registry = ModelRegistry::default_registry();
        let resolved = registry.resolve_model("opus");
        assert_eq!(resolved.canonical_id, "claude-opus-4-6");
        assert_eq!(resolved.provider, ProviderKind::ClawApi);
    }

    #[test]
    fn registry_resolves_haiku_alias_and_legacy_id_to_official_api_model() {
        let registry = ModelRegistry::default_registry();
        for input in ["haiku", "claude-haiku-4-5-20251213"] {
            let resolved = registry.resolve_model(input);
            assert_eq!(resolved.canonical_id, "claude-haiku-4-5-20251001");
            assert_eq!(resolved.api_model_id, "claude-haiku-4-5-20251001");
            assert_eq!(resolved.provider, ProviderKind::ClawApi);
        }
    }

    #[test]
    fn registry_resolves_grok_alias() {
        let registry = ModelRegistry::default_registry();
        let resolved = registry.resolve_model("grok");
        assert_eq!(resolved.canonical_id, "grok-3");
        assert_eq!(resolved.provider, ProviderKind::Xai);
    }

    #[test]
    fn registry_resolves_glm_alias() {
        let registry = ModelRegistry::default_registry();
        let resolved = registry.resolve_model("glm");
        assert_eq!(resolved.canonical_id, "glm-4.7");
    }

    #[test]
    fn registry_resolves_bailian_glm_51_to_alibaba_china() {
        let registry = ModelRegistry::default_registry();
        let resolved = registry.resolve_model("GLM-5.1");
        assert_eq!(resolved.canonical_id, "glm-5.1");
        assert_eq!(resolved.api_model_id, "glm-5.1");
        assert_eq!(resolved.provider, ProviderKind::AlibabaBailian);
        assert_eq!(
            resolved.base_url,
            "https://dashscope.aliyuncs.com/compatible-mode/v1"
        );
    }

    #[test]
    fn registry_detects_provider_from_model_prefix() {
        let registry = ModelRegistry::default_registry();
        let resolved = registry.resolve_model("grok-3-mini");
        assert_eq!(resolved.provider, ProviderKind::Xai);
    }

    #[test]
    fn registry_fallback_to_clawapi() {
        let registry = ModelRegistry::default_registry();
        let resolved = registry.resolve_model("unknown-model");
        assert_eq!(resolved.canonical_id, "unknown-model");
    }

    #[test]
    fn parse_provider_kind_normalizes() {
        assert_eq!(parse_provider_kind("Zhipu AI"), Some(ProviderKind::ZhipuAi));
        assert_eq!(
            parse_provider_kind("alibaba-bailian"),
            Some(ProviderKind::AlibabaBailian)
        );
        assert_eq!(
            parse_provider_kind("dashscope"),
            Some(ProviderKind::AlibabaBailian)
        );
    }

    #[test]
    fn provider_kind_id() {
        assert_eq!(ProviderKind::AlibabaBailian.id(), "alibaba-bailian");
        assert_eq!(ProviderKind::ZhipuAi.id(), "zhipuai");
    }
}
