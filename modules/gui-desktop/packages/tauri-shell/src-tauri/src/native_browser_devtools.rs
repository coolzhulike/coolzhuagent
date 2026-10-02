//! 宿主固定只读CDP方法。方法枚举不能由网页、模型或HTTP请求提供。
use native_browser_protocol::PanelResource;
use tauri::{AppHandle, Manager};

pub(super) enum ReadMethod {
    FrameTree,
    Document,
    DocumentNodes,
    Accessibility(String),
    BoxModel(i64),
    LayoutMetrics,
    HitTest(i32, i32),
    ResolveEditor(i64),
    EditorState(String),
    ReleaseEditor(String),
}
impl ReadMethod {
    pub(super) fn request(&self) -> (&'static str, String) {
        match self {
            Self::FrameTree => ("Page.getFrameTree", "{}".into()),
            Self::Document => ("DOM.getDocument", r#"{"depth":0,"pierce":false}"#.into()),
            Self::DocumentNodes => ("DOM.getDocument", r#"{"depth":8,"pierce":false}"#.into()),
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
            Self::ReleaseEditor(object) => ("Runtime.releaseObject",serde_json::json!({"objectId":object}).to_string()),
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
    if super::browser_panel::input_resource(app).as_ref() != Some(expected) {
        return Err("native_browser_resource_changed".into());
    }
    #[cfg(windows)]
    {
        use webview2_com::{CoTaskMemPWSTR, Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler};
        let view = app.get_webview(&expected.label).ok_or("native_browser_unavailable")?;
        let url = view.url().map_err(|_| "native_browser_unavailable")?.to_string();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = std::sync::Arc::new(std::sync::Mutex::new(Some(sender)));
        let app_on_ui = app.clone();
        let resource_on_ui = expected.clone();
        let (method, parameters) = method.request();
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
                let result = if status.is_err() { Err("native_observation_failed".into()) }
                else if super::browser_panel::input_resource(&callback_app).as_ref() != Some(&resource_on_ui)
                    || callback_app.get_webview(&resource_on_ui.label).and_then(|view| view.url().ok())
                        .as_ref().map(|value| value.as_str()) != Some(url.as_str()) {
                    Err("native_browser_resource_changed".into())
                } else {
                    unsafe { bounded_callback::read(text) }.and_then(|raw|
                        serde_json::from_str(&raw).map_err(|_| "native_observation_invalid".into()))
                };
                if let Ok(mut sender) = callback_sender.lock() { if let Some(sender) = sender.take() { let _ = sender.send(result); } }
                Ok(())
            })).into();
            let method = CoTaskMemPWSTR::from(method);
            let parameters = CoTaskMemPWSTR::from(parameters.as_str());
            if unsafe { platform.controller().CoreWebView2().and_then(|core|
                core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(), *parameters.as_ref().as_pcwstr(), &handler)) }.is_err() {
                send(Err("native_observation_failed".into()));
            }
        }).map_err(|_| "native_browser_unavailable".to_string())?;
        tokio::time::timeout(std::time::Duration::from_secs(2), receiver).await
            .map_err(|_| "native_observation_timeout".to_string())?
            .map_err(|_| "native_observation_cancelled".to_string())?
    }
    #[cfg(not(windows))]
    { let _ = method; Err("native_observation_platform_unsupported".into()) }
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
