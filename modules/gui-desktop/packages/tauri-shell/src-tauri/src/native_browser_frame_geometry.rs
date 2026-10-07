//! 跨进程视口换算与父owner命中。输入始终由原顶层WebView派发，不能把子坐标当顶层坐标。
use super::{
    native_browser_devtools::{self, ReadMethod},
    native_browser_document::DocumentScope,
    native_browser_target,
};
use native_browser_protocol::PanelResource;
use serde_json::Value;
use tauri::AppHandle;

#[derive(PartialEq)]
pub(super) struct MappedPoint {
    pub x: i32,
    pub y: i32,
    pub local_x: i32,
    pub local_y: i32,
    pub local_metrics: Value,
    pub proof: Vec<Value>,
}
fn local_metrics(raw: Value) -> Result<Value, String> {
    let mut layout = raw
        .get("cssLayoutViewport")
        .filter(|v| v.is_object())
        .ok_or("native_browser_frame_geometry_invalid")?
        .clone();
    // OOP的VisualViewport尺寸描述顶层窗口，但pageX/Y来自本session的真实滚动偏移。
    // LayoutViewport只用于子视口尺寸：其矩形起点包含RTL滚动原点，不能当文档偏移。
    for key in ["pageX", "pageY"] {
        let offset = raw["cssVisualViewport"][key].as_f64()
            .filter(|n| n.is_finite() && (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(n))
            .ok_or("native_browser_frame_geometry_invalid")?;
        layout[key] = serde_json::json!(offset);
    }
    Ok(serde_json::json!({"cssVisualViewport":layout}))
}
fn map_owner(
    x: i32,
    y: i32,
    child: &Value,
    geometry: &Value,
    parent: &Value,
) -> Result<(i32, i32), String> {
    let quad = geometry
        .pointer("/model/content")
        .and_then(Value::as_array)
        .filter(|q| q.len() == 8)
        .ok_or("native_browser_frame_geometry_invalid")?;
    let q = quad
        .iter()
        .map(|v| v.as_f64().filter(|n| n.is_finite()))
        .collect::<Option<Vec<_>>>()
        .ok_or("native_browser_frame_geometry_invalid")?;
    // 先闭环真实验证过的矩形/缩放owner。旋转、斜切和透视另列验收，不能用偏移猜测坐标。
    if (q[1] - q[3]).abs() > 0.5
        || (q[3] - q[5]).abs() < 2.0
        || (q[5] - q[7]).abs() > 0.5
        || (q[0] - q[6]).abs() > 0.5
        || (q[2] - q[4]).abs() > 0.5
        || q[2] - q[0] < 2.0
        || q[5] - q[1] < 2.0
    {
        return Err("native_browser_frame_transform_unsupported".into());
    }
    let width = child["cssVisualViewport"]["clientWidth"]
        .as_f64()
        .filter(|n| n.is_finite() && *n > 2.0)
        .ok_or("native_browser_frame_geometry_invalid")?;
    let height = child["cssVisualViewport"]["clientHeight"]
        .as_f64()
        .filter(|n| n.is_finite() && *n > 2.0)
        .ok_or("native_browser_frame_geometry_invalid")?;
    if x <= 0 || y <= 0 || f64::from(x) >= width || f64::from(y) >= height {
        return Err("native_browser_target_outside_viewport".into());
    }
    let x = (q[0] + f64::from(x) * (q[2] - q[0]) / width).round();
    let y = (q[1] + f64::from(y) * (q[7] - q[1]) / height).round();
    let viewport = &parent["cssVisualViewport"];
    let w = viewport["clientWidth"]
        .as_f64()
        .filter(|n| n.is_finite() && *n <= 32768.0)
        .ok_or("native_browser_frame_geometry_invalid")?;
    let h = viewport["clientHeight"]
        .as_f64()
        .filter(|n| n.is_finite() && *n <= 32768.0)
        .ok_or("native_browser_frame_geometry_invalid")?;
    if x < 1.0 || y < 1.0 || x >= w - 1.0 || y >= h - 1.0 {
        return Err("native_browser_target_outside_viewport".into());
    }
    Ok((x as i32, y as i32))
}

pub(super) async fn locate(
    app: &AppHandle,
    resource: &PanelResource,
    scope: &DocumentScope,
    geometry: &Value,
    top_metrics: &Value,
) -> Result<MappedPoint, String> {
    let raw = native_browser_devtools::read_session(
        app,
        resource,
        scope.session(),
        ReadMethod::LayoutMetrics,
    )
    .await?;
    let initial = local_metrics(raw)?;
    let (local_x, local_y) = native_browser_target::point(geometry, &initial)?;
    map_point(app, resource, scope, initial, local_x, local_y, top_metrics).await
}

/// 独立目标根文档的滚动点来自该目标的布局视口，父owner仍逐层核对。
pub(super) async fn viewport_center(
    app: &AppHandle, resource: &PanelResource, scope: &DocumentScope, top_metrics: &Value,
) -> Result<MappedPoint, String> {
    let initial = local_metrics(native_browser_devtools::read_session(
        app, resource, scope.session(), ReadMethod::LayoutMetrics).await?)?;
    let width = initial["cssVisualViewport"]["clientWidth"].as_f64().ok_or("native_browser_frame_geometry_invalid")?;
    let height = initial["cssVisualViewport"]["clientHeight"].as_f64().ok_or("native_browser_frame_geometry_invalid")?;
    let geometry = serde_json::json!({"model":{"content":[0.0,0.0,width,0.0,width,height,0.0,height]}});
    let (x, y) = native_browser_target::point(&geometry, &initial)?;
    map_point(app, resource, scope, initial, x, y, top_metrics).await
}

async fn map_point(
    app: &AppHandle, resource: &PanelResource, scope: &DocumentScope, initial: Value,
    local_x: i32, local_y: i32, top_metrics: &Value,
) -> Result<MappedPoint, String> {
    let (mut x, mut y) = (local_x, local_y);
    let mut metrics = initial.clone();
    let mut proof = Vec::new();
    for index in (1..scope.chain.len()).rev() {
        let child = &scope.chain[index];
        let parent = &scope.chain[index - 1];
        if child.session == parent.session {
            continue;
        }
        let parent_metrics = match parent.session.as_deref() {
            None => top_metrics.clone(),
            session => local_metrics(
                native_browser_devtools::read_session(
                    app,
                    resource,
                    session,
                    ReadMethod::LayoutMetrics,
                )
                .await?,
            )?,
        };
        let owner = child.owner.ok_or("native_browser_frame_geometry_invalid")?;
        let owner_box = native_browser_devtools::read_session(
            app,
            resource,
            parent.session.as_deref(),
            ReadMethod::BoxModel(owner),
        )
        .await?;
        (x, y) = map_owner(x, y, &metrics, &owner_box, &parent_metrics)?;
        let (hx, hy) = native_browser_target::hit_test_point(x, y, &parent_metrics)?;
        let hit = native_browser_devtools::read_session(
            app,
            resource,
            parent.session.as_deref(),
            ReadMethod::HitTest(hx, hy),
        )
        .await?;
        if hit["frameId"].as_str() != Some(parent.document.frame_id.as_str())
            || hit["backendNodeId"].as_i64() != Some(owner)
        {
            return Err("native_browser_target_hit_mismatch".into());
        }
        proof.push(serde_json::json!({"child_viewport":metrics["cssVisualViewport"],"owner":owner_box["model"]["content"],"parent_viewport":parent_metrics["cssVisualViewport"]}));
        metrics = parent_metrics;
    }
    Ok(MappedPoint {
        x,
        y,
        local_x,
        local_y,
        local_metrics: initial,
        proof,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rtl_child_uses_local_dimensions_and_signed_document_offset() {
        // 真实OOP RTL回执：布局起点1409，滚动偏移0，顶层视口宽446，子视口宽391。
        for offset in [0.0_f64, -195.3333282470703, -600.0] {
            let raw = serde_json::json!({
                "cssLayoutViewport":{"clientWidth":391,"clientHeight":284,"pageX":1409.0+offset,"pageY":0},
                "cssVisualViewport":{"clientWidth":446,"clientHeight":522,"pageX":offset,"pageY":0}});
            let metrics = local_metrics(raw).unwrap();
            assert_eq!(metrics["cssVisualViewport"]["clientWidth"], 391);
            assert_eq!(native_browser_target::hit_test_point(196, 142, &metrics).unwrap(),
                ((196.0+offset).round() as i32,142));
        }
        assert!(local_metrics(serde_json::json!({"cssLayoutViewport":{},"cssVisualViewport":{"pageY":0}})).is_err());
    }
    #[test]
    fn child_point_uses_owner_content_and_local_layout_including_scale() {
        let child = serde_json::json!({"cssVisualViewport":{"clientWidth":400,"clientHeight":320}});
        let parent =
            serde_json::json!({"cssVisualViewport":{"clientWidth":800,"clientHeight":600}});
        let mut box_model =
            serde_json::json!({"model":{"content":[22,202,422,202,422,522,22,522]}});
        assert_eq!(
            map_owner(65, 145, &child, &box_model, &parent).unwrap(),
            (87, 347)
        );
        box_model["model"]["content"] = serde_json::json!([22, 202, 222, 202, 222, 362, 22, 362]);
        assert_eq!(
            map_owner(66, 146, &child, &box_model, &parent).unwrap(),
            (55, 275)
        );
        assert!(map_owner(66, 330, &child, &box_model, &parent).is_err());
        box_model["model"]["content"] = serde_json::json!([22, 202, 422, 222, 402, 522, 2, 502]);
        assert!(map_owner(65, 145, &child, &box_model, &parent).is_err());
    }
}
