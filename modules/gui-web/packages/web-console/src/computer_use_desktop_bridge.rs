use std::time::Duration;
use std::sync::Arc;
use base64::Engine;

use computer_use::{
    ComputerUseAction, ComputerUseActionKind, ComputerUseError, ComputerUseRetryOwner, Observation,
    StepExecution, Verification,
};
use serde_json::{json, Value as JsonValue};
use uia_resolver::{snapshot_foreground_window, UiaElementSnapshot, UiaError};

use crate::computer_use_adapters::{DesktopBridge, DesktopSnapshot};

const INPUT_TIMEOUT: Duration = Duration::from_secs(8);
const UIA_ELEMENT_LIMIT: usize = 500;

#[derive(Clone)]
pub(crate) struct DesktopNativeBridge {
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Default for DesktopNativeBridge {
    fn default() -> Self { Self { cancelled: Arc::new(|| false) } }
}

impl std::fmt::Debug for DesktopNativeBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("DesktopNativeBridge").finish_non_exhaustive() }
}

impl DesktopNativeBridge {
    pub(crate) fn with_cancelled(cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self { Self { cancelled } }
    pub(crate) fn preflight() -> Result<(), ComputerUseError> {
        let input = computer_use::input::preflight_report();
        if !input.ready {
            return Err(backend_error(format!(
                "desktop input backend is not ready: {}",
                input.detail
            )));
        }
        snapshot_foreground_window(1).map_err(map_uia_error)?;
        Ok(())
    }
}

impl DesktopBridge for DesktopNativeBridge {
    fn snapshot(
        &self,
        request: &computer_use::ComputerUseRequest,
    ) -> Result<DesktopSnapshot, ComputerUseError> {
        let target = request.target.as_ref();
        let application = target.and_then(|target| target.application.as_deref());
        let window = target.and_then(|target| target.window.as_deref());
        if uia_resolver::focus_window_by_hint(application, window, Some(&request.objective))
            .map_err(map_uia_error)?
            .is_some()
        {
            std::thread::sleep(Duration::from_millis(120));
        }
        let snapshot = snapshot_foreground_window(UIA_ELEMENT_LIMIT).map_err(map_uia_error)?;
        let webview2_overlay = snapshot.elements.iter().any(|element| {
            element.class_name.as_deref().is_some_and(|class_name| {
                let class_name = class_name.to_ascii_lowercase();
                class_name.contains("webview2")
                    || class_name.contains("chrome_renderwidgethosthwnd")
            })
        });
        let elements = snapshot
            .elements
            .iter()
            .map(element_json)
            .collect::<Vec<_>>();
        let window_id = format!("hwnd-{:x}", snapshot.native_window_handle);
        let identity = computer_use::input::StrokeWindow {
            handle: snapshot.native_window_handle, process_id: snapshot.process_id,
            rect: [snapshot.bounding_rect.x, snapshot.bounding_rect.y, snapshot.bounding_rect.width, snapshot.bounding_rect.height], dpi: snapshot.dpi,
        };
        let image = capture_image(identity)?;
        let canvas_rect = intersect_rect(rect_from_element(&json!({"rect":image["client_rect"]}))?, rect_from_element(&json!({"rect":image["screen_rect"]}))?)?;
        let image_evidence = image_evidence(&image);
        Ok(DesktopSnapshot {
            window_id: window_id.clone(),
            process_id: snapshot.process_id,
            window_rect: [
                snapshot.bounding_rect.x,
                snapshot.bounding_rect.y,
                snapshot.bounding_rect.width,
                snapshot.bounding_rect.height,
            ],
            dpi: snapshot.dpi,
            webview2_overlay,
            state: json!({
                "window": {
                    "reference": format!("uia-window-{:x}", snapshot.native_window_handle),
                    "name": snapshot.name,
                    "process_id": snapshot.process_id,
                    "native_window_handle": snapshot.native_window_handle,
                },
                "elements": elements,
                "image": image,
                "canvas_target": format!("window-canvas:{:x}", snapshot.native_window_handle),
                "canvas_rect": canvas_rect,
                "drag_contract": {
                    "kind": "drag", "target": "当前 UIA 画布 reference，或 canvas_target（相对 canvas_rect，不是整张截图）",
                    "points": "2–256 个 [x,y]，坐标均为 0..1，相对目标 rect；使用当前截图，不猜测或复用旧位置",
                    "coordinate_space": "所有 rect 是桌面物理像素 [x,y,width,height]；截图像素原点对应 image.screen_rect，归一化笔画相对目标 rect",
                    "fallback_scope": "window-canvas 仅代表可见 client 区域，包含工具栏、菜单和状态区，不等于语义绘画画布；必须依据当前原图和 rect 找到其中实际可绘画区域，不能猜位置",
                    "duration_ms": "0..5000", "button": "left", "cancel": "宿主取消或 Escape 中止笔画并尝试释放鼠标，释放失败明确报错",
                },
            }),
            evidence: vec![image_evidence, format!(
                "uia_snapshot:{window_id}:elements={}",
                snapshot.elements.len()
            )],
        })
    }

    fn execute(
        &self,
        action: &ComputerUseAction,
        expected: &DesktopSnapshot,
    ) -> Result<StepExecution, ComputerUseError> {
        if (self.cancelled)() { return Err(cancelled_error()); }
        let screenshot_target = action.kind == ComputerUseActionKind::Drag
            && expected.state.get("canvas_target").and_then(JsonValue::as_str) == Some(action.target.as_str());
        let fallback;
        let element = if screenshot_target {
            fallback = json!({"rect":expected.state["canvas_rect"],"enabled":true,"offscreen":false,"control_type":"ScreenshotCanvas"});
            &fallback
        } else { find_element(&expected.state, &action.target)? };
        if !element
            .get("enabled")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            return Err(blocked("target_disabled", "target element is disabled"));
        }
        if element
            .get("offscreen")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true)
        {
            return Err(blocked("target_offscreen", "target element is offscreen"));
        }
        let rect = rect_from_element(element)?;
        let x = rect[0].saturating_add(rect[2] / 2);
        let y = rect[1].saturating_add(rect[3] / 2);
        let native_window_handle = expected
            .state
            .pointer("/window/native_window_handle")
            .and_then(JsonValue::as_i64)
            .and_then(|value| isize::try_from(value).ok())
            .ok_or_else(|| stale("foreground window identity is missing"))?;
        uia_resolver::focus_window(native_window_handle).map_err(map_uia_error)?;

        match action.kind {
            ComputerUseActionKind::Click => {
                computer_use::input::click_point(x, y, 1, INPUT_TIMEOUT).map_err(input_error)?;
            }
            ComputerUseActionKind::DoubleClick => {
                computer_use::input::click_point(x, y, 2, INPUT_TIMEOUT).map_err(input_error)?;
            }
            ComputerUseActionKind::TextInput => {
                let control_type = element
                    .get("control_type")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("");
                if !matches!(control_type, "Edit" | "Document") {
                    return Err(blocked(
                        "target_not_editable",
                        "text input requires an Edit or Document UIA control",
                    ));
                }
                let text = action
                    .arguments
                    .get("text")
                    .and_then(JsonValue::as_str)
                    .filter(|text| !text.is_empty() && text.len() <= 4_000)
                    .ok_or_else(|| blocked("invalid_text", "text input is empty or too long"))?;
                computer_use::input::click_point(x, y, 1, INPUT_TIMEOUT).map_err(input_error)?;
                computer_use::input::type_text(text, INPUT_TIMEOUT).map_err(input_error)?;
            }
            ComputerUseActionKind::Scroll => {
                let direction = action
                    .arguments
                    .get("direction")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("down");
                let amount = action
                    .arguments
                    .get("amount")
                    .and_then(JsonValue::as_i64)
                    .unwrap_or(1)
                    .clamp(1, 5) as i32;
                let sign = if matches!(direction, "up" | "left") {
                    1
                } else {
                    -1
                };
                computer_use::input::click_point(x, y, 1, INPUT_TIMEOUT).map_err(input_error)?;
                computer_use::input::scroll_wheel(sign * amount * 120, INPUT_TIMEOUT)
                    .map_err(input_error)?;
            }
            ComputerUseActionKind::KeyCombination => {
                let keys = action
                    .arguments
                    .get("keys")
                    .and_then(JsonValue::as_array)
                    .ok_or_else(|| blocked("invalid_keys", "keys array is missing"))?;
                let virtual_keys = keys
                    .iter()
                    .map(|key| key.as_str().and_then(virtual_key))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| blocked("invalid_keys", "key combination is not allowlisted"))?;
                computer_use::input::send_virtual_key_combo(&virtual_keys, INPUT_TIMEOUT)
                    .map_err(input_error)?;
            }
            ComputerUseActionKind::Drag => {
                if !screenshot_target && !canvas_element(element) {
                    return Err(blocked("target_not_canvas", "笔画目标应为 UIA 画布或当前截图的 canvas_target"));
                }
                let (points, duration_ms) = stroke_arguments(&action.arguments, rect)?;
                let visible_rect = rect_from_element(&json!({"rect":expected.state["image"]["screen_rect"]}))?;
                if intersect_rect(rect, visible_rect)? != rect { return Err(blocked("target_offscreen", "画布边界部分位于截图之外，需调整窗口后重新观察")); }
                let identity = computer_use::input::StrokeWindow { handle: native_window_handle, process_id: expected.process_id, rect: expected.window_rect, dpi: expected.dpi };
                computer_use::input::validate_stroke(identity, rect, &points, duration_ms).map_err(|error| blocked("invalid_stroke_path", error))?;
                let result = computer_use::input::controlled_drag_path(identity, rect, &points, duration_ms, Duration::from_secs(10), &*self.cancelled);
                // 成功和可恢复失败都尝试留后图；截图本身不会移动鼠标或改变画布。
                let after = capture_image(identity);
                if let Err(error) = result {
                    let evidence = after.as_ref().map(image_evidence).unwrap_or_else(|error| format!("after_capture_failed:{error:?}"));
                    return Err(if error.contains("mouse_release_failed") { ComputerUseError::new("mouse_release_failed", format!("无法确认鼠标释放：{error};{evidence}"), false, ComputerUseRetryOwner::None) }
                        else if error.contains("stroke_cancelled") { ComputerUseError::new("cancelled", format!("笔画已取消；{evidence}"), false, ComputerUseRetryOwner::None) }
                        else if error.contains("stale_observation") { stale(format!("{error};{evidence}")) }
                        else { input_error(format!("{error};{evidence}")) });
                }
                return Ok(completed_stroke_execution(&action.target, points.len(), duration_ms, &expected.state["image"], after));
            }
            _ => {
                return Err(blocked(
                    "unsupported_action",
                    "desktop bridge does not implement the requested action",
                ));
            }
        }
        Ok(StepExecution {
            input_sent: true,
            summary: format!("desktop {:?} sent to {}", action.kind, action.target),
            evidence: vec![format!("sendinput:{:?}:{}", action.kind, action.target)],
        })
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
    ) -> Result<Verification, ComputerUseError> {
        let image_changed = before.state.pointer("/image/sha256").zip(after.state.pointer("/image/sha256")).is_some_and(|(a,b)| a != b);
        // 截图路径、编码与时间戳不参与 UIA 文本成功匹配，避免证据元数据伪装成成功。
        let before_desktop = before.state.get("desktop").unwrap_or(&before.state);
        let after_desktop = after.state.get("desktop").unwrap_or(&after.state);
        let visible_progress = image_changed || before_desktop.get("elements") != after_desktop.get("elements");
        let corpus = after_desktop.get("elements").unwrap_or(&JsonValue::Null).to_string().to_lowercase();
        let achieved = !criteria.is_empty()
            && criteria
                .iter()
                .all(|criterion| criterion_visible(criterion, &corpus));
        Ok(Verification {
            achieved,
            visible_progress,
            summary: if achieved {
                "desktop success criteria are visible in the fresh UIA snapshot".to_string()
            } else if visible_progress {
                "desktop UI changed but success criteria are not all visible".to_string()
            } else {
                "desktop UIA snapshot shows no visible progress".to_string()
            },
            evidence: after.evidence.iter().cloned().chain(std::iter::once(format!("image_changed:{image_changed}"))).collect(),
        })
    }
}

fn cancelled_error() -> ComputerUseError { ComputerUseError::new("cancelled", "桌面输入开始前已取消", false, ComputerUseRetryOwner::None) }

/// 原生输入已经成功后，后截图失败不能抹掉输入事实；控制器仍需重新观察和视觉验收。
fn completed_stroke_execution(target: &str, count: usize, duration_ms: u64, before: &JsonValue, after: Result<JsonValue,ComputerUseError>) -> StepExecution {
    let mut evidence = vec![format!("native_stroke:{target}:points={count}:duration_ms={duration_ms}:released"),format!("before:{}",image_evidence(before))];
    let summary = match after {
        Ok(image) => {
            evidence.push(format!("after:{}",image_evidence(&image)));
            evidence.push(format!("image_changed:{}",before["sha256"] != image["sha256"]));
            format!("受控画布笔画输入完成：{count} 点，鼠标已释放；画布结果等待验收")
        }
        Err(error) => {
            evidence.push(format!("after_capture_failed:{}",error.code));
            format!("受控画布笔画输入完成：{count} 点，鼠标已释放；后截图失败，尚未确认画布结果")
        }
    };
    StepExecution { input_sent:true,summary,evidence }
}

fn capture_image(identity: computer_use::input::StrokeWindow) -> Result<JsonValue, ComputerUseError> {
    let mut image = computer_use::input::capture_window_image(identity, Duration::from_secs(10)).map_err(|error| {
        if error.contains("stale_observation") { stale(error) }
        else if error.contains("stroke_cancelled") { ComputerUseError::new("cancelled", error, false, ComputerUseRetryOwner::None) }
        else { input_error(error) }
    })?;
    let data = image.get("data_url").and_then(JsonValue::as_str).and_then(|value| value.strip_prefix("data:image/png;base64,")).ok_or_else(|| backend_error("截图缺少 PNG 像素"))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|_| backend_error("截图 PNG 编码无效"))?;
    let hash = image.get("sha256").and_then(JsonValue::as_str).filter(|value| value.len()==64 && value.bytes().all(|c|c.is_ascii_hexdigit())).ok_or_else(|| backend_error("截图 hash 无效"))?;
    let directory = vision::default_latest_desktop_capture_dir().join("computer-use");
    std::fs::create_dir_all(&directory).map_err(|error| backend_error(format!("截图证据目录不可用: {error}")))?;
    let path = directory.join(format!("window-{}-{hash}.png",identity.process_id));
    if !path.exists() { std::fs::write(&path,bytes).map_err(|error| backend_error(format!("截图证据写入失败: {error}")))?; }
    image["path"] = json!(path.to_string_lossy());
    Ok(image)
}

fn image_evidence(image: &JsonValue) -> String {
    format!("screenshot:{}:sha256={}:{}x{}", image["path"].as_str().unwrap_or(""), image["sha256"].as_str().unwrap_or(""), image["width"], image["height"])
}

fn canvas_element(element: &JsonValue) -> bool {
    matches!(element["control_type"].as_str(),Some("Document") | Some("Image"))
        || ["name","class_name","automation_id"].iter().filter_map(|key|element[key].as_str()).any(|value| {
            let value=value.to_lowercase();value.contains("canvas")||value.contains("画布")||value.contains("mspaintview")
        })
}

fn stroke_arguments(arguments: &JsonValue, rect: [i32;4]) -> Result<(Vec<computer_use::input::MousePoint>,u64),ComputerUseError> {
    if rect[2]<=0 || rect[3]<=0 || rect[0].checked_add(rect[2]).is_none() || rect[1].checked_add(rect[3]).is_none() { return Err(blocked("invalid_stroke_bounds", "画布边界无效或溢出")); }
    let values=arguments["points"].as_array().filter(|p|(2..=256).contains(&p.len())).ok_or_else(||blocked("invalid_stroke_path","points 应为 2–256 个相对坐标点"))?;
    let duration=arguments.get("duration_ms").map(|v|v.as_u64()).unwrap_or(Some(600)).filter(|v|*v<=5000).ok_or_else(||blocked("invalid_stroke_duration","duration_ms 应为 0–5000"))?;
    let points=values.iter().map(|point| {
        let point=point.as_array().filter(|p|p.len()==2).ok_or_else(||blocked("invalid_stroke_point","每个点应为 [x,y]"))?;
        let axis=|i:usize|point[i].as_f64().filter(|v|v.is_finite()&&(0.0..=1.0).contains(v)).ok_or_else(||blocked("invalid_stroke_point","相对坐标必须在 0..1 内"));
        Ok(computer_use::input::MousePoint {x:rect[0]+(axis(0)?*f64::from(rect[2]-1)).round() as i32,y:rect[1]+(axis(1)?*f64::from(rect[3]-1)).round() as i32})
    }).collect::<Result<Vec<_>,ComputerUseError>>()?;
    Ok((points,duration))
}

fn intersect_rect(a:[i32;4],b:[i32;4])->Result<[i32;4],ComputerUseError> {
    let x=i64::from(a[0].max(b[0]));let y=i64::from(a[1].max(b[1]));
    let right=(i64::from(a[0])+i64::from(a[2])).min(i64::from(b[0])+i64::from(b[2]));
    let bottom=(i64::from(a[1])+i64::from(a[3])).min(i64::from(b[1])+i64::from(b[3]));
    if right<=x || bottom<=y { return Err(blocked("target_offscreen", "目标没有可见屏幕区域")); }
    Ok([x as i32,y as i32,i32::try_from(right-x).map_err(|_|stale("rectangle overflow"))?,i32::try_from(bottom-y).map_err(|_|stale("rectangle overflow"))?])
}

fn element_json(element: &UiaElementSnapshot) -> JsonValue {
    json!({
        "reference": element.reference,
        "name": element.name,
        "automation_id": element.automation_id,
        "class_name": element.class_name,
        "control_type": element.control_type,
        "value": element.value,
        "rect": [
            element.bounding_rect.x,
            element.bounding_rect.y,
            element.bounding_rect.width,
            element.bounding_rect.height,
        ],
        "offscreen": element.is_offscreen,
        "enabled": element.is_enabled,
    })
}

fn find_element<'a>(
    state: &'a JsonValue,
    reference: &str,
) -> Result<&'a JsonValue, ComputerUseError> {
    let matches = state
        .get("elements")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter(|element| element.get("reference").and_then(JsonValue::as_str) == Some(reference))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [element] => Ok(*element),
        [] => Err(blocked(
            "target_not_found",
            "UIA reference is not present in the expected snapshot",
        )),
        _ => Err(blocked(
            "target_ambiguous",
            "UIA reference is not unique in the expected snapshot",
        )),
    }
}

fn rect_from_element(element: &JsonValue) -> Result<[i32; 4], ComputerUseError> {
    let values = element
        .get("rect")
        .and_then(JsonValue::as_array)
        .filter(|values| values.len() == 4)
        .ok_or_else(|| stale("UIA target rectangle is missing"))?;
    let mut rect = [0i32; 4];
    for (index, value) in values.iter().enumerate() {
        rect[index] = value
            .as_i64()
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| stale("UIA target rectangle is invalid"))?;
    }
    if rect[2] <= 0 || rect[3] <= 0 {
        return Err(blocked(
            "target_offscreen",
            "UIA target has no visible bounds",
        ));
    }
    Ok(rect)
}

fn criterion_visible(criterion: &str, corpus: &str) -> bool {
    let normalized = criterion.trim().to_lowercase();
    if normalized.len() >= 4 && corpus.contains(&normalized) {
        return true;
    }
    let tokens = normalized
        .split(|character: char| {
            !character.is_alphanumeric() && character != '-' && character != '_'
        })
        .filter(|token| token.len() >= 4)
        .filter(|token| {
            !matches!(
                *token,
                "visible"
                    | "result"
                    | "success"
                    | "window"
                    | "desktop"
                    | "shows"
                    | "uia"
                    | "element"
                    | "field"
                    | "input"
                    | "value"
                    | "text"
                    | "document"
                    | "edit"
                    | "contains"
                    | "contain"
                    | "记事本"
                    | "文本区域"
                    | "文本编辑区域"
                    | "输入框"
                    | "文本"
                    | "包含"
            )
        })
        .collect::<Vec<_>>();
    let evidence_tokens = tokens
        .iter()
        .copied()
        .filter(|token| is_strong_evidence_token(token))
        .collect::<Vec<_>>();
    if !evidence_tokens.is_empty() {
        return evidence_tokens.iter().all(|token| corpus.contains(*token));
    }
    !tokens.is_empty() && tokens.iter().all(|token| corpus.contains(*token))
}

fn is_strong_evidence_token(token: &str) -> bool {
    let has_digit = token.chars().any(|character| character.is_ascii_digit());
    let has_ascii_alpha = token
        .chars()
        .any(|character| character.is_ascii_alphabetic());
    let has_separator = token.contains('-') || token.contains('_');
    token.starts_with("coolzhu-")
        || (token.len() >= 12 && has_digit && has_ascii_alpha && has_separator)
        || (token.len() >= 24 && has_digit && has_ascii_alpha)
}

fn virtual_key(value: &str) -> Option<u8> {
    Some(match value.to_ascii_lowercase().as_str() {
        "ctrl" => 0x11,
        "shift" => 0x10,
        "alt" => 0x12,
        "enter" => 0x0D,
        "escape" => 0x1B,
        "tab" => 0x09,
        "home" => 0x24,
        "end" => 0x23,
        "a" => 0x41,
        "c" => 0x43,
        "l" => 0x4C,
        "v" => 0x56,
        "x" => 0x58,
        "y" => 0x59,
        "z" => 0x5A,
        _ => return None,
    })
}

fn map_uia_error(error: UiaError) -> ComputerUseError {
    match error {
        UiaError::ElementNotFound => blocked("target_not_found", "UIA target was not found"),
        UiaError::ElementAmbiguous => blocked("target_ambiguous", "UIA target is ambiguous"),
        UiaError::UnsupportedPlatform => backend_error("desktop bridge requires Windows"),
        UiaError::ComInitFailed(message) | UiaError::QueryError(message) => backend_error(message),
    }
}

fn input_error(message: String) -> ComputerUseError {
    ComputerUseError::new("input_failed", message, true, ComputerUseRetryOwner::System)
}

fn stale(message: impl Into<String>) -> ComputerUseError {
    ComputerUseError::recoverable("stale_observation", message)
}

fn blocked(code: &'static str, message: impl Into<String>) -> ComputerUseError {
    ComputerUseError::blocked(code, message, ComputerUseRetryOwner::Model)
}

fn backend_error(message: impl Into<String>) -> ComputerUseError {
    ComputerUseError::new(
        "backend_unavailable",
        message,
        true,
        ComputerUseRetryOwner::System,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_stroke_keeps_input_fact_when_after_capture_fails() {
        let before=json!({"sha256":"before","path":"before.png","width":10,"height":10});
        let execution=completed_stroke_execution("window-canvas:1",3,600,&before,Err(input_error("capture unavailable".into())));
        assert!(execution.input_sent);
        assert!(execution.evidence.iter().any(|value|value.contains("native_stroke:")&&value.ends_with(":released")));
        assert!(execution.evidence.iter().any(|value|value=="after_capture_failed:input_failed"));
        assert!(!execution.evidence.iter().any(|value|value.starts_with("image_changed:")));
        assert!(execution.summary.contains("尚未确认"));
        // 缺后图不成为可见进展，更不能成为视觉目标成功证据。
        let observation=Observation{generation:1,surface:computer_use::ComputerUseSurface::Desktop,surface_identity:"desktop:1".into(),state:json!({"desktop":{"elements":[]},"image":before}),evidence:execution.evidence};
        let verified=DesktopNativeBridge::default().verify(&["two-new-strokes".into()],&observation,&observation).unwrap();
        assert!(!verified.achieved);assert!(!verified.visible_progress);
    }

    #[test]
    fn desktop_stroke_relative_points_stay_inside_physical_bounds() {
        let (points,duration)=stroke_arguments(&json!({"points":[[0,0],[1,1],[0.5,0.5]]}),[-200,50,101,51]).unwrap();
        assert_eq!(duration,600);
        assert_eq!((points[0].x,points[0].y),(-200,50));
        assert_eq!((points[1].x,points[1].y),(-100,100));
        assert_eq!((points[2].x,points[2].y),(-150,75));
        // rect 已是物理像素：150% DPI 不再次乘比例，左右端点也不越界。
        let identity=computer_use::input::StrokeWindow{handle:1,process_id:2,rect:[-200,0,500,500],dpi:144};
        assert!(computer_use::input::validate_stroke(identity,[-200,50,101,51],&points,duration).is_ok());
    }

    #[test]
    fn desktop_stroke_rejects_invalid_points_duration_and_overflow() {
        for args in [json!({"points":[[0,0]]}),json!({"points":[[0,0],[1.01,1]]}),json!({"points":[[0,0],[-0.1,1]]}),json!({"points":[[0,0],[1,1]],"duration_ms":5001}),json!({"points":[[0,0],[1,1]],"duration_ms":-1}),json!({"points":[[0,0],[null,1]]})] {
            assert!(stroke_arguments(&args,[0,0,100,100]).is_err());
        }
        assert!(stroke_arguments(&json!({"points":[[0,0],[1,1]]}),[i32::MAX,0,2,100]).is_err());
        assert_eq!(intersect_rect([-8,-8,1016,716],[0,0,1000,700]).unwrap(),[0,0,1000,700]);
        assert!(intersect_rect([-100,-100,50,50],[0,0,1000,700]).is_err());
    }

    #[test]
    fn desktop_verifier_ignores_capture_metadata_and_requires_goal_evidence() {
        let before=Observation{generation:1,surface:computer_use::ComputerUseSurface::Desktop,surface_identity:"desktop:1".into(),state:json!({"desktop":{"elements":[]},"image":{"sha256":"before","path":"before.png"}}),evidence:vec![]};
        let mut after=before.clone();after.generation=2;after.state["image"]=json!({"sha256":"before","path":"red-triangle.png","data_url":"red-triangle"});
        let unchanged=DesktopNativeBridge::default().verify(&["red-triangle".into()],&before,&after).unwrap();
        assert!(!unchanged.achieved);assert!(!unchanged.visible_progress);
        after.state["image"]["sha256"]=json!("after");
        let changed=DesktopNativeBridge::default().verify(&["red-triangle".into()],&before,&after).unwrap();
        assert!(!changed.achieved);assert!(changed.visible_progress);
    }

    #[test]
    fn desktop_bridge_requires_one_enabled_visible_reference() {
        let state = json!({
            "elements": [{"reference":"uia-1","enabled":true,"offscreen":false,"rect":[1,2,30,40]}]
        });
        assert!(find_element(&state, "uia-1").is_ok());
        assert_eq!(
            find_element(&state, "uia-2").unwrap_err().code,
            "target_not_found"
        );
    }

    #[test]
    fn desktop_verifier_uses_fresh_visible_value_evidence() {
        let bridge = DesktopNativeBridge::default();
        let before = Observation {
            generation: 1,
            surface: computer_use::ComputerUseSurface::Desktop,
            surface_identity: "desktop:1".into(),
            state: json!({"desktop":{"elements":[]}}),
            evidence: vec!["before".into()],
        };
        let after = Observation {
            generation: 2,
            surface: computer_use::ComputerUseSurface::Desktop,
            surface_identity: "desktop:1".into(),
            state: json!({"desktop":{"elements":[{"value":"COOLZHU-CU-E2E-20260629"}]}}),
            evidence: vec!["after".into()],
        };
        let verification = bridge
            .verify(
                &["COOLZHU-CU-E2E-20260629 is visible".into()],
                &before,
                &after,
            )
            .unwrap();
        assert!(verification.achieved);
        assert!(verification.visible_progress);
    }

    #[test]
    fn desktop_criterion_requires_marker_not_uia_field_name() {
        let corpus_without_marker = json!({
            "elements": [
                {"control_type": "Document", "value": "", "text": ""}
            ]
        })
        .to_string()
        .to_lowercase();
        let corpus_with_marker = json!({
            "elements": [
                {
                    "control_type": "Document",
                    "value": "COOLZHU-DESKTOP-E2E-91ae1b92738c",
                    "text": "COOLZHU-DESKTOP-E2E-91ae1b92738c"
                }
            ]
        })
        .to_string()
        .to_lowercase();
        let criterion = "记事本文本区域 value 包含 COOLZHU-DESKTOP-E2E-91ae1b92738c";

        assert!(!criterion_visible(criterion, &corpus_without_marker));
        assert!(criterion_visible(criterion, &corpus_with_marker));
    }
}
