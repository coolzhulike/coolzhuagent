//! 右栏 WebView2 的只读观察。固定 Accessibility 调用，不新增网页 IPC，不执行 JS。
use native_browser_protocol::{ObservedNode, PageObservation, PanelResource};
use tauri::{AppHandle, Manager};

#[cfg(windows)]
mod bounded_callback {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2CallDevToolsProtocolMethodCompletedHandler,
        ICoreWebView2CallDevToolsProtocolMethodCompletedHandler_Impl,
    };
    use windows::core::{implement, HRESULT, PCWSTR};

    #[implement(ICoreWebView2CallDevToolsProtocolMethodCompletedHandler)]
    pub(super) struct Handler(pub(super) Box<dyn Fn(HRESULT, &PCWSTR) -> windows::core::Result<()>>);

    impl ICoreWebView2CallDevToolsProtocolMethodCompletedHandler_Impl for Handler_Impl {
        fn Invoke(&self, status: HRESULT, text: &PCWSTR) -> windows::core::Result<()> {
            (self.0)(status, text)
        }
    }

    /// COM 保证返回值是调用期间有效的零终止字符串；在创建 Rust String 前限长。
    pub(super) unsafe fn read(text: &PCWSTR) -> Result<String, String> {
        if text.is_null() { return Err("native_observation_invalid".into()); }
        // 最坏 UTF-8 字节数仍低于 project_tree 的 1MiB 上限，拒绝而非截断 JSON。
        let limit = 262_144;
        let mut length = 0;
        while length <= limit {
            if unsafe { *text.0.add(length) } == 0 {
                return String::from_utf16(unsafe { std::slice::from_raw_parts(text.0, length) })
                    .map_err(|_| "native_observation_invalid".into());
            }
            length += 1;
        }
        Err("native_observation_too_large".into())
    }
}

fn bounded_text(value: Option<&serde_json::Value>, limit: usize) -> String {
    value.and_then(|value| value.get("value")).and_then(serde_json::Value::as_str)
        .unwrap_or_default().chars().filter(|character| !character.is_control()).take(limit).collect()
}

fn project_tree(raw: &str) -> Result<PageObservation, String> {
    if raw.len() > 1_048_576 { return Err("native_observation_too_large".into()); }
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|_| "native_observation_invalid")?;
    let nodes = value.get("nodes").and_then(serde_json::Value::as_array)
        .ok_or("native_observation_invalid")?;
    let mut result = PageObservation {url:String::new(), title:String::new(), nodes:Vec::new(), truncated:false};
    for node in nodes {
        if node.get("ignored").and_then(serde_json::Value::as_bool) != Some(false) { continue; }
        let role = bounded_text(node.get("role"), 64);
        let name = bounded_text(node.get("name"), 256);
        if role == "RootWebArea" { result.title = name.clone(); }
        if role.is_empty() && name.is_empty() { continue; }
        if result.nodes.len() == 128 { result.truncated = true; break; }
        // 不提取 value、properties 或属性；name 仍是可能含私密文本的不可信页面内容。
        result.nodes.push(ObservedNode {role, name});
    }
    Ok(result)
}

pub(super) async fn observe(app: &AppHandle, expected: &PanelResource) -> Result<PageObservation, String> {
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
        view.with_webview(move |platform| {
            // COM 派发前在所属 UI 线程再次核对资源；租约不是授权，也不替代这里的核对。
            let send = |result| {
                if let Ok(mut sender) = sender.lock() {
                    if let Some(sender) = sender.take() { let _ = sender.send(result); }
                }
            };
            if super::browser_panel::input_resource(&app_on_ui).as_ref() != Some(&resource_on_ui) {
                send(Err("native_browser_resource_changed".into())); return;
            }
            let callback_sender = sender.clone();
            let callback_app = app_on_ui.clone();
            let handler: ICoreWebView2CallDevToolsProtocolMethodCompletedHandler = bounded_callback::Handler(Box::new(move |status, text| {
                let result = if status.is_err() {
                    Err("native_observation_failed".into())
                } else if super::browser_panel::input_resource(&callback_app).as_ref() != Some(&resource_on_ui) {
                    Err("native_browser_resource_changed".into())
                } else if callback_app.get_webview(&resource_on_ui.label)
                    .and_then(|view| view.url().ok()).as_ref().map(|value| value.as_str()) != Some(url.as_str()) {
                    Err("native_browser_resource_changed".into())
                } else {
                    // 在 UTF-16 转 String 前限长；只读结果也不写原始 CDP 响应到日志。
                    unsafe { bounded_callback::read(text) }.and_then(|text| project_tree(&text)).and_then(|mut observation| {
                        observation.url = url.clone();
                        if observation.valid_shape() { Ok(observation) } else { Err("native_observation_invalid".into()) }
                    })
                };
                if let Ok(mut sender) = callback_sender.lock() {
                    if let Some(sender) = sender.take() { let _ = sender.send(result); }
                }
                Ok(())
            })).into();
            let method = CoTaskMemPWSTR::from("Accessibility.getFullAXTree");
            let parameters = CoTaskMemPWSTR::from(r#"{"depth":6}"#);
            let result = unsafe { platform.controller().CoreWebView2()
                .and_then(|core| core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(), *parameters.as_ref().as_pcwstr(), &handler)) };
            if result.is_err() { send(Err("native_observation_failed".into())); }
        }).map_err(|_| "native_browser_unavailable".to_string())?;
        tokio::time::timeout(std::time::Duration::from_secs(2), receiver).await
            .map_err(|_| "native_observation_timeout".to_string())?
            .map_err(|_| "native_observation_cancelled".to_string())?
    }
    #[cfg(not(windows))]
    { Err("native_observation_platform_unsupported".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ax_projection_is_bounded_and_does_not_collect_input_values() {
        let raw = r#"{"nodes":[{"ignored":false,"role":{"value":"textbox"},"name":{"value":"口令"},"value":{"value":"SECRET"},"properties":[{"name":"value","value":"SECRET"}]},{"ignored":true,"name":{"value":"HIDDEN"}},{"ignored":false,"role":{"value":"button"},"name":{"value":"查询"}}]}"#;
        let mut observation = project_tree(raw).unwrap();
        observation.url = "https://example.invalid/".into();
        let json = serde_json::to_string(&observation).unwrap();
        assert!(observation.valid_shape());
        assert!(!json.contains("SECRET") && !json.contains("HIDDEN"));
        assert_eq!(observation.nodes.len(), 2);
        assert!(project_tree(&"x".repeat(1_048_577)).is_err());
        #[cfg(windows)]
        {
            let normal: Vec<u16> = "页面".encode_utf16().chain(std::iter::once(0)).collect();
            let oversized: Vec<u16> = std::iter::repeat_n(120u16, 262_145).chain(std::iter::once(0)).collect();
            assert_eq!(unsafe { bounded_callback::read(&windows::core::PCWSTR(normal.as_ptr())) }.unwrap(), "页面");
            assert!(unsafe { bounded_callback::read(&windows::core::PCWSTR(oversized.as_ptr())) }.is_err());
            assert!(unsafe { bounded_callback::read(&windows::core::PCWSTR::null()) }.is_err());
        }
    }
}
