//! ACP 图片编码边界；聊天附件与同会话 CU 原图复用相同协议块。
use api::InputContentBlock;
use serde_json::{json, Value};

pub(super) fn block(url: &str) -> Result<Value, String> {
    let (mime, data) = url.strip_prefix("data:")
        .and_then(|value| value.split_once(";base64,"))
        .filter(|(mime, data)| matches!(*mime, "image/png" | "image/jpeg" | "image/webp") && !data.is_empty())
        .ok_or("Devin 图片只支持受控附件的 PNG、JPEG 或 WebP 数据；未发送。")?;
    Ok(json!({"type":"image", "mimeType":mime, "data":data}))
}

/// 原图通过协议块传递，正文仅保留顺序标记，避免 base64 混入文本上下文。
pub(super) fn split(message: &api::InputMessage) -> Result<(api::InputMessage, Vec<Value>), String> {
    let mut projected = message.clone();
    let mut images = Vec::new();
    for content in &mut projected.content {
        if let InputContentBlock::ImageUrl { url, .. } = content {
            images.push(block(url)?);
            *content = InputContentBlock::Text { text: format!("[本轮附件图片 {}，原图随本次协议消息提供]", images.len()) };
        }
    }
    Ok((projected, images))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images_leave_text_context_and_keep_protocol_order() {
        let message: api::InputMessage = serde_json::from_value(json!({"role":"user", "content":[
            {"type":"text", "text":"只描述附件"},
            {"type":"image_url", "url":"data:image/png;base64,YQ=="},
            {"type":"image_url", "url":"data:image/jpeg;base64,Yg=="}
        ]})).unwrap();
        let (text, images) = split(&message).unwrap();
        let encoded = serde_json::to_string(&text).unwrap();
        assert!(!encoded.contains("base64") && !encoded.contains("YQ=="));
        assert!(encoded.contains("本轮附件图片 1") && encoded.contains("本轮附件图片 2"));
        assert_eq!(images[0], json!({"type":"image", "mimeType":"image/png", "data":"YQ=="}));
        assert_eq!(images[1]["data"], "Yg==");
    }

    #[test]
    fn unsupported_sources_are_explicitly_rejected() {
        for url in ["https://example.com/image.png", "data:image/gif;base64,YQ==", "data:image/png;base64,"] {
            assert!(block(url).is_err());
        }
    }
}
