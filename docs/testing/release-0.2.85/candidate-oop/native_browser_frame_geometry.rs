//! 跨进程视口换算与父owner命中。输入始终由原顶层WebView派发，不能把子坐标当顶层坐标。
use super::{
    native_browser_devtools::{self, ReadMethod},
    native_browser_document::DocumentScope,
    native_browser_target,
};
use native_browser_protocol::PanelResource;
use serde_json::Value;
use tauri::AppHandle;

pub(super) struct MappedPoint {
    pub x: i32,
    pub y: i32,
    pub local_x: i32,
    pub local_y: i32,
    pub local_metrics: Value,
    pub proof: Vec<Value>,
}
fn local_metrics(raw: Value) -> Result<Value, String> {
    let layout = raw
        .get("cssLayoutViewport")
        .filter(|v| v.is_object())
        .ok_or("native_browser_frame_geometry_invalid")?
        .clone();
    // OOP的VisualViewport仍描述顶层窗口；子根边界和文档偏移要使用本session的LayoutViewport。
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
