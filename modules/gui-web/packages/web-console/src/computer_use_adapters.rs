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

pub(crate) trait BrowserBridge: Send + Sync {
    fn snapshot(&self) -> Result<BrowserSnapshot, ComputerUseError>;
    fn execute(
        &self,
        action: &ComputerUseAction,
        expected: &BrowserSnapshot,
    ) -> Result<StepExecution, ComputerUseError>;
    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
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

    fn observe(&self, _request: &ComputerUseRequest) -> Result<Observation, ComputerUseError> {
        let snapshot = self.bridge.snapshot()?;
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
    ) -> Result<StepExecution, ComputerUseError> {
        if !self.capabilities().supports(action.kind) {
            return Err(unsupported_action(action));
        }
        let expected = self
            .observed
            .lock()
            .expect("browser observation lock")
            .clone()
            .ok_or_else(|| stale_observation("browser action has no observation"))?;
        if expected.0 != expected_generation {
            return Err(stale_observation("browser observation generation changed"));
        }
        let current = self.bridge.snapshot()?;
        if !expected.1.same_input_identity(&current) {
            return Err(stale_observation(
                "browser page identity or DOM revision changed before input",
            ));
        }
        self.bridge.execute(action, &current)
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
    ) -> Result<Verification, ComputerUseError> {
        self.bridge.verify(criteria, before, after)
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
    fn snapshot(&self, request: &ComputerUseRequest) -> Result<DesktopSnapshot, ComputerUseError>;
    fn execute(
        &self,
        action: &ComputerUseAction,
        expected: &DesktopSnapshot,
    ) -> Result<StepExecution, ComputerUseError>;
    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
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

    fn observe(&self, request: &ComputerUseRequest) -> Result<Observation, ComputerUseError> {
        let snapshot = self.bridge.snapshot(request)?;
        if snapshot.webview2_overlay {
            return Err(surface_conflict(
                "desktop target is covered by a WebView2 surface",
            ));
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let mut desktop_state = snapshot.state.clone();
        let image = desktop_state.as_object_mut().and_then(|state| state.remove("image"));
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
    ) -> Result<StepExecution, ComputerUseError> {
        if !self.capabilities().supports(action.kind) {
            return Err(unsupported_action(action));
        }
        let expected = self
            .observed
            .lock()
            .expect("desktop observation lock")
            .clone()
            .ok_or_else(|| stale_observation("desktop action has no observation"))?;
        if expected.0 != expected_generation {
            return Err(stale_observation("desktop observation generation changed"));
        }
        let current = self.bridge.snapshot(&ComputerUseRequest {
            objective: String::new(),
            surface: ComputerUseSurface::Desktop,
            target: None,
            success_criteria: Vec::new(),
            constraints: Vec::new(),
        })?;
        if current.webview2_overlay {
            return Err(surface_conflict(
                "desktop target became covered by a WebView2 surface",
            ));
        }
        if !expected.1.same_input_identity(&current) {
            return Err(stale_observation(
                "foreground window, process, DPI, or window rectangle changed before input",
            ));
        }
        if action.kind == computer_use::ComputerUseActionKind::Drag {
            let screenshot_target = expected.1.state.get("canvas_target").and_then(JsonValue::as_str) == Some(action.target.as_str());
            if screenshot_target {
                if expected.1.state.pointer("/image/sha256").is_none() || expected.1.state.pointer("/image/sha256") != current.state.pointer("/image/sha256") {
                    return Err(stale_observation("截图画布已变化，需重新观察后规划笔画"));
                }
            } else {
                let target = |state:&JsonValue| state.get("elements").and_then(JsonValue::as_array).and_then(|elements|elements.iter().find(|element|element["reference"].as_str()==Some(action.target.as_str()))).cloned();
                if target(&expected.1.state).is_none() || target(&expected.1.state) != target(&current.state) {
                    return Err(stale_observation("UIA 画布的边界或身份已变化"));
                }
            }
        }
        // 一次观察只授权一次输入尝试；成功或失败后都需要重新观察。
        *self.observed.lock().expect("desktop observation lock") = None;
        self.bridge.execute(action, &current)
    }

    fn verify(
        &self,
        criteria: &[String],
        before: &Observation,
        after: &Observation,
    ) -> Result<Verification, ComputerUseError> {
        self.bridge.verify(criteria, before, after)
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
        fn snapshot(&self) -> Result<BrowserSnapshot, computer_use::ComputerUseError> {
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
        ) -> Result<computer_use::StepExecution, computer_use::ComputerUseError> {
            self.action_count.fetch_add(1, Ordering::SeqCst);
            Ok(computer_use::StepExecution {
                input_sent: true,
                summary: "browser action sent".into(),
                evidence: vec![],
            })
        }

        fn verify(
            &self,
            _criteria: &[String],
            _before: &computer_use::Observation,
            _after: &computer_use::Observation,
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
            .observe(&request("browser", json!({"element":"Submit"})))
            .unwrap();
        for kind in [
            ComputerUseActionKind::Drag,
            ComputerUseActionKind::SliderDrag,
            ComputerUseActionKind::KeyCombination,
        ] {
            adapter
                .act(&action(kind), observation.generation)
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
            .observe(&request("browser", json!({"element":"Submit"})))
            .unwrap();
        for kind in [
            ComputerUseActionKind::Drag,
            ComputerUseActionKind::SliderDrag,
            ComputerUseActionKind::KeyCombination,
        ] {
            let error = adapter
                .act(&action(kind), observation.generation)
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
            .observe(&request("browser", json!({"element":"Submit"})))
            .unwrap();

        let error = adapter
            .act(
                &action(ComputerUseActionKind::Click),
                observation.generation,
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
        ) -> Result<computer_use::StepExecution, computer_use::ComputerUseError> {
            self.action_count.fetch_add(1, Ordering::SeqCst);
            Ok(computer_use::StepExecution {
                input_sent: true,
                summary: "desktop input sent".into(),
                evidence: vec![],
            })
        }

        fn verify(
            &self,
            _criteria: &[String],
            _before: &computer_use::Observation,
            _after: &computer_use::Observation,
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
                .observe(&request("desktop", json!({"application":"notepad"})))
                .unwrap();

            let error = adapter
                .act(
                    &action(ComputerUseActionKind::Click),
                    observation.generation,
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
        for changed in [false,true] {
            let mut second=first.clone();
            if changed { second.state["image"]["sha256"]=json!("changed"); }
            let adapter=DesktopComputerUseAdapter::new(FakeDesktopBridge{snapshots:Mutex::new(vec![first.clone(),second].into()),action_count:AtomicUsize::new(0)});
            assert!(adapter.capabilities().drag);
            let observed=adapter.observe(&request("desktop",json!({"application":"paint"}))).unwrap();
            assert_eq!(observed.state["image"]["sha256"],"same");
            assert!(observed.state["desktop"].get("image").is_none());
            let mut draw=action(ComputerUseActionKind::Drag);draw.target="window-canvas:1".into();draw.arguments=json!({"points":[[0.1,0.2],[0.8,0.9]],"duration_ms":100});
            let result=adapter.act(&draw,observed.generation);
            if changed { assert_eq!(result.unwrap_err().code,"stale_observation");assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst),0); }
            else { assert!(result.is_ok());assert_eq!(adapter.act(&draw,observed.generation).unwrap_err().code,"stale_observation");assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst),1); }
        }
    }

    #[test]
    fn desktop_uia_canvas_bounds_change_blocks_before_input() {
        let mut first=desktop_snapshot("window-1",96);
        first.state=json!({"elements":[{"reference":"canvas-1","rect":[10,20,100,80],"control_type":"Image","enabled":true,"offscreen":false}]});
        let mut second=first.clone();second.state["elements"][0]["rect"][0]=json!(11);
        let adapter=DesktopComputerUseAdapter::new(FakeDesktopBridge{snapshots:Mutex::new(vec![first,second].into()),action_count:AtomicUsize::new(0)});
        let observed=adapter.observe(&request("desktop",json!({"application":"paint"}))).unwrap();
        let mut draw=action(ComputerUseActionKind::Drag);draw.target="canvas-1".into();
        assert_eq!(adapter.act(&draw,observed.generation).unwrap_err().code,"stale_observation");
        assert_eq!(adapter.bridge().action_count.load(Ordering::SeqCst),0);
    }
}
