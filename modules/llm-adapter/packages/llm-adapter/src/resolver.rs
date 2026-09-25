use crate::error::ApiError;
use crate::providers::ProviderKind;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProtocol {
    OpenAiChatCompletions,
    AnthropicMessages,
    OpenAiImagesGenerations,
    OpenAiVideos,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestCapability {
    Chat,
    VisionChat,
    ImageGeneration,
    VideoGeneration,
    Audio,
    Embedding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthPolicy {
    Required,
    Optional,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProviderRoute {
    pub protocol: ProviderProtocol,
    pub capability: RequestCapability,
    pub endpoint: String,
    pub auth_policy: AuthPolicy,
}

pub struct EndpointResolver;

impl EndpointResolver {
    pub fn resolve(
        base_or_endpoint: &str,
        protocol: ProviderProtocol,
        endpoint_override: Option<&str>,
    ) -> Result<String, ApiError> {
        let base = normalize_non_empty_url(base_or_endpoint)?;
        if let Some(override_endpoint) = endpoint_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Ok(resolve_override(&base, override_endpoint));
        }
        Ok(resolve_protocol_endpoint(&base, protocol))
    }

    pub fn protocol_for_capability(
        capability: RequestCapability,
    ) -> Result<ProviderProtocol, ApiError> {
        match capability {
            RequestCapability::Chat | RequestCapability::VisionChat => {
                Ok(ProviderProtocol::OpenAiChatCompletions)
            }
            RequestCapability::ImageGeneration => Ok(ProviderProtocol::OpenAiImagesGenerations),
            RequestCapability::VideoGeneration => Ok(ProviderProtocol::OpenAiVideos),
            RequestCapability::Audio | RequestCapability::Embedding => {
                Err(ApiError::UnsupportedCapability {
                    capability: format!("{capability:?}"),
                })
            }
        }
    }

    pub fn protocol_for_provider_capability(
        provider: ProviderKind,
        capability: RequestCapability,
    ) -> Result<ProviderProtocol, ApiError> {
        match (provider, capability) {
            (ProviderKind::ClawApi | ProviderKind::Anthropic, RequestCapability::Chat)
            | (ProviderKind::ClawApi | ProviderKind::Anthropic, RequestCapability::VisionChat) => {
                Ok(ProviderProtocol::AnthropicMessages)
            }
            (_, capability) => Self::protocol_for_capability(capability),
        }
    }

    pub fn resolve_route(
        base_or_endpoint: &str,
        provider: ProviderKind,
        capability: RequestCapability,
        endpoint_override: Option<&str>,
        auth_policy: AuthPolicy,
    ) -> Result<ResolvedProviderRoute, ApiError> {
        let protocol = Self::protocol_for_provider_capability(provider, capability)?;
        let endpoint = Self::resolve(base_or_endpoint, protocol, endpoint_override)?;
        Ok(ResolvedProviderRoute {
            protocol,
            capability,
            endpoint,
            auth_policy,
        })
    }
}

fn normalize_non_empty_url(value: &str) -> Result<String, ApiError> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(ApiError::ConfigError {
            path: "provider.base_url".to_string(),
            message: "base_url or endpoint must not be empty".to_string(),
        });
    }
    Ok(trimmed.to_string())
}

fn resolve_override(base: &str, override_endpoint: &str) -> String {
    let override_endpoint = override_endpoint.trim().trim_end_matches('/');
    if is_absolute_http_url(override_endpoint) {
        return override_endpoint.to_string();
    }

    append_path_without_duplicate(base, override_endpoint.trim_start_matches('/'))
}

fn resolve_protocol_endpoint(base: &str, protocol: ProviderProtocol) -> String {
    let default_path = protocol.default_path();
    let terminal_tail = protocol.terminal_tail();
    let lower = base.to_ascii_lowercase();
    if lower.ends_with(&format!("/{default_path}")) || lower.ends_with(&format!("/{terminal_tail}"))
    {
        return base.to_string();
    }

    if lower.ends_with("/v1") && default_path.starts_with("v1/") {
        return format!("{base}/{}", default_path.trim_start_matches("v1/"));
    }

    let append_path = if has_path_after_authority(base)
        && !matches!(protocol, ProviderProtocol::AnthropicMessages)
    {
        terminal_tail
    } else {
        default_path
    };
    append_path_without_duplicate(base, append_path)
}

fn append_path_without_duplicate(base: &str, path: &str) -> String {
    let base = base.trim_end_matches('/');
    let path = path.trim_matches('/');
    if path.is_empty() {
        return base.to_string();
    }

    let lower_base = base.to_ascii_lowercase();
    let lower_path = path.to_ascii_lowercase();
    if lower_base.ends_with(&format!("/{lower_path}")) {
        return base.to_string();
    }
    if lower_base.ends_with("/v1") && lower_path.starts_with("v1/") {
        return format!("{base}/{}", &path[3..]);
    }
    format!("{base}/{path}")
}

fn has_path_after_authority(url: &str) -> bool {
    let without_scheme = url
        .split_once("://")
        .map_or(url, |(_, remainder)| remainder);
    without_scheme.contains('/')
}

fn is_absolute_http_url(value: &str) -> bool {
    value.starts_with("http://") || value.starts_with("https://")
}

impl ProviderProtocol {
    const fn default_path(self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => "v1/chat/completions",
            Self::AnthropicMessages => "v1/messages",
            Self::OpenAiImagesGenerations => "v1/images/generations",
            Self::OpenAiVideos => "v1/videos",
        }
    }

    const fn terminal_tail(self) -> &'static str {
        match self {
            Self::OpenAiChatCompletions => "chat/completions",
            Self::AnthropicMessages => "messages",
            Self::OpenAiImagesGenerations => "images/generations",
            Self::OpenAiVideos => "videos",
        }
    }
}
