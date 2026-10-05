//! 原生网页观察：固定宿主读调用、实际文档身份与有限AX；不执行JS或发送输入。
use native_browser_protocol::{ObservedNode, PageObservation, PanelResource};
use tauri::{AppHandle, Manager};
use super::{native_browser_devtools::{self,ReadMethod},native_browser_nodes::DocumentIdentity};

pub(super) fn bounded_text(value: Option<&serde_json::Value>, limit: usize) -> String {
    value.and_then(|value| value.get("value")).and_then(serde_json::Value::as_str)
        .unwrap_or_default().chars().filter(|c| !c.is_control()).take(limit).collect()
}
fn focused_editor(node: &serde_json::Value, role: &str) -> bool {
    matches!(role, "textbox" | "searchbox" | "combobox" | "button" | "link" | "checkbox" | "radio")
        && node.get("properties").and_then(serde_json::Value::as_array).is_some_and(|properties|
            properties.iter().any(|property| property["name"] == "focused"
                && property.pointer("/value/value").and_then(serde_json::Value::as_bool) == Some(true)))
}
fn project_tree(value: &serde_json::Value) -> Result<PageObservation,String> {
    let nodes = value.get("nodes").and_then(serde_json::Value::as_array).ok_or("native_observation_invalid")?;
    let mut result = PageObservation {url:String::new(),title:String::new(),nodes:Vec::new(),truncated:false,
        document_token:None,node_handles:Vec::new(),viewport:None,focused_node_index:None};
    for node in nodes {
        if node.get("ignored").and_then(serde_json::Value::as_bool) != Some(false) { continue; }
        let role = bounded_text(node.get("role"),64);
        let name = bounded_text(node.get("name"),256);
        if role == "RootWebArea" && result.title.is_empty() { result.title = name.clone(); }
        if role.is_empty() && name.is_empty() { continue; }
        if result.nodes.len() == 128 { result.truncated = true; break; }
        // 不提取value/properties；name仍是可能含私密内容的不可信页面文字。
        result.nodes.push(ObservedNode {role,name});
    }
    Ok(result)
}
pub(super) async fn document(app: &AppHandle, resource: &PanelResource) -> Result<DocumentIdentity,String> {
    let frame = native_browser_devtools::read(app,resource,ReadMethod::FrameTree).await?;
    let root = native_browser_devtools::read(app,resource,ReadMethod::Document).await?;
    DocumentIdentity::from_host(&frame,&root)
}

pub(super) fn regular_document_nodes(tree: &serde_json::Value) -> std::collections::HashSet<i64> {
    let mut found = std::collections::HashSet::new();
    let Some(root) = tree.get("root") else { return found; };
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        // 只遍历children，不进入shadowRoots/contentDocument/pseudoElements。
        if found.len() >= 2048 { return std::collections::HashSet::new(); }
        if let Some(id) = node["backendNodeId"].as_i64().filter(|id| *id > 0) { found.insert(id); }
        if let Some(children) = node["children"].as_array() { pending.extend(children); }
    }
    found
}
pub(super) async fn observe(app: &AppHandle, expected: &PanelResource, observation_id: &str) -> Result<PageObservation,String> {
    if super::browser_panel::input_resource(app).as_ref() != Some(expected) { return Err("native_browser_resource_changed".into()); }
    let url = app.get_webview(&expected.label).ok_or("native_browser_unavailable")?
        .url().map_err(|_| "native_browser_unavailable")?.to_string();
    let stamp = document(app,expected).await?;
    // 仅采顶层已核实frame；第一阶段不提供iframe/shadow-document可操作引用。
    let tree = native_browser_devtools::read(app,expected,ReadMethod::Accessibility(stamp.frame_id.clone())).await?;
    let mut page = project_tree(&tree)?;
    let metrics = native_browser_devtools::read(app,expected,ReadMethod::LayoutMetrics).await?;
    let viewport = &metrics["cssVisualViewport"];
    let number = |key| viewport[key].as_f64().ok_or("native_browser_viewport_invalid");
    let viewport = native_browser_protocol::PageViewport {
        page_x:number("pageX")?, page_y:number("pageY")?,
        width:number("clientWidth")?, height:number("clientHeight")?,
    };
    if !viewport.valid_shape() { return Err("native_browser_viewport_invalid".into()); }
    page.viewport = Some(viewport);
    let ordinary_nodes = regular_document_nodes(&native_browser_devtools::read(app,expected,ReadMethod::DocumentNodes).await?);
    if document(app,expected).await? != stamp || super::browser_panel::input_resource(app).as_ref() != Some(expected)
        || app.get_webview(&expected.label).and_then(|view| view.url().ok()).as_ref().map(|url| url.as_str()) != Some(url.as_str()) {
        return Err("native_browser_document_changed".into());
    }
    let mut candidates = Vec::new();
    let mut focused_editors = Vec::new();
    let mut index = 0;
    for node in tree["nodes"].as_array().ok_or("native_observation_invalid")? {
        if node["ignored"].as_bool() != Some(false) { continue; }
        let role = bounded_text(node.get("role"),64);
        let name = bounded_text(node.get("name"),256);
        if role.is_empty() && name.is_empty() { continue; }
        if index >= page.nodes.len() { break; }
        if ["RootWebArea","button","link","textbox","searchbox","checkbox","radio","combobox"].contains(&role.as_str())
            && node.get("frameId").and_then(serde_json::Value::as_str).is_none_or(|frame| frame == stamp.frame_id) {
            if let Some(backend) = node.get("backendDOMNodeId").and_then(serde_json::Value::as_i64).filter(|id| ordinary_nodes.contains(id)) {
                candidates.push((index,backend,role,name));
                // 只读取类型化focused布尔值，绝不透传value或整个AX properties。
                if focused_editor(node, &page.nodes[index].role) {
                    focused_editors.push(index);
                }
            }
        }
        index += 1;
    }
    let (token,handles) = super::native_browser_nodes::register(expected,&stamp,observation_id,&candidates)?;
    page.url = url; page.document_token = Some(token); page.node_handles = handles;
    page.focused_node_index = (focused_editors.len() == 1).then(|| focused_editors[0]);
    if page.valid_shape() { Ok(page) } else { Err("native_observation_invalid".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ax_projection_omits_private_values_and_preserves_top_title() {
        let raw = serde_json::json!({"nodes":[
            {"ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"主页面"}},
            {"ignored":false,"role":{"value":"textbox"},"name":{"value":"口令"},"value":{"value":"SECRET"}},
            {"ignored":true,"name":{"value":"HIDDEN"}},
            {"ignored":false,"role":{"value":"RootWebArea"},"name":{"value":"子页面"}}]});
        let mut page = project_tree(&raw).unwrap(); page.url = "https://example.invalid/".into();
        let text = serde_json::to_string(&page).unwrap();
        assert!(page.valid_shape()); assert_eq!(page.title,"主页面");
        assert!(!text.contains("SECRET") && !text.contains("HIDDEN"));
        let focus = serde_json::json!({"properties":[{"name":"focused","value":{"value":true}},
            {"name":"value","value":{"value":"SECRET"}}]});
        assert!(focused_editor(&focus,"textbox"));
        assert!(focused_editor(&focus,"button"));
        assert!(!focused_editor(&serde_json::json!({"properties":[{"name":"focused","value":{"value":"true"}}]}),"textbox"));
        page.document_token = Some("a".repeat(32));
        page.node_handles.push(native_browser_protocol::NodeHandle {index:1,node_id:"b".repeat(32)});
        page.focused_node_index = Some(1);
        assert!(page.valid_shape());
        page.focused_node_index = Some(0);
        assert!(!page.valid_shape());
        page.focused_node_index = None;
        page.node_handles.push(native_browser_protocol::NodeHandle {index:999,node_id:"0".repeat(32)});
        assert!(!page.valid_shape());
        let dom = serde_json::json!({"root":{"backendNodeId":1,"children":[{"backendNodeId":2,
            "shadowRoots":[{"backendNodeId":3}],"contentDocument":{"backendNodeId":4}}]}});
        let ids = regular_document_nodes(&dom);
        assert!(ids.contains(&1) && ids.contains(&2) && !ids.contains(&3) && !ids.contains(&4));
    }
}
