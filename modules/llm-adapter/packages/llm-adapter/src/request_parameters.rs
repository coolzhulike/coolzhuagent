//! 显式会话参数独立于模型目录；未指定的参数保持原有请求行为。
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequestParameters {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    /// auto 沿用能力目录；其它值是用户明确选择的原生编码。
    pub reasoning_mode: Option<String>,
    pub thinking_budget: Option<u32>,
}

impl RequestParameters {
    /// 原生 Qwen3.8 的显式编码必须可表达，防止界面已保存但请求静默忽略。
    pub(crate) fn validate_for_model(&self, model: &str, effort: Option<&str>, anthropic: bool) -> Result<(), crate::ApiError> {
        if anthropic || !crate::reasoning::is_qwen38_reasoning_model(model) {
            return Ok(());
        }
        let parsed = crate::reasoning::parse_reasoning_effort(effort).map_err(|_| crate::ApiError::ConfigError {
            path: "reasoning_effort".to_string(),
            message: "Qwen3.8 思考层级应为 auto/none/minimal/low/medium/high/xhigh/max".to_string(),
        })?;
        if parsed == crate::ReasoningEffort::None {
            return Ok(());
        }
        match self.reasoning_mode.as_deref().unwrap_or("auto") {
            "auto" | "effort" | "thinking" => Ok(()),
            "budget" if self.thinking_budget.is_some_and(|budget| budget <= 262_144) => Ok(()),
            "budget" => Err(crate::ApiError::ConfigError {
                path: "thinking_budget".to_string(),
                message: "Qwen3.8 budget 模式需显式设置 0–262144 的 thinking_budget；不会同时发送 reasoning_effort".to_string(),
            }),
            _ => Err(crate::ApiError::ConfigError {
                path: "reasoning_mode".to_string(),
                message: "Qwen3.8 支持 auto/effort/thinking/budget；不支持 Anthropic adaptive 编码".to_string(),
            }),
        }
    }

    pub fn apply(&self, payload: &mut Value, effort: Option<&str>, anthropic: bool) {
        if let Some(value) = self.temperature {
            payload["temperature"] = json!(value);
        }
        if let Some(value) = self.top_p {
            payload["top_p"] = json!(value);
        }
        let mode = self.reasoning_mode.as_deref().unwrap_or("auto");
        let qwen38 = !anthropic && payload.get("model").and_then(Value::as_str)
            .is_some_and(crate::reasoning::is_qwen38_reasoning_model);
        if qwen38 && mode != "auto" {
            if let Some(object) = payload.as_object_mut() {
                for field in ["thinking", "thinking_budget", "reasoning_effort", "enable_thinking", "output_config"] {
                    object.remove(field);
                }
            }
            let resolution = crate::reasoning::resolve_reasoning("alibaba-bailian", "qwen3.8-flash", effort);
            if let Ok(resolution) = resolution {
                if resolution.requested == crate::ReasoningEffort::None {
                    // 关闭优先于预算与模式，避免旧配置中的 budget 重新开启思考。
                    payload["enable_thinking"] = json!(false);
                } else if mode == "budget" {
                    if let Some(budget) = self.thinking_budget {
                        payload["enable_thinking"] = json!(true);
                        payload["thinking_budget"] = json!(budget);
                    }
                } else {
                    resolution.preflight_wire.apply_to_payload(payload);
                    if mode == "thinking" && resolution.requested == crate::ReasoningEffort::Auto {
                        payload["enable_thinking"] = json!(true);
                    }
                }
            }
            return;
        }
        if mode == "auto" {
            return;
        }
        // 显式参数优先，移除目录推断的旧编码，避免同时发两种思考协议。
        if let Some(object) = payload.as_object_mut() {
            object.remove("reasoning_effort");
            object.remove("thinking");
            object.remove("output_config");
        }
        let effort = effort.unwrap_or("auto");
        match (anthropic, mode) {
            (false, "effort") if effort != "auto" => {
                payload["reasoning_effort"] = json!(effort);
            }
            (false, "thinking") if effort != "auto" => {
                payload["thinking"] = json!({"type": if effort == "none" { "disabled" } else { "enabled" }});
            }
            (true, "budget") => {
                if effort == "none" {
                    payload["thinking"] = json!({"type": "disabled"});
                } else if let Some(budget) = self.thinking_budget {
                    payload["thinking"] = json!({"type": "enabled", "budget_tokens": budget});
                }
            }
            (true, "adaptive") if effort != "auto" => {
                if effort == "none" {
                    payload["thinking"] = json!({"type": "disabled"});
                } else {
                    payload["thinking"] = json!({"type": "adaptive"});
                    payload["output_config"] = json!({"effort": effort});
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_parameters_support_unregistered_models_and_keep_wire_separate() {
        let settings = RequestParameters {
            temperature: Some(0.4), top_p: Some(0.8),
            reasoning_mode: Some("effort".into()), thinking_budget: None,
        };
        let mut payload = json!({"model":"future-model","thinking":{"type":"enabled"}});
        settings.apply(&mut payload, Some("xhigh"), false);
        assert_eq!(payload["reasoning_effort"], "xhigh");
        assert_eq!(payload["temperature"], 0.4);
        assert_eq!(payload["top_p"], 0.8);
        assert!(payload.get("thinking").is_none());
    }

    #[test]
    fn anthropic_budget_uses_native_thinking_field() {
        let settings = RequestParameters {
            reasoning_mode: Some("budget".into()), thinking_budget: Some(4096),
            ..Default::default()
        };
        let mut payload = json!({"reasoning_effort":"high","output_config":{"effort":"high"}});
        settings.apply(&mut payload, Some("high"), true);
        assert_eq!(payload["thinking"]["budget_tokens"], 4096);
        assert!(payload.get("reasoning_effort").is_none());
        assert!(payload.get("output_config").is_none());
    }

    #[test]
    fn unspecified_parameters_preserve_legacy_payload() {
        let original = json!({"thinking":{"type":"adaptive"},"output_config":{"effort":"high"}});
        let mut payload = original.clone();
        RequestParameters::default().apply(&mut payload, Some("high"), true);
        assert_eq!(payload, original);
    }

    fn qwen_payload(model: &str, effort: &str) -> Value {
        crate::build_chat_completion_request_for("alibaba-bailian", &crate::MessageRequest {
            model: model.to_string(), max_tokens: 32768,
            messages: vec![crate::InputMessage::user_text("协议验收")],
            system: None, tools: None, tool_choice: None,
            reasoning_effort: Some(effort.to_string()), stream: false,
        })
    }

    #[test]
    fn qwen38_default_mode_maps_every_effort_to_native_wire() {
        for (requested, expected) in [
            ("auto", None), ("none", None), ("minimal", Some("low")),
            ("low", Some("low")), ("medium", Some("medium")),
            ("high", Some("xhigh")), ("xhigh", Some("xhigh")), ("max", Some("xhigh")),
        ] {
            let payload = qwen_payload("qwen3.8-flash", requested);
            assert_eq!(payload.get("reasoning_effort").and_then(Value::as_str), expected, "{requested}");
            assert_eq!(payload.get("enable_thinking").and_then(Value::as_bool),
                if requested == "auto" { None } else { Some(requested != "none") }, "{requested}");
            assert!(payload.get("thinking").is_none());
            assert!(payload.get("thinking_budget").is_none());
            assert_eq!(payload["model"], "qwen3.8-flash");
        }
    }

    #[test]
    fn qwen38_explicit_effort_and_thinking_modes_use_native_fields() {
        for mode in ["effort", "thinking"] {
            for (requested, expected) in [("minimal", "low"), ("low", "low"),
                ("medium", "medium"), ("high", "xhigh"), ("xhigh", "xhigh"), ("max", "xhigh")] {
                let settings = RequestParameters { reasoning_mode: Some(mode.into()),
                    thinking_budget: Some(8192), ..Default::default() };
                settings.validate_for_model("qwen3.8-flash", Some(requested), false).expect("有效参数");
                let mut payload = qwen_payload("qwen3.8-flash", requested);
                settings.apply(&mut payload, Some(requested), false);
                assert_eq!(payload["enable_thinking"], true);
                assert_eq!(payload["reasoning_effort"], expected);
                assert!(payload.get("thinking_budget").is_none(), "不能带上旧预算");
                assert!(payload.get("thinking").is_none());
            }
            let settings = RequestParameters { reasoning_mode: Some(mode.into()), ..Default::default() };
            let mut payload = qwen_payload("qwen3.8-flash", "auto");
            settings.apply(&mut payload, Some("auto"), false);
            assert_eq!(payload.get("enable_thinking").and_then(Value::as_bool),
                if mode == "thinking" { Some(true) } else { None });
            assert!(payload.get("reasoning_effort").is_none());
        }
    }

    #[test]
    fn qwen38_budget_is_exclusive_and_none_always_disables_thinking() {
        for budget in [0, 4096, 262_144] {
            let settings = RequestParameters { reasoning_mode: Some("budget".into()),
                thinking_budget: Some(budget), ..Default::default() };
            settings.validate_for_model("qwen3.8-flash", Some("high"), false).expect("有效预算");
            let mut payload = qwen_payload("qwen3.8-flash", "high");
            settings.apply(&mut payload, Some("high"), false);
            assert_eq!(payload["enable_thinking"], true);
            assert_eq!(payload["thinking_budget"], budget);
            assert!(payload.get("reasoning_effort").is_none());
            assert!(payload.get("thinking").is_none());
        }
        for mode in ["auto", "effort", "thinking", "budget", "adaptive"] {
            let settings = RequestParameters { reasoning_mode: Some(mode.into()),
                thinking_budget: Some(999_999), ..Default::default() };
            settings.validate_for_model("qwen3.8-flash", Some("none"), false).expect("关闭优先");
            let mut payload = qwen_payload("qwen3.8-flash", "none");
            settings.apply(&mut payload, Some("none"), false);
            assert_eq!(payload["enable_thinking"], false, "{mode}");
            assert!(payload.get("reasoning_effort").is_none());
            assert!(payload.get("thinking_budget").is_none());
            assert!(payload.get("thinking").is_none());
        }
    }

    #[test]
    fn qwen38_invalid_modes_and_budgets_are_explainable_configuration_errors() {
        for (mode, budget) in [("budget", None), ("budget", Some(262_145)), ("adaptive", None)] {
            let settings = RequestParameters { reasoning_mode: Some(mode.into()),
                thinking_budget: budget, ..Default::default() };
            let error = settings.validate_for_model("qwen3.8-flash", Some("medium"), false)
                .expect_err("不能静默忽略无效参数");
            assert!(matches!(error, crate::ApiError::ConfigError { .. }));
            assert!(!error.is_retryable());
        }
        assert!(RequestParameters::default().validate_for_model("qwen3.8-flash", Some("unknown"), false).is_err());
    }

    #[test]
    fn qwen38_mapping_does_not_change_other_models_or_anthropic_encoding() {
        for model in ["qwen3.7-max", "qwen3.8-omni-flash", "qwen3.8-flash-unknown", "future-model"] {
            let mut payload = qwen_payload(model, "high");
            assert!(payload.get("enable_thinking").is_none(), "{model}");
            let settings = RequestParameters { reasoning_mode: Some("effort".into()), ..Default::default() };
            settings.apply(&mut payload, Some("high"), false);
            assert_eq!(payload["reasoning_effort"], "high");
            assert!(payload.get("enable_thinking").is_none());
            assert_eq!(payload["model"], model);
        }
        let mut payload = json!({"model":"qwen3.8-flash"});
        let settings = RequestParameters { reasoning_mode: Some("budget".into()),
            thinking_budget: Some(2048), ..Default::default() };
        settings.apply(&mut payload, Some("high"), true);
        assert_eq!(payload["thinking"]["budget_tokens"], 2048);
        assert!(payload.get("enable_thinking").is_none());
    }
}
