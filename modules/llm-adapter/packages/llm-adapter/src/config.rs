use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdapterConfig {
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    pub models: HashMap<String, ModelConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            base_url: None,
            api_key_env: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_model_id: Option<String>,
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            provider: None,
            base_url: None,
            api_model_id: None,
        }
    }
}

#[allow(dead_code)]
fn config_file_paths() -> Vec<String> {
    let mut paths = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        paths.push(format!("{}/.coolzhu/config.json", home));
    }
    if let Ok(userprofile) = std::env::var("USERPROFILE") {
        paths.push(format!("{}/.coolzhu/config.json", userprofile));
    }
    paths.push("./coolzhu.json".to_string());
    paths.push("./.coolzhu/config.json".to_string());
    paths
}

#[allow(dead_code)]
pub fn load_config() -> Result<AdapterConfig, ApiError> {
    let paths = config_file_paths();
    for path in paths {
        let config = load_config_from_path(Path::new(&path))?;
        if !config.providers.is_empty() || !config.models.is_empty() {
            return Ok(config);
        }
    }
    Ok(AdapterConfig::default())
}

#[allow(dead_code)]
fn load_config_from_path(path: &Path) -> Result<AdapterConfig, ApiError> {
    if !path.exists() {
        return Ok(AdapterConfig::default());
    }
    let content = fs::read_to_string(path).map_err(|e| ApiError::ConfigError {
        path: path.display().to_string(),
        message: e.to_string(),
    })?;
    serde_json::from_str(&content).map_err(|e| ApiError::ConfigError {
        path: path.display().to_string(),
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn adapter_config_default() {
        let config = AdapterConfig::default();
        assert!(config.providers.is_empty());
        assert!(config.models.is_empty());
    }

    #[test]
    fn provider_config_default() {
        let config = ProviderConfig::default();
        assert!(config.base_url.is_none());
        assert!(config.api_key_env.is_none());
    }

    #[test]
    fn model_config_default() {
        let config = ModelConfig::default();
        assert!(config.provider.is_none());
        assert!(config.base_url.is_none());
        assert!(config.api_model_id.is_none());
    }

    #[test]
    fn parse_adapter_config_json() {
        let json = json!({
            "providers": {
                "alibaba-bailian": {
                    "base_url": "https://dashscope.aliyuncs.com/compatible-mode/v1"
                }
            },
            "models": {
                "glm-5-bailian": {
                    "provider": "alibaba-bailian",
                    "api_model_id": "glm-zhipu-5"
                }
            }
        });
        let config: AdapterConfig = serde_json::from_value(json).expect("parse config");
        assert_eq!(config.providers.len(), 1);
        assert_eq!(config.models.len(), 1);
        let bailian = config.providers.get("alibaba-bailian").expect("provider");
        assert_eq!(
            bailian.base_url,
            Some("https://dashscope.aliyuncs.com/compatible-mode/v1".to_string())
        );
        let glm5 = config.models.get("glm-5-bailian").expect("model");
        assert_eq!(glm5.provider, Some("alibaba-bailian".to_string()));
        assert_eq!(glm5.api_model_id, Some("glm-zhipu-5".to_string()));
    }

    #[test]
    fn load_config_missing_file() {
        let config = load_config_from_path(Path::new("/nonexistent/path.json")).expect("load");
        assert!(config.providers.is_empty());
        assert!(config.models.is_empty());
    }
}
