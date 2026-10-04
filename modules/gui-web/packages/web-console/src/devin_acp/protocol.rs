//! ACP v1 数据校验。没有协商到的能力保持未知，不用模型名称补齐。
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub(super) const PROTOCOL_VERSION: u64 = 1;

pub(super) fn initialize_params() -> Value {
    json!({"protocolVersion": PROTOCOL_VERSION,
        "clientCapabilities": {"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false},
        "clientInfo": {"name": "coolzhu-agent", "version": env!("CARGO_PKG_VERSION")}})
}

pub(super) fn validate_initialize(result: &Value) -> Result<(), &'static str> {
    if result.get("protocolVersion").and_then(Value::as_u64) != Some(PROTOCOL_VERSION) {
        return Err("Devin 返回的 ACP 协议版本不兼容。");
    }
    if !result
        .get("agentCapabilities")
        .is_some_and(Value::is_object)
    {
        return Err("ACP 初始化缺少能力声明。");
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ExecutionScope {
    pub workspace_id: String,
    pub room_id: String,
    pub agent_id: String,
    pub run_id: String,
    pub turn_id: String,
    pub attempt_id: String,
    pub owner_epoch: u64,
    pub generation: u64,
}

impl ExecutionScope {
    pub(super) fn accepts(&self, incoming: &Self) -> bool {
        self == incoming
            && self.owner_epoch > 0
            && self.generation > 0
            && [
                &self.workspace_id,
                &self.room_id,
                &self.agent_id,
                &self.run_id,
                &self.turn_id,
                &self.attempt_id,
            ]
            .iter()
            .all(|value| !value.trim().is_empty() && value.len()<=4096 && !value.chars().any(char::is_control))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct ModelSelection {
    pub requested: String,
    pub effective: Option<String>,
    // ACP 选项不保证暴露路由后真正底模；无法确认时保持空。
    pub resolved_model: Option<String>,
}

fn option_has_value(options: &[Value], selected: &str) -> bool {
    options.iter().any(|option| {
        option.get("value").and_then(Value::as_str) == Some(selected)
            || option
                .get("options")
                .and_then(Value::as_array)
                .is_some_and(|group| option_has_value(group, selected))
    })
}

/// 只使用 category=model 的真实选项，不能把 mode 或 reasoning 当模型。
pub(super) fn model_config(options: &Value) -> Result<(&str, &str, &[Value]), &'static str> {
    let options = options.as_array().ok_or("ACP 配置目录格式无效。")?;
    let models: Vec<_> = options
        .iter()
        .filter(|option| option.get("category").and_then(Value::as_str) == Some("model"))
        .collect();
    if models.len() != 1 {
        return Err("ACP 未提供唯一的模型配置选项，无法确认生效模型。");
    }
    let model = models[0];
    if model.get("type").and_then(Value::as_str) != Some("select") {
        return Err("ACP 模型配置类型尚不支持。");
    }
    let id = model
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("ACP 模型配置缺少 ID。")?;
    let current = model
        .get("currentValue")
        .and_then(Value::as_str)
        .ok_or("ACP 模型配置缺少生效值。")?;
    let choices = model
        .get("options")
        .and_then(Value::as_array)
        .ok_or("ACP 模型配置缺少选项。")?;
    if !option_has_value(choices, current) {
        return Err("ACP 生效模型不在返回的选项中。");
    }
    Ok((id, current, choices))
}

pub(super) fn set_model_params(
    session_id: &str,
    requested: &str,
    options: &Value,
) -> Result<Value, &'static str> {
    let (id, _, choices) = model_config(options)?;
    if !option_has_value(choices, requested) {
        return Err("请求的模型不在当前 ACP 会话的可用选项中。");
    }
    Ok(json!({"sessionId": session_id, "configId": id, "value": requested}))
}

pub(super) fn confirmed_model(
    requested: &str,
    result: &Value,
) -> Result<ModelSelection, &'static str> {
    let (_, effective, _) = model_config(
        result
            .get("configOptions")
            .ok_or("ACP 换模响应缺少完整配置。")?,
    )?;
    if effective != requested {
        return Err("ACP 返回的生效模型与请求值不一致，换模未确认。");
    }
    Ok(ModelSelection {
        requested: requested.into(),
        effective: Some(effective.into()),
        resolved_model: None,
    })
}

pub(super) fn denied_client_request(request: &Value) -> Result<Value, &'static str> {
    let id = request
        .get("id")
        .filter(|id| id.is_u64() || id.is_i64() || id.is_string())
        .ok_or("ACP 请求 ID 无效。")?;
    if request.get("method").and_then(Value::as_str) == Some("session/request_permission") {
        // 未接入真实权限与执行台账前只返回 cancelled，绝不自动选择 allow。
        Ok(json!({"jsonrpc": "2.0", "id": id, "result": {"outcome": {"outcome": "cancelled"}}}))
    } else {
        Ok(json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601,
            "message": "本客户端未开放文件、终端或工具执行能力"}}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options() -> Value {
        json!([
            {"id":"mode", "category":"mode", "type":"select", "currentValue":"code", "options":[{"value":"code"}]},
            {"id":"route-model", "category":"model", "type":"select", "currentValue":"alias-a",
             "options":[{"name":"分组", "options":[{"value":"alias-a"},{"value":"alias-b"}]}]}
        ])
    }
    #[test]
    fn model_is_negotiated_and_only_confirmed_by_full_response() {
        assert_eq!(
            set_model_params("s1", "alias-b", &options()).unwrap()["configId"],
            "route-model"
        );
        assert!(set_model_params("s1", "code", &options()).is_err());
        assert!(confirmed_model("alias-b", &json!({"configOptions": options()})).is_err());
        let confirmed = confirmed_model("alias-a", &json!({"configOptions": options()})).unwrap();
        assert_eq!(confirmed.effective.as_deref(), Some("alias-a"));
        assert!(confirmed.resolved_model.is_none());
        assert!(confirmed_model("alias-a", &json!({})).is_err());
    }
    #[test]
    fn initialization_never_advertises_unimplemented_tools() {
        assert_eq!(initialize_params()["clientCapabilities"]["terminal"], false);
        assert!(validate_initialize(&json!({"protocolVersion":2,"agentCapabilities":{}})).is_err());
        assert!(validate_initialize(&json!({"protocolVersion":1,"agentCapabilities":{}})).is_ok());
    }
    #[test]
    fn permission_is_cancelled_and_files_and_terminal_are_denied() {
        assert_eq!(
            denied_client_request(&json!({"id":8,"method":"session/request_permission"})).unwrap()
                ["result"]["outcome"]["outcome"],
            "cancelled"
        );
        for method in ["fs/read_text_file", "fs/write_text_file", "terminal/create"] {
            assert_eq!(
                denied_client_request(&json!({"id":"server-1","method":method})).unwrap()["error"]
                    ["code"],
                -32601
            );
        }
    }
    #[test]
    fn stale_owner_attempt_and_cross_room_updates_are_rejected() {
        let scope = ExecutionScope {
            workspace_id: "w".into(),
            room_id: "r".into(),
            agent_id: "a".into(),
            run_id: "run".into(),
            turn_id: "turn".into(),
            attempt_id: "attempt-1".into(),
            owner_epoch: 1,
            generation: 2,
        };
        assert!(scope.accepts(&scope));
        for changed in [
            ExecutionScope {
                room_id: "other".into(),
                ..scope.clone()
            },
            ExecutionScope {
                attempt_id: "attempt-2".into(),
                ..scope.clone()
            },
            ExecutionScope {
                owner_epoch: 2,
                ..scope.clone()
            },
            ExecutionScope {
                generation: 1,
                ..scope.clone()
            },
        ] {
            assert!(!scope.accepts(&changed));
        }
    }
}
