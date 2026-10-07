//! 子文档滚动预检：真实滚动根、owner几何及命中；执行仍复用原wheel与许可。
use super::{
    native_browser_devtools::{self, ReadMethod},
    native_browser_document::{self, DocumentScope, DocumentSnapshot},
    native_browser_frame_geometry::{self, MappedPoint},
    native_browser_nodes::NodeBinding,
    native_browser_observation,
    native_browser_target::{self, VerifiedTarget},
};
use native_browser_protocol::{PanelResource, ScrollDirection};
use serde_json::Value;
use std::time::{Duration, Instant};
use tauri::AppHandle;

// 固定宿主只读函数；不设置滚动位置，不接受网页或模型提供的脚本。
pub(super) const READ_SCROLL: &str = r#"function(){
 const e=this.scrollingElement;if(!e)return null;
 return {width:e.clientWidth,height:e.clientHeight,extent:e.scrollHeight,top:e.scrollTop};
}"#;

async fn read_state(app: &AppHandle, resource: &PanelResource, scope: &DocumentScope, root: i64) -> Result<Value, String> {
    // 复用宿主固定节点对象解析/释放；不读取编辑器原值或调用编辑状态函数。
    let raw = native_browser_devtools::read_session(app, resource, scope.session(), ReadMethod::ResolveEditor(root)).await?;
    let object = raw
        .pointer("/object/objectId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= 256)
        .ok_or("native_browser_scroll_state_unavailable")?
        .to_owned();
    let result = native_browser_devtools::read_session(
        app,
        resource,
        scope.session(),
        ReadMethod::DocumentScrollState(object.clone()),
    )
    .await;
    let _ = native_browser_devtools::read_session(app, resource, scope.session(), ReadMethod::ReleaseEditor(object)).await;
    let raw = result?;
    if raw.get("exceptionDetails").is_some() {
        return Err("native_browser_scroll_read_rejected".into());
    }
    let value = raw
        .pointer("/result/value")
        .filter(|v| v.is_object())
        .ok_or("native_browser_scroll_state_unavailable")?;
    for key in ["width", "height", "extent", "top"] {
        if !value[key]
            .as_f64()
            .is_some_and(|n| n.is_finite() && (0.0..=2_147_483_647.0).contains(&n))
        {
            return Err("native_browser_scroll_state_unavailable".into());
        }
    }
    if value["width"].as_f64().unwrap() <= 2.0 || value["height"].as_f64().unwrap() <= 2.0 {
        return Err("native_browser_scroll_state_unavailable".into());
    }
    Ok(value.clone())
}

async fn scroll_point(app: &AppHandle, resource: &PanelResource, scope: &DocumentScope, metrics: &Value) -> Result<MappedPoint, String> {
    let child = scope.chain.last().ok_or("native_browser_scroll_target_invalid")?;
    let parent = scope.chain.iter().rev().nth(1).ok_or("native_browser_scroll_target_invalid")?;
    if child.session.is_some() && child.session != parent.session {
        return native_browser_frame_geometry::viewport_center(app, resource, scope, metrics).await;
    }
    // 同一目标会话内的子Frame用其owner中心；不能误用所属目标根的整个视口。
    let owner = child.owner.ok_or("native_browser_scroll_target_invalid")?;
    let geometry = native_browser_devtools::read_session(app, resource, parent.session.as_deref(), ReadMethod::BoxModel(owner)).await?;
    let mut point = if scope.session().is_some() {
        native_browser_frame_geometry::locate(app, resource, scope, &geometry, metrics).await?
    } else {
        let (x, y) = native_browser_target::point(&geometry, metrics)?;
        MappedPoint {x,y,local_x:x,local_y:y,local_metrics:metrics.clone(),proof:Vec::new()}
    };
    point.proof.push(geometry["model"]["content"].clone());
    Ok(point)
}

pub(super) async fn verify_child(
    app: &AppHandle,
    resource: &PanelResource,
    snapshot: DocumentSnapshot,
    node: NodeBinding,
    direction: ScrollDirection,
    started: Instant,
) -> Result<VerifiedTarget, String> {
    if !matches!(direction, ScrollDirection::Up | ScrollDirection::Down) {
        return Err("native_browser_scroll_axis_unsupported".into());
    }
    if node.scope.is_top()
        || node.role != "RootWebArea"
        || node.backend_node != node.scope.document().backend_root
    {
        return Err("native_browser_scroll_target_invalid".into());
    }
    let tree = native_browser_devtools::read_session(
        app,
        resource,
        node.scope.session(),
        ReadMethod::Accessibility(node.scope.document().frame_id.clone()),
    )
    .await?;
    native_browser_target::current_node(&tree, node.scope.document(), &node)?;
    let metrics = native_browser_devtools::read(app, resource, ReadMethod::LayoutMetrics).await?;
    let point = scroll_point(app, resource, &node.scope, &metrics).await?;
    let state = read_state(app, resource, &node.scope, node.backend_node).await?;
    let top = state["top"].as_f64().unwrap();
    let remaining = if direction == ScrollDirection::Up {
        top
    } else {
        (state["extent"].as_f64().unwrap() - state["height"].as_f64().unwrap() - top).max(0.0)
    };
    if remaining <= 0.0 {
        return Err("native_browser_scroll_boundary".into());
    }
    let (hit_x, hit_y) = native_browser_target::hit_test_point(point.local_x, point.local_y, &point.local_metrics)?;
    let hit =
        native_browser_devtools::read_session(app, resource, node.scope.session(), ReadMethod::HitTest(hit_x, hit_y)).await?;
    let (fresh, dom) = native_browser_observation::snapshot(app, resource).await?;
    if fresh != snapshot {
        return Err("native_browser_document_changed".into());
    }
    let root = native_browser_document::root_for_scope(&dom, &node.scope)?;
    if hit["frameId"].as_str() != Some(node.scope.document().frame_id.as_str())
        || !hit["backendNodeId"]
            .as_i64()
            .is_some_and(|id| super::native_browser_dom::operable_root_nodes(root).contains(&id))
    {
        return Err("native_browser_target_hit_mismatch".into());
    }
    let second_metrics =
        native_browser_devtools::read(app, resource, ReadMethod::LayoutMetrics).await?;
    if scroll_point(app, resource, &node.scope, &second_metrics).await? != point
        || second_metrics["cssVisualViewport"] != metrics["cssVisualViewport"]
        || read_state(app, resource, &node.scope, node.backend_node).await? != state
        || native_browser_observation::snapshot(app, resource).await?.0 != snapshot
        || super::browser_panel::input_resource(app).as_ref() != Some(resource)
        || started.elapsed() >= Duration::from_secs(2)
    {
        return Err("native_browser_target_changed".into());
    }
    // 只供原wheel计算距离及票据重检；不把数值作为模型目标达成证据。
    let mut viewport = second_metrics["cssVisualViewport"].clone();
    viewport["clientWidth"] = state["width"].clone();
    viewport["clientHeight"] = state["height"].clone();
    viewport["wheel_limit"] = serde_json::json!(remaining);
    viewport["scroll_state"] = state;
    Ok(VerifiedTarget {
        resource: resource.clone(),
        document: snapshot.top.clone(),
        snapshot,
        node,
        x:point.x,
        y:point.y,
        viewport,
        editor: None,
    })
}
