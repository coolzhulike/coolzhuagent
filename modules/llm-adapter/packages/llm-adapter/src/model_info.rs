use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Modality {
    Text,
    Audio,
    Image,
    Video,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ModelStatus {
    #[default]
    Active,
    Alpha,
    Beta,
    Deprecated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

impl Default for ModelCost {
    fn default() -> Self {
        Self {
            input: 0.0,
            output: 0.0,
            cache_read: None,
            cache_write: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelLimit {
    pub context: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<u32>,
    pub output: u32,
}

impl Default for ModelLimit {
    fn default() -> Self {
        Self {
            context: 128_000,
            input: None,
            output: 16_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelModalities {
    #[serde(default)]
    pub input: Vec<Modality>,
    #[serde(default)]
    pub output: Vec<Modality>,
}

impl Default for ModelModalities {
    fn default() -> Self {
        Self {
            input: vec![Modality::Text],
            output: vec![Modality::Text],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub api_model_id: String,
    #[serde(default)]
    pub default_base_url: String,
    #[serde(default)]
    pub cost: ModelCost,
    #[serde(default)]
    pub limit: ModelLimit,
    #[serde(default)]
    pub modalities: ModelModalities,
    #[serde(default)]
    pub status: ModelStatus,
}

impl ModelInfo {
    pub fn new(id: impl Into<String>, provider_id: impl Into<String>) -> Self {
        let id_str = id.into();
        Self {
            id: id_str.clone(),
            name: id_str.clone(),
            provider_id: provider_id.into(),
            api_model_id: id_str,
            default_base_url: String::new(),
            cost: ModelCost::default(),
            limit: ModelLimit::default(),
            modalities: ModelModalities::default(),
            status: ModelStatus::default(),
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_api_model_id(mut self, api_id: impl Into<String>) -> Self {
        self.api_model_id = api_id.into();
        self
    }

    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.default_base_url = url.into();
        self
    }

    pub fn with_cost(mut self, cost: ModelCost) -> Self {
        self.cost = cost;
        self
    }

    pub fn with_limit(mut self, limit: ModelLimit) -> Self {
        self.limit = limit;
        self
    }

    pub fn with_modalities(mut self, modalities: ModelModalities) -> Self {
        self.modalities = modalities;
        self
    }

    pub fn with_status(mut self, status: ModelStatus) -> Self {
        self.status = status;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub env_keys: Vec<String>,
    pub base_url_env: Vec<String>,
    pub default_base_url: String,
}

impl ProviderInfo {
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            env_keys: Vec::new(),
            base_url_env: Vec::new(),
            default_base_url: String::new(),
        }
    }

    pub fn with_env_keys(mut self, keys: Vec<String>) -> Self {
        self.env_keys = keys;
        self
    }

    pub fn with_base_url_env(mut self, envs: Vec<String>) -> Self {
        self.base_url_env = envs;
        self
    }

    pub fn with_default_base_url(mut self, url: impl Into<String>) -> Self {
        self.default_base_url = url.into();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_info_builder() {
        let model = ModelInfo::new("glm-5", "zhipuai")
            .with_name("GLM-5")
            .with_api_model_id("glm-zhipu-5")
            .with_base_url("https://open.bigmodel.cn/api/paas/v4")
            .with_limit(ModelLimit {
                context: 128_000,
                input: None,
                output: 8_000,
            });
        assert_eq!(model.id, "glm-5");
        assert_eq!(model.provider_id, "zhipuai");
        assert_eq!(model.api_model_id, "glm-zhipu-5");
        assert_eq!(model.limit.context, 128_000);
    }

    #[test]
    fn provider_info_builder() {
        let provider = ProviderInfo::new("zhipuai", "Zhipu AI")
            .with_env_keys(vec!["ZAI_API_KEY".into(), "BIGMODEL_API_KEY".into()])
            .with_base_url_env(vec!["ZAI_BASE_URL".into()])
            .with_default_base_url("https://open.bigmodel.cn/api/paas/v4");
        assert_eq!(provider.id, "zhipuai");
        assert_eq!(provider.env_keys.len(), 2);
    }

    #[test]
    fn model_cost_default() {
        let cost = ModelCost::default();
        assert_eq!(cost.input, 0.0);
        assert_eq!(cost.output, 0.0);
        assert!(cost.cache_read.is_none());
    }

    #[test]
    fn model_modalities_default() {
        let mods = ModelModalities::default();
        assert_eq!(mods.input, vec![Modality::Text]);
        assert_eq!(mods.output, vec![Modality::Text]);
    }

    #[test]
    fn model_status_default() {
        let status = ModelStatus::default();
        assert_eq!(status, ModelStatus::Active);
    }
}
