//! 宿主重新核对真实文档、AX、几何与命中结果；节点引用本身不授予输入资格。
use native_browser_protocol::{PanelResource,ScrollDirection};
use serde_json::Value;
use tauri::AppHandle;

use super::{
    native_browser_devtools::{self, ReadMethod},
    native_browser_nodes::{self, DocumentIdentity, NodeBinding},
    native_browser_observation::{self, bounded_text},
    native_browser_dom::{operable_document_nodes, open_shadow_roots, NODE_LIMIT},
    native_browser_document::{self,DocumentSnapshot},
};

#[derive(Clone)]
pub(super) struct VerifiedTarget {
    pub resource: PanelResource,
    pub document: DocumentIdentity,
    pub snapshot: DocumentSnapshot,
    pub node: NodeBinding,
    pub x: i32,
    pub y: i32,
    pub viewport: Value,
    pub editor: Option<Value>,
}

pub(super) fn current_node(tree: &Value, document: &DocumentIdentity, node: &NodeBinding) -> Result<(), String> {
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

pub(super) fn point(box_model: &Value, metrics: &Value) -> Result<(i32, i32), String> {
    let quad = box_model.pointer("/model/content").and_then(Value::as_array)
        .filter(|quad| quad.len() == 8).ok_or("native_browser_target_geometry_invalid")?;
    let coordinates = quad.iter().map(|value| value.as_f64().filter(|value| value.is_finite()))
        .collect::<Option<Vec<_>>>().ok_or("native_browser_target_geometry_invalid")?;
    let viewport = metrics.get("cssVisualViewport").ok_or("native_browser_target_geometry_invalid")?;
    let number = |key| viewport.get(key).and_then(Value::as_f64).filter(|value| value.is_finite())
        .ok_or("native_browser_target_geometry_invalid");
    let width = number("clientWidth")?;
    let height = number("clientHeight")?;
    if width <= 0.0 || height <= 0.0 || width > 32768.0 || height > 32768.0 {
        return Err("native_browser_target_geometry_invalid".into());
    }
    // Chromium GetBoxModel 已通过 FrameQuadToViewport 返回视口坐标。
    // pageX/pageY 是文档滚动偏移，再减一次会使滚动后的控件产生错误位置。
    let x = (coordinates[0] + coordinates[2] + coordinates[4] + coordinates[6]) / 4.0;
    let y = (coordinates[1] + coordinates[3] + coordinates[5] + coordinates[7]) / 4.0;
    let min_x = coordinates.iter().step_by(2).copied().fold(f64::INFINITY, f64::min);
    let max_x = coordinates.iter().step_by(2).copied().fold(f64::NEG_INFINITY, f64::max);
    let min_y = coordinates.iter().skip(1).step_by(2).copied().fold(f64::INFINITY, f64::min);
    let max_y = coordinates.iter().skip(1).step_by(2).copied().fold(f64::NEG_INFINITY, f64::max);
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

/// DOM.getNodeForLocation 接收文档坐标；鼠标派发仍接收视口坐标，不能混用。
pub(super) fn hit_test_point(x: i32, y: i32, metrics: &Value) -> Result<(i32, i32), String> {
    let viewport = metrics.get("cssVisualViewport").ok_or("native_browser_target_geometry_invalid")?;
    let coordinate = |value: i32, key: &str| {
        let offset = viewport.get(key).and_then(Value::as_f64).filter(|value|value.is_finite())
            .ok_or("native_browser_target_geometry_invalid")?;
        let point = (f64::from(value) + offset).round();
        // RTL文档的合法坐标可为负；视口内点已由point/map_owner独立校验。
        if !(f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&point) {
            return Err("native_browser_target_geometry_invalid");
        }
        Ok(point as i32)
    };
    Ok((coordinate(x,"pageX")?,coordinate(y,"pageY")?))
}

/// 普通文字、图标容器的点击会冒泡给原控件；其它控件与覆盖层不能代替它。
/// 开放 Shadow Root 内的目标须独立绑定；进入根时切断父控件的祖先点击资格。
#[cfg(test)]
fn hit_belongs_to_target(dom: &Value, target: i64, hit: i64) -> bool {
    let Some(root) = dom.get("root") else { return false; };
    hit_belongs_to_root(root,target,hit)
}

fn hit_belongs_to_root(root:&Value,target:i64,hit:i64) -> bool {
    if target <= 0 || hit <= 0 { return false; }
    let mut pending = vec![(root, false)];
    let mut visited = std::collections::HashSet::new();
    let mut matched = false;
    while let Some((node, inside)) = pending.pop() {
        if visited.len() >= NODE_LIMIT { return false; }
        let Some(id) = node["backendNodeId"].as_i64().filter(|id| *id > 0) else { return false; };
        if !visited.insert(id) { return false; }
        let inside = id == target || (inside && !nested_interactive(node));
        if id == hit { matched = inside; }
        if let Some(children) = node["children"].as_array() {
            pending.extend(children.iter().map(|child| (child, inside)));
        }
        pending.extend(open_shadow_roots(node).map(|root| (root, false)));
    }
    matched
}

fn nested_interactive(node: &Value) -> bool {
    if ["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA", "SUMMARY", "DETAILS", "IFRAME", "FRAME", "OBJECT", "EMBED"]
        .iter().any(|name| node["nodeName"].as_str().is_some_and(|actual| actual.eq_ignore_ascii_case(name))) { return true; }
    node["attributes"].as_array().is_some_and(|attributes| attributes.chunks_exact(2).any(|pair| {
        let name = pair[0].as_str().unwrap_or_default();
        let value = pair[1].as_str().unwrap_or_default().to_ascii_lowercase();
        name.eq_ignore_ascii_case("tabindex")
            || (name.eq_ignore_ascii_case("contenteditable") && value != "false")
            || (name.eq_ignore_ascii_case("role") && value.split_ascii_whitespace().any(|role|
                ["button", "link", "checkbox", "radio", "switch", "textbox", "searchbox", "combobox", "listbox", "option",
                 "menuitem", "menuitemcheckbox", "menuitemradio", "treeitem", "tab", "slider", "spinbutton"].contains(&role)))
    }))
}

/// 只返回预检结果。调用方仍须获得一次性许可、提交本轮派发并在owner UI线程再核资源。
pub(super) async fn verify(
    app: &AppHandle, resource: &PanelResource, observation_id: &str,
    document_token: &str, node_id: &str,
) -> Result<VerifiedTarget, String> {
    let started = std::time::Instant::now();
    let (snapshot,dom) = native_browser_observation::snapshot(app,resource).await?;
    let document = snapshot.top.clone();
    let node = native_browser_nodes::resolve(resource, &snapshot, observation_id, document_token, node_id)?;
    let tree = native_browser_devtools::read_session(app, resource, node.scope.session(), ReadMethod::Accessibility(node.scope.document().frame_id.clone())).await?;
    current_node(&tree, node.scope.document(), &node)?;
    let root = native_browser_document::root_for_scope(&dom,&node.scope)?;
    if !super::native_browser_dom::operable_root_nodes(root).contains(&node.backend_node) {
        return Err("native_browser_target_document_unsupported".into());
    }
    let geometry = native_browser_devtools::read_session(app, resource, node.scope.session(), ReadMethod::BoxModel(node.backend_node)).await?;
    let metrics = native_browser_devtools::read(app, resource, ReadMethod::LayoutMetrics).await?;
    let mapped=if node.scope.session().is_some() {Some(super::native_browser_frame_geometry::locate(app,resource,&node.scope,&geometry,&metrics).await?)} else {None};
    let (x,y)=match &mapped {Some(p)=>(p.x,p.y),None=>point(&geometry,&metrics)?};
    let (hit_x,hit_y)=match &mapped {Some(p)=>hit_test_point(p.local_x,p.local_y,&p.local_metrics)?,None=>hit_test_point(x,y,&metrics)?};
    let hit = native_browser_devtools::read_session(app, resource, node.scope.session(), ReadMethod::HitTest(hit_x,hit_y)).await?;
    // 在命中之后重新读取DOM，证明命中点仍属于原绑定控件，避免复用布局前的祖先关系。
    let (hit_snapshot,hit_dom) = native_browser_observation::snapshot(app,resource).await?;
    if hit_snapshot != snapshot { return Err("native_browser_document_changed".into()); }
    let hit_root = native_browser_document::root_for_scope(&hit_dom,&node.scope)?;
    if hit["frameId"].as_str() != Some(node.scope.document().frame_id.as_str())
        || !hit["backendNodeId"].as_i64().is_some_and(|id| hit_belongs_to_root(hit_root, node.backend_node, id)) {
        return Err("native_browser_target_hit_mismatch".into());
    }
    let second_geometry = native_browser_devtools::read_session(app, resource, node.scope.session(), ReadMethod::BoxModel(node.backend_node)).await?;
    let second_metrics = native_browser_devtools::read(app, resource, ReadMethod::LayoutMetrics).await?;
    let second_point=if let Some(original)=&mapped {
        let fresh=super::native_browser_frame_geometry::locate(app,resource,&node.scope,&second_geometry,&second_metrics).await?;
        if original.proof!=fresh.proof {return Err("native_browser_target_changed".into());}
        (fresh.x,fresh.y)
    } else {point(&second_geometry,&second_metrics)?};
    if geometry.pointer("/model/content") != second_geometry.pointer("/model/content")
        || second_point != (x, y)
        || metrics["cssVisualViewport"] != second_metrics["cssVisualViewport"]
        || native_browser_observation::snapshot(app, resource).await?.0 != snapshot
        || super::browser_panel::input_resource(app).as_ref() != Some(resource)
        || started.elapsed() >= std::time::Duration::from_secs(2) {
        return Err("native_browser_target_changed".into());
    }
    Ok(VerifiedTarget { resource: resource.clone(), document, snapshot, node, x, y, viewport:second_metrics["cssVisualViewport"].clone(), editor:None })
}

/// 导航只核对顶层实际文档，不借用滚动的视口命中限制。
pub(super) async fn verify_document(app:&AppHandle,resource:&PanelResource,observation_id:&str,document_token:&str,node_id:&str) -> Result<VerifiedTarget,String> {
    let started=std::time::Instant::now();
    let (snapshot,_)=native_browser_observation::snapshot(app,resource).await?;
    let document=snapshot.top.clone();
    let node=native_browser_nodes::resolve(resource,&snapshot,observation_id,document_token,node_id)?;
    if !node.scope.is_top() || node.role!="RootWebArea" || node.backend_node!=document.backend_root {return Err("native_browser_navigation_target_invalid".into());}
    let tree=native_browser_devtools::read(app,resource,ReadMethod::Accessibility(document.frame_id.clone())).await?;
    current_node(&tree,&document,&node)?;
    if native_browser_observation::snapshot(app,resource).await?.0!=snapshot || super::browser_panel::input_resource(app).as_ref()!=Some(resource)
        || started.elapsed()>=std::time::Duration::from_secs(2) {return Err("native_browser_target_changed".into());}
    Ok(VerifiedTarget {resource:resource.clone(),document,snapshot,node,x:0,y:0,viewport:Value::Null,editor:None})
}

/// 模型选择文档RootWebArea，宿主分别核父视口或子owner的真实命中。
pub(super) async fn verify_viewport(app:&AppHandle,resource:&PanelResource,observation_id:&str,document_token:&str,node_id:&str,direction:ScrollDirection) -> Result<VerifiedTarget,String> {
    let started=std::time::Instant::now();
    let (snapshot,_)=native_browser_observation::snapshot(app,resource).await?;
    let document=snapshot.top.clone();
    let node=native_browser_nodes::resolve(resource,&snapshot,observation_id,document_token,node_id)?;
    if !node.scope.is_top() {return super::native_browser_scroll::verify_child(app,resource,snapshot,node,direction,started).await;}
    if !node.scope.is_top() || node.role!="RootWebArea" || node.backend_node!=document.backend_root {return Err("native_browser_scroll_target_invalid".into());}
    let tree=native_browser_devtools::read(app,resource,ReadMethod::Accessibility(document.frame_id.clone())).await?;
    current_node(&tree,&document,&node)?;
    let metrics=native_browser_devtools::read(app,resource,ReadMethod::LayoutMetrics).await?;
    let viewport=metrics["cssVisualViewport"].clone();
    let width=viewport["clientWidth"].as_f64().filter(|v|v.is_finite() && *v>2.0 && *v<=32768.0).ok_or("native_browser_target_geometry_invalid")?;
    let height=viewport["clientHeight"].as_f64().filter(|v|v.is_finite() && *v>2.0 && *v<=32768.0).ok_or("native_browser_target_geometry_invalid")?;
    let (x,y)=((width/2.0).round() as i32,(height/2.0).round() as i32);
    let (hit_x,hit_y)=hit_test_point(x,y,&metrics)?;
    let hit=native_browser_devtools::read(app,resource,ReadMethod::HitTest(hit_x,hit_y)).await?;
    let dom=native_browser_devtools::read(app,resource,ReadMethod::DocumentNodes).await?;
    if hit["frameId"].as_str()!=Some(document.frame_id.as_str())
        || !hit["backendNodeId"].as_i64().is_some_and(|id|operable_document_nodes(&dom).contains(&id)) {
        return Err("native_browser_target_hit_mismatch".into());
    }
    let fresh=native_browser_devtools::read(app,resource,ReadMethod::LayoutMetrics).await?;
    if fresh["cssVisualViewport"]!=viewport || native_browser_observation::snapshot(app,resource).await?.0!=snapshot
        || super::browser_panel::input_resource(app).as_ref()!=Some(resource) || started.elapsed()>=std::time::Duration::from_secs(2) {
        return Err("native_browser_target_changed".into());
    }
    Ok(VerifiedTarget {resource:resource.clone(),document,snapshot,node,x,y,viewport,editor:None})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_accepts_only_the_original_control_or_its_passive_regular_descendants() {
        let mut dom = serde_json::json!({"root":{"backendNodeId":1,"nodeName":"#document","children":[
            {"backendNodeId":2,"nodeName":"BUTTON","children":[{"backendNodeId":3,"nodeName":"SPAN",
                "children":[{"backendNodeId":4,"nodeName":"svg","children":[{"backendNodeId":5,"nodeName":"path"}]}]}]},
            {"backendNodeId":6,"nodeName":"DIV"}]}});
        for hit in [2,3,4,5] { assert!(hit_belongs_to_target(&dom,2,hit)); }
        for hit in [0,1,6,99] { assert!(!hit_belongs_to_target(&dom,2,hit)); }
        for attributes in [serde_json::json!(["tabindex","0"]),serde_json::json!(["role","button"]),
            serde_json::json!(["contenteditable","true"])] {
            dom["root"]["children"][0]["children"][0]["attributes"] = attributes;
            assert!(!hit_belongs_to_target(&dom,2,5));
        }
        dom["root"]["children"][0]["children"][0]["attributes"] = serde_json::json!([]);
        dom["root"]["children"][0]["children"][0]["nodeName"] = serde_json::json!("A");
        assert!(!hit_belongs_to_target(&dom,2,5));
        dom["root"]["children"][0]["shadowRoots"] = serde_json::json!([{"backendNodeId":7}]);
        dom["root"]["children"][0]["contentDocument"] = serde_json::json!({"backendNodeId":8});
        for hit in [7,8] { assert!(!hit_belongs_to_target(&dom,2,hit)); }
    }

    #[test]
    fn shadow_child_requires_its_own_binding_and_does_not_inherit_parent_ownership() {
        let mut dom = serde_json::json!({"root":{"backendNodeId":1,"nodeName":"#document","children":[
            {"backendNodeId":2,"nodeName":"BUTTON","children":[{"backendNodeId":3,"nodeName":"SPAN",
                "shadowRoots":[{"backendNodeId":4,"nodeType":11,"shadowRootType":"open","children":[
                    {"backendNodeId":5,"nodeName":"BUTTON","children":[{"backendNodeId":6,"nodeName":"SPAN"}]}]}],
                "contentDocument":{"backendNodeId":7,"children":[{"backendNodeId":8,"nodeName":"BUTTON"}]}}]}]}});
        assert!(hit_belongs_to_target(&dom,5,5));
        assert!(hit_belongs_to_target(&dom,5,6));
        assert!(!hit_belongs_to_target(&dom,2,5));
        assert!(!hit_belongs_to_target(&dom,3,6));
        assert!(!hit_belongs_to_target(&dom,8,8));
        for kind in ["closed","user-agent"] {
            dom["root"]["children"][0]["children"][0]["shadowRoots"][0]["shadowRootType"] = serde_json::json!(kind);
            assert!(!hit_belongs_to_target(&dom,5,5));
        }
    }

    #[test]
    fn geometry_is_css_viewport_relative_and_rejects_clipped_or_degenerate_targets() {
        let model = serde_json::json!({"model":{"content":[100,200,140,200,140,240,100,240]}});
        let viewport = serde_json::json!({"cssVisualViewport":{"pageX":80,"pageY":180,"clientWidth":800,"clientHeight":600}});
        assert_eq!(point(&model, &viewport).unwrap(), (120,220));
        // 文档滚动量不能二次平移已属于视口的 BoxModel；裁剪仍按当前视口边界判定。
        let scrolled = serde_json::json!({"cssVisualViewport":{"pageX":150,"pageY":800,"clientWidth":800,"clientHeight":600}});
        assert_eq!(point(&model, &scrolled).unwrap(), (120,220));
        assert_eq!(hit_test_point(120,220,&scrolled).unwrap(),(270,1020));
        assert!(hit_test_point(120,220,&serde_json::json!({"cssVisualViewport":{"pageX":1e20,"pageY":800}})).is_err());
        let clipped = serde_json::json!({"cssVisualViewport":{"pageX":150,"pageY":180,"clientWidth":110,"clientHeight":600}});
        assert!(point(&model, &clipped).is_err());
        assert!(point(&serde_json::json!({"model":{"content":[1,1,1,1,1,1,1,1]}}), &viewport).is_err());
        assert!(point(&model, &serde_json::json!({})).is_err());
    }

    #[test]
    fn fresh_ax_must_match_backend_role_name_and_interactable_state() {
        let document = DocumentIdentity {frame_id:"frame-a".into(),loader_id:"loader-a".into(),backend_root:1};
        let node = NodeBinding {node_id:"0".repeat(32),index:0,backend_node:9,role:"button".into(),name:"提交".into(),
            scope:super::super::native_browser_document::DocumentScope::top(document.clone())};
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
