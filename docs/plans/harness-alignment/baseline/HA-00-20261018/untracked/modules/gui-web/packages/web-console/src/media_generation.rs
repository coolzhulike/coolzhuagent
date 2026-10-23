//! 图像/视频使用独立协议与参数；产物落盘，用量与对话 token 分表。
use super::*;
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(super) struct Config {
    #[serde(default)]
    pub parameters: BTreeMap<String, JsonValue>,
    #[serde(default, skip_serializing_if = "JsonValue::is_null")]
    pub capabilities: JsonValue,
}
const IMAGE: &[&str] = &[
    "n",
    "size",
    "aspect_ratio",
    "resolution",
    "quality",
    "output_format",
    "seed",
];
const VIDEO: &[&str] = &[
    "duration",
    "size",
    "aspect_ratio",
    "resolution",
    "generate_audio",
    "seed",
    "callback_url",
    "width",
    "height",
    "num_frames",
    "frame_rate",
];
fn agnes_video_25(agent: &AgentSessionDto) -> bool {
    agent.model_type == "video" && matches!(agent.model.as_str(), "agnes-video-2.5" | "agnes-video-2.5-flash")
}
pub(super) fn validate(config: &Config, agent: &AgentSessionDto) -> Result<(), String> {
    let kind = agent.model_type.as_str();
    let allowed = match kind {
        "image" => IMAGE,
        "video" => VIDEO,
        _ => {
            return if config.parameters.is_empty() {
                Ok(())
            } else {
                Err("对话模型不能保留图像或视频参数".into())
            }
        }
    };
    let cap = &config.capabilities;
    if config.parameters.len() > 16 {
        return Err("生成参数过多".into());
    }
    if !cap.is_null()
        && (cap["model"] != agent.model
            || cap["model_type"] != kind
            || cap["base_url"].as_str().unwrap_or("").trim_end_matches('/')
                != agent
                    .base_url
                    .as_deref()
                    .unwrap_or("")
                    .trim_end_matches('/'))
    {
        return Err("生成能力目录属于其它模型或地址，请重新获取目录".into());
    }
    for (key, value) in &config.parameters {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("当前生成类型不支持参数 {key}"));
        }
        if value.is_null() {
            return Err(format!("未设置的生成参数 {key} 请省略，不能保存空值"));
        }
        if agnes_video_25(agent) {
            if !["duration", "size", "resolution", "aspect_ratio", "seed"].contains(&key.as_str()) {
                return Err(format!("此 Agnes 视频模型不支持 {key}，请清除此参数"));
            }
            if key == "duration" && !value.as_u64().is_some_and(|n| (4..=12).contains(&n)) {
                return Err("此 Agnes 视频模型时长为 4–12 秒".into());
            }
            if ["size", "resolution"].contains(&key.as_str()) {
                let allowed = if agent.model.ends_with("-flash") { &["720P"][..] } else { &["720P", "1080P", "1K", "2K"][..] };
                if !value.as_str().is_some_and(|v| allowed.contains(&v)) { return Err(format!("此模型分辨率支持 {}", allowed.join(" / "))); }
            }
            if key == "aspect_ratio" && !value.as_str().is_some_and(|v| ["21:9", "16:9", "4:3", "1:1", "3:4", "9:16"].contains(&v)) {
                return Err("此视频模型不支持该宽高比".into());
            }
        }
        let numeric = match key.as_str() {
            "n" => Some((1, 10)),
            "duration" => Some((1, 600)),
            "width" | "height" => Some((64, 8192)),
            "num_frames" => Some((1, 14401)),
            "frame_rate" => Some((1, 120)),
            "seed" => Some((0, 4_294_967_295)),
            _ => None,
        };
        if let Some((min, max)) = numeric {
            if !value.as_u64().is_some_and(|n| (min..=max).contains(&n)) {
                return Err(format!("{key} 必须为 {min}–{max} 的整数"));
            }
        } else if key == "generate_audio" {
            if !value.is_boolean() {
                return Err("音频选项必须为开或关".into());
            }
        } else if !value
            .as_str()
            .is_some_and(|v| !v.is_empty() && v.len() <= 2048)
        {
            return Err(format!("{key} 内容无效"));
        }
        if key == "size" && !agnes_video_25(agent) {
            let text = value.as_str().unwrap();
            if text != "auto" {
                let (w, h) = text
                    .split_once('x')
                    .ok_or("尺寸格式为 宽x高，例如 1024x1024")?;
                let w = w.parse::<u32>().map_err(|_| "宽度无效")?;
                let h = h.parse::<u32>().map_err(|_| "高度无效")?;
                if !(64..=8192).contains(&w)
                    || !(64..=8192).contains(&h)
                    || u64::from(w) * u64::from(h) > 33_554_432
                {
                    return Err("尺寸超出支持范围".into());
                }
            }
        }
        if key == "output_format" && !matches!(value.as_str(), Some("png" | "jpeg" | "webp")) {
            return Err("输出格式支持 PNG、JPEG、WebP".into());
        }
        if key == "callback_url" {
            let url = reqwest::Url::parse(value.as_str().unwrap()).map_err(|_| "回调地址无效")?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err("回调须为不含账号密码的 HTTPS 地址".into());
            }
        }
        if !cap.is_null() && key != "callback_url" {
            let rule = &cap["fields"][key];
            if rule.is_null() && !agnes_video_25(agent) {
                return Err(format!("模型目录未声明支持 {key}"));
            }
            if let Some(values) = rule["values"].as_array() {
                if !values.contains(value) {
                    return Err(format!("{key} 不在模型目录允许范围内"));
                }
            }
            if let Some(n) = value.as_f64() {
                if rule["minimum"].as_f64().is_some_and(|m| n < m)
                    || rule["maximum"].as_f64().is_some_and(|m| n > m)
                {
                    return Err(format!("{key} 超出模型目录范围"));
                }
            }
        } else if agent.provider == "openrouter"
            && !(key == "n" && value == 1)
            && key != "callback_url"
        {
            return Err(format!("请先获取此模型的生成目录，再设置 {key}"));
        }
    }
    if config.parameters.contains_key("size")
        && (config.parameters.contains_key("aspect_ratio")
            || config.parameters.contains_key("resolution"))
    {
        return Err("尺寸与宽高比/分辨率不能同时指定".into());
    }
    if config.parameters.contains_key("width") != config.parameters.contains_key("height") {
        return Err("宽度与高度须同时填写".into());
    }
    Ok(())
}
pub(super) fn endpoint(
    agent: &AgentSessionDto,
    settings: &SessionModelLimitOverride,
) -> Result<String, String> {
    let protocol = if agent.model_type == "video" {
        api::ProviderProtocol::OpenAiVideos
    } else {
        api::ProviderProtocol::OpenAiImagesGenerations
    };
    let supplied = settings
        .endpoint
        .as_deref()
        .or(agent.endpoint.as_deref())
        .filter(|s| !s.trim().is_empty());
    let endpoint =
        if agent.provider == "openrouter" && agent.model_type == "image" && supplied.is_none() {
            Some("images")
        } else {
            supplied
        };
    api::EndpointResolver::resolve(
        &model_settings_base_url(agent, settings),
        protocol,
        endpoint,
    )
    .map_err(|e| e.to_string())
}
pub(super) fn body(
    agent: &AgentSessionDto,
    config: &Config,
    prompt: &str,
    reference: Option<&str>,
) -> Result<JsonValue, String> {
    validate(config, agent)?;
    let mut result = json!({"model":agent.model,"prompt":prompt});
    if agent.model_type == "image" {
        result["n"] = json!(1);
    }
    let agnes = agent.model.to_ascii_lowercase().starts_with("agnes");
    // 留空交给供应商选择默认值；新版 Agnes 会拒绝旧版宽高/帧数参数。
    if agnes_video_25(agent) { result["mode"] = json!("text"); }
    for (key, value) in &config.parameters {
        if !value.is_null() {
            if agnes_video_25(agent) && key == "duration" { result["seconds"] = json!(value.as_u64().unwrap().to_string()); }
            else if agnes_video_25(agent) && key == "resolution" { result["size"] = value.clone(); }
            else { result[key] = value.clone(); }
        }
    }
    if let Some(image) = reference {
        if agent.provider == "openrouter" {
            result["input_references"] = json!([{"type":"image_url","image_url":{"url":image}}]);
        } else if agnes_video_25(agent) {
            if !reqwest::Url::parse(image).ok().is_some_and(|url| matches!(url.scheme(), "https" | "http")) {
                return Err("此视频模型需要公开的参考图链接，暂不支持本地图片附件。".into());
            }
            result["mode"] = json!("reference");
            result["images"] = json!([image]);
        } else if agnes && agent.model_type == "video" {
            result["image"] = json!(agnes_video_image_base64(image)?);
        } else if agnes {
            result["extra_body"] = json!({"image":image});
        } else {
            return Err("此生成协议尚未适配参考图；请移除图片附件，或使用声明支持参考图的 OpenRouter / Agnes 模型。".into());
        }
    }
    Ok(result)
}
pub(super) fn video_poll_url(agent: &AgentSessionDto, submit_url: &str, task_id: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(submit_url).map_err(|_| "视频轮询地址无效")?;
    if agnes_video_25(agent) {
        // 与提交同源；官方新版用 video_id + model_name 检索，不使用 /videos/{id}。
        url.set_path("/agnesapi"); url.set_query(None);
        url.query_pairs_mut().append_pair("video_id", task_id).append_pair("model_name", &agent.model);
    } else {
        url.path_segments_mut().map_err(|_| "视频轮询地址无效")?.pop_if_empty().push(task_id);
    }
    Ok(url.to_string())
}
pub(super) fn video_task_id(agent: &AgentSessionDto, response: &JsonValue) -> Option<String> {
    if agnes_video_25(agent) {
        ["/video_id", "/data/video_id"].iter().find_map(|p| response.pointer(p)?.as_str().filter(|id| !id.is_empty()).map(str::to_owned))
    } else { extract_video_task_id(response) }
}
pub(super) async fn save_remote(url: &str, kind: &str) -> Result<ChatAttachmentDto, String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "生成产物地址无效")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("生成产物必须为 HTTP(S) 地址".into());
    }
    let max = if kind == "video" {
        256 * 1024 * 1024
    } else {
        32 * 1024 * 1024
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|_| "产物下载连接不可用")?;
    let mut response = client
        .get(parsed)
        .send()
        .await
        .map_err(|_| "产物下载失败；未将临时地址当作已保存结果")?;
    if !response.status().is_success() {
        return Err(format!("产物下载 HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|n| n > max) {
        return Err("生成产物超过下载上限（图像 32 MB、视频 256 MB）".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "产物下载中断")? {
        if bytes.len() + chunk.len() > max as usize {
            return Err("产物下载超过大小上限".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    save_bytes(&attachment_store_dir(), &bytes, kind)
}
pub(super) fn save_base64(data: &str) -> Result<ChatAttachmentDto, String> {
    use base64::Engine;
    if data.len() > 45 * 1024 * 1024 {
        return Err("生成图像超过 32 MB".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "生成图像编码无效")?;
    save_bytes(&attachment_store_dir(), &bytes, "image")
}
fn save_bytes(root: &Path, bytes: &[u8], kind: &str) -> Result<ChatAttachmentDto, String> {
    let (extension, mime) = if kind == "image" {
        match image::guess_format(bytes).map_err(|_| "产物不是支持的图片格式")? {
            image::ImageFormat::Png => ("png", "image/png"),
            image::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
            image::ImageFormat::WebP => ("webp", "image/webp"),
            _ => return Err("产物格式仅支持 PNG、JPEG、WebP".into()),
        }
    } else if bytes.get(4..8) == Some(b"ftyp") {
        ("mp4", "video/mp4")
    } else if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        ("webm", "video/webm")
    } else {
        return Err("产物不是 MP4 或 WebM 视频".into());
    };
    let blob = content_blob_store::put(root, bytes, extension)
        .map_err(|e| format!("产物保存失败：{e}"))?;
    Ok(ChatAttachmentDto {
        kind: kind.into(),
        name: format!("generated-{}.{}", &blob.sha256[..12], extension),
        url: attachment_file_url(&blob.file_name),
        mime_type: Some(mime.into()),
        text_snapshot: None,
    })
}
pub(super) fn record(
    agent: &AgentSessionDto,
    id: &str,
    response: &JsonValue,
    seconds: Option<u64>,
) {
    let result = (|| -> rusqlite::Result<()> {
        let c = open_session_connection(&default_session_sqlite_path())?;
        c.execute_batch("CREATE TABLE IF NOT EXISTS media_usage_events(id TEXT PRIMARY KEY,session_id TEXT NOT NULL,model TEXT NOT NULL,kind TEXT NOT NULL,created_at INTEGER NOT NULL,units INTEGER NOT NULL,duration_seconds INTEGER,cost REAL,usage_json TEXT NOT NULL);")?;
        let usage = response.get("usage").cloned().unwrap_or(JsonValue::Null);
        let count = response["data"].as_array().map_or(1, |v| v.len());
        c.execute(
            "INSERT OR IGNORE INTO media_usage_events VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                id,
                agent.id,
                agent.model,
                agent.model_type,
                unix_timestamp_millis(),
                count,
                seconds,
                usage["cost"].as_f64(),
                usage.to_string()
            ],
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        diag_log(&format!("[MEDIA] 用量保存失败：{error}"));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn 未返回能力目录的生成配置可保存并重新加载() {
        let config: Config = serde_json::from_value(json!({"parameters":{},"capabilities":null})).unwrap();
        let stored = toml::to_string(&config).unwrap();
        let restored: Config = toml::from_str(&stored).unwrap();
        assert!(restored.capabilities.is_null());
        assert!(restored.parameters.is_empty());
        let invalid: Config = serde_json::from_value(json!({"parameters":{"size":null}})).unwrap();
        assert!(validate(&invalid, &agent()).is_err());
    }
    fn agent() -> AgentSessionDto {
        AgentSessionDto {
            id: "media".into(),
            name: "Media".into(),
            display_name: "Media".into(),
            avatar: None,
            model: "custom-image".into(),
            model_type: "image".into(),
            provider: "custom".into(),
            base_url: Some("https://example.com/v1".into()),
            endpoint: None,
            reasoning_effort: "auto".into(),
            api_key_status: "未配置".into(),
            selectable: true,
            enabled: true,
            system: false,
            default_timeout_ms: 20000,
            memory_beads: vec![],
        }
    }
    #[test]
    fn routes_and_model_parameters_are_not_silently_mixed() {
        let mut agent = agent();
        assert_eq!(
            endpoint(&agent, &Default::default()).unwrap(),
            "https://example.com/v1/images/generations"
        );
        agent.provider = "openrouter".into();
        assert_eq!(
            endpoint(&agent, &Default::default()).unwrap(),
            "https://example.com/v1/images"
        );
        let mut config = Config::default();
        config.parameters.insert("n".into(), json!(11));
        assert!(validate(&config, &agent).is_err());
        config.parameters.clear();
        config.parameters.insert("duration".into(), json!(8));
        assert!(validate(&config, &agent).is_err());
        agent.model_type = "video".into();
        config.parameters.clear();
        let body = body(&agent, &config, "p", None).unwrap();
        assert!(body.get("width").is_none());
        config.parameters.insert("duration".into(), json!(8));
        assert!(validate(&config, &agent).is_err());
        config.capabilities = json!({"model":agent.model,"model_type":"video","base_url":agent.base_url,"fields":{"duration":{"values":[5,8]}}});
        assert!(validate(&config, &agent).is_ok());
        config.parameters.insert("duration".into(), json!(7));
        assert!(validate(&config, &agent).is_err());
    }
    #[test]
    fn 新版视频模型不混入旧版帧参数而显式配置仍可发送() {
        let mut agent = agent();
        agent.model = "agnes-video-2.5-flash".into();
        agent.model_type = "video".into();
        let mut config = Config::default();
        assert_eq!(body(&agent, &config, "p", None).unwrap(), json!({"model":agent.model,"prompt":"p","mode":"text"}));
        config.parameters.insert("duration".into(), json!(5));
        assert_eq!(body(&agent, &config, "p", None).unwrap()["seconds"], "5");
        config.parameters.insert("resolution".into(), json!("720P"));
        assert_eq!(body(&agent, &config, "p", None).unwrap()["size"], "720P");
        config.parameters.insert("width".into(), json!(1152));
        assert!(body(&agent, &config, "p", None).is_err());
        agent.model = "agnes-video-v2.0".into();
        config.parameters.insert("width".into(), json!(1152));
        config.parameters.insert("height".into(), json!(768));
        assert_eq!(body(&agent, &config, "p", None).unwrap()["width"], 1152);
    }
    #[test]
    fn 新版视频使用视频编号查询且不把任务编号混用() {
        let mut agent = agent(); agent.model_type = "video".into(); agent.model = "agnes-video-2.5-flash".into();
        let response = json!({"id":"task-1","task_id":"task-1","video_id":"video/a b"});
        assert_eq!(video_task_id(&agent, &response).as_deref(), Some("video/a b"));
        assert!(video_task_id(&agent, &json!({"id":"task-1"})).is_none());
        let url = reqwest::Url::parse(&video_poll_url(&agent,"https://apihub.agnes-ai.com/v1/videos","video/a b").unwrap()).unwrap();
        assert_eq!(url.path(),"/agnesapi");
        assert!(url.query_pairs().any(|(k,v)| k == "video_id" && v == "video/a b"));
        assert!(url.query_pairs().any(|(k,v)| k == "model_name" && v == agent.model));
        agent.model = "other-video".into();
        assert_eq!(video_task_id(&agent, &response).as_deref(),Some("task-1"));
        assert_eq!(video_poll_url(&agent,"https://example.com/v1/videos/","task-1").unwrap(),"https://example.com/v1/videos/task-1");
    }
    #[test]
    fn generated_assets_use_content_files_and_not_inline_base64() {
        let root = tempfile::tempdir().unwrap();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let asset = save_bytes(root.path(), bytes.get_ref(), "image").unwrap();
        assert!(asset.url.contains("sha256-"));
        assert!(!asset.url.contains("base64"));
        let name = asset.url.rsplit('/').next().unwrap();
        assert_eq!(
            std::fs::read(root.path().join(name)).unwrap(),
            *bytes.get_ref()
        );
        let same = save_bytes(root.path(), bytes.get_ref(), "image").unwrap();
        assert_eq!(same.url, asset.url);
        assert!(save_bytes(root.path(), b"<html>error</html>", "video").is_err());
    }
}
