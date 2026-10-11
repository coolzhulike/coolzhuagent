//! 固定只读编辑状态；原值和选区仅用于宿主短期重检，不进入模型或运行记录。
use serde_json::Value;
use tauri::AppHandle;
use super::{native_browser_devtools::{self,ReadMethod},native_browser_target::VerifiedTarget};

// 不接受请求提供的脚本；副作用保护拒绝页面自定义getter等无法证明只读的行为。
pub(super) const READ_EDITOR: &str = r#"function(){
 if(!this.ownerDocument.hasFocus())return null;
 const tag=this.tagName;const kind=tag==='INPUT'?this.type:'textarea';
 if(!this.isConnected||this.ownerDocument.activeElement!==this||this.disabled||this.readOnly||
    !(tag==='TEXTAREA'||(tag==='INPUT'&&(kind==='text'||kind==='search'))))return null;
 const value=this.value;if(typeof value!=='string'||value.length>4096)return null;
 const start=this.selectionStart,end=this.selectionEnd,direction=this.selectionDirection;
 if(!Number.isInteger(start)||!Number.isInteger(end)||start<0||end<start||end>value.length)return null;
 return {tag,kind,value,start,end,direction};
}"#;

pub(super) fn state(raw:&Value) -> Result<Value,String> {
    if raw.get("exceptionDetails").is_some() {
        // 只暴露固定错误类别，不记录网页异常正文、原值或选区。
        let side_effect = raw.pointer("/exceptionDetails/exception/description").and_then(Value::as_str)
            .is_some_and(|text| text.contains("Possible side-effect"));
        return Err(if side_effect {"native_browser_focus_read_side_effect"} else {"native_browser_editor_unavailable"}.into());
    }
    let value=raw.pointer("/result/value").filter(|v|v.is_object()).ok_or("native_browser_editor_not_focused")?;
    let text=value["value"].as_str().ok_or("native_browser_editor_unavailable")?;
    let start=value["start"].as_u64().ok_or("native_browser_editor_unavailable")?;
    let end=value["end"].as_u64().ok_or("native_browser_editor_unavailable")?;
    if text.len()>16384 || start>end || end>text.encode_utf16().count() as u64
        || !matches!(value["tag"].as_str(),Some("INPUT"|"TEXTAREA"))
        || !matches!(value["kind"].as_str(),Some("text"|"search"|"textarea")) {
        return Err("native_browser_editor_unavailable".into());
    }
    Ok(value.clone())
}

/// 只在宿主短期比较UTF-16选区替换；不返回原值、选区或预期正文。
pub(super) fn inserted_changed(before:&Value,after:&Value,text:&str) -> Option<bool> {
    if before["tag"]!=after["tag"] || before["kind"]!=after["kind"] {return None;}
    let original:Vec<u16>=before["value"].as_str()?.encode_utf16().collect();
    let actual:Vec<u16>=after["value"].as_str()?.encode_utf16().collect();
    let start=usize::try_from(before["start"].as_u64()?).ok()?;
    let end=usize::try_from(before["end"].as_u64()?).ok()?;
    if start>end || end>original.len() {return None;}
    let mut expected=original.clone();expected.splice(start..end,text.encode_utf16());
    if expected.len()>4096 {return None;}
    Some(actual==expected && actual!=original)
}

pub(super) async fn resolve(app:&AppHandle,target:&VerifiedTarget) -> Result<String,String> {
    native_browser_devtools::read_session(app,&target.resource,target.node.scope.session(),ReadMethod::ResolveEditor(target.node.backend_node)).await?
        .pointer("/object/objectId").and_then(Value::as_str).filter(|s|!s.is_empty() && s.len()<=256)
        .map(str::to_string).ok_or_else(||"native_browser_editor_unavailable".into())
}
pub(super) async fn read(app:&AppHandle,target:&VerifiedTarget) -> Result<Value,String> {
    let object=resolve(app,target).await?;
    let session=target.node.scope.session();
    let result=native_browser_devtools::read_session(app,&target.resource,session,ReadMethod::EditorState(object.clone())).await
        .and_then(|value|state(&value));
    let _=native_browser_devtools::read_session(app,&target.resource,session,ReadMethod::ReleaseEditor(object)).await;
    result
}

pub(super) async fn verify(app:&AppHandle,resource:&native_browser_protocol::PanelResource,target:&native_browser_protocol::PanelClickTarget) -> Result<VerifiedTarget,String> {
    let mut verified=super::native_browser_target::verify(app,resource,&target.observation_id,&target.document_token,&target.node_id).await?;
    if !matches!(verified.node.role.as_str(),"textbox"|"searchbox") {return Err("native_browser_editor_target_invalid".into());}
    verified.editor=Some(read(app,&verified).await?);
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn insertion_compares_utf16_replacement_without_exposing_text() {
        let before=serde_json::json!({"tag":"INPUT","kind":"text","value":"玉😀竹","start":1,"end":3});
        let mut after=before.clone();after["value"]=serde_json::json!("玉剑竹");
        assert_eq!(inserted_changed(&before,&after,"剑"),Some(true));
        assert_eq!(inserted_changed(&before,&before,"😀"),Some(false));
        assert_eq!(inserted_changed(&before,&after,"槊"),Some(false));
        let mut invalid=before.clone();invalid["end"]=serde_json::json!(99);
        assert_eq!(inserted_changed(&invalid,&after,"剑"),None);
        after["kind"]=serde_json::json!("search");
        assert_eq!(inserted_changed(&before,&after,"剑"),None);
    }
}
