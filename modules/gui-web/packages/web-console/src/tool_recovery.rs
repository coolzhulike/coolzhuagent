//! 恢复决策只消费宿主事实；工具正文不能赋予重放资格。
use std::{cell::RefCell, future::Future};
use crate::JsonValue;

tokio::task_local! { static TERMINAL: RefCell<Option<String>>; }
pub(crate) async fn capture<F:Future>(future:F)->(F::Output,Option<String>) {
    TERMINAL.scope(RefCell::new(None),async {
        let output=future.await;
        let reason=TERMINAL.with(|cell|cell.borrow_mut().take());
        (output,reason)
    }).await
}
pub(crate) fn terminal(reason: impl Into<String>) {
    let reason=reason.into();
    let _=TERMINAL.try_with(|cell| {cell.borrow_mut().get_or_insert(reason);});
}

/// 只核对最终已发送schema明确声明的必要字段/基础类型，不猜测业务语义。
/// 校验发生在整批派发之前，因此此类错误的目标工具进入数确定为0。
pub(crate) fn invalid_arguments(request:&api::MessageRequest,calls:&[(String,String,JsonValue)])->Option<String> {
    for (_,name,input) in calls {
        let Some(definition)=request.tools.as_ref().and_then(|tools|tools.iter().find(|tool|tool.name==*name)) else {continue;};
        if let Some(reason)=invalid(&definition.input_schema,input,"$",0) {
            return Some(format!("工具 `{name}` 参数不符合本次声明：{reason}。本批目标工具均未派发。"));
        }
    }
    None
}
fn invalid(schema:&JsonValue,value:&JsonValue,path:&str,depth:u8)->Option<String> {
    if depth>6 {return None;}
    if let Some(kind)=schema["type"].as_str() {
        let valid=match kind {"object"=>value.is_object(),"array"=>value.is_array(),"string"=>value.is_string(),
            "boolean"=>value.is_boolean(),"number"=>value.is_number(),"integer"=>value.as_i64().is_some()||value.as_u64().is_some()||value.as_f64().is_some_and(|n|n.is_finite()&&n.fract()==0.0),"null"=>value.is_null(),_=>true};
        if !valid {return Some(format!("{path} 必须为{kind}"));}
    }
    if let Some(object)=value.as_object() {
        if let Some(required)=schema["required"].as_array() {
            for field in required.iter().filter_map(JsonValue::as_str) {
                if !object.contains_key(field) {return Some(format!("{path} 缺少必要字段 {field}"));}
            }
        }
        if let Some(properties)=schema["properties"].as_object() {
            for (field,subschema) in properties {
                if let Some(v)=object.get(field) {
                    if let Some(reason)=invalid(subschema,v,&format!("{path}.{field}"),depth+1) {return Some(reason);}
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_declared_structural_errors_are_repairable() {
        let schema=serde_json::json!({"type":"object","required":["path"],"properties":{"path":{"type":"string"}}});
        assert!(invalid(&schema,&serde_json::json!({}),"$",0).is_some());
        assert!(invalid(&schema,&serde_json::json!({"path":3}),"$",0).is_some());
        assert!(invalid(&schema,&serde_json::json!({"path":"","unknown":"retained"}),"$",0).is_none());
        assert!(invalid(&serde_json::json!({"type":["string","null"]}),&JsonValue::Null,"$",0).is_none());
        assert!(invalid(&serde_json::json!({"type":"integer"}),&serde_json::json!(1.0),"$",0).is_none());
    }
}
