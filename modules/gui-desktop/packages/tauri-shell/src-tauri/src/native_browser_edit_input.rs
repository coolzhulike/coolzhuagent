//! 固定文字输入和导航派发；不自动聚焦，不写DOM值，不将ACK当作目标达成。
use native_browser_protocol::{PanelInputOutcome,PanelNavigationReceipt};
use tauri::{AppHandle,Manager};
use super::native_browser_target::VerifiedTarget;
fn now()->u64 {std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64}

pub(super) async fn insert_text(app:&AppHandle,target:VerifiedTarget,text:String,expires_ms:u64)->PanelInputOutcome {
    #[cfg(windows)]
    {
        use std::sync::{Arc,Mutex};
        use webview2_com::{CoTaskMemPWSTR,Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler};
        use super::native_browser_devtools::{self,ReadMethod,bounded_callback};
        if target.editor.is_none() || now()>=expires_ms {return PanelInputOutcome::NotDispatched;}
        let object=match super::native_browser_editor::resolve(app,&target).await {Ok(value)=>value,Err(_)=>return PanelInputOutcome::NotDispatched};
        let Some(view)=app.get_webview(&target.resource.label) else {return PanelInputOutcome::NotDispatched;};
        let (sender,receiver)=tokio::sync::oneshot::channel();let sender=Arc::new(Mutex::new(Some(sender)));
        let app_ui=app.clone();let resource=target.resource.clone();
        let (method,params)=ReadMethod::EditorState(object.clone()).request();
        let queued=view.with_webview(move |platform| {
            let finish=|result| {if let Ok(mut slot)=sender.lock() {if let Some(sender)=slot.take() {let _=sender.send(result);}}};
            if now()>=expires_ms || super::browser_panel::input_resource(&app_ui).as_ref()!=Some(&target.resource) {
                finish(PanelInputOutcome::NotDispatched);return;
            }
            let core=match unsafe {platform.controller().CoreWebView2()} {Ok(core)=>core,Err(_)=>{finish(PanelInputOutcome::NotDispatched);return;}};
            let callback_core=core.clone();let callback_sender=sender.clone();
            let handler:ICoreWebView2CallDevToolsProtocolMethodCompletedHandler=bounded_callback::Handler(Box::new(move |status,raw| {
                let finish=|result| {if let Ok(mut slot)=callback_sender.lock() {if let Some(sender)=slot.take() {let _=sender.send(result);}}};
                // 最后一次读回调中核对原控件/焦点/选区/值，再排队唯一一次insertText。
                let actual=if status.is_ok() {unsafe {bounded_callback::read(raw)}.ok()
                    .and_then(|raw|serde_json::from_str(&raw).ok()).and_then(|raw|super::native_browser_editor::state(&raw).ok())} else {None};
                if now()>=expires_ms || super::browser_panel::input_resource(&app_ui).as_ref()!=Some(&target.resource)
                    || actual.is_none() || actual!=target.editor {finish(PanelInputOutcome::NotDispatched);return Ok(());}
                let ack_sender=callback_sender.clone();
                let ack:ICoreWebView2CallDevToolsProtocolMethodCompletedHandler=bounded_callback::Handler(Box::new(move |status,raw| {
                    let acknowledged=status.is_ok() && unsafe {bounded_callback::read(raw)}.is_ok_and(|raw|
                        serde_json::from_str::<serde_json::Value>(&raw).is_ok_and(|value|value.as_object().is_some_and(|value|value.is_empty())));
                    if let Ok(mut slot)=ack_sender.lock() {if let Some(sender)=slot.take() {let _=sender.send(
                        if acknowledged {PanelInputOutcome::Acknowledged} else {PanelInputOutcome::DispatchUnknown});}}
                    Ok(())
                })).into();
                let method=CoTaskMemPWSTR::from("Input.insertText");let params=CoTaskMemPWSTR::from(serde_json::json!({"text":text}).to_string().as_str());
                if unsafe {callback_core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*params.as_ref().as_pcwstr(),&ack)}.is_err() {
                    finish(PanelInputOutcome::DispatchUnknown);
                }
                Ok(())
            })).into();
            let method=CoTaskMemPWSTR::from(method);let params=CoTaskMemPWSTR::from(params.as_str());
            if unsafe {core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*params.as_ref().as_pcwstr(),&handler)}.is_err() {finish(PanelInputOutcome::NotDispatched);}
        });
        let outcome=if queued.is_err() {PanelInputOutcome::DispatchUnknown} else {
            tokio::time::timeout(std::time::Duration::from_secs(3),receiver).await.ok().and_then(Result::ok).unwrap_or(PanelInputOutcome::DispatchUnknown)
        };
        let _=native_browser_devtools::read(app,&resource,ReadMethod::ReleaseEditor(object)).await;
        outcome
    }
    #[cfg(not(windows))]
    {let _=(app,target,text,expires_ms);PanelInputOutcome::NotDispatched}
}

pub(super) async fn navigate(app:&AppHandle,resource:native_browser_protocol::PanelResource,control:Option<super::native_browser_navigation::ControlResource>,url:String,expires_ms:u64)->(PanelInputOutcome,Option<PanelNavigationReceipt>) {
    #[cfg(windows)]
    {
        use webview2_com::CoTaskMemPWSTR;
        let url=match super::browser_panel::native_destination(&url) {Ok(url)=>url,Err(_)=>return (PanelInputOutcome::NotDispatched,None)};
        let Some(view)=app.get_webview(&resource.label) else {return (PanelInputOutcome::NotDispatched,None);};
        let (sender,receiver)=tokio::sync::oneshot::channel();let app_ui=app.clone();
        let queued=view.with_webview(move |platform| {
            let result=(|| {
                if now()>=expires_ms {return (PanelInputOutcome::NotDispatched,None);}
                // 在同一UI闭包里核导航序号；另一自然弹窗不能借旧控制引用获得覆盖许可。
                if control.as_ref().is_some_and(|expected|super::browser_panel::control_snapshot(&app_ui)
                    .as_ref().map(|(current,_)|current)!=Some(expected)) {return (PanelInputOutcome::NotDispatched,None);}
                // 文档引用保持原输入资格；只有独立控制引用可接管加载中的导航。
                if control.is_none() && super::browser_panel::input_resource(&app_ui).as_ref()!=Some(&resource) {
                    return (PanelInputOutcome::NotDispatched,None);
                }
                let core=match unsafe {platform.controller().CoreWebView2()} {Ok(core)=>core,Err(_)=>return (PanelInputOutcome::NotDispatched,None)};
                let navigation=match super::browser_panel::begin_native_navigation(&app_ui,&resource,&url) {
                    Ok(value)=>value,Err(_)=>return (PanelInputOutcome::NotDispatched,None)
                };
                let destination=CoTaskMemPWSTR::from(url.as_str());
                if unsafe {core.Navigate(*destination.as_ref().as_pcwstr())}.is_err() {return (PanelInputOutcome::DispatchUnknown,None);}
                (PanelInputOutcome::Acknowledged,Some(navigation))
            })();let _=sender.send(result);
        });
        if queued.is_err() {return (PanelInputOutcome::DispatchUnknown,None);}
        tokio::time::timeout(std::time::Duration::from_secs(3),receiver).await.ok().and_then(Result::ok)
            .unwrap_or((PanelInputOutcome::DispatchUnknown,None))
    }
    #[cfg(not(windows))]
    {let _=(app,resource,control,url,expires_ms);(PanelInputOutcome::NotDispatched,None)}
}
