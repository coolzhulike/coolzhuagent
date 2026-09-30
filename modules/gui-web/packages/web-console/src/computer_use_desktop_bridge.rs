use base64::Engine;
use std::sync::Arc;
use std::time::Duration;

use computer_use::{
    action_attempt_id, ComputerUseAction, ComputerUseActionKind, ComputerUseError,
    ComputerUseRetryOwner, Observation, StepExecution, StepInputReleaseStatus, Verification,
};
use runtime::{ActionReceipt, InputReleaseStatus};
use serde_json::{json, Value as JsonValue};
use uia_resolver::{snapshot_foreground_window, UiaElementSnapshot, UiaError};

use crate::computer_use_adapters::{clamp_stage_timeout, DesktopBridge, DesktopSnapshot};
use crate::computer_use_frame::{unit_axis_to_pixel, FrameRef};

const INPUT_TIMEOUT: Duration = Duration::from_secs(8);
const UIA_ELEMENT_LIMIT: usize = 500;

#[derive(Clone)]
pub(crate) struct DesktopNativeBridge {
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Default for DesktopNativeBridge {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(|| false),
        }
    }
}

impl std::fmt::Debug for DesktopNativeBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopNativeBridge")
            .finish_non_exhaustive()
    }
}

impl DesktopNativeBridge {
    pub(crate) fn with_cancelled(cancelled: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        Self { cancelled }
    }
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
        remaining: std::time::Duration,
    ) -> Result<DesktopSnapshot, ComputerUseError> {
        if (self.cancelled)() || remaining.is_zero() {
            return Err(cancelled_error());
        }
        let target = request.target.as_ref();
        let application = target.and_then(|target| target.application.as_deref());
        let window = target.and_then(|target| target.window.as_deref());
        // 正式运行已持有桌面 owner；此处只用 Win32/UIA 选择观察窗口，不合成键鼠输入。
        let selected_window = uia_resolver::focus_window_by_hint(
            application, window, Some(&request.objective),
        ).map_err(map_uia_error)?;
        if selected_window.is_some() {
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
            handle: snapshot.native_window_handle,
            process_id: snapshot.process_id,
            rect: [
                snapshot.bounding_rect.x,
                snapshot.bounding_rect.y,
                snapshot.bounding_rect.width,
                snapshot.bounding_rect.height,
            ],
            dpi: snapshot.dpi,
        };
        let image = capture_image(identity, remaining)?;
        let canvas_rect = intersect_rect(
            rect_from_element(&json!({"rect":image["client_rect"]}))?,
            rect_from_element(&json!({"rect":image["screen_rect"]}))?,
        )?;
        let image_evidence = image_evidence(&image);
        // CU-04 帧绑定：把"这份观察的图像版本 + 裁剪 + 缩放 + 坐标容器"写成证据，
        // 使"坐标属于哪一版图"事后可核对。**只记录，不拒绝**——观察本身不该因为
        // 几何不全而失败；真正需要拒绝的是"要用这组坐标去发输入"的那一刻（见 Drag 分支）。
        let state = json!({
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
        });
        let frame_marker = FrameRef::bind(&state, std::slice::from_ref(&image_evidence), canvas_rect)
            .map_or_else(|error| error.evidence(), |frame| frame.evidence());
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
            state,
            evidence: vec![
                image_evidence,
                format!(
                    "uia_snapshot:{window_id}:elements={}",
                    snapshot.elements.len()
                ),
                frame_marker,
                format!("observation_window_selection:{selected_window:?}:uia_win32_no_synthetic_input"),
            ],
        })
    }

    fn execute(
        &self,
        action: &ComputerUseAction,
        expected: &DesktopSnapshot,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError> {
        self.execute_inner(action, expected, remaining, None)
    }

    fn execute_authorized(
        &self,
        action: &ComputerUseAction,
        expected: &DesktopSnapshot,
        remaining: std::time::Duration,
        authorization: &dyn computer_use::prepared_input::NativeInputAuthorization,
    ) -> Result<StepExecution, ComputerUseError> {
        self.execute_inner(action, expected, remaining, Some(authorization))
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        _remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError> {
        let image_changed = before
            .state
            .pointer("/image/sha256")
            .zip(after.state.pointer("/image/sha256"))
            .is_some_and(|(a, b)| a != b);
        // 截图路径、编码与时间戳不参与 UIA 文本成功匹配，避免证据元数据伪装成成功。
        let before_desktop = before.state.get("desktop").unwrap_or(&before.state);
        let after_desktop = after.state.get("desktop").unwrap_or(&after.state);
        let visible_progress =
            image_changed || before_desktop.get("elements") != after_desktop.get("elements");
        let corpus = after_desktop
            .get("elements")
            .unwrap_or(&JsonValue::Null)
            .to_string()
            .to_lowercase();
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
            evidence: after
                .evidence
                .iter()
                .cloned()
                .chain(std::iter::once(format!("image_changed:{image_changed}")))
                .collect(),
        })
    }
}

impl DesktopNativeBridge {
    fn execute_inner(
        &self,
        action: &ComputerUseAction,
        expected: &DesktopSnapshot,
        remaining: std::time::Duration,
        authorization: Option<&dyn computer_use::prepared_input::NativeInputAuthorization>,
    ) -> Result<StepExecution, ComputerUseError> {
        // 回执身份：受信执行链按动作内容计算，消费者据此判断事实是否属于当前动作。
        let action_id = action_attempt_id(computer_use::ComputerUseSurface::Desktop, action);
        // 仅输入准备阶段可证明未发送；helper 启动后的错误继续使用真实输入事实。
        let pre_input = |error: ComputerUseError| error.with_receipt(pre_input_receipt(
            &action_id, action.kind == ComputerUseActionKind::Drag,
        ));
        if (self.cancelled)() {
            // 输入尚未开始：明确的"未发送"事实，而不是留给上层猜。
            return Err(cancelled_error().with_receipt(pre_input_receipt(&action_id, false)));
        }
        let screenshot_target = action.kind == ComputerUseActionKind::Drag
            && expected
                .state
                .get("canvas_target")
                .and_then(JsonValue::as_str)
                == Some(action.target.as_str());
        let fallback;
        let element = if screenshot_target {
            fallback = json!({"rect":expected.state["canvas_rect"],"enabled":true,"offscreen":false,"control_type":"ScreenshotCanvas"});
            &fallback
        } else {
            find_element(&expected.state, &action.target).map_err(pre_input)?
        };
        if !element
            .get("enabled")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            return Err(pre_input(blocked("target_disabled", "target element is disabled")));
        }
        if element
            .get("offscreen")
            .and_then(JsonValue::as_bool)
            .unwrap_or(true)
        {
            return Err(pre_input(blocked("target_offscreen", "target element is offscreen")));
        }
        let rect = rect_from_element(element).map_err(pre_input)?;
        let x = rect[0].saturating_add(rect[2] / 2);
        let y = rect[1].saturating_add(rect[3] / 2);
        let native_window_handle = expected
            .state
            .pointer("/window/native_window_handle")
            .and_then(JsonValue::as_i64)
            .and_then(|value| isize::try_from(value).ok())
            .ok_or_else(|| pre_input(stale("foreground window identity is missing")))?;

        // 输入动作一律走受控原生输入生命周期：输入前身份/权限/scope 校验 → 登记释放义务 →
        // 受监督执行 → 阶段回执 → 取消/超时/失败收尾 → 静止与释放对账。
        // 旧原语（click_point/type_text/scroll_wheel/send_virtual_key_combo）超时后不终止子进程、
        // 不做静止确认、也没有释放登记，因此不再用于自动输入。
        let identity = computer_use::input::StrokeWindow {
            handle: native_window_handle,
            process_id: expected.process_id,
            rect: expected.window_rect,
            dpi: expected.dpi,
        };
        let attempt =
            computer_use::input::NativeInputAttempt::with_window(identity, &*self.cancelled);
        let attempt = match authorization {
            Some(port) => attempt.with_authorization(port),
            None => attempt,
        };
        match action.kind {
            ComputerUseActionKind::Click => {
                let outcome = controlled_input(
                    &action_id,
                    computer_use::input::controlled_click(
                        x,
                        y,
                        1,
                        &attempt,
                        clamp_stage_timeout(remaining, INPUT_TIMEOUT),
                    ),
                )?;
                return Ok(native_input_execution(
                    &action_id,
                    &action.target,
                    "click",
                    &outcome,
                    Vec::new(),
                ));
            }
            ComputerUseActionKind::DoubleClick => {
                let outcome = controlled_input(
                    &action_id,
                    computer_use::input::controlled_click(
                        x,
                        y,
                        2,
                        &attempt,
                        clamp_stage_timeout(remaining, INPUT_TIMEOUT),
                    ),
                )?;
                return Ok(native_input_execution(
                    &action_id,
                    &action.target,
                    "double_click",
                    &outcome,
                    Vec::new(),
                ));
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
                // 点击必须确认成功，才谈得上"点到了但没输入文字"。
                let click = controlled_input(
                    &action_id,
                    computer_use::input::controlled_click(
                        x,
                        y,
                        1,
                        &attempt,
                        clamp_stage_timeout(remaining, INPUT_TIMEOUT),
                    ),
                )?;
                let click_release = click.release_status();
                // 点击已确认成功，打字失败属于"已知的部分输入"（点到了但没有输入文字）：
                // 两段的释放对账都要带上，回执不得退回"未发送"。
                let typed = computer_use::input::controlled_type_text(
                    text,
                    &attempt,
                    clamp_stage_timeout(remaining, INPUT_TIMEOUT),
                )
                .map_err(|failure| {
                    native_failure_error_with_prior(&action_id, &failure, Some(click_release))
                })?;
                return Ok(native_input_execution(
                    &action_id,
                    &action.target,
                    "text_input",
                    &typed,
                    vec![format!("click:{}", native_outcome_evidence(&click))],
                ));
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
                let click = controlled_input(
                    &action_id,
                    computer_use::input::controlled_click(
                        x,
                        y,
                        1,
                        &attempt,
                        clamp_stage_timeout(remaining, INPUT_TIMEOUT),
                    ),
                )?;
                let click_release = click.release_status();
                // 滚轮失败时点击已经确认成功：同样是"已知的部分输入"。
                let scrolled = computer_use::input::controlled_scroll(
                    sign * amount * 120,
                    &attempt,
                    clamp_stage_timeout(remaining, INPUT_TIMEOUT),
                )
                .map_err(|failure| {
                    native_failure_error_with_prior(&action_id, &failure, Some(click_release))
                })?;
                return Ok(native_input_execution(
                    &action_id,
                    &action.target,
                    "scroll",
                    &scrolled,
                    vec![format!("click:{}", native_outcome_evidence(&click))],
                ));
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
                let outcome = controlled_input(
                    &action_id,
                    computer_use::input::controlled_key_combo(
                        &virtual_keys,
                        &attempt,
                        clamp_stage_timeout(remaining, INPUT_TIMEOUT),
                    ),
                )?;
                return Ok(native_input_execution(
                    &action_id,
                    &action.target,
                    "key_combination",
                    &outcome,
                    Vec::new(),
                ));
            }
            ComputerUseActionKind::Drag => {
                if !screenshot_target && !canvas_element(element) {
                    return Err(blocked(
                        "target_not_canvas",
                        "笔画目标应为 UIA 画布或当前截图的 canvas_target",
                    ));
                }
                // CU-04 帧绑定：坐标**即将**被映射成物理像素，此刻必须能说明它们属于
                // 哪一版图像、哪次裁剪、哪个缩放（容器 = 这次映射实际用的 rect）。
                // 与快照阶段的"只记录"不同，这里是**拒绝点**：绑定不成立就不许映射，
                // 否则 0..1 点会被画到错误的图像/裁剪/缩放上而无人发现。
                let frame = FrameRef::bind(&expected.state, &expected.evidence, rect)
                    .map_err(|error| blocked(error.code(), error.describe()))?;
                let (points, duration_ms) = stroke_arguments(&action.arguments, rect)?;
                let visible_rect =
                    rect_from_element(&json!({"rect":expected.state["image"]["screen_rect"]}))?;
                if intersect_rect(rect, visible_rect)? != rect {
                    return Err(blocked(
                        "target_offscreen",
                        "画布边界部分位于截图之外，需调整窗口后重新观察",
                    ));
                }
                computer_use::input::validate_stroke(identity, rect, &points, duration_ms)
                    .map_err(|error| {
                        // 校验失败发生在 helper 启动之前：明确"未发送"，不留猜测空间。
                        blocked("invalid_stroke_path", error)
                            .with_receipt(pre_input_receipt(&action_id, true))
                    })?;
                let result = match authorization {
                    Some(port) => computer_use::input::controlled_drag_path_authorized(
                        identity, rect, &points, duration_ms,
                        clamp_stage_timeout(remaining, Duration::from_secs(10)),
                        &*self.cancelled, port,
                    ).map(|outcome| outcome.facts),
                    None => computer_use::input::controlled_drag_path(
                        identity, rect, &points, duration_ms,
                        clamp_stage_timeout(remaining, Duration::from_secs(10)),
                        &*self.cancelled,
                    ),
                };
                // 成功和可恢复失败都尝试留后图；截图本身不会移动鼠标或改变画布。
                let after = capture_image(identity, remaining);
                let facts = match result {
                    Ok(facts) => facts,
                    Err(failure) => {
                        let evidence = after
                            .as_ref()
                            .map(image_evidence)
                            .unwrap_or_else(|error| format!("after_capture_failed:{error:?}"));
                        // 映射时用的绑定（容器 = 这次真正用的 rect，未必等于快照的 canvas_rect）
                        // 必须跟着失败事实一起留下，否则事后无法说明这组点相对谁。
                        let evidence = format!("{};mapping={}", evidence, frame.evidence());
                        // 回执直接由 helper 自己的事实构成：部分注入、路径完成、释放义务分别记录，
                        // 不合并成"整段没执行"，也不因失败就推断"零输入"。
                        let receipt =
                            computer_use::input::helper_failure_receipt(&action_id, &failure);
                        return Err(stroke_failure_error(&failure, &receipt, &evidence));
                    }
                };
                return Ok(completed_stroke_execution(
                    &action_id,
                    &action.target,
                    duration_ms,
                    &facts,
                    &expected.state["image"],
                    after,
                    &frame.evidence(),
                ));
            }
            _ => {
                Err(blocked(
                    "unsupported_action",
                    "desktop bridge does not implement the requested action",
                )
                .with_receipt(pre_input_receipt(&action_id, false)))
            }
        }
    }
}

fn cancelled_error() -> ComputerUseError {
    ComputerUseError::new(
        "cancelled",
        "桌面输入开始前已取消",
        false,
        ComputerUseRetryOwner::None,
    )
}

/// 把回执投影成 `StepExecution` 的兼容字段。
///
/// 回执是唯一事实来源；`partial/path_completed/confirmed_point_count/input_release_status`
/// 只是它的旧消费者投影，不允许各写一套。
fn step_execution_from_receipt(
    input_sent: bool,
    summary: String,
    evidence: Vec<String>,
    receipt: ActionReceipt,
) -> StepExecution {
    debug_assert!(
        receipt.validate().is_ok(),
        "StepExecution 必须携带自洽回执：{receipt:?}"
    );
    StepExecution {
        input_sent,
        summary,
        evidence,
        partial: receipt.partial,
        path_completed: receipt.path_completed,
        confirmed_point_count: receipt.confirmed_point_count,
        input_release_status: Some(match receipt.input_release {
            InputReleaseStatus::NotNeeded => StepInputReleaseStatus::NotNeeded,
            InputReleaseStatus::Released => StepInputReleaseStatus::Released,
            InputReleaseStatus::Unknown => StepInputReleaseStatus::Unknown,
        }),
        receipt: Some(receipt),
    }
}

/// 输入开始之前就失败（helper 未启动）：明确的"未发送"事实。
///
/// 构造函数与其它入口共用 `computer_use::input` 里的同一份实现，避免各写一套。
fn pre_input_receipt(action_id: &str, is_path: bool) -> ActionReceipt {
    computer_use::input::pre_input_receipt(action_id, is_path)
}

/// 受控笔画失败 → 既有错误类型 + 自洽回执 + 有界收尾事实。
///
/// 裁决 §7 的两个维度**分开保留**，互不覆盖：
/// - **原始执行原因**：只看 helper 自己的报告（[`classify_stroke_failure`]），逐字留在
///   message/证据里；补发结果不会把它顶掉；
/// - **安全收尾结果**：由释放义务状态给出（无需释放 / 已结清 / 未确认）。真实义务
///   **未结清**时外部行为按安全阻断（错误码取 `mouse_release_failed`、不可重试），
///   但原始原因仍在同一段 message 里可读；反过来，已证明没有义务时**不得**拼接一个
///   清理错误串去制造不存在的释放风险（那是"零输入却报告释放风险"）。
fn stroke_failure_error(
    failure: &computer_use::input::StrokeFailure,
    receipt: &ActionReceipt,
    evidence: &str,
) -> ComputerUseError {
    let (cause_code, cause_retryable) = classify_stroke_failure(&failure.message);
    let release = failure.release_state();
    let (code, retryable) = if release.carries_unsettled_duty() {
        ("mouse_release_failed", false)
    } else {
        (cause_code, cause_retryable)
    };
    let release_note = format!(
        "释放义务={}：收尾={}",
        release.as_str(),
        failure
            .cleanup()
            .map_or("未进入收尾", |cleanup| cleanup.release.as_str())
    );
    let message = format!("{failure}；{release_note}；{evidence}");
    let error = match code {
        "mouse_release_failed" => ComputerUseError::new(
            "mouse_release_failed",
            format!("无法确认鼠标释放：{message}"),
            retryable,
            ComputerUseRetryOwner::None,
        ),
        "cancelled" => ComputerUseError::new(
            "cancelled",
            format!("笔画已取消；{message}"),
            retryable,
            ComputerUseRetryOwner::None,
        ),
        _ if !retryable => {
            ComputerUseError::blocked(code, message, ComputerUseRetryOwner::None)
        }
        "stale_observation" => stale(message),
        _ => input_error(message),
    };
    // 有界收尾的事实随错误一起上抛：业务期限、停止输入、收尾起止、
    // 释放状态与是否隔离由上层分开记录，不在这里合并成一个结论。
    let error = match failure.cleanup() {
        Some(cleanup) => error.with_cleanup(*cleanup),
        None => error,
    };
    error.with_receipt(receipt.clone())
}

/// 受控原生输入失败 → 既有失败码 + 自洽回执 + 有界收尾事实。
///
/// 与笔画同一条规则（同一个释放义务推导）：原始原因保留在 message 里，
/// 释放义务未结清时外部行为按安全阻断，而不是因为"读不懂就退回乐观"。
fn native_failure_error_with_release(
    failure: &computer_use::input::NativeInputFailure,
    receipt: &ActionReceipt,
    evidence: &str,
    cause: ComputerUseError,
) -> ComputerUseError {
    let release = failure.release_state();
    let error = if release.carries_unsettled_duty() {
        ComputerUseError::new(
            "mouse_release_failed",
            format!(
                "无法确认原生输入释放（释放义务={}，收尾={}）：原始原因={}：{}；{evidence}",
                release.as_str(),
                failure
                    .cleanup()
                    .map_or("未进入收尾", |cleanup| cleanup.release.as_str()),
                cause.code,
                cause.message
            ),
            false,
            ComputerUseRetryOwner::None,
        )
    } else {
        cause
    };
    let error = match failure.cleanup() {
        Some(cleanup) => error.with_cleanup(*cleanup),
        None => error,
    };
    error.with_receipt(receipt.clone())
}

/// 受控原生输入的结果搬运：失败立刻映射成带完整事实的 `ComputerUseError`。
///
/// 自动输入只走这一条通道——未获得生命周期保证的旧原语不进入"已验收的自动输入集合"。
fn controlled_input(
    action_id: &str,
    result: Result<computer_use::input::NativeInputOutcome, computer_use::input::NativeInputFailure>,
) -> Result<computer_use::input::NativeInputOutcome, ComputerUseError> {
    result.map_err(|failure| native_failure_error(action_id, &failure))
}

/// 受控原生输入失败 → 既有失败码 + 自洽回执 + 有界收尾事实。
///
/// 分类来源只有一个：`NativeInputFailure::kind()`（与受控笔画同一套码表）。
/// 释放未确认与取消都不可重试：按键状态未知时重试 = 在未知状态下继续注入输入。
fn native_failure_error(
    action_id: &str,
    failure: &computer_use::input::NativeInputFailure,
) -> ComputerUseError {
    native_failure_error_with_prior(action_id, failure, None)
}

/// 同上，但同一次动作里前一段输入（例如点击）已确认成功：释放对账必须两段一起算，
/// 回执也不得因此退回"未发送"。
fn native_failure_error_with_prior(
    action_id: &str,
    failure: &computer_use::input::NativeInputFailure,
    prior_release: Option<InputReleaseStatus>,
) -> ComputerUseError {
    let evidence = native_failure_evidence(failure);
    let cause = match failure.kind() {
        computer_use::input::StrokeFailureKind::ReleaseUnconfirmed => ComputerUseError::new(
            "mouse_release_failed",
            format!("无法确认原生输入释放：{failure}；{evidence}"),
            false,
            ComputerUseRetryOwner::None,
        ),
        computer_use::input::StrokeFailureKind::Cancelled => ComputerUseError::new(
            "cancelled",
            format!("受控原生输入已取消或超时：{failure}；{evidence}"),
            false,
            ComputerUseRetryOwner::None,
        ),
        computer_use::input::StrokeFailureKind::Stale => stale(format!("{failure}；{evidence}")),
        computer_use::input::StrokeFailureKind::HelperLost
        | computer_use::input::StrokeFailureKind::Failed => {
            input_error(format!("{failure}；{evidence}"))
        }
    };
    let receipt = match prior_release {
        Some(prior) => computer_use::input::native_failure_receipt_after_confirmed_input(
            action_id, failure, prior,
        ),
        None => computer_use::input::native_failure_receipt(action_id, failure),
    };
    // 收尾事实与释放义务维度在这里合并：原始原因保留在 message 里，
    // 释放义务未结清时外部行为按安全阻断（见 `native_failure_error_with_release`）。
    native_failure_error_with_release(failure, &receipt, &evidence, cause)
}

/// 失败证据：**收到回复**与**子进程确认静止**分别记录（超时后两者会不一致）。
fn native_failure_evidence(failure: &computer_use::input::NativeInputFailure) -> String {
    format!(
        "obligation={}:{}",
        failure.obligation.summary(),
        native_outcome_evidence(&failure.outcome)
    )
}

/// 一次受控原生运行的证据行：步数 + 收到回复的时刻 + 确认静止的时刻。
fn native_outcome_evidence(outcome: &computer_use::input::NativeInputOutcome) -> String {
    let steps = outcome
        .facts()
        .map_or(-1, |facts| i64::from(facts.injected_steps));
    format!(
        "steps={steps}:reply_at_ms={}:exit_confirmed_at_ms={}",
        milliseconds(outcome.reply_received_at_ms),
        milliseconds(outcome.process_exit_confirmed_at_ms),
    )
}

fn milliseconds(value: Option<u64>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}

/// 受控原生输入成功的 `StepExecution`（回执是唯一事实来源，兼容字段是它的投影）。
fn native_input_execution(
    action_id: &str,
    target: &str,
    label: &str,
    outcome: &computer_use::input::NativeInputOutcome,
    mut prior_evidence: Vec<String>,
) -> StepExecution {
    prior_evidence.push(format!(
        "native_input:{label}:{target}:{}",
        native_outcome_evidence(outcome)
    ));
    let steps = outcome
        .facts()
        .map_or(-1, |facts| i64::from(facts.injected_steps));
    let release = match outcome.release_status() {
        InputReleaseStatus::NotNeeded => "not_needed",
        InputReleaseStatus::Released => "released",
        InputReleaseStatus::Unknown => "unknown",
    };
    step_execution_from_receipt(
        true,
        format!(
            "受控原生输入完成（{label}）：已确认 {steps} 步，释放={release}；回复与子进程结束分别记录"
        ),
        prior_evidence,
        computer_use::input::native_success_receipt(action_id, outcome),
    )
}

/// 原生输入已经成功后，后截图失败不能抹掉输入事实；控制器仍需重新观察和视觉验收。
fn completed_stroke_execution(
    action_id: &str,
    target: &str,
    duration_ms: u64,
    facts: &computer_use::input::HelperInputFacts,
    before: &JsonValue,
    after: Result<JsonValue, ComputerUseError>,
    frame_evidence: &str,
) -> StepExecution {
    let mut evidence = vec![
        format!(
            "native_stroke:{target}:points={}:duration_ms={duration_ms}:released",
            facts.injected_points
        ),
        format!("before:{}", image_evidence(before)),
        // 映射时真正使用的坐标容器（UIA 画布 rect 或 canvas_rect），与快照记录分开留证。
        format!("mapping:{frame_evidence}"),
    ];
    let summary = match after {
        Ok(image) => {
            evidence.push(format!("after:{}", image_evidence(&image)));
            evidence.push(format!(
                "image_changed:{}",
                before["sha256"] != image["sha256"]
            ));
            format!(
                "受控画布笔画输入完成：{} 点，鼠标已释放；画布结果等待验收",
                facts.injected_points
            )
        }
        Err(error) => {
            evidence.push(format!("after_capture_failed:{}", error.code));
            format!(
                "受控画布笔画输入完成：{} 点，鼠标已释放；后截图失败，尚未确认画布结果",
                facts.injected_points
            )
        }
    };
    step_execution_from_receipt(
        true,
        summary,
        evidence,
        computer_use::input::helper_success_receipt(action_id, facts.clone()),
    )
}

const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

fn capture_image(
    identity: computer_use::input::StrokeWindow,
    remaining: Duration,
) -> Result<JsonValue, ComputerUseError> {
    let mut image = computer_use::input::capture_window_image(
        identity,
        clamp_stage_timeout(remaining, CAPTURE_TIMEOUT),
    )
        .map_err(|error| {
            if error.contains("stale_observation") {
                stale(error)
            } else if error.contains("stroke_cancelled") {
                ComputerUseError::new("cancelled", error, false, ComputerUseRetryOwner::None)
            } else {
                input_error(error)
            }
        })?;
    let data = image
        .get("data_url")
        .and_then(JsonValue::as_str)
        .and_then(|value| value.strip_prefix("data:image/png;base64,"))
        .ok_or_else(|| backend_error("截图缺少 PNG 像素"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| backend_error("截图 PNG 编码无效"))?;
    let hash = image
        .get("sha256")
        .and_then(JsonValue::as_str)
        .filter(|value| value.len() == 64 && value.bytes().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| backend_error("截图 hash 无效"))?;
    let directory = vision::default_latest_desktop_capture_dir().join("computer-use");
    std::fs::create_dir_all(&directory)
        .map_err(|error| backend_error(format!("截图证据目录不可用: {error}")))?;
    let path = directory.join(format!("window-{}-{hash}.png", identity.process_id));
    if !path.exists() {
        std::fs::write(&path, bytes)
            .map_err(|error| backend_error(format!("截图证据写入失败: {error}")))?;
    }
    image["path"] = json!(path.to_string_lossy());
    Ok(image)
}

fn image_evidence(image: &JsonValue) -> String {
    format!(
        "screenshot:{}:sha256={}:{}x{}",
        image["path"].as_str().unwrap_or(""),
        image["sha256"].as_str().unwrap_or(""),
        image["width"],
        image["height"]
    )
}

fn canvas_element(element: &JsonValue) -> bool {
    matches!(
        element["control_type"].as_str(),
        Some("Document") | Some("Image")
    ) || ["name", "class_name", "automation_id"]
        .iter()
        .filter_map(|key| element[key].as_str())
        .any(|value| {
            let value = value.to_lowercase();
            value.contains("canvas") || value.contains("画布") || value.contains("mspaintview")
        })
}

fn stroke_arguments(
    arguments: &JsonValue,
    rect: [i32; 4],
) -> Result<(Vec<computer_use::input::MousePoint>, u64), ComputerUseError> {
    if rect[2] <= 0
        || rect[3] <= 0
        || rect[0].checked_add(rect[2]).is_none()
        || rect[1].checked_add(rect[3]).is_none()
    {
        return Err(blocked("invalid_stroke_bounds", "画布边界无效或溢出"));
    }
    let values = arguments["points"]
        .as_array()
        .filter(|p| (2..=256).contains(&p.len()))
        .ok_or_else(|| blocked("invalid_stroke_path", "points 应为 2–256 个相对坐标点"))?;
    let duration = arguments
        .get("duration_ms")
        .map(|v| v.as_u64())
        .unwrap_or(Some(600))
        .filter(|v| *v <= 5000)
        .ok_or_else(|| blocked("invalid_stroke_duration", "duration_ms 应为 0–5000"))?;
    let points = values
        .iter()
        .map(|point| {
            let point = point
                .as_array()
                .filter(|p| p.len() == 2)
                .ok_or_else(|| blocked("invalid_stroke_point", "每个点应为 [x,y]"))?;
            let axis = |i: usize| {
                point[i]
                    .as_f64()
                    .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    .ok_or_else(|| blocked("invalid_stroke_point", "相对坐标必须在 0..1 内"))
            };
            Ok(computer_use::input::MousePoint {
                x: unit_axis_to_pixel(axis(0)?, rect[0], rect[2]),
                y: unit_axis_to_pixel(axis(1)?, rect[1], rect[3]),
            })
        })
        .collect::<Result<Vec<_>, ComputerUseError>>()?;
    Ok((points, duration))
}

fn intersect_rect(a: [i32; 4], b: [i32; 4]) -> Result<[i32; 4], ComputerUseError> {
    let x = i64::from(a[0].max(b[0]));
    let y = i64::from(a[1].max(b[1]));
    let right = (i64::from(a[0]) + i64::from(a[2])).min(i64::from(b[0]) + i64::from(b[2]));
    let bottom = (i64::from(a[1]) + i64::from(a[3])).min(i64::from(b[1]) + i64::from(b[3]));
    if right <= x || bottom <= y {
        return Err(blocked("target_offscreen", "目标没有可见屏幕区域"));
    }
    Ok([
        x as i32,
        y as i32,
        i32::try_from(right - x).map_err(|_| stale("rectangle overflow"))?,
        i32::try_from(bottom - y).map_err(|_| stale("rectangle overflow"))?,
    ])
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
        // CU-05：状态原样进入观测 `state.elements`（plan 侧由此看到"可切换/已选中"）。
        // `null` = **该元素不支持该模式**（不是 false），与 UIA 侧口径一致；
        // `toggle_state` 认不出的取值保持 `"unknown"`，不回落成 `"off"`。
        "selected": element.is_selected,
        "keyboard_focus": element.has_keyboard_focus,
        "toggle_state": element.toggle_state,
        "patterns": element.patterns,
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

/// 受控笔画失败的分类：返回 `(错误码, 是否可重试)`。
///
/// 分类本身由 `computer_use::input::StrokeFailureKind` 决定（单一事实来源），这里只是
/// 保留既有调用形态。两种"释放未确认"（helper 明确报 `mouse_release_failed`、helper 退出后
/// 补发释放仍失败 `input_release_unconfirmed`）都**不可重试**：鼠标按键状态未知时必须隔离，
/// 重试等于在未知按键状态下继续注入输入——这正是 §2.2「未确认释放则隔离，不开始新动作」。
fn classify_stroke_failure(error: &str) -> (&'static str, bool) {
    let kind = computer_use::input::StrokeFailureKind::classify(error);
    (kind.code(), kind.retryable())
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

    /// **CU-05**：元素状态进入观测 `state.elements`，且"不支持"保持 `null`（不是 `false`）。
    #[test]
    fn element_json_carries_uia_state_without_inventing_false_values() {
        let element = uia_resolver::UiaElementSnapshot {
            reference: "uia-1".to_string(),
            process_id: 42,
            native_window_handle: 7,
            name: Some("启用".to_string()),
            automation_id: Some("CheckBox1".to_string()),
            class_name: Some("Button".to_string()),
            control_type: "CheckBox".to_string(),
            value: None,
            bounding_rect: vision::locate::BBoxPx {
                x: 10,
                y: 20,
                width: 30,
                height: 40,
            },
            is_offscreen: false,
            is_enabled: true,
            is_selected: None,
            has_keyboard_focus: Some(false),
            toggle_state: Some("unknown".to_string()),
            patterns: vec!["toggle".to_string()],
        };
        let value = super::element_json(&element);
        assert!(
            value.get("selected").is_some_and(JsonValue::is_null),
            "不支持选择模式必须是 null（不是 false）：{value}"
        );
        assert_eq!(value.get("keyboard_focus").and_then(JsonValue::as_bool), Some(false));
        assert_eq!(
            value.get("toggle_state").and_then(JsonValue::as_str),
            Some("unknown"),
            "认不出的开关取值不得回落成 off"
        );
        assert_eq!(
            value
                .get("patterns")
                .and_then(JsonValue::as_array)
                .map(Vec::len),
            Some(1)
        );
        // 既有字段不受影响（追加是加性的）。
        assert_eq!(value.get("reference").and_then(JsonValue::as_str), Some("uia-1"));
        assert_eq!(value.get("enabled").and_then(JsonValue::as_bool), Some(true));
    }

    use super::*;
    // 回执事实的断言需要取值枚举；生产代码只经由 `computer_use::input` 的构造函数。
    use runtime::InputDelivery;

    /// 真实 M 轮在控件查找失败时尚未启动 helper，不能误报为未知输入/待释放。
    #[test]
    fn missing_uia_target_is_not_sent_before_native_input_starts() {
        let action = ComputerUseAction {
            kind: ComputerUseActionKind::Click,
            target: "uia-missing".into(),
            arguments: json!({}),
            risk: computer_use::ComputerUseRiskClass::ReversibleLocal,
        };
        let snapshot = DesktopSnapshot {
            window_id: "hwnd-1".into(), process_id: 1,
            window_rect: [0, 0, 100, 100], dpi: 96,
            webview2_overlay: false, state: json!({"elements":[]}), evidence: Vec::new(),
        };
        let error = DesktopNativeBridge::default()
            .execute(&action, &snapshot, Duration::from_secs(1))
            .expect_err("不存在的目标必须在输入前拒绝");
        assert_eq!(error.code, "target_not_found");
        let receipt = error.receipt().expect("输入前失败必须有事实回执");
        receipt.validate().unwrap();
        assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
        assert!(error.receipt_matches(&action_attempt_id(computer_use::ComputerUseSurface::Desktop, &action)));
    }

    #[test]
    fn successful_stroke_keeps_input_fact_when_after_capture_fails() {
        let before = json!({"sha256":"before","path":"before.png","width":10,"height":10});
        let facts = stroke_facts(3, true, true, Some(true));
        let execution = completed_stroke_execution(
            "desktop:window-canvas:1:0",
            "window-canvas:1",
            600,
            &facts,
            &before,
            Err(input_error("capture unavailable".into())),
            "frame_binding:test",
        );
        assert!(execution.input_sent);
        assert_eq!(execution.partial, Some(false));
        assert_eq!(execution.path_completed, Some(true));
        assert_eq!(execution.confirmed_point_count, Some(3));
        assert_eq!(
            execution.input_release_status,
            Some(StepInputReleaseStatus::Released)
        );
        // 成功路径也必须携带回执，且四个兼容字段必须与回执一致。
        let receipt = execution.receipt.as_ref().expect("成功路径必须产生回执");
        receipt.validate().expect("成功回执必须自洽");
        assert_eq!(receipt.action_id, "desktop:window-canvas:1:0");
        assert_eq!(receipt.input_delivery, InputDelivery::Sent);
        assert_eq!(receipt.confirmed_point_count, Some(3));
        assert_eq!(receipt.partial, execution.partial);
        assert_eq!(receipt.path_completed, execution.path_completed);
        assert!(execution
            .evidence
            .iter()
            .any(|value| value.contains("native_stroke:") && value.ends_with(":released")));
        assert!(execution
            .evidence
            .iter()
            .any(|value| value == "after_capture_failed:input_failed"));
        assert!(!execution
            .evidence
            .iter()
            .any(|value| value.starts_with("image_changed:")));
        assert!(execution.summary.contains("尚未确认"));
        // 缺后图不成为可见进展，更不能成为视觉目标成功证据。
        let observation = Observation {
            generation: 1,
            surface: computer_use::ComputerUseSurface::Desktop,
            surface_identity: "desktop:1".into(),
            state: json!({"desktop":{"elements":[]},"image":before}),
            evidence: execution.evidence,
        };
        let verified = DesktopNativeBridge::default()
            .verify(&["two-new-strokes".into()], &observation, &observation, std::time::Duration::from_secs(30))
            .unwrap();
        assert!(!verified.achieved);
        assert!(!verified.visible_progress);
    }

    fn stroke_facts(
        points: u32,
        button_down: bool,
        path_completed: bool,
        released: Option<bool>,
    ) -> computer_use::input::HelperInputFacts {
        // CU-F01：手写事实一律按"收尾之后的最终事实"给，并声明光标移动过
        //（只有收尾封闭 + 覆盖光标移动的记录才能支撑"零输入"证明）。
        computer_use::input::HelperInputFacts {
            protocol: computer_use::input::HELPER_FACT_PROTOCOL_V2,
            request_id: Some("desktop:stroke:abc".to_string()),
            phase: computer_use::input::HelperFactPhase::Final,
            cursor_moved: Some(true),
            injected_points: points,
            button_down,
            path_completed,
            released,
        }
    }

    /// 零输入证明：收尾已封闭、光标没动过、没按下、没有注入。
    fn zero_input_facts(
    ) -> computer_use::input::HelperFactRead<computer_use::input::HelperInputFacts> {
        computer_use::input::HelperFactRead::Trusted(computer_use::input::HelperInputFacts {
            protocol: computer_use::input::HELPER_FACT_PROTOCOL_V2,
            request_id: Some("desktop:stroke:abc".to_string()),
            phase: computer_use::input::HelperFactPhase::Final,
            cursor_moved: Some(false),
            injected_points: 0,
            button_down: false,
            path_completed: false,
            released: None,
        })
    }

    fn stroke_facts_read(
        points: u32,
        button_down: bool,
        path_completed: bool,
        released: Option<bool>,
    ) -> computer_use::input::HelperFactRead<computer_use::input::HelperInputFacts> {
        computer_use::input::HelperFactRead::Trusted(stroke_facts(
            points,
            button_down,
            path_completed,
            released,
        ))
    }

    /// RPR-04b：受控 helper 的中途失败必须变成带真实事实的回执，
    /// 并使步骤记录同时保留"部分已注入"与"释放是否完成"两个维度。
    #[test]
    fn stroke_failure_error_carries_a_receipt_built_from_helper_facts() {
        use computer_use::input::{helper_failure_receipt, StrokeFailure};

        // 路径第 2 点后失败：1 点已注入、路径未完成、释放已确认。
        let failure = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            stroke_facts_read(1, true, false, Some(true)),
        );
        let receipt = helper_failure_receipt("desktop:stroke:abc", &failure);
        receipt.validate().expect("回执必须自洽");
        let error = stale(failure.message.clone()).with_receipt(receipt);
        assert!(error.receipt_matches("desktop:stroke:abc"));
        assert!(error.receipt_shows_input_may_have_been_sent("desktop:stroke:abc"));
        let receipt = error.receipt().expect("错误必须带回执");
        assert_eq!(receipt.input_delivery, InputDelivery::Sent);
        assert_eq!(receipt.partial, Some(true));
        assert_eq!(receipt.path_completed, Some(false));
        assert_eq!(receipt.confirmed_point_count, Some(1));
        assert_eq!(receipt.input_release, InputReleaseStatus::Released);

        // 路径完成但释放失败：两个事实必须分开记录。
        let failure = StrokeFailure::after_input(
            "mouse_release_failed: Up() 失败",
            stroke_facts_read(6, true, true, Some(false)),
        );
        let receipt = helper_failure_receipt("desktop:stroke:abc", &failure);
        assert_eq!(receipt.path_completed, Some(true));
        assert_eq!(receipt.confirmed_point_count, Some(6));
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
        assert_eq!(classify_stroke_failure(&failure.message), ("mouse_release_failed", false));

        // helper 失联且没有任何事实：维持未知，不推断零输入。
        let failure = StrokeFailure::after_input(
            "helper_lost: 未正常结束",
            computer_use::input::HelperFactRead::Missing,
        );
        let receipt = helper_failure_receipt("desktop:stroke:abc", &failure);
        receipt.validate().expect("未知也必须自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
        assert_eq!(receipt.partial, None);
        assert_eq!(receipt.path_completed, None);
        assert_eq!(receipt.confirmed_point_count, None);
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
    }

    /// CU-F01 §7：错误分类保留两个维度。
    ///
    /// - 已证明没有释放义务 → 错误码保持**原始执行原因**，且**不得**拼接清理错误串
    ///   去制造一个不存在的释放风险；
    /// - 释放义务未结清 → 外部行为按安全阻断（`mouse_release_failed`、不可重试），
    ///   但原始原因逐字保留在 message 里。
    #[test]
    fn stroke_error_keeps_cause_and_release_dimensions_separate() {
        use computer_use::input::{helper_failure_receipt, StrokeFailure};

        // ① 可信的零输入证明：未移动光标、未按下、零注入、收尾已封闭。
        let proven = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            zero_input_facts(),
        );
        let receipt = helper_failure_receipt("desktop:stroke:abc", &proven);
        let error = super::stroke_failure_error(&proven, &receipt, "after:img");
        assert_eq!(error.code, "stale_observation", "原始原因就是外部错误码");
        assert!(error.retryable);
        assert!(
            !error.message.contains("mouse_release_failed")
                && !error.message.contains("input_release_unconfirmed"),
            "不存在义务时不得拼接清理错误串：{}",
            error.message
        );
        assert!(!error.receipt_shows_input_may_have_been_sent("desktop:stroke:abc"));

        // ② 按下过、但释放没有结清：安全维度接管外部分类，原始原因保留。
        let unsettled = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            stroke_facts_read(3, true, false, None),
        );
        let receipt = helper_failure_receipt("desktop:stroke:abc", &unsettled);
        let error = super::stroke_failure_error(&unsettled, &receipt, "after:img");
        assert_eq!(error.code, "mouse_release_failed", "未结清必须阻断");
        assert!(!error.retryable);
        assert!(
            error.message.contains("stale_observation"),
            "原始原因必须保留在证据里：{}",
            error.message
        );
        assert!(error.receipt_shows_input_may_have_been_sent("desktop:stroke:abc"));
    }

    /// CU-F01 §6／§8：旧记录里的 `NotSent + input_release = Unknown` 仍可读取与复核，
    /// 但它**不是**零输入证明——消费者必须按"可能已发送"处理，不得静默放行。
    #[test]
    fn legacy_not_sent_with_unknown_release_is_readable_but_not_a_proof() {
        use runtime::ActionReceipt;

        let legacy = json!({
            "action_id": "desktop:stroke:legacy",
            "input_delivery": "not_sent",
            "partial": false,
            "path_completed": false,
            "confirmed_point_count": 0,
            "effect": "not_observed",
            "goal_verdict": "not_checked",
            "input_release": "unknown"
        });
        let receipt: ActionReceipt = serde_json::from_value(legacy).expect("旧记录必须仍可读取");
        assert!(receipt.validate().is_ok(), "旧记录的字段一致性必须原样保留");
        let error = stale("历史记录".to_string()).with_receipt(receipt);
        assert!(error.receipt_matches("desktop:stroke:legacy"));
        assert!(
            error.receipt_shows_input_may_have_been_sent("desktop:stroke:legacy"),
            "旧异常记录不得被当成零输入证明（否则会放行重放）"
        );
    }

    /// 输入开始前就失败：回执必须写明确的"未发送"，而不是让上层按可能已发送处理。
    #[test]
    fn pre_input_rejections_declare_not_sent() {
        let receipt = pre_input_receipt("desktop:click:abc", false);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.path_completed, None);
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);

        let path = pre_input_receipt("desktop:drag:abc", true);
        path.validate().expect("自洽");
        assert_eq!(path.path_completed, Some(false));
        assert_eq!(path.confirmed_point_count, Some(0));

        // 已确认发出前一段输入（点到但没打字）：部分输入必须保留，释放对账两段合并。
        let failure = computer_use::input::NativeInputFailure {
            message: "invalid_native_text: 文本为空".into(),
            input_possible: false,
            obligation: computer_use::input::ReleaseObligation::none(),
            outcome: computer_use::input::NativeInputOutcome::default(),
        };
        let partial = computer_use::input::native_failure_receipt_after_confirmed_input(
            "desktop:text_input:abc",
            &failure,
            InputReleaseStatus::Released,
        );
        partial.validate().expect("自洽");
        assert_eq!(partial.input_delivery, InputDelivery::Sent);
        assert_eq!(partial.partial, Some(true));
        assert_eq!(partial.input_release, InputReleaseStatus::Released);

        // 回执身份属于别的动作时，消费者必须能识别出协议异常。
        let other = computer_use::ComputerUseError::new("input_failed", "x", true, ComputerUseRetryOwner::System)
            .with_receipt(computer_use::input::partial_input_receipt(
                "desktop:text_input:other",
            ));
        assert!(!other.receipt_matches("desktop:text_input:abc"));
        assert!(other.receipt_shows_input_may_have_been_sent("desktop:text_input:abc"));
    }

    /// 动作身份指纹必须由动作内容决定，且同一动作可复算。
    #[test]
    fn action_attempt_id_is_content_derived_and_stable() {
        let action = ComputerUseAction {
            kind: ComputerUseActionKind::Drag,
            target: "window-canvas:1".into(),
            arguments: json!({"points":[[0.1,0.2],[0.8,0.9]]}),
            risk: computer_use::ComputerUseRiskClass::ReversibleLocal,
        };
        let first = computer_use::action_attempt_id(computer_use::ComputerUseSurface::Desktop, &action);
        let second = computer_use::action_attempt_id(computer_use::ComputerUseSurface::Desktop, &action);
        assert_eq!(first, second);
        assert!(first.starts_with("desktop:window-canvas:1:"));
        let mut changed = action.clone();
        changed.arguments = json!({"points":[[0.1,0.2],[0.8,0.95]]});
        assert_ne!(
            first,
            computer_use::action_attempt_id(computer_use::ComputerUseSurface::Desktop, &changed)
        );
        assert_ne!(
            first,
            computer_use::action_attempt_id(computer_use::ComputerUseSurface::Browser, &action)
        );
    }

    #[test]
    fn desktop_stroke_relative_points_stay_inside_physical_bounds() {
        let (points, duration) = stroke_arguments(
            &json!({"points":[[0,0],[1,1],[0.5,0.5]]}),
            [-200, 50, 101, 51],
        )
        .unwrap();
        assert_eq!(duration, 600);
        assert_eq!((points[0].x, points[0].y), (-200, 50));
        assert_eq!((points[1].x, points[1].y), (-100, 100));
        assert_eq!((points[2].x, points[2].y), (-150, 75));
        // rect 已是物理像素：150% DPI 不再次乘比例，左右端点也不越界。
        let identity = computer_use::input::StrokeWindow {
            handle: 1,
            process_id: 2,
            rect: [-200, 0, 500, 500],
            dpi: 144,
        };
        assert!(computer_use::input::validate_stroke(
            identity,
            [-200, 50, 101, 51],
            &points,
            duration
        )
        .is_ok());
    }

    #[test]
    fn desktop_stroke_rejects_invalid_points_duration_and_overflow() {
        for args in [
            json!({"points":[[0,0]]}),
            json!({"points":[[0,0],[1.01,1]]}),
            json!({"points":[[0,0],[-0.1,1]]}),
            json!({"points":[[0,0],[1,1]],"duration_ms":5001}),
            json!({"points":[[0,0],[1,1]],"duration_ms":-1}),
            json!({"points":[[0,0],[null,1]]}),
        ] {
            assert!(stroke_arguments(&args, [0, 0, 100, 100]).is_err());
        }
        assert!(stroke_arguments(&json!({"points":[[0,0],[1,1]]}), [i32::MAX, 0, 2, 100]).is_err());
        assert_eq!(
            intersect_rect([-8, -8, 1016, 716], [0, 0, 1000, 700]).unwrap(),
            [0, 0, 1000, 700]
        );
        assert!(intersect_rect([-100, -100, 50, 50], [0, 0, 1000, 700]).is_err());
    }

    #[test]
    fn desktop_verifier_ignores_capture_metadata_and_requires_goal_evidence() {
        let before = Observation {
            generation: 1,
            surface: computer_use::ComputerUseSurface::Desktop,
            surface_identity: "desktop:1".into(),
            state: json!({"desktop":{"elements":[]},"image":{"sha256":"before","path":"before.png"}}),
            evidence: vec![],
        };
        let mut after = before.clone();
        after.generation = 2;
        after.state["image"] =
            json!({"sha256":"before","path":"red-triangle.png","data_url":"red-triangle"});
        let unchanged = DesktopNativeBridge::default()
            .verify(&["red-triangle".into()], &before, &after, std::time::Duration::from_secs(30))
            .unwrap();
        assert!(!unchanged.achieved);
        assert!(!unchanged.visible_progress);
        after.state["image"]["sha256"] = json!("after");
        let changed = DesktopNativeBridge::default()
            .verify(&["red-triangle".into()], &before, &after, std::time::Duration::from_secs(30))
            .unwrap();
        assert!(!changed.achieved);
        assert!(changed.visible_progress);
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
                std::time::Duration::from_secs(30),
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

    /// S1.5：笔画失败分类必须稳定，且两种"释放未确认"都不可重试
    /// （按键状态未知时重试 = 在未知按键状态下继续注入输入）。
    #[test]
    fn stroke_failure_classification_covers_release_and_retryability() {
        assert_eq!(
            super::classify_stroke_failure("mouse_release_failed: Up() 失败"),
            ("mouse_release_failed", false)
        );
        assert_eq!(
            super::classify_stroke_failure(
                "input_release_unconfirmed: helper 退出后补发释放失败: x"
            ),
            ("mouse_release_failed", false)
        );
        assert_eq!(
            super::classify_stroke_failure("stroke_cancelled: 已取消或超时"),
            ("cancelled", false)
        );
        assert_eq!(
            super::classify_stroke_failure("stale_observation: 窗口已变化"),
            ("stale_observation", true)
        );
        // 执行者失联：释放已由本进程补发成功，因此可重试（但仍不能填零输入）。
        assert_eq!(
            super::classify_stroke_failure("helper_lost: 未正常结束且未给出原因"),
            ("helper_lost", true)
        );
        assert_eq!(
            super::classify_stroke_failure("受控桌面操作失败: boom"),
            ("input_failed", true)
        );
    }

    /// S1.5：阶段子请求的超时上限不得超过整体 deadline 的剩余预算（§2.3）。
    #[test]
    fn stage_timeout_never_exceeds_the_remaining_budget() {
        let cap = Duration::from_secs(10);
        // 预算充裕：仍用固定上限，不放大。
        assert_eq!(clamp_stage_timeout(Duration::from_secs(30), cap), cap);
        // 预算不足：必须收紧到剩余量，而不是继续用 10 秒。
        assert_eq!(
            clamp_stage_timeout(Duration::from_secs(3), cap),
            Duration::from_secs(3)
        );
        // 预算耗尽：保持 0，由调用方按"立即取消"处理。
        assert_eq!(clamp_stage_timeout(Duration::ZERO, cap), Duration::ZERO);
    }
}
