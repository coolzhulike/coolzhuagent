//! 宿主重新核对真实文档、AX、几何与命中结果；节点引用本身不授予输入资格。
use native_browser_protocol::PanelResource;
use serde_json::Value;
use tauri::AppHandle;

use super::{
    native_browser_devtools::{self, ReadMethod},
    native_browser_nodes::{self, DocumentIdentity, NodeBinding},
    native_browser_observation::{self, bounded_text, regular_document_nodes},
};

#[derive(Clone)]
pub(super) struct VerifiedTarget {
    pub resource: PanelResource,
    pub document: DocumentIdentity,
    pub node: NodeBinding,
    pub x: i32,
    pub y: i32,
    pub viewport: Value,
    pub editor: Option<Value>,
}

fn current_node(tree: &Value, document: &DocumentIdentity, node: &NodeBinding) -> Result<(), String> {
    let nodes = tree["nodes"].as_array().ok_or("native_browser_target_unavailable")?;
    let mut matching = nodes.iter().filter(|candidate|
        candidate["ignored"].as_bool() == Some(false)
        && candidate["backendDOMNodeId"].as_i64() == Some(node.backend_node));
    let actual = matching.next().ok_or("native_browser_node_changed")?;
    if matching.next().is_some()
        || actual["frameId"].as_str().is_some_and(|frame| frame != document.frame_id)
        || bounded_text(actual.get("role"), 64) != node.role
        || bounded_text(actual.get("name"), 256) != node.name {
        return Err("native_browser_node_changed".into());
    }
    // 初期不操作不可编辑、禁用或口令控件；不能靠页面自报的授权字段放行。
    if actual["properties"].as_array().is_some_and(|properties| properties.iter().any(|property|
        ["disabled", "readonly", "protected", "hidden"].contains(&property["name"].as_str().unwrap_or_default())
            && property.pointer("/value/value").and_then(Value::as_bool) == Some(true))) {
        return Err("native_browser_target_not_interactable".into());
    }
    Ok(())
}

fn point(box_model: &Value, metrics: &Value) -> Result<(i32, i32), String> {
    let quad = box_model.pointer("/model/content").and_then(Value::as_array)
        .filter(|quad| quad.len() == 8).ok_or("native_browser_target_geometry_invalid")?;
    let coordinates = quad.iter().map(|value| value.as_f64().filter(|value| value.is_finite()))
        .collect::<Option<Vec<_>>>().ok_or("native_browser_target_geometry_invalid")?;
    let viewport = metrics.get("cssVisualViewport").ok_or("native_browser_target_geometry_invalid")?;
    let number = |key| viewport.get(key).and_then(Value::as_f64).filter(|value| value.is_finite())
        .ok_or("native_browser_target_geometry_invalid");
    let offset_x = number("pageX")?;
    let offset_y = number("pageY")?;
    let width = number("clientWidth")?;
    let height = number("clientHeight")?;
    if width <= 0.0 || height <= 0.0 || width > 32768.0 || height > 32768.0 {
        return Err("native_browser_target_geometry_invalid".into());
    }
    let x = (coordinates[0] + coordinates[2] + coordinates[4] + coordinates[6]) / 4.0 - offset_x;
    let y = (coordinates[1] + coordinates[3] + coordinates[5] + coordinates[7]) / 4.0 - offset_y;
    let min_x = coordinates.iter().step_by(2).copied().fold(f64::INFINITY, f64::min) - offset_x;
    let max_x = coordinates.iter().step_by(2).copied().fold(f64::NEG_INFINITY, f64::max) - offset_x;
    let min_y = coordinates.iter().skip(1).step_by(2).copied().fold(f64::INFINITY, f64::min) - offset_y;
    let max_y = coordinates.iter().skip(1).step_by(2).copied().fold(f64::NEG_INFINITY, f64::max) - offset_y;
    let x = x.round();
    let y = y.round();
    // 至少留1CSS像素内边距；不自动滚动，也不将出界坐标钳制成另一个目标。
    if max_x - min_x < 2.0 || max_y - min_y < 2.0
        || x <= min_x || x >= max_x || y <= min_y || y >= max_y
        || x < 1.0 || y < 1.0 || x >= width - 1.0 || y >= height - 1.0 {
        return Err("native_browser_target_outside_viewport".into());
    }
    Ok((x as i32, y as i32))
}

/// 只返回预检结果。调用方仍须获得一次性许可、提交本轮派发并在owner UI线程再核资源。
pub(super) async fn verify(
    app: &AppHandle, resource: &PanelResource, observation_id: &str,
    document_token: &str, node_id: &str,
) -> Result<VerifiedTarget, String> {
    let started = std::time::Instant::now();
    let document = native_browser_observation::document(app, resource).await?;
    let node = native_browser_nodes::resolve(resource, &document, observation_id, document_token, node_id)?;
    let tree = native_browser_devtools::read(app, resource, ReadMethod::Accessibility(document.frame_id.clone())).await?;
    current_node(&tree, &document, &node)?;
    let dom = native_browser_devtools::read(app, resource, ReadMethod::DocumentNodes).await?;
    if !regular_document_nodes(&dom).contains(&node.backend_node) {
        return Err("native_browser_target_document_unsupported".into());
    }
    let geometry = native_browser_devtools::read(app, resource, ReadMethod::BoxModel(node.backend_node)).await?;
    let metrics = native_browser_devtools::read(app, resource, ReadMethod::LayoutMetrics).await?;
    let (x, y) = point(&geometry, &metrics)?;
    let hit = native_browser_devtools::read(app, resource, ReadMethod::HitTest(x, y)).await?;
    // 不接受相似文本、覆盖层、后代或另一个frame来替代原绑定节点。
    if hit["backendNodeId"].as_i64() != Some(node.backend_node)
        || hit["frameId"].as_str() != Some(document.frame_id.as_str()) {
        return Err("native_browser_target_hit_mismatch".into());
    }
    let second_geometry = native_browser_devtools::read(app, resource, ReadMethod::BoxModel(node.backend_node)).await?;
    let second_metrics = native_browser_devtools::read(app, resource, ReadMethod::LayoutMetrics).await?;
    if geometry.pointer("/model/content") != second_geometry.pointer("/model/content")
        || point(&second_geometry, &second_metrics)? != (x, y)
        || native_browser_observation::document(app, resource).await? != document
        || super::browser_panel::input_resource(app).as_ref() != Some(resource)
        || started.elapsed() >= std::time::Duration::from_secs(2) {
        return Err("native_browser_target_changed".into());
    }
    Ok(VerifiedTarget { resource: resource.clone(), document, node, x, y, viewport:second_metrics["cssVisualViewport"].clone(), editor:None })
}

/// 导航只核对顶层实际文档，不借用滚动的视口命中限制。
pub(super) async fn verify_document(app:&AppHandle,resource:&PanelResource,observation_id:&str,document_token:&str,node_id:&str) -> Result<VerifiedTarget,String> {
    let started=std::time::Instant::now();
    let document=native_browser_observation::document(app,resource).await?;
    let node=native_browser_nodes::resolve(resource,&document,observation_id,document_token,node_id)?;
    if node.role!="RootWebArea" || node.backend_node!=document.backend_root {return Err("native_browser_navigation_target_invalid".into());}
    let tree=native_browser_devtools::read(app,resource,ReadMethod::Accessibility(document.frame_id.clone())).await?;
    current_node(&tree,&document,&node)?;
    if native_browser_observation::document(app,resource).await?!=document || super::browser_panel::input_resource(app).as_ref()!=Some(resource)
        || started.elapsed()>=std::time::Duration::from_secs(2) {return Err("native_browser_target_changed".into());}
    Ok(VerifiedTarget {resource:resource.clone(),document,node,x:0,y:0,viewport:Value::Null,editor:None})
}

/// 只允许顶层可见viewport；模型选择RootWebArea随机引用，坐标由宿主生成。
pub(super) async fn verify_viewport(app:&AppHandle,resource:&PanelResource,observation_id:&str,document_token:&str,node_id:&str) -> Result<VerifiedTarget,String> {
    let started=std::time::Instant::now();
    let document=native_browser_observation::document(app,resource).await?;
    let node=native_browser_nodes::resolve(resource,&document,observation_id,document_token,node_id)?;
    if node.role!="RootWebArea" || node.backend_node!=document.backend_root {return Err("native_browser_scroll_target_invalid".into());}
    let tree=native_browser_devtools::read(app,resource,ReadMethod::Accessibility(document.frame_id.clone())).await?;
    current_node(&tree,&document,&node)?;
    let metrics=native_browser_devtools::read(app,resource,ReadMethod::LayoutMetrics).await?;
    let viewport=metrics["cssVisualViewport"].clone();
    let width=viewport["clientWidth"].as_f64().filter(|v|v.is_finite() && *v>2.0 && *v<=32768.0).ok_or("native_browser_target_geometry_invalid")?;
    let height=viewport["clientHeight"].as_f64().filter(|v|v.is_finite() && *v>2.0 && *v<=32768.0).ok_or("native_browser_target_geometry_invalid")?;
    let (x,y)=((width/2.0).round() as i32,(height/2.0).round() as i32);
    let hit=native_browser_devtools::read(app,resource,ReadMethod::HitTest(x,y)).await?;
    let dom=native_browser_devtools::read(app,resource,ReadMethod::DocumentNodes).await?;
    if hit["frameId"].as_str()!=Some(document.frame_id.as_str())
        || !hit["backendNodeId"].as_i64().is_some_and(|id|regular_document_nodes(&dom).contains(&id)) {
        return Err("native_browser_target_hit_mismatch".into());
    }
    let fresh=native_browser_devtools::read(app,resource,ReadMethod::LayoutMetrics).await?;
    if fresh["cssVisualViewport"]!=viewport || native_browser_observation::document(app,resource).await?!=document
        || super::browser_panel::input_resource(app).as_ref()!=Some(resource) || started.elapsed()>=std::time::Duration::from_secs(2) {
        return Err("native_browser_target_changed".into());
    }
    Ok(VerifiedTarget {resource:resource.clone(),document,node,x,y,viewport,editor:None})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_css_viewport_relative_and_rejects_clipped_or_degenerate_targets() {
        let model = serde_json::json!({"model":{"content":[100,200,140,200,140,240,100,240]}});
        let viewport = serde_json::json!({"cssVisualViewport":{"pageX":80,"pageY":180,"clientWidth":800,"clientHeight":600}});
        assert_eq!(point(&model, &viewport).unwrap(), (40,40));
        let clipped = serde_json::json!({"cssVisualViewport":{"pageX":150,"pageY":180,"clientWidth":800,"clientHeight":600}});
        assert!(point(&model, &clipped).is_err());
        assert!(point(&serde_json::json!({"model":{"content":[1,1,1,1,1,1,1,1]}}), &viewport).is_err());
        assert!(point(&model, &serde_json::json!({})).is_err());
    }

    #[test]
    fn fresh_ax_must_match_backend_role_name_and_interactable_state() {
        let document = DocumentIdentity {frame_id:"frame-a".into(),loader_id:"loader-a".into(),backend_root:1};
        let node = NodeBinding {node_id:"0".repeat(32),index:0,backend_node:9,role:"button".into(),name:"提交".into()};
        let mut tree = serde_json::json!({"nodes":[{"ignored":false,"backendDOMNodeId":9,"frameId":"frame-a",
            "role":{"value":"button"},"name":{"value":"提交"}}]});
        assert!(current_node(&tree, &document, &node).is_ok());
        tree["nodes"][0]["name"]["value"] = serde_json::json!("其它操作");
        assert!(current_node(&tree, &document, &node).is_err());
        tree["nodes"][0]["name"]["value"] = serde_json::json!("提交");
        tree["nodes"][0]["properties"] = serde_json::json!([{"name":"disabled","value":{"value":true}}]);
        assert!(current_node(&tree, &document, &node).is_err());
    }
}
