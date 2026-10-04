//! 远程模型目录查询：只返回草稿候选，不保存会话，也不发送生成或图片请求。
use super::{
    api_error, effective_provider_key, model_settings_base_url, model_settings_protocol,
    session_api_key, session_model_settings_for, session_store, ApiResult,
};
use axum::{http::StatusCode, Json};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    time::Duration,
};

const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_MODELS: usize = 5000;

// 不派生 Debug，避免未来诊断输出意外记录一次性凭据。
#[derive(Deserialize)]
pub(super) struct DiscoveryRequest {
    session_id: Option<String>,
    base_url: String,
    protocol: String,
    endpoint: Option<String>,
    api_key: Option<String>,
    #[serde(default = "default_true")]
    use_saved_key: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub(super) struct DiscoveredModel {
    id: String,
    name: String,
    input_modalities: Option<Vec<String>>,
    image_input: Option<bool>,
    capability_source: &'static str,
    capability_reference: Option<&'static str>,
    capability_checked_at: Option<&'static str>,
    context_window: Option<u64>,
    max_output_tokens: Option<u64>,
}

#[derive(Serialize)]
pub(super) struct DiscoveryResponse {
    models: Vec<DiscoveredModel>,
    provider_hint: String,
    complete: bool,
    warnings: Vec<String>,
}

fn invalid(message: impl AsRef<str>) -> super::ApiError {
    api_error(StatusCode::BAD_REQUEST, message.as_ref())
}

fn base_url(raw: &str) -> Result<Url, &'static str> {
    let url = Url::parse(raw.trim()).map_err(|_| "请输入有效的远程 Base URL。")?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("模型发现只支持 HTTP(S) 远程地址。");
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Base URL 不应包含用户名、密码、查询参数或片段；请将密钥填写在 API Key 字段。");
    }
    let host = url
        .host_str()
        .unwrap_or_default()
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || !host.contains('.') && host.parse::<IpAddr>().is_err()
    {
        return Err("本地或内网模型保留手动填写，不进行远程模型发现。");
    }
    if host.parse::<IpAddr>().is_ok_and(|ip| !public_ip(ip)) {
        return Err("本地或内网模型保留手动填写，不进行远程模型发现。");
    }
    Ok(url)
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        || a >= 224
        || a == 100 && (64..=127).contains(&b)
        || a == 198 && (18..=19).contains(&b)
        || a == 192 && b == 0 && c == 0)
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_v4(ip),
        IpAddr::V6(ip) => {
            if let Some(ip) = ip.to_ipv4_mapped() {
                return public_v4(ip);
            }
            let segments = ip.segments();
            // 仅接收全局单播段；排除文档地址和可能封装内网 IPv4 的过渡地址。
            segments[0] & 0xe000 == 0x2000
                && segments[0] != 0x2002
                && !(segments[0] == 0x2001 && (segments[1] == 0 || segments[1] == 0x0db8))
        }
    }
}

fn listing_url(base: &Url, protocol: &str, endpoint: Option<&str>) -> Result<Url, &'static str> {
    let mut url = base.clone();
    if let Some(endpoint) = endpoint.map(str::trim).filter(|value| !value.is_empty()) {
        let route_protocol = match protocol {
            "openai_chat_completions" => api::ProviderProtocol::OpenAiChatCompletions,
            "anthropic_messages" => api::ProviderProtocol::AnthropicMessages,
            _ => return Err("该协议暂不支持发现模型，请手动填写模型 ID。"),
        };
        let resolved = api::EndpointResolver::resolve(base.as_str(), route_protocol, Some(endpoint))
            .map_err(|_| "无法解析请求路径 Endpoint。")?;
        url = base_url(&resolved)?;
        if url.origin() != base.origin() {
            return Err("模型目录的请求路径不能跳转到其他服务器，请检查 Endpoint。");
        }
    }
    let route_path = url.path().trim_end_matches('/').to_string();
    let path = route_path.strip_suffix("/chat/completions")
        .or_else(|| route_path.strip_suffix("/messages"))
        .unwrap_or(&route_path);
    match protocol {
        "openai_chat_completions" => url.set_path(&format!("{path}/models")),
        "anthropic_messages" => {
            let root = path.strip_suffix("/v1").unwrap_or(path);
            url.set_path(&format!("{root}/v1/models"));
            url.set_query(Some("limit=1000"));
        }
        _ => return Err("该协议暂不支持发现模型，请手动填写模型 ID。"),
    }
    Ok(url)
}

fn resolved_address_allowed(base: &Url, ip: IpAddr) -> bool {
    if public_ip(ip) { return true; }
    // Clash/TUN 的 Fake-IP 只作为域名解析结果使用，连接仍校验原域名的标准 TLS。
    // 不将该段标为公网，不接受字面 IP、HTTP 或其他内网/回环地址。
    let domain_https = base.scheme() == "https" && base.host_str().is_some_and(|host| host.trim_matches(['[', ']']).parse::<IpAddr>().is_err());
    domain_https && matches!(ip, IpAddr::V4(ip) if ip.octets()[0] == 198 && (18..=19).contains(&ip.octets()[1]))
}

fn same_target(left: &str, right: &str) -> bool {
    match (base_url(left), base_url(right)) {
        (Ok(left), Ok(right)) => {
            left.as_str().trim_end_matches('/') == right.as_str().trim_end_matches('/')
        }
        _ => false,
    }
}

fn provider_hint(host: &str) -> String {
    match host {
        "dashscope.aliyuncs.com" | "dashscope-intl.aliyuncs.com" | "dashscope-us.aliyuncs.com" => {
            "阿里百炼".into()
        }
        "api.openai.com" => "OpenAI".into(),
        "api.anthropic.com" => "Anthropic".into(),
        "api.deepseek.com" => "DeepSeek".into(),
        "openrouter.ai" => "OpenRouter".into(),
        "open.bigmodel.cn" => "智谱".into(),
        _ => host.to_string(),
    }
}

fn label(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 1024)
        .map(str::to_string)
}

fn capacity(entry: &Value, paths: &[&str]) -> Option<u64> {
    paths.iter().find_map(|path| {
        entry
            .pointer(path)
            .and_then(Value::as_u64)
            .filter(|v| *v > 0)
    })
}

fn modalities(entry: &Value) -> (Option<Vec<String>>, Option<bool>) {
    for path in [
        "/input_modalities",
        "/inputModalities",
        "/architecture/input_modalities",
        "/modalities/input",
    ] {
        let Some(values) = entry.pointer(path).and_then(Value::as_array) else {
            continue;
        };
        if values.is_empty() || values.iter().any(|v| !v.is_string()) {
            continue;
        }
        let values: Vec<String> = values
            .iter()
            .filter_map(Value::as_str)
            .map(|s| s.to_ascii_lowercase())
            .collect();
        let known = values.iter().all(|s| {
            matches!(
                s.as_str(),
                "text" | "image" | "audio" | "video" | "file" | "pdf"
            )
        });
        let has_image = values.iter().any(|s| s == "image");
        // 未知枚举可能含新的图像类型，不能把缺少 image 猜成明确不支持。
        let supports = if has_image {
            Some(true)
        } else if known {
            Some(false)
        } else {
            None
        };
        return (Some(values), supports);
    }
    (None, None)
}

fn parse_listing(body: &Value) -> Result<(Vec<DiscoveredModel>, bool), &'static str> {
    let rows: Vec<(Option<&str>, &Value)> =
        if let Some(rows) = body.get("data").and_then(Value::as_array) {
            rows.iter().map(|v| (None, v)).collect()
        } else if let Some(rows) = body.get("models").and_then(Value::as_object) {
            rows.iter()
                .map(|(key, value)| (Some(key.as_str()), value))
                .collect()
        } else {
            return Err("服务器没有返回可识别的模型目录，可继续手动填写模型 ID。");
        };
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    let mut complete = body.get("has_more").and_then(Value::as_bool) != Some(true)
        && body
            .get("next_page_token")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        && body.get("next").filter(|v| !v.is_null()).is_none();
    for (key, entry) in rows {
        if !entry.is_object() {
            continue;
        }
        let id = key
            .filter(|k| !k.is_empty() && k.len() <= 1024)
            .map(str::to_string)
            .or_else(|| label(entry.get("id")));
        let Some(id) = id else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        if result.len() >= MAX_MODELS {
            complete = false;
            break;
        }
        let name = ["name", "display_name", "displayName"]
            .iter()
            .find_map(|key| label(entry.get(key)))
            .unwrap_or_else(|| id.clone());
        let (input_modalities, image_input) = modalities(entry);
        let capability_source = if image_input.is_some() {
            "remote_metadata"
        } else {
            "unknown"
        };
        result.push(DiscoveredModel {
            id,
            name,
            input_modalities,
            image_input,
            capability_source,
            capability_reference: None,
            capability_checked_at: None,
            context_window: capacity(
                entry,
                &[
                    "/context_window",
                    "/contextWindow",
                    "/context_length",
                    "/max_input_tokens",
                    "/limit/context",
                ],
            ),
            max_output_tokens: capacity(
                entry,
                &[
                    "/max_output_tokens",
                    "/maxOutputTokens",
                    "/max_tokens",
                    "/maxTokens",
                    "/limit/output",
                    "/top_provider/max_completion_tokens",
                ],
            ),
        });
    }
    Ok((result, complete))
}

/// 只补齐精确官方端点与精确模型的缺失声明，不推测代理重命名模型的能力。
fn supplement_official_catalog(base: &Url, models: &mut [DiscoveredModel]) {
    if base.scheme() != "https"
        || base.host_str() != Some("dashscope.aliyuncs.com")
        || base.port_or_known_default() != Some(443)
        || !matches!(base.path().trim_end_matches('/'), "/compatible-mode" | "/compatible-mode/v1")
    {
        return;
    }
    for model in models.iter_mut().filter(|model| model.id == "qwen3.8-flash") {
        // 服务器明确声明时保持服务器结果；随包目录只补 unknown，不覆盖显式否定。
        if model.image_input.is_none() {
            model.input_modalities = Some(vec!["text".into(), "image".into(), "video".into()]);
            model.image_input = Some(true);
            model.capability_source = "official_catalog";
            model.capability_reference = Some("https://help.aliyun.com/zh/model-studio/qwen3-8-flash");
            model.capability_checked_at = Some("2026-09-26");
            model.context_window.get_or_insert(1_000_000);
            model.max_output_tokens.get_or_insert(131_072);
        }
    }
}

fn credential(request: &DiscoveryRequest) -> ApiResult<Option<String>> {
    if let Some(key) = &request.api_key {
        let key = key.trim();
        if key.is_empty() {
            return Ok(None);
        }
        if key.len() > 4096 || !key.is_ascii() || key.bytes().any(|v| v < 0x21 || v == 0x7f) {
            return Err(invalid("API Key 不是有效的 HTTP 凭据，请仅粘贴原始密钥。"));
        }
        return Ok(Some(key.to_string()));
    }
    if !request.use_saved_key {
        return Ok(None);
    }
    let Some(session_id) = request.session_id.as_deref() else {
        return Ok(None);
    };
    let agent = {
        let store = session_store()
            .lock()
            .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法读取会话配置。"))?;
        let session = store
            .find_session(session_id)
            .ok_or_else(|| api_error(StatusCode::NOT_FOUND, "会话不存在。"))?;
        session.to_agent_session(store.is_active(session_id))
    };
    let settings = session_model_settings_for(session_id);
    if super::agent_session_backend::AgentSessionBackend::for_provider(&agent.provider)
        != super::agent_session_backend::AgentSessionBackend::LlmHttp {
        return Err(invalid("Devin 使用 CLI 模型发现，不能继承 HTTP 查询凭据。"));
    }
    if !same_target(
        &request.base_url,
        &model_settings_base_url(&agent, &settings),
    ) || request.protocol != model_settings_protocol(&agent, &settings)
    {
        return Err(invalid("地址或协议已更改。为避免把已有密钥发送到新目标，请填写此次查询使用的 API Key，或选择无密钥查询。"));
    }
    let provider_kind = api::provider_kind_from_name(&agent.provider)
        .unwrap_or_else(|| api::detect_provider_kind(&agent.model));
    let env_keys = api::ModelRegistry::global()
        .resolve_model_for_provider(&agent.model, provider_kind)
        .env_keys;
    let key = effective_provider_key(session_api_key(session_id), &env_keys, |name| {
        std::env::var(name).ok()
    });
    if key.as_ref().is_some_and(|key| {
        key.len() > 4096 || !key.is_ascii() || key.bytes().any(|v| v < 0x21 || v == 0x7f)
    }) {
        return Err(invalid(
            "保存的 API Key 不是有效 HTTP 凭据，请在表单中填写新的密钥后重试。",
        ));
    }
    Ok(key)
}

pub(super) async fn discover(
    Json(request): Json<DiscoveryRequest>,
) -> ApiResult<Json<DiscoveryResponse>> {
    let base = base_url(&request.base_url).map_err(invalid)?;
    let url = listing_url(&base, &request.protocol, request.endpoint.as_deref()).map_err(invalid)?;
    let key = credential(&request)?;
    if key.is_some() && base.scheme() != "https" {
        return Err(invalid("携带 API Key 查询远程目录需要 HTTPS 地址。"));
    }
    let host = base.host_str().unwrap_or_default().trim_matches(['[', ']']);
    let port = base.port_or_known_default().unwrap_or(443);
    let addresses: Vec<SocketAddr> = tokio::time::timeout(
        Duration::from_secs(3),
        tokio::net::lookup_host((host, port)),
    )
    .await
    .map_err(|_| api_error(StatusCode::GATEWAY_TIMEOUT, "模型目录域名解析超时。"))?
    .map_err(|_| {
        api_error(
            StatusCode::BAD_GATEWAY,
            "无法解析模型目录域名，请检查 Base URL。",
        )
    })?
    .collect();
    if addresses.is_empty() || addresses.iter().any(|address| !resolved_address_allowed(&base, address.ip())) {
        return Err(invalid("此地址指向本地或内网服务；本地模型保留手动填写。"));
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .resolve_to_addrs(host, &addresses)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| api_error(StatusCode::INTERNAL_SERVER_ERROR, "无法创建模型目录连接。"))?;
    let mut query = client.get(url).header("Accept", "application/json");
    if request.protocol == "anthropic_messages" {
        query = query.header("anthropic-version", "2023-06-01");
        if let Some(key) = key {
            query = query.header("x-api-key", key);
        }
    } else if let Some(key) = key {
        query = query.bearer_auth(key);
    }
    let mut response = query.send().await.map_err(|_| {
        api_error(
            StatusCode::BAD_GATEWAY,
            "无法读取远程模型目录，请检查地址、网络或协议。可继续手动填写模型。",
        )
    })?;
    let status = response.status();
    if !status.is_success() {
        let message = match status.as_u16() {
            401 | 403 => "模型目录拒绝认证，请检查 API Key 或该密钥的访问范围。".to_string(),
            404 | 405 => {
                "该地址没有提供此协议的模型目录，请确认 Base URL，或手动填写模型。".to_string()
            }
            300..=399 => {
                "模型目录返回重定向；查询已停止，请直接填写目标服务的 Base URL。".to_string()
            }
            429 => "模型目录请求过于频繁，请稍后重试。".to_string(),
            code => format!("模型目录返回 HTTP {code}，可稍后重试或手动填写。"),
        };
        return Err(api_error(StatusCode::BAD_GATEWAY, &message));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BYTES as u64)
    {
        return Err(api_error(
            StatusCode::BAD_GATEWAY,
            "模型目录超过 4 MiB 限制。",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| api_error(StatusCode::BAD_GATEWAY, "读取模型目录中断，请重试。"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_BYTES {
            return Err(api_error(
                StatusCode::BAD_GATEWAY,
                "模型目录超过 4 MiB 限制。",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    let body: Value = serde_json::from_slice(&bytes).map_err(|_| {
        api_error(
            StatusCode::BAD_GATEWAY,
            "模型目录未返回有效 JSON，可继续手动填写模型。",
        )
    })?;
    let (mut models, complete) =
        parse_listing(&body).map_err(|message| api_error(StatusCode::BAD_GATEWAY, message))?;
    supplement_official_catalog(&base, &mut models);
    let mut warnings = Vec::new();
    if !complete {
        warnings.push("服务器仍有后续结果或目录超过显示上限；当前只展示已获取的部分。".into());
    }
    if models.iter().any(|model| model.image_input.is_none()) {
        warnings.push("部分模型未声明图片输入能力；未知不等于不支持，可手动选择。".into());
    }
    if models.is_empty() {
        warnings.push("服务器返回空目录；这不代表手动填写的模型无法调用。".into());
    }
    warnings.push(
        "能力来源见模型详情（服务端声明或已核对的官方资料），查询未进行实测，也不更改保存的配置。".into(),
    );
    Ok(Json(DiscoveryResponse {
        models,
        provider_hint: provider_hint(host),
        complete,
        warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn listing_preserves_prefix_and_original_base() {
        let base = base_url("https://gateway.example/compatible-mode/v1/").unwrap();
        assert_eq!(
            listing_url(&base, "openai_chat_completions", None)
                .unwrap()
                .as_str(),
            "https://gateway.example/compatible-mode/v1/models"
        );
        assert_eq!(base.as_str(), "https://gateway.example/compatible-mode/v1/");
        assert_eq!(
            listing_url(&base, "anthropic_messages", None).unwrap().as_str(),
            "https://gateway.example/compatible-mode/v1/models?limit=1000"
        );
        assert!(!same_target(
            base.as_str(),
            "https://gateway.example/another/v1"
        ));
        let split = base_url("https://dashscope.aliyuncs.com/compatible-mode").unwrap();
        assert_eq!(listing_url(&split, "openai_chat_completions", Some("v1/chat/completions")).unwrap().as_str(), "https://dashscope.aliyuncs.com/compatible-mode/v1/models");
        assert_eq!(listing_url(&base, "openai_chat_completions", Some("v1/chat/completions")).unwrap().as_str(), "https://gateway.example/compatible-mode/v1/models");
        assert!(listing_url(&base, "openai_chat_completions", Some("https://other.example/v1/chat/completions")).is_err());
    }

    #[test]
    fn capabilities_come_only_from_explicit_input_metadata() {
        let (models, complete) = parse_listing(&json!({"data":[
            {"id":"vision-name-only"}, {"id":"actual","architecture":{"input_modalities":["text","image"]}},
            {"id":"text","modalities":{"input":["text"],"output":["image"]}},
            {"id":"future","input_modalities":["future-image"]}, {"id":"actual"}
        ]})).unwrap();
        assert!(complete);
        assert_eq!(models.len(), 4);
        assert_eq!(
            models
                .iter()
                .map(|model| model.image_input)
                .collect::<Vec<_>>(),
            vec![None, Some(true), Some(false), None]
        );
    }

    #[test]
    fn gateway_alias_and_partial_are_reported() {
        let (models, complete) = parse_listing(&json!({"models":{"alias":{"id":"canonical","limit":{"context":9000}}},"has_more":true})).unwrap();
        assert!(!complete);
        assert_eq!(models[0].id, "alias");
        assert_eq!(models[0].context_window, Some(9000));
        assert!(parse_listing(&json!({"error":"not a directory"})).is_err());
    }

    #[test]
    fn official_capabilities_require_exact_endpoint_and_do_not_override_server() {
        let listing = json!({"data":[{"id":"qwen3.8-flash"},{"id":"qwen3.8-flash-alias"}]});
        let (mut models, _) = parse_listing(&listing).unwrap();
        supplement_official_catalog(&base_url("https://proxy.example/compatible-mode").unwrap(), &mut models);
        assert_eq!(models[0].image_input, None);
        supplement_official_catalog(&base_url("https://dashscope.aliyuncs.com/other").unwrap(), &mut models);
        assert_eq!(models[0].image_input, None);
        supplement_official_catalog(&base_url("https://dashscope.aliyuncs.com/compatible-mode").unwrap(), &mut models);
        assert_eq!(models[0].image_input, Some(true));
        assert_eq!(models[0].capability_source, "official_catalog");
        assert_eq!(models[1].image_input, None);
        let (mut declared, _) = parse_listing(&json!({"data":[{"id":"qwen3.8-flash","input_modalities":["text"]}]})).unwrap();
        supplement_official_catalog(&base_url("https://dashscope.aliyuncs.com/compatible-mode").unwrap(), &mut declared);
        assert_eq!(declared[0].image_input, Some(false));
        assert_eq!(declared[0].capability_source, "remote_metadata");
    }

    #[test]
    fn local_targets_and_embedded_credentials_are_refused() {
        for target in [
            "http://localhost:8080/v1",
            "http://127.0.0.1/v1",
            "http://10.1.2.3/v1",
            "http://[::1]/v1",
            "https://host.local/v1",
            "https://key@example.com/v1",
            "https://example.com/v1?key=secret",
        ] {
            assert!(base_url(target).is_err(), "{target}");
        }
        assert!(!public_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!public_ip("2002:7f00:1::".parse().unwrap()));
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(public_ip("2606:4700::1111".parse().unwrap()));
        let remote = base_url("https://dashscope.aliyuncs.com/compatible-mode").unwrap();
        assert!(resolved_address_allowed(&remote, "198.18.0.133".parse().unwrap()));
        for ip in ["127.0.0.1", "10.0.0.1", "192.168.0.1", "169.254.169.254", "::1"] {
            assert!(!resolved_address_allowed(&remote, ip.parse().unwrap()));
        }
        assert!(!resolved_address_allowed(&base_url("http://example.com/v1").unwrap(), "198.18.0.133".parse().unwrap()));
        assert!(base_url("https://198.18.0.133/v1").is_err());
    }
}
