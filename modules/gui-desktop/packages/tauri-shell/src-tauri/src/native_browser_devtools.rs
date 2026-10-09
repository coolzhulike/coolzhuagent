//! 宿主固定文档读取及私有会话附着方法，不派发输入。方法枚举不能由网页、模型或HTTP请求提供。
use native_browser_protocol::PanelResource;
use tauri::{AppHandle, Manager};

pub(super) enum ReadMethod {
    FrameTargets,
    AttachFrame(String),
    DetachFrame(String),
    FrameTree,
    DocumentNodes,
    Accessibility(String),
    BoxModel(i64),
    LayoutMetrics,
    /// 文档 CSS 坐标，与 Input.dispatchMouseEvent 的视口坐标不同。
    HitTest(i32, i32),
    ResolveEditor(i64),
    EditorState(String),
    FocusedTargetState(String),
    DocumentScrollState(String),
    ReleaseEditor(String),
}
impl ReadMethod {
    pub(super) fn request(&self) -> (&'static str, String) {
        match self {
            Self::FrameTargets => ("Target.getTargets", "{}".into()),
            Self::AttachFrame(target) => ("Target.attachToTarget", serde_json::json!({"targetId":target,"flatten":true}).to_string()),
            Self::DetachFrame(session) => ("Target.detachFromTarget", serde_json::json!({"sessionId":session}).to_string()),
            Self::FrameTree => ("Page.getFrameTree", "{}".into()),
            // 只读展开树由document模块核归属，再由dom模块划定单文档范围；封闭根不授予引用。
            Self::DocumentNodes => ("DOM.getDocument", r#"{"depth":16,"pierce":true}"#.into()),
            Self::Accessibility(frame) => ("Accessibility.getFullAXTree", serde_json::json!({"depth":6,"frameId":frame}).to_string()),
            Self::BoxModel(node) => ("DOM.getBoxModel", serde_json::json!({"backendNodeId":node}).to_string()),
            Self::LayoutMetrics => ("Page.getLayoutMetrics", "{}".into()),
            Self::HitTest(x, y) => ("DOM.getNodeForLocation", serde_json::json!({
                "x":x,"y":y,"includeUserAgentShadowDOM":false,"ignorePointerEventsNone":false
            }).to_string()),
            Self::ResolveEditor(node) => ("DOM.resolveNode", serde_json::json!({"backendNodeId":node,"objectGroup":"coolzhu-native-editor"}).to_string()),
            Self::EditorState(object) => ("Runtime.callFunctionOn", serde_json::json!({
                "objectId":object,"functionDeclaration":super::native_browser_editor::READ_EDITOR,
                "returnByValue":true,"throwOnSideEffect":true,"silent":true}).to_string()),
            Self::FocusedTargetState(object) => ("Runtime.callFunctionOn", serde_json::json!({
                "objectId":object,"functionDeclaration":super::native_browser_key_input::READ_FOCUS,
                "returnByValue":true,"throwOnSideEffect":true,"silent":true}).to_string()),
            Self::DocumentScrollState(object) => ("Runtime.callFunctionOn", serde_json::json!({
                "objectId":object,"functionDeclaration":super::native_browser_scroll::READ_SCROLL,
                "returnByValue":true,"throwOnSideEffect":true,"silent":true}).to_string()),
            Self::ReleaseEditor(object) => ("Runtime.releaseObject",serde_json::json!({"objectId":object}).to_string()),
        }
    }
}

/// 固定读取方法与节点所属会话一起派发；对象ID不能借顶层会话解释。
#[cfg(windows)]
pub(super) fn dispatch_read(
    core: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
    session: Option<&str>,
    method: &ReadMethod,
    callback: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler,
) -> windows::core::Result<()> {
    use webview2_com::{CoTaskMemPWSTR, Microsoft::Web::WebView2::Win32::ICoreWebView2_11};
    use windows::core::Interface;
    let (method, parameters) = method.request();
    let method = CoTaskMemPWSTR::from(method);
    let parameters = CoTaskMemPWSTR::from(parameters.as_str());
    unsafe {
        if let Some(session) = session {
            let session = CoTaskMemPWSTR::from(session);
            core.cast::<ICoreWebView2_11>()?.CallDevToolsProtocolMethodForSession(
                *session.as_ref().as_pcwstr(), *method.as_ref().as_pcwstr(), *parameters.as_ref().as_pcwstr(), callback)
        } else {
            core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(), *parameters.as_ref().as_pcwstr(), callback)
        }
    }
}

#[cfg(windows)]
pub(super) mod bounded_callback {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2CallDevToolsProtocolMethodCompletedHandler,
        ICoreWebView2CallDevToolsProtocolMethodCompletedHandler_Impl,
    };
    use windows::core::{implement, HRESULT, PCWSTR};
    #[implement(ICoreWebView2CallDevToolsProtocolMethodCompletedHandler)]
    pub(crate) struct Handler(pub(crate) Box<dyn Fn(HRESULT, &PCWSTR) -> windows::core::Result<()>>);
    impl ICoreWebView2CallDevToolsProtocolMethodCompletedHandler_Impl for Handler_Impl {
        fn Invoke(&self, status: HRESULT, text: &PCWSTR) -> windows::core::Result<()> { (self.0)(status, text) }
    }
    /// 先核UTF-16终止符与长度，再创建String；拒绝而非截断JSON。
    pub(crate) unsafe fn read(text: &PCWSTR) -> Result<String, String> {
        if text.is_null() { return Err("native_observation_invalid".into()); }
        let mut length = 0;
        while length <= 262_144 {
            if unsafe { *text.0.add(length) } == 0 {
                return String::from_utf16(unsafe { std::slice::from_raw_parts(text.0, length) })
                    .map_err(|_| "native_observation_invalid".into());
            }
            length += 1;
        }
        Err("native_observation_too_large".into())
    }
}

pub(super) async fn read(app: &AppHandle, expected: &PanelResource, method: ReadMethod) -> Result<serde_json::Value, String> {
    read_session(app, expected, None, method).await
}

#[cfg(windows)]
fn read_resource_changed(app: &AppHandle, expected: &PanelResource, url: &str) -> bool {
    super::browser_panel::input_resource(app).as_ref() != Some(expected)
        || app.get_webview(&expected.label).and_then(|view| view.url().ok())
            .as_ref().map(|value| value.as_str()) != Some(url)
}

/// 会话由宿主目标归属模块产生，不接受网页或模型提供的CDP会话和方法。
pub(super) async fn read_session(app: &AppHandle, expected: &PanelResource, session: Option<&str>, method: ReadMethod) -> Result<serde_json::Value, String> {
    if super::browser_panel::input_resource(app).as_ref() != Some(expected) {
        return Err("native_browser_resource_changed".into());
    }
    #[cfg(windows)]
    {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler;
        let session = session.map(str::to_owned);
        let view = app.get_webview(&expected.label).ok_or("native_browser_unavailable")?;
        let url = view.url().map_err(|_| "native_browser_unavailable")?.to_string();
        let url_after_wait = url.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        let app_on_ui = app.clone();
        let resource_on_ui = expected.clone();
        let method_name = method.request().0;
        view.with_webview(move |platform| {
            let send = |result| {
                if let Ok(mut sender) = sender.lock() { if let Some(sender) = sender.take() { let _ = sender.send(result); } }
            };
            if super::browser_panel::input_resource(&app_on_ui).as_ref() != Some(&resource_on_ui) {
                send(Err("native_browser_resource_changed".into())); return;
            }
            let callback_sender = sender.clone();
            let callback_app = app_on_ui.clone();
            let handler: ICoreWebView2CallDevToolsProtocolMethodCompletedHandler = bounded_callback::Handler(Box::new(move |status, text| {
                // 固定方法名用于诊断，不回显任意浏览器错误正文或查询参数。
                let failure = if method_name == "DOM.getNodeForLocation" { "native_browser_hit_test_failed" } else { "native_observation_failed" };
                // 资源失效优先于回调失败；已知文档变化不能被通用读取错误覆盖。
                let result = if read_resource_changed(&callback_app, &resource_on_ui, &url) {
                    Err("native_browser_resource_changed".into())
                } else if status.is_err() { Err(failure.into()) } else {
                    unsafe { bounded_callback::read(text) }.and_then(|raw|
                        serde_json::from_str(&raw).map_err(|_| "native_observation_invalid".into()))
                };
                if let Ok(mut sender) = callback_sender.lock() { if let Some(sender) = sender.take() { let _ = sender.send(result); } }
                Ok(())
            })).into();
            if unsafe { platform.controller().CoreWebView2() }
                .and_then(|core| dispatch_read(&core, session.as_deref(), &method, &handler)).is_err() {
                send(Err("native_observation_failed".into()));
            }
        }).map_err(|_| if read_resource_changed(app, expected, &url_after_wait) {
            "native_browser_resource_changed".to_string()
        } else { "native_browser_unavailable".to_string() })?;
        let result = match tokio::time::timeout(std::time::Duration::from_secs(2), receiver).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err("native_observation_cancelled".to_string()),
            Err(_) => Err("native_observation_timeout".to_string()),
        };
        // 回调结果到异步消费之间也可能关闭或导航；结束归属失效优先于成功或通用错误。
        if read_resource_changed(app, expected, &url_after_wait) {
            Err("native_browser_resource_changed".into())
        } else { result }
    }
    #[cfg(not(windows))]
    { let _ = (method,session); Err("native_observation_platform_unsupported".into()) }
}

#[cfg(all(test,windows))]
mod tests {
    #[test]
    fn cdp_utf16_is_bounded_before_copying() {
        let normal: Vec<u16> = "页面".encode_utf16().chain(std::iter::once(0)).collect();
        let large: Vec<u16> = std::iter::repeat_n(120u16, 262_145).chain(std::iter::once(0)).collect();
        assert_eq!(unsafe { super::bounded_callback::read(&windows::core::PCWSTR(normal.as_ptr())) }.unwrap(), "页面");
        assert!(unsafe { super::bounded_callback::read(&windows::core::PCWSTR(large.as_ptr())) }.is_err());
        assert!(unsafe { super::bounded_callback::read(&windows::core::PCWSTR::null()) }.is_err());
    }
}
