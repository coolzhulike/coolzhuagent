//! 只实现官方 v3 HTTP 协议，不处理工程、权限或本地会话状态。
use serde_json::{json, Value};
use std::time::Duration;

pub(super) const BASE_URL: &str = "https://api.devin.ai/v3";
const RESPONSE_LIMIT: usize = 1024 * 1024;

pub(super) fn valid_id(value: &str, prefix: &str) -> bool {
    value.starts_with(prefix)
        && value.len() > prefix.len()
        && value.len() <= 180
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(super) fn valid_mode(value: &str) -> bool {
    matches!(value, "" | "normal" | "fast" | "lite" | "ultra" | "fusion")
}

pub(super) fn create_body(prompt: &str, mode: &str) -> Value {
    let mut body = json!({"prompt": prompt});
    // 空值表示沿用服务端默认；mode 不是底层模型或思考程度。
    if !mode.is_empty() {
        body["devin_mode"] = json!(mode);
    }
    body
}

pub(super) async fn request(
    key: &str,
    org: &str,
    session: &str,
    action: &str,
    text: &str,
    mode: &str,
    after: &str,
) -> Result<Value, String> {
    if !valid_id(org, "org-") || (!session.is_empty() && !valid_id(session, "devin-")) {
        return Err("Devin 组织或会话 ID 格式无效".into());
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(25))
        .connect_timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .https_only(true)
        .user_agent("coolzhu-agent/Devin-cloud-plugin")
        .build()
        .map_err(|_| "Devin 连接初始化失败")?;
    let root = format!("{BASE_URL}/organizations/{org}/sessions");
    let builder = match action {
        "create" => client.post(&root).json(&create_body(text, mode)),
        "send" => client
            .post(format!("{root}/{session}/messages"))
            .json(&json!({"message":text})),
        "get" | "bind" => client.get(format!("{root}/{session}")),
        "messages" => {
            let builder = client
                .get(format!("{root}/{session}/messages"))
                .query(&[("first", "100")]);
            if after.is_empty() {
                builder
            } else {
                builder.query(&[("after", after)])
            }
        }
        _ => return Err("Devin 操作无效".into()),
    };
    let mut response = builder
        .bearer_auth(key)
        .send()
        .await
        .map_err(|_| "Devin 连接中断或超时；写操作结果可能未知，请先到官网核对")?;
    let status = response.status();
    // 不回显远端错误正文，避免服务端回显 token 或敏感请求。
    if !status.is_success() {
        return Err(format!(
            "Devin 返回 HTTP {}，请核对密钥、组织权限与账户状态",
            status.as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|len| len > RESPONSE_LIMIT as u64)
    {
        return Err("Devin 响应超过 1 MiB，写操作结果可能未知".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Devin 响应读取中断，结果可能未知")?
    {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_LIMIT {
            return Err("Devin 响应超过 1 MiB，结果可能未知".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Devin 响应不是合法 JSON，写操作结果可能未知".into())
}

pub(super) fn session_fact(value: &Value, org: &str, expected: &str) -> Result<Value, String> {
    let id = value["session_id"]
        .as_str()
        .ok_or("Devin 响应缺少会话 ID，结果可能未知")?;
    if !valid_id(id, "devin-")
        || value["org_id"].as_str() != Some(org)
        || (!expected.is_empty() && expected != id)
    {
        return Err("Devin 响应的组织或会话身份不匹配，未登记".into());
    }
    // 保留官方状态；HTTP 请求成功不表示远端任务完成。
    Ok(
        json!({"session_id":id,"url":format!("https://app.devin.ai/sessions/{id}"),
        "status":value["status"],"status_detail":value["status_detail"],
        "title":value["title"],"devin_mode":value["devin_mode"],
        "acus_consumed":value["acus_consumed"],"created_at":value["created_at"],
        "updated_at":value["updated_at"],"structured_output":value["structured_output"],
        "pull_requests":value["pull_requests"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_and_identity_are_not_model_parameters() {
        assert_eq!(create_body("你好", ""), json!({"prompt":"你好"}));
        assert_eq!(
            create_body("你好", "ultra"),
            json!({"prompt":"你好","devin_mode":"ultra"})
        );
        assert!(!valid_id("org-x/../other", "org-"));
        assert!(!valid_mode("gpt-6"));
        let fact = session_fact(&json!({"session_id":"devin-a","org_id":"org-a","status":"running","status_detail":"working"}), "org-a", "devin-a").unwrap();
        assert_eq!(fact["status"], "running");
        assert!(session_fact(
            &json!({"session_id":"devin-b","org_id":"org-b"}),
            "org-a",
            ""
        )
        .is_err());
    }
}
