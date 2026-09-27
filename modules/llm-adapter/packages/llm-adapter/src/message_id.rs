//! **顶层 provider 消息 ID 的归一化**（裁决 COMPAT-ID）。
//!
//! 背景：Anthropic 官方消息类型把顶层 `id` 定义为字符串型唯一对象标识，流式 `message_start`
//! 同样携带该对象；而**第三方兼容端点**可能不返回它（实测某代理的 `/v1/messages` 缺 `id`，
//! 严格解析器报 `missing field id`）。裁决批准**有边界兼容**，但**不接受**"给 String 加默认值"
//! 了事——缺失必须**显式表示为"未提供"**，不能冒充有效身份。
//!
//! 归一化表（裁决 §2.2 逐字对应）：
//!
//! | 输入 | 严格模式 | 兼容模式 |
//! | --- | --- | --- |
//! | 正常非空字符串 | 原值保留 | 原值保留 |
//! | 顶层缺失 | 明确协议错误 | `None` + 兼容告警 |
//! | 顶层为 `null` | 明确协议错误 | `None` + 兼容告警（形态如实记录） |
//! | 空串／全空白 | **拒绝** | **仍拒绝**（不把异常字符串当合法 ID） |
//! | 数字／对象等错误类型 | 拒绝 | 仍拒绝（不自动转字符串） |
//!
//! 另外两条边界（同样来自裁决）：
//! - **只**针对这一处差异：本模块不处理 `tool_use.id`／`tool_result.tool_use_id` 的配对，
//!   那些仍然严格（配对错误 ⇒ 不执行工具）；
//! - 缺失**不触发重发**：响应已经到达、模型可能已计费，兼容应处理收到的响应，而不是再请求一次。

use crate::error::ApiError;

/// 顶层 `id` 缺失／异常的**形态**（用于把"缺成什么样"如实写进诊断，而不是一律说"缺"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingMessageIdShape {
    /// 字段不存在。
    Absent,
    /// 字段存在但为 `null`。
    Null,
}

impl MissingMessageIdShape {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Null => "null",
        }
    }
}

/// 兼容能力位对应的诊断口径（写进日志/请求快照的一句话）。
pub const ALLOW_MISSING_MESSAGE_ID_RULE: &str = "allow_missing_top_level_message_id";

/// 归一化结果：`Ok(Some(id))` = 有有效身份；`Ok(None)` = **未提供**（仅在允许缺失时）。
pub type NormalizedMessageId = Option<String>;

/// **唯一**的顶层 message ID 归一化实现点。
///
/// `allow_missing` 是**连接级**兼容能力位（默认 `false` ⇒ 严格）：官方直连与未启用该能力的连接
/// 保持严格检查；只有显式启用的第三方兼容连接才接受"缺失"。
pub fn normalize_top_level_message_id(
    raw: Option<&serde_json::Value>,
    allow_missing: bool,
) -> Result<(NormalizedMessageId, Option<MissingMessageIdShape>), ApiError> {
    let Some(value) = raw else {
        return if allow_missing {
            Ok((None, Some(MissingMessageIdShape::Absent)))
        } else {
            Err(message_id_protocol_error(
                "缺少顶层 message id（严格模式）：第三方兼容端点需显式启用 allow_missing_top_level_message_id",
            ))
        };
    };
    if value.is_null() {
        return if allow_missing {
            Ok((None, Some(MissingMessageIdShape::Null)))
        } else {
            Err(message_id_protocol_error(
                "顶层 message id 为 null（严格模式）：不得把 null 当成合法身份",
            ))
        };
    }
    let Some(text) = value.as_str() else {
        return Err(message_id_protocol_error(
            "顶层 message id 类型不是字符串：不自动转换成字符串，按协议错误处理",
        ));
    };
    if text.trim().is_empty() {
        // 空串/全空白**两种模式都拒绝**：它既不是有效身份，也不是"未提供"的合法表达。
        return Err(message_id_protocol_error(
            "顶层 message id 为空串或全空白：不得当作合法身份（也不视为缺失）",
        ));
    }
    Ok((Some(text.to_string()), None))
}

fn message_id_protocol_error(message: impl Into<String>) -> ApiError {
    ApiError::Json(serde_json::Error::io(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        message.into(),
    )))
}

/// 从响应 JSON 里取出顶层 `id` 的**原始值**（不解释类型，交给归一化器判定）。
#[must_use]
pub fn top_level_message_id_value(body: &serde_json::Value) -> Option<&serde_json::Value> {
    body.get("id")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 裁决 §2.2 的表：**逐行**验证两种模式下的行为。
    #[test]
    fn normalization_table_matches_the_ruling() {
        // ① 正常非空字符串：两种模式都保留原值。
        for allow_missing in [false, true] {
            let (id, shape) =
                normalize_top_level_message_id(Some(&json!("msg_01ABC")), allow_missing)
                    .expect("有效 ID 必须通过");
            assert_eq!(id.as_deref(), Some("msg_01ABC"));
            assert_eq!(shape, None, "有效 ID 不得有缺失形态");
        }
        // ② 顶层缺失：严格 ⇒ 拒绝；兼容 ⇒ None + 形态 absent。
        assert!(
            normalize_top_level_message_id(None, false).is_err(),
            "严格模式必须拒绝缺失"
        );
        let (id, shape) = normalize_top_level_message_id(None, true).expect("兼容模式接受缺失");
        assert_eq!(id, None, "缺失必须显式为 None，不得伪造成空串");
        assert_eq!(shape, Some(MissingMessageIdShape::Absent));
        // ③ 顶层为 null：严格 ⇒ 拒绝；兼容 ⇒ None + 形态 null（与 absent 可分辨）。
        assert!(normalize_top_level_message_id(Some(&json!(null)), false).is_err());
        let (id, shape) =
            normalize_top_level_message_id(Some(&json!(null)), true).expect("兼容模式接受 null");
        assert_eq!(id, None);
        assert_eq!(shape, Some(MissingMessageIdShape::Null));
        // ④ 空串／全空白：**两种模式都拒绝**。
        for bad in [json!(""), json!("   ")] {
            for allow_missing in [false, true] {
                assert!(
                    normalize_top_level_message_id(Some(&bad), allow_missing).is_err(),
                    "空串/全空白不得被当作合法身份（allow_missing={allow_missing}）"
                );
            }
        }
        // ⑤ 错误类型：两种模式都拒绝（不自动转字符串）。
        for bad in [json!(123), json!({"nested": true}), json!([1, 2])] {
            for allow_missing in [false, true] {
                assert!(
                    normalize_top_level_message_id(Some(&bad), allow_missing).is_err(),
                    "错误类型必须拒绝（allow_missing={allow_missing}）"
                );
            }
        }
    }

    /// 诊断口径：形态字符串与规则名是**稳定契约**（日志/请求快照按它们落字段）。
    #[test]
    fn diagnostics_wording_is_stable() {
        assert_eq!(MissingMessageIdShape::Absent.as_str(), "absent");
        assert_eq!(MissingMessageIdShape::Null.as_str(), "null");
        assert_eq!(
            ALLOW_MISSING_MESSAGE_ID_RULE,
            "allow_missing_top_level_message_id"
        );
    }

    /// 取出原始值：字段不在 ⇒ `None`；在 ⇒ 原样（**不**在取的时候解释类型）。
    #[test]
    fn raw_value_extraction_does_not_interpret() {
        assert!(top_level_message_id_value(&json!({"type": "message"})).is_none());
        assert_eq!(top_level_message_id_value(&json!({"id": null})), Some(&json!(null)));
        assert_eq!(
            top_level_message_id_value(&json!({"id": 7})),
            Some(&json!(7))
        );
    }
}
