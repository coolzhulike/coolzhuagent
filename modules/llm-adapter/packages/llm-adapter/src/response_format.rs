//! 单次请求的输出协议；不改变会话采样、思考、连接或工具权限。
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum ResponseFormat {
    JsonObject,
    JsonSchema { name: String, schema: Value },
}

impl ResponseFormat {
    /// 官方 Qwen3.8 结构化输出能力；多模态输入只使用 JSON Object。
    pub fn for_qwen38(model: &str, schema: Option<Value>, has_images: bool) -> Option<Self> {
        if !crate::reasoning::is_qwen38_reasoning_model(model) { return None; }
        Some(match schema.filter(|_| !has_images) {
            Some(schema) => Self::JsonSchema { name: "computer_use_verdict".into(), schema },
            None => Self::JsonObject,
        })
    }

    pub(crate) fn apply(&self, payload: &mut Value) {
        payload["response_format"] = match self {
            Self::JsonObject => json!({"type":"json_object"}),
            Self::JsonSchema {name,schema} => json!({"type":"json_schema","json_schema":{"name":name,"strict":true,"schema":schema}}),
        };
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn multimodal_schema_is_not_sent_and_reasoning_is_preserved() {
        use super::*;
        let schema=json!({"type":"object","properties":{"met":{"type":"boolean"}},"required":["met"],"additionalProperties":false});
        let mut payload=json!({"model":"qwen3.8-flash","enable_thinking":true,"reasoning_effort":"medium"});
        ResponseFormat::for_qwen38("qwen3.8-flash",Some(schema.clone()),false).unwrap().apply(&mut payload);
        assert_eq!(payload["response_format"]["json_schema"]["schema"],schema);
        assert_eq!(payload["reasoning_effort"],"medium");
        assert_eq!(payload["enable_thinking"],true);
        ResponseFormat::for_qwen38("qwen3.8-flash",Some(schema),true).unwrap().apply(&mut payload);
        assert_eq!(payload["response_format"],json!({"type":"json_object"}));
        assert!(ResponseFormat::for_qwen38("unregistered-model",None,false).is_none());
    }
}
