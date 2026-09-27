use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

use computer_use::{
    ComputerUseAction, ComputerUseAdapter, ComputerUseCapabilities, ComputerUseError,
    ComputerUseRequest, ComputerUseRetryOwner, ComputerUseSurface, Observation, StepExecution,
    Verification,
};
use serde_json::{json, Value as JsonValue};

use crate::computer_use_frame::{rect_from_json, FrameRef};

#[derive(Debug, Clone, Copy)]
pub(crate) struct SurfaceRoutingContext {
    pub foreground_is_webview2: bool,
    pub desktop_available: bool,
    pub browser_available: bool,
}

impl Default for SurfaceRoutingContext {
    fn default() -> Self {
        Self {
            foreground_is_webview2: false,
            desktop_available: true,
            browser_available: true,
        }
    }
}

pub(crate) fn route_computer_use_surface(
    request: &ComputerUseRequest,
    context: &SurfaceRoutingContext,
) -> Result<ComputerUseSurface, ComputerUseError> {
    let target = request.target.as_ref();
    let has_native_target = target.is_some_and(|target| {
        target.application.as_deref().is_some_and(non_empty)
            || target.window.as_deref().is_some_and(non_empty)
    });
    let has_element_hint =
        target.is_some_and(|target| target.element.as_deref().is_some_and(non_empty));
    let has_browser_target = target.is_some_and(|target| {
        target.url.as_deref().is_some_and(non_empty)
            || (target.element.as_deref().is_some_and(non_empty)
                && request.surface != ComputerUseSurface::Desktop)
    });

    if has_native_target && has_browser_target {
        return Err(surface_conflict(
            "request mixes a native application/window target with a URL/DOM target",
        ));
    }
    if context.foreground_is_webview2
        && has_native_target
        && has_element_hint
        && request.surface != ComputerUseSurface::Browser
    {
        return Err(surface_conflict(
            "desktop target is covered by a WebView2 surface",
        ));
    }
    if context.foreground_is_webview2
        && request.surface == ComputerUseSurface::Desktop
        && has_browser_target
    {
        return Err(surface_conflict(
            "a WebView2 target cannot be controlled through the desktop adapter",
        ));
    }

    let selected = match request.surface {
        ComputerUseSurface::Desktop if has_browser_target => {
            return Err(surface_conflict(
                "desktop surface was requested for a URL or DOM target",
            ));
        }
        ComputerUseSurface::Browser if has_native_target => {
            return Err(surface_conflict(
                "browser surface was requested for a native application or window",
            ));
        }
        ComputerUseSurface::Desktop => ComputerUseSurface::Desktop,
        ComputerUseSurface::Browser => ComputerUseSurface::Browser,
        ComputerUseSurface::Auto if has_browser_target => ComputerUseSurface::Browser,
        ComputerUseSurface::Auto if has_native_target => ComputerUseSurface::Desktop,
        ComputerUseSurface::Auto if context.foreground_is_webview2 => {
            return Err(surface_conflict(
                "foreground WebView2 content is ambiguous without an explicit DOM target",
            ));
        }
        ComputerUseSurface::Auto => {
            return Err(ComputerUseError::blocked(
                "surface_ambiguous",
                "request does not identify a desktop or browser target",
                ComputerUseRetryOwner::Model,
            ));
        }
    };

    if (selected == ComputerUseSurface::Desktop && !context.desktop_available)
        || (selected == ComputerUseSurface::Browser && !context.browser_available)
    {
        return Err(backend_error(&format!(
            "{} adapter is unavailable",
            selected.as_str()
        )));
    }
    Ok(selected)
}

fn non_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn surface_conflict(message: &str) -> ComputerUseError {
    ComputerUseError::blocked("surface_conflict", message, ComputerUseRetryOwner::Model)
}

pub(crate) fn backend_error(message: &str) -> ComputerUseError {
    ComputerUseError::new(
        "backend_unavailable",
        message,
        true,
        ComputerUseRetryOwner::System,
    )
}

fn unsupported_action(action: &ComputerUseAction) -> ComputerUseError {
    ComputerUseError::blocked(
        "unsupported_action",
        format!("adapter does not support {:?}", action.kind),
        ComputerUseRetryOwner::Model,
    )
}

fn stale_observation(message: &str) -> ComputerUseError {
    ComputerUseError::recoverable("stale_observation", message)
}

/// 输入开始之前的拒绝：附加"明确未发送"的回执（原生输入边界尚未被调用）。
///
/// 这里只补"没有事实"的错误；已经带事实（例如桥层给出部分注入）的错误原样保留。
fn pre_input_rejection(
    error: ComputerUseError,
    surface: ComputerUseSurface,
    action: &ComputerUseAction,
) -> ComputerUseError {
    if error.receipt.is_some() {
        return error;
    }
    error.with_receipt(computer_use::input::pre_input_receipt(
        &computer_use::action_attempt_id(surface, action),
        false,
    ))
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BrowserSnapshot {
    pub page_id: String,
    pub url: String,
    pub dom_revision: u64,
    pub state: JsonValue,
    pub evidence: Vec<String>,
}

impl BrowserSnapshot {
    fn same_input_identity(&self, other: &Self) -> bool {
        self.page_id == other.page_id
            && self.url == other.url
            && self.dom_revision == other.dom_revision
    }
}

/// 子请求上限不得超过整体 deadline 的剩余预算（§2.3）。
///
/// 各阶段的固定上限只表示"最多愿意等多久"；真正可用的是本 run 剩余的 deadline。
/// 两者取小，避免 3 秒剩余时仍发起一个 8 秒上限的输入或 10 秒上限的桥往返。
pub(crate) fn clamp_stage_timeout(
    remaining: std::time::Duration,
    cap: std::time::Duration,
) -> std::time::Duration {
    remaining.min(cap)
}

pub(crate) trait BrowserBridge: Send + Sync {
    fn snapshot(&self, remaining: std::time::Duration) -> Result<BrowserSnapshot, ComputerUseError>;
    fn execute(
        &self,
        action: &ComputerUseAction,
        expected: &BrowserSnapshot,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError>;
    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError>;
}

pub(crate) struct BrowserComputerUseAdapter<B> {
    bridge: B,
    capabilities: ComputerUseCapabilities,
    generation: AtomicU64,
    observed: Mutex<Option<(u64, BrowserSnapshot)>>,
}

impl<B> BrowserComputerUseAdapter<B> {
    pub(crate) fn new(bridge: B) -> Self {
        Self::with_policy(bridge, BrowserComputerUsePolicy::default())
    }

    pub(crate) fn with_policy(bridge: B, policy: BrowserComputerUsePolicy) -> Self {
        Self {
            bridge,
            capabilities: policy.capabilities(),
            generation: AtomicU64::new(0),
            observed: Mutex::new(None),
        }
    }

    pub(crate) const fn bridge(&self) -> &B {
        &self.bridge
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BrowserComputerUsePolicy {
    pub(crate) allow_drag: bool,
    pub(crate) allow_key_combinations: bool,
    pub(crate) allow_multiple_tabs: bool,
}

impl BrowserComputerUsePolicy {
    fn capabilities(self) -> ComputerUseCapabilities {
        ComputerUseCapabilities {
            navigate: true,
            click: true,
            double_click: false,
            text_input: true,
            select: true,
            check: true,
            submit: true,
            scroll: true,
            history: true,
            drag: self.allow_drag,
            slider_drag: self.allow_drag,
            key_combinations: self.allow_key_combinations,
            multiple_tabs: self.allow_multiple_tabs,
        }
    }
}

impl Default for BrowserComputerUsePolicy {
    fn default() -> Self {
        Self {
            allow_drag: true,
            allow_key_combinations: true,
            allow_multiple_tabs: true,
        }
    }
}

impl<B: BrowserBridge> ComputerUseAdapter for BrowserComputerUseAdapter<B> {
    fn surface(&self) -> ComputerUseSurface {
        ComputerUseSurface::Browser
    }

    fn capabilities(&self) -> ComputerUseCapabilities {
        self.capabilities
    }

    fn observe(
        &self,
        _request: &ComputerUseRequest,
        remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError> {
        let snapshot = self.bridge.snapshot(remaining)?;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let observation = Observation {
            generation,
            surface: ComputerUseSurface::Browser,
            surface_identity: format!("browser:{}:{}", snapshot.page_id, snapshot.url),
            state: json!({
                "page_id": snapshot.page_id,
                "url": snapshot.url,
                "dom_revision": snapshot.dom_revision,
                "page": snapshot.state,
            }),
            evidence: snapshot.evidence.clone(),
        };
        *self.observed.lock().expect("browser observation lock") = Some((generation, snapshot));
        Ok(observation)
    }

    fn act(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError> {
        let pre_input = |error: ComputerUseError| pre_input_rejection(error, self.surface(), action);
        if !self.capabilities().supports(action.kind) {
            return Err(pre_input(unsupported_action(action)));
        }
        let expected = self
            .observed
            .lock()
            .expect("browser observation lock")
            .clone()
            .ok_or_else(|| pre_input(stale_observation("browser action has no observation")))?;
        if expected.0 != expected_generation {
            return Err(pre_input(stale_observation(
                "browser observation generation changed",
            )));
        }
        let current = self.bridge.snapshot(remaining).map_err(pre_input)?;
        if !expected.1.same_input_identity(&current) {
            return Err(pre_input(stale_observation(
                "browser page identity or DOM revision changed before input",
            )));
        }
        self.bridge.execute(action, &current, remaining)
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError> {
        self.bridge.verify(criteria, before, after, remaining)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DesktopSnapshot {
    pub window_id: String,
    pub process_id: u32,
    pub window_rect: [i32; 4],
    pub dpi: u32,
    pub webview2_overlay: bool,
    pub state: JsonValue,
    pub evidence: Vec<String>,
}

impl DesktopSnapshot {
    fn same_input_identity(&self, other: &Self) -> bool {
        self.window_id == other.window_id
            && self.process_id == other.process_id
            && self.window_rect == other.window_rect
            && self.dpi == other.dpi
            && self.webview2_overlay == other.webview2_overlay
    }
}

pub(crate) trait DesktopBridge: Send + Sync {
    fn snapshot(
        &self,
        request: &ComputerUseRequest,
        remaining: std::time::Duration,
    ) -> Result<DesktopSnapshot, ComputerUseError>;
    fn execute(
        &self,
        action: &ComputerUseAction,
        expected: &DesktopSnapshot,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError>;
    fn execute_authorized(
        &self,
        action: &ComputerUseAction,
        expected: &DesktopSnapshot,
        remaining: std::time::Duration,
        _authorization: &dyn computer_use::prepared_input::NativeInputAuthorization,
    ) -> Result<StepExecution, ComputerUseError> {
        self.execute(action, expected, remaining)
    }
    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError>;
}

pub(crate) struct DesktopComputerUseAdapter<B> {
    bridge: B,
    generation: AtomicU64,
    observed: Mutex<Option<(u64, DesktopSnapshot)>>,
}

impl<B> DesktopComputerUseAdapter<B> {
    pub(crate) fn new(bridge: B) -> Self {
        Self {
            bridge,
            generation: AtomicU64::new(0),
            observed: Mutex::new(None),
        }
    }

    pub(crate) const fn bridge(&self) -> &B {
        &self.bridge
    }
}

impl<B: DesktopBridge> ComputerUseAdapter for DesktopComputerUseAdapter<B> {
    fn surface(&self) -> ComputerUseSurface {
        ComputerUseSurface::Desktop
    }

    fn capabilities(&self) -> ComputerUseCapabilities {
        ComputerUseCapabilities {
            navigate: false,
            click: true,
            double_click: true,
            text_input: true,
            select: false,
            check: false,
            submit: false,
            scroll: true,
            history: false,
            drag: true,
            slider_drag: false,
            key_combinations: true,
            multiple_tabs: false,
        }
    }

    fn observe(
        &self,
        request: &ComputerUseRequest,
        remaining: std::time::Duration,
    ) -> Result<Observation, ComputerUseError> {
        let snapshot = self.bridge.snapshot(request, remaining)?;
        if snapshot.webview2_overlay {
            return Err(surface_conflict(
                "desktop target is covered by a WebView2 surface",
            ));
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut desktop_state = snapshot.state.clone();
        let image = desktop_state
            .as_object_mut()
            .and_then(|state| state.remove("image"));
        let observation = Observation {
            generation,
            surface: ComputerUseSurface::Desktop,
            surface_identity: format!("desktop:{}:{}", snapshot.process_id, snapshot.window_id),
            state: json!({
                "window_id": snapshot.window_id,
                "process_id": snapshot.process_id,
                "window_rect": snapshot.window_rect,
                "dpi": snapshot.dpi,
                "desktop": desktop_state,
                "image": image,
            }),
            evidence: snapshot.evidence.clone(),
        };
        *self.observed.lock().expect("desktop observation lock") = Some((generation, snapshot));
        Ok(observation)
    }

    fn act(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
    ) -> Result<StepExecution, ComputerUseError> {
        self.act_inner(action, expected_generation, remaining, None)
    }

    fn act_authorized(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
        authorization: &dyn computer_use::prepared_input::NativeInputAuthorization,
    ) -> Result<StepExecution, ComputerUseError> {
        self.act_inner(action, expected_generation, remaining, Some(authorization))
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
        remaining: std::time::Duration,
    ) -> Result<Verification, ComputerUseError> {
        self.bridge.verify(criteria, before, after, remaining)
    }
}

impl<B: DesktopBridge> DesktopComputerUseAdapter<B> {
    fn act_inner(
        &self,
        action: &ComputerUseAction,
        expected_generation: u64,
        remaining: std::time::Duration,
        authorization: Option<&dyn computer_use::prepared_input::NativeInputAuthorization>,
    ) -> Result<StepExecution, ComputerUseError> {
        let pre_input = |error: ComputerUseError| pre_input_rejection(error, self.surface(), action);
        if !self.capabilities().supports(action.kind) {
            return Err(pre_input(unsupported_action(action)));
        }
        let expected = self
            .observed
            .lock()
            .expect("desktop observation lock")
            .clone()
            .ok_or_else(|| pre_input(stale_observation("desktop action has no observation")))?;
        if expected.0 != expected_generation {
            return Err(pre_input(stale_observation(
                "desktop observation generation changed",
            )));
        }
        let current = self
            .bridge
            .snapshot(
                &ComputerUseRequest {
                    objective: String::new(),
                    surface: ComputerUseSurface::Desktop,
                    target: None,
                    success_criteria: Vec::new(),
                    constraints: Vec::new(),
                },
                remaining,
            )
            .map_err(pre_input)?;
        if current.webview2_overlay {
            return Err(pre_input(surface_conflict(
                "desktop target became covered by a WebView2 surface",
            )));
        }
        if !expected.1.same_input_identity(&current) {
            return Err(pre_input(stale_observation(
                "foreground window, process, DPI, or window rectangle changed before input",
            )));
        }
        if action.kind == computer_use::ComputerUseActionKind::Drag {
            let screenshot_target = expected
                .1
                .state
                .get("canvas_target")
                .and_then(JsonValue::as_str)
                == Some(action.target.as_str());
            if screenshot_target {
                if expected.1.state.pointer("/image/sha256").is_none()
                    || expected.1.state.pointer("/image/sha256")
                        != current.state.pointer("/image/sha256")
                {
                    return Err(pre_input(stale_observation(
                        "截图画布已变化，需重新观察后规划笔画",
                    )));
                }
                // CU-04 帧绑定补的一格：上面只比了**图像内容**，没有比**坐标容器**。
                // 窗口矩形不变、图像内容不变时 client_rect 仍可能变化 ⇒ canvas_rect 变 ⇒
                // 同一组 0..1 点落到不同物理区域。既有检查看不出这一类，这里按可分辨原因拒绝。
                //
                // 比较的两端刻意选择：**观察当时记下的**绑定（`frame_binding:` 证据，
                // 由快照层写入）对**当前实时**绑定。这样核对的正是"规划时用的那一帧"
                // 与"现在这一帧"，而不是两次实时重算（那只能证明现在和现在一样）。
                let recorded = expected.1.evidence.iter().find_map(|item| FrameRef::parse(item));
                let live = current
                    .state
                    .get("canvas_rect")
                    .and_then(rect_from_json)
                    .and_then(|container| {
                        FrameRef::bind(&current.state, &current.evidence, container).ok()
                    });
                // 只在两边都可用时比较：几何不全或绑定缺失由快照层的
                // `frame_binding:unbindable` 如实记录，不在这里新增拒绝。
                if let (Some(recorded), Some(live)) = (recorded, live) {
                    if let Some(mismatch) = recorded.classify(&live) {
                        return Err(pre_input(stale_observation(mismatch.reason())));
                    }
                }
            } else {
                let target = |state: &JsonValue| {
                    state
                        .get("elements")
                        .and_then(JsonValue::as_array)
                        .and_then(|elements| {
                            elements.iter().find(|element| {
                                element["reference"].as_str() == Some(action.target.as_str())
                            })
                        })
                        .cloned()
                };
                if target(&expected.1.state).is_none()
                    || target(&expected.1.state) != target(&current.state)
                {
                    return Err(pre_input(stale_observation(
                        "UIA 画布的边界或身份已变化",
                    )));
                }
            }
        }
        // 一次观察只授权一次输入尝试；成功或失败后都需要重新观察。
        *self.observed.lock().expect("desktop observation lock") = None;
        match authorization {
            Some(port) => self.bridge.execute_authorized(action, &current, remaining, port),
            None => self.bridge.execute(action, &current, remaining),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    };

    use computer_use::{
        ComputerUseAction, ComputerUseActionKind, ComputerUseAdapter, ComputerUseRequest,
        ComputerUseRiskClass, ComputerUseSurface, Verification,
    };
    use serde_json::json;

    use super::*;

    /// **CU-04 无侧改守卫**：桌面输入前的窗口身份比较必须仍然是**五项全比**。
    ///
    /// 加 `frame_binding` 的诱因是"给帧一个身份"，而最省事的错法就是把窗口身份放松成
    /// 只比句柄或只比 pid。这里把五项逐一钉住：任何一项变化都必须判为不同身份。
    /// 它同时是 `computer_use_frame` 那层绑定的前提——帧绑定只负责"图像/裁剪/缩放"，
    /// 窗口物理身份仍由这里独立把关，两者不得互相替代。
    #[test]
    fn desktop_input_identity_still_compares_all_five_fields() {
        let base = DesktopSnapshot {
            window_id: "hwnd-6400".to_string(),
            process_id: 4242,
            window_rect: [10, 20, 800, 600],
            dpi: 96,
            webview2_overlay: false,
            state: json!({}),
            evidence: Vec::new(),
        };
        assert!(base.same_input_identity(&base.clone()), "自身必须同身份");
        for mutate in [
            (|s: &mut DesktopSnapshot| s.window_id = "hwnd-6401".to_string()) as fn(&mut DesktopSnapshot),
            |s: &mut DesktopSnapshot| s.process_id = 4243,
            |s: &mut DesktopSnapshot| s.window_rect = [11, 20, 800, 600],
            |s: &mut DesktopSnapshot| s.dpi = 120,
            |s: &mut DesktopSnapshot| s.webview2_overlay = true,
        ] {
            let mut other = base.clone();
            mutate(&mut other);
            assert!(
                !base.same_input_identity(&other),
                "五项身份中任一项变化都必须判为不同身份"
            );
        }
        // 不在身份内的字段（状态与证据）变化**不得**被判成身份变化：
        // 内容层的新旧由各自的守卫负责，不能混进物理身份。
        let mut content_only = base.clone();
        content_only.state = json!({"changed": true});
        content_only.evidence = vec!["screenshot:x:sha256=0:1x1".to_string()];
        assert!(
            base.same_input_identity(&content_only),
            "状态/证据变化不是物理身份变化，不得在此被判为陈旧"
        );
    }

    fn request(surface: &str, target: serde_json::Value) -> ComputerUseRequest {
        serde_json::from_value(json!({
            "objective": "complete the UI task",
            "surface": surface,
            "target": target,
            "success_criteria": ["result is visible"]
        }))
        .unwrap()
    }

    fn action(kind: ComputerUseActionKind) -> ComputerUseAction {
        ComputerUseAction {
            kind,
            target: "target".into(),
            arguments: json!({}),
            risk: ComputerUseRiskClass::ReversibleLocal,
        }
    }

    #[test]
    fn url_and_dom_targets_route_to_browser() {
        let context = SurfaceRoutingContext::default();
        assert_eq!(
            route_computer_use_surface(
                &request("auto", json!({"url":"https://example.test"})),
                &context,
            )
            .unwrap(),
            ComputerUseSurface::Browser
        );
        assert_eq!(
            route_computer_use_surface(
                &request("auto", json!({"element":"button[name=Submit]"})),
                &context,
            )
            .unwrap(),
            ComputerUseSurface::Browser
        );
    }

    #[test]
    fn native_application_and_window_targets_route_to_desktop() {
        let context = SurfaceRoutingContext::default();
        assert_eq!(
            route_computer_use_surface(
                &request("auto", json!({"application":"notepad"})),
                &context,
            )
            .unwrap(),
            ComputerUseSurface::Desktop
        );
        assert_eq!(
            route_computer_use_surface(
                &request("auto", json!({"window":"Untitled - Notepad"})),
                &context,
            )
            .unwrap(),
            ComputerUseSurface::Desktop
        );
    }

    #[test]
    fn explicit_desktop_allows_native_target_with_element_hint() {
        let context = SurfaceRoutingContext::default();
        assert_eq!(
            route_computer_use_surface(
                &request(
                    "desktop",
                    json!({"application":"notepad", "element":"Edit control"})
                ),
                &context,
            )
            .unwrap(),
            ComputerUseSurface::Desktop
        );
    }

    #[test]
    fn ambiguous_webview2_target_is_a_surface_conflict() {
        let context = SurfaceRoutingContext {
            foreground_is_webview2: true,
            ..SurfaceRoutingContext::default()
        };
        let error = route_computer_use_surface(
            &request(
                "auto",
                json!({"application":"Coolzhu Agent", "element":"Submit"}),
            ),
            &context,
        )
        .expect_err("mixed native and DOM target must not guess");

        assert_eq!(error.code, "surface_conflict");
        assert!(!error.retryable);
    }

    struct FakeBrowserBridge {
        snapshots: Mutex<VecDeque<BrowserSnapshot>>,
        action_count: AtomicUsize,
    }

    impl BrowserBridge for FakeBrowserBridge {
        fn snapshot(
            &self,
            _remaining: std::time::Duration,
        ) -> Result<BrowserSnapshot, computer_use::ComputerUseError> {
            self.snapshots
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| backend_error("missing browser snapshot"))
        }

        fn execute(
            &self,
            _action: &ComputerUseAction,
            _expected: &BrowserSnapshot,
            _remaining: std::time::Duration,
        ) -> Result<computer_use::StepExecution, computer_use::ComputerUseError> {
            self.action_count.fetch_add(1, Ordering::SeqCst);
            Ok(computer_use::StepExecution {
                input_sent: true,
                summary: "browser action sent".into(),
                evidence: vec![],
                ..computer_use::StepExecution::default()
            })
        }

        fn verify(
            &self,
            _criteria: &[String],
            _before: &computer_use::Observation,
            _after: &computer_use::Observation,
            _remaining: std::time::Duration,
        ) -> Result<Verification, computer_use::ComputerUseError> {
            Ok(Verification {
                achieved: false,
                visible_progress: false,
                summary: "not verified".into(),
                evidence: vec![],
            })
        }
    }

    fn browser_snapshot(revision: u64) -> BrowserSnapshot {
        BrowserSnapshot {
            page_id: "page-1".into(),
            url: "https://example.test/form".into(),
            dom_revision: revision,
            state: json!({"button":"Submit"}),
            evidence: vec![format!("dom-{revision}")],
        }
    }

    #[test]
    fn browser_capabilities_are_truthful_and_complex_dom_input_executes_when_enabled() {
        let bridge = FakeBrowserBridge {
            snapshots: Mutex::new(
                vec![
                    browser_snapshot(1),
                    browser_snapshot(1),
                    browser_snapshot(1),
                    browser_snapshot(1),
                ]
                .into(),
            ),
            action_count: AtomicUsize::new(0),
        };
        let adapter = BrowserComputerUseAdapter::new(bridge);
        let capabilities = adapter.capabilities();

        assert!(capabilities.navigate);
        assert!(capabilities.click);
        assert!(capabilities.text_input);
        assert!(capabilities.select);
        assert!(capabilities.submit);
        assert!(capabilities.scroll);
        assert!(capabilities.history);
        assert!(capabilities.drag);
        assert!(capabilities.slider_drag);
        assert!(capabilities.key_combinations);
        assert!(capabilities.multiple_tabs);

        let observation = adapter
            .observe(&request("browser", json!({"element":"Submit"})), std::time::Duration::from_secs(30))
            .unwrap();
        for kind in [
            ComputerUseActionKind::Drag,
            ComputerUseActionKind::SliderDrag,
            ComputerUseActionKind::KeyCombination,
        ] {
            adapter
                .act(&action(kind), observation.generation, std::time::Duration::from_secs(30))
                .expect("enabled complex browser action must reach the bridge");
        }
        assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn browser_policy_can_disable_complex_dom_input_before_execution() {
        let bridge = FakeBrowserBridge {
            snapshots: Mutex::new(vec![browser_snapshot(1)].into()),
            action_count: AtomicUsize::new(0),
        };
        let adapter = BrowserComputerUseAdapter::with_policy(
            bridge,
            BrowserComputerUsePolicy {
                allow_drag: false,
                allow_key_combinations: false,
                allow_multiple_tabs: false,
            },
        );

        let observation = adapter
            .observe(&request("browser", json!({"element":"Submit"})), std::time::Duration::from_secs(30))
            .unwrap();
        for kind in [
            ComputerUseActionKind::Drag,
            ComputerUseActionKind::SliderDrag,
            ComputerUseActionKind::KeyCombination,
        ] {
            let error = adapter
                .act(&action(kind), observation.generation, std::time::Duration::from_secs(30))
                .expect_err("disabled action must be rejected");
            assert_eq!(error.code, "unsupported_action");
        }
        assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn browser_dom_change_is_stale_before_input() {
        let bridge = FakeBrowserBridge {
            snapshots: Mutex::new(vec![browser_snapshot(1), browser_snapshot(2)].into()),
            action_count: AtomicUsize::new(0),
        };
        let adapter = BrowserComputerUseAdapter::new(bridge);
        let observation = adapter
            .observe(&request("browser", json!({"element":"Submit"})), std::time::Duration::from_secs(30))
            .unwrap();

        let error = adapter
            .act(
                &action(ComputerUseActionKind::Click),
                observation.generation,
                std::time::Duration::from_secs(30),
            )
            .expect_err("changed DOM invalidates the action");

        assert_eq!(error.code, "stale_observation");
        assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 0);
    }

    struct FakeDesktopBridge {
        snapshots: Mutex<VecDeque<DesktopSnapshot>>,
        action_count: AtomicUsize,
    }

    impl DesktopBridge for FakeDesktopBridge {
        fn snapshot(
            &self,
            _request: &ComputerUseRequest,
            _remaining: std::time::Duration,
        ) -> Result<DesktopSnapshot, computer_use::ComputerUseError> {
            self.snapshots
                .lock()
                .unwrap()
                .pop_front()
                .ok_or_else(|| backend_error("missing desktop snapshot"))
        }

        fn execute(
            &self,
            _action: &ComputerUseAction,
            _expected: &DesktopSnapshot,
            _remaining: std::time::Duration,
        ) -> Result<computer_use::StepExecution, computer_use::ComputerUseError> {
            self.action_count.fetch_add(1, Ordering::SeqCst);
            Ok(computer_use::StepExecution {
                input_sent: true,
                summary: "desktop input sent".into(),
                evidence: vec![],
                ..computer_use::StepExecution::default()
            })
        }

        fn verify(
            &self,
            _criteria: &[String],
            _before: &computer_use::Observation,
            _after: &computer_use::Observation,
            _remaining: std::time::Duration,
        ) -> Result<Verification, computer_use::ComputerUseError> {
            Ok(Verification {
                achieved: false,
                visible_progress: false,
                summary: "not verified".into(),
                evidence: vec![],
            })
        }
    }

    fn desktop_snapshot(window_id: &str, dpi: u32) -> DesktopSnapshot {
        DesktopSnapshot {
            window_id: window_id.into(),
            process_id: 42,
            window_rect: [0, 0, 800, 600],
            dpi,
            webview2_overlay: false,
            state: json!({"window":"Notepad"}),
            evidence: vec![format!("capture-{window_id}")],
        }
    }

    #[test]
    fn desktop_window_or_dpi_change_is_stale_before_input() {
        for changed in [
            desktop_snapshot("window-2", 96),
            desktop_snapshot("window-1", 144),
        ] {
            let bridge = FakeDesktopBridge {
                snapshots: Mutex::new(vec![desktop_snapshot("window-1", 96), changed].into()),
                action_count: AtomicUsize::new(0),
            };
            let adapter = DesktopComputerUseAdapter::new(bridge);
            let observation = adapter
                .observe(&request("desktop", json!({"application":"notepad"})), std::time::Duration::from_secs(30))
                .unwrap();

            let error = adapter
                .act(
                    &action(ComputerUseActionKind::Click),
                    observation.generation,
                    std::time::Duration::from_secs(30),
                )
                .expect_err("changed desktop identity invalidates coordinates");

            assert_eq!(error.code, "stale_observation");
            assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn desktop_canvas_requires_fresh_image_and_consumes_generation_once() {
        let mut first = desktop_snapshot("window-1", 144);
        first.state = json!({"canvas_target":"window-canvas:1","canvas_rect":[0,40,800,560],"image":{"data_url":"data:image/png;base64,mock","sha256":"same"}});
        for changed in [false, true] {
            let mut second = first.clone();
            if changed {
                second.state["image"]["sha256"] = json!("changed");
            }
            let adapter = DesktopComputerUseAdapter::new(FakeDesktopBridge {
                snapshots: Mutex::new(vec![first.clone(), second].into()),
                action_count: AtomicUsize::new(0),
            });
            assert!(adapter.capabilities().drag);
            let observed = adapter
                .observe(&request("desktop", json!({"application":"paint"})), std::time::Duration::from_secs(30))
                .unwrap();
            assert_eq!(observed.state["image"]["sha256"], "same");
            assert!(observed.state["desktop"].get("image").is_none());
            let mut draw = action(ComputerUseActionKind::Drag);
            draw.target = "window-canvas:1".into();
            draw.arguments = json!({"points":[[0.1,0.2],[0.8,0.9]],"duration_ms":100});
            let result = adapter.act(&draw, observed.generation, std::time::Duration::from_secs(30));
            if changed {
                assert_eq!(result.unwrap_err().code, "stale_observation");
                assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 0);
            } else {
                assert!(result.is_ok());
                assert_eq!(
                    adapter.act(&draw, observed.generation, std::time::Duration::from_secs(30)).unwrap_err().code,
                    "stale_observation"
                );
                assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 1);
            }
        }
    }

    #[test]
    fn desktop_uia_canvas_bounds_change_blocks_before_input() {
        let mut first = desktop_snapshot("window-1", 96);
        first.state = json!({"elements":[{"reference":"canvas-1","rect":[10,20,100,80],"control_type":"Image","enabled":true,"offscreen":false}]});
        let mut second = first.clone();
        second.state["elements"][0]["rect"][0] = json!(11);
        let adapter = DesktopComputerUseAdapter::new(FakeDesktopBridge {
            snapshots: Mutex::new(vec![first, second].into()),
            action_count: AtomicUsize::new(0),
        });
        let observed = adapter
            .observe(&request("desktop", json!({"application":"paint"})), std::time::Duration::from_secs(30))
            .unwrap();
        let mut draw = action(ComputerUseActionKind::Drag);
        draw.target = "canvas-1".into();
        assert_eq!(
            adapter.act(&draw, observed.generation, std::time::Duration::from_secs(30)).unwrap_err().code,
            "stale_observation"
        );
        assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 0);
    }

    /// RPR-04b §2.4：适配器层"输入前"的拒绝必须自带明确未发送的回执，
    /// 而不是留给上层按错误码猜；身份也必须指向当前动作。
    #[test]
    fn pre_input_rejections_carry_a_not_sent_receipt_for_the_current_action() {
        let adapter = DesktopComputerUseAdapter::new(FakeDesktopBridge {
            snapshots: Mutex::new(vec![desktop_snapshot("window-1", 96)].into()),
            action_count: AtomicUsize::new(0),
        });
        // 没有观察就用同一个动作尝试输入：输入前就被拒绝。
        let mut draw = action(ComputerUseActionKind::Drag);
        draw.target = "canvas-1".into();
        let error = adapter
            .act(&draw, 1, std::time::Duration::from_secs(30))
            .expect_err("没有观察必须拒绝");
        assert_eq!(error.code, "stale_observation");
        let receipt = error.receipt().expect("输入前拒绝必须带回执");
        receipt.validate().expect("回执必须自洽");
        assert_eq!(receipt.input_delivery, runtime::InputDelivery::NotSent);
        assert_eq!(receipt.partial, Some(false));
        assert_eq!(receipt.input_release, runtime::InputReleaseStatus::NotNeeded);
        assert!(error.receipt_matches(&computer_use::action_attempt_id(
            ComputerUseSurface::Desktop,
            &draw
        )));
        // 输入前失败不得阻断控制器的一次重新观察。
        assert!(!error.receipt_shows_input_may_have_been_sent(&computer_use::action_attempt_id(
            ComputerUseSurface::Desktop,
            &draw
        )));
        assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst), 0);
    }
}
