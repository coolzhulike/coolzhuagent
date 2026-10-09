//! 工具参数的诊断投影；权威调用与执行参数仍由原事实链保存。
//! 不序列化字段名或值，避免未知插件参数绕过按名称列举的脱敏规则。
use serde_json::Value;

pub(super) fn input_shape(input: &Value) -> String {
    match input {
        Value::Object(fields) => format!("[object fields={}；参数内容已隐藏]", fields.len()),
        Value::Array(items) => format!("[array items={}；参数内容已隐藏]", items.len()),
        Value::Null => "[null]".into(),
        Value::Bool(_) => "[boolean；参数内容已隐藏]".into(),
        Value::Number(_) => "[number；参数内容已隐藏]".into(),
        Value::String(_) => "[string；参数内容已隐藏]".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shape_does_not_depend_on_keys_values_or_nested_secrets() {
        let secret = json!({"SECRET-KEY": {"Authorization":"SECRET-TOKEN"}, "path":"PRIVATE-PATH"});
        let ordinary = json!({"first":0, "second":false});
        assert_eq!(input_shape(&secret), input_shape(&ordinary));
        assert_eq!(input_shape(&json!([secret])), input_shape(&json!([null])));
        assert_eq!(input_shape(&json!("SECRET-TOKEN")), input_shape(&json!("ordinary")));
        assert_eq!(input_shape(&json!(123456)), input_shape(&json!(0)));
        assert_eq!(input_shape(&json!(true)), input_shape(&json!(false)));
        assert_eq!(input_shape(&Value::Null), "[null]");
    }
}
