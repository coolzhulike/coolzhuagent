//! 微信图片传输接入统一附件库；CDN/AES 只在 provider 层，模型路由复用普通聊天。
use super::*;
use base64::Engine;
use sha2::Digest;

pub(super) const MAX_INBOUND_BODY_BYTES: usize = 46 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 32 * 1024 * 1024;

/// 在 JSON body 提取之前限并发，避免多份合法大消息叠加耗尽内存。
pub(super) async fn limit_inflight(request: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let Ok(_slot) = SLOTS.try_acquire() else {
        return (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error":"微信附件处理中，请稍后重试"}))).into_response();
    };
    next.run(request).await
}

/// Inbox 留内容摘要而非重复存整张 base64；重复消息仍按实际内容摘要检测冲突。
pub(super) fn inbox_payload(envelope: &clawbot_channel::ClawbotInboundEnvelope) -> ApiResult<String> {
    if envelope.message.media_refs.len() > 8 { return Err(api_error(StatusCode::BAD_REQUEST, "每条微信消息最多接收 8 个媒体附件")); }
    let mut clean = envelope.clone();
    // 接收时间是本地投递元数据；重连重新拉取时不得把同一上游消息误判成内容冲突。
    clean.message.received_at_ms = 0;
    let mut digests = Vec::new();
    let mut total = 0usize;
    for media in &mut clean.message.media_refs {
        let digest = if let Some(encoded) = media.content_base64.take() {
            total = total.saturating_add(encoded.len());
            if encoded.len() > (MAX_IMAGE_BYTES + 2) / 3 * 4 || total > (MAX_TOTAL_BYTES + 2) / 3 * 4 {
                return Err(api_error(StatusCode::PAYLOAD_TOO_LARGE, "微信图片超过单张 20 MiB 或总计 32 MiB 限制"));
            }
            Some(format!("{:x}", sha2::Sha256::digest(encoded.as_bytes())))
        } else { None };
        digests.push(digest);
    }
    let mut value = serde_json::to_value(clean).map_err(|e| api_error(StatusCode::BAD_REQUEST, &e.to_string()))?;
    if let Some(media) = value["message"]["media_refs"].as_array_mut() {
        for (item, digest) in media.iter_mut().zip(digests) {
            if let Some(digest) = digest { item["encoded_content_sha256"] = json!(digest); }
        }
    }
    serde_json::to_string(&value).map_err(|e| api_error(StatusCode::BAD_REQUEST, &e.to_string()))
}

pub(super) fn attachments(message: &clawbot_channel::ClawbotInboundMessage) -> ApiResult<Vec<ChatAttachmentDto>> {
    materialize(message, &attachment_store_dir(), attachment_upload_limit_bytes())
}

fn image_type(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") { Some(("image/png", "png")) }
    else if bytes.starts_with(b"\xff\xd8\xff") { Some(("image/jpeg", "jpg")) }
    else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") { Some(("image/gif", "gif")) }
    else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") { Some(("image/webp", "webp")) }
    else { None }
}

/// 不以签名头代替图片有效性：先限制维度，再完成受限解码，原始附件保持不变。
fn validate_image(bytes: &[u8]) -> Result<(&'static str, &'static str), &'static str> {
    let kind = image_type(bytes).ok_or("微信附件不是 PNG、JPEG、GIF 或 WebP 图片")?;
    let reader = || -> Result<image::ImageReader<std::io::Cursor<&[u8]>>, &'static str> {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()
            .map_err(|_| "无法识别微信图片格式")?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16384);
        limits.max_image_height = Some(16384);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        Ok(reader)
    };
    let (width, height) = reader()?.into_dimensions().map_err(|_| "微信图片头无效或维度超过限制")?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err("微信图片像素总数超过 32 Mi 限制");
    }
    // decode 校验压缩流/截断等错误；图片释放后才处理下一张，分配不会按附件数累加。
    let decoded = reader()?.decode().map_err(|_| "微信图片内容损坏或解码资源超过限制")?;
    if decoded.width() != width || decoded.height() != height { return Err("微信图片尺寸不一致"); }
    Ok(kind)
}

fn materialize(message: &clawbot_channel::ClawbotInboundMessage, root: &Path, upload_limit: usize) -> ApiResult<Vec<ChatAttachmentDto>> {
    let fail = |reason: &str| api_error(StatusCode::BAD_REQUEST, reason);
    if message.media_refs.len() > 8 { return Err(fail("每条微信消息最多接收 8 张图片")); }
    if message.kind == clawbot_channel::ClawbotMessageKind::Image && message.media_refs.is_empty() {
        return Err(fail("微信图片缺少真实内容；未调用模型识图"));
    }
    let mut decoded = Vec::new(); let mut total = 0usize;
    for media in &message.media_refs {
        if media.error.is_some() { return Err(fail("微信图片下载或解密失败；未把失败图片当作纯文本交给模型")); }
        let encoded = media.content_base64.as_deref().ok_or_else(|| fail("微信附件仅有描述信息，缺少实际文件内容"))?;
        if encoded.len() > (MAX_IMAGE_BYTES + 2) / 3 * 4 { return Err(fail("微信图片超过大小限制")); }
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| fail("微信图片编码无效"))?;
        total = total.saturating_add(bytes.len());
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES.min(upload_limit) || total > MAX_TOTAL_BYTES { return Err(fail("微信图片超过单张或总计大小限制")); }
        let digest = format!("{:x}", sha2::Sha256::digest(&bytes));
        if media.media_id != format!("sha256:{digest}") || media.size_bytes != Some(bytes.len() as u64) {
            return Err(fail("微信图片实际内容与传输摘要不符"));
        }
        let (mime, extension) = validate_image(&bytes).map_err(fail)?;
        decoded.push((bytes, digest, mime, extension));
    }
    if decoded.is_empty() { return Ok(Vec::new()); }
    let mut attachments = Vec::new();
    for (index, (bytes, digest, mime, extension)) in decoded.into_iter().enumerate() {
        let blob = content_blob_store::put(root, &bytes, extension)
            .map_err(|_| fail("微信图片内容发布或已有摘要核验失败"))?;
        let mut response = uploaded_attachment_response(&format!("att-wx-{digest}"), &format!("wechat-image-{}.{}", index + 1, extension), "image", Some(mime.into()), bytes.len() as u64);
        response.file_name = blob.file_name;
        response.attachment.url = attachment_file_url(&response.file_name);
        attachments.push(response.attachment);
    }
    Ok(attachments)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_picture_becomes_shared_attachment_and_inbox_never_contains_base64() {
        let mut bytes = Vec::new();
        image::ImageEncoder::write_image(image::codecs::png::PngEncoder::new(&mut bytes), &[20, 120, 80], 1, 1, image::ExtendedColorType::Rgb8).unwrap();
        let encoded = base64_encode(&bytes);
        let envelope: clawbot_channel::ClawbotInboundEnvelope = serde_json::from_value(json!({
            "source":"weixin_user","hop_count":0,"message":{
                "account_id":"wx-test","peer_id":"peer","peer_name":null,"context_token":null,
                "external_msg_id":"image-1","kind":"image","text":null,"received_at_ms":1,
                "media_refs":[{"media_id":format!("sha256:{:x}",sha2::Sha256::digest(&bytes)),"size_bytes":bytes.len(),"file_name":"../bad.png","mime_type":"text/plain","content_base64":encoded}]
            }
        })).unwrap();
        let payload = inbox_payload(&envelope).unwrap();
        assert!(!payload.contains(&encoded)); assert!(payload.contains("encoded_content_sha256"));
        let temp = tempfile::tempdir().unwrap();
        let attachments = materialize(&envelope.message, temp.path(), MAX_IMAGE_BYTES).unwrap();
        assert_eq!(attachments[0].mime_type.as_deref(), Some("image/png"));
        let images = multimodal_input::encode_images_from_store(&attachments, temp.path()).unwrap();
        assert_eq!(images, vec![format!("data:image/png;base64,{encoded}")]);
        assert_eq!(materialize(&envelope.message, temp.path(), MAX_IMAGE_BYTES).unwrap()[0].url, attachments[0].url);
        let mut corrupted = envelope.message.clone(); corrupted.media_refs[0].media_id = "sha256:wrong".into();
        assert!(materialize(&corrupted, temp.path(), MAX_IMAGE_BYTES).is_err());
        assert!(validate_image(b"\x89PNG\r\n\x1a\n").is_err());
        let mut truncated = bytes.clone(); truncated.truncate(bytes.len() / 2);
        assert!(validate_image(&truncated).is_err());
        let mut oversized = Vec::new();
        image::ImageEncoder::write_image(image::codecs::png::PngEncoder::new(&mut oversized), &vec![0; 16385 * 3], 16385, 1, image::ExtendedColorType::Rgb8).unwrap();
        assert!(validate_image(&oversized).is_err());
    }
}
