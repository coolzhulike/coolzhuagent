//! 页面内单键输入；固定只读焦点预检，真实按下和释放，不写 DOM 或发送系统快捷键。
use native_browser_protocol::{PanelClickTarget, PanelInputOutcome, PanelResource};
use serde_json::Value;
use tauri::{AppHandle, Manager};
use super::{native_browser_devtools::{self, ReadMethod}, native_browser_target::VerifiedTarget};

pub(super) const READ_FOCUS: &str = r#"function(){
 const tag=this.tagName;const kind=tag==='INPUT'?this.type:tag.toLowerCase();
 if(!this.isConnected||this.ownerDocument.activeElement!==this||this.disabled||this.readOnly||
    !['SELECT','INPUT','BUTTON','TEXTAREA','A'].includes(tag)||
    (tag==='INPUT'&&['password','file','hidden'].includes(kind)))return null;
 return {focused:true,tag,kind};
}"#;

fn state(raw:&Value)->Result<Value,String> {
    let value=raw.pointer("/result/value").filter(|v|v.is_object()).ok_or("native_browser_key_target_not_focused")?;
    if raw.get("exceptionDetails").is_some() || value["focused"]!=true
        || !matches!(value["tag"].as_str(),Some("SELECT"|"INPUT"|"BUTTON"|"TEXTAREA"|"A"))
        || value["kind"].as_str().is_none_or(|s|s.len()>32 || matches!(s,"password"|"file"|"hidden")) {
        return Err("native_browser_key_target_invalid".into());
    }
    Ok(value.clone())
}

pub(super) async fn verify(app:&AppHandle,resource:&PanelResource,target:&PanelClickTarget)->Result<VerifiedTarget,String> {
    let mut verified=super::native_browser_target::verify(app,resource,&target.observation_id,&target.document_token,&target.node_id).await?;
    if !verified.node.scope.is_top() {return Err("native_browser_frame_operation_unsupported".into());}
    let object=super::native_browser_editor::resolve(app,&verified).await?;
    let focus=native_browser_devtools::read(app,resource,ReadMethod::FocusedTargetState(object.clone())).await.and_then(|v|state(&v));
    let _=native_browser_devtools::read(app,resource,ReadMethod::ReleaseEditor(object)).await;
    verified.editor=Some(focus?);
    Ok(verified)
}

fn now()->u64 {std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64}

pub(super) async fn press(app:&AppHandle,target:VerifiedTarget,keys:Vec<String>,expires_ms:u64)->(PanelInputOutcome,bool,bool) {
    if !native_browser_protocol::valid_input_keys(&keys) || target.editor.is_none() || now()>=expires_ms {
        return (PanelInputOutcome::NotDispatched,false,false);
    }
    #[cfg(windows)]
    {
        use std::sync::{Arc,Mutex};
        use webview2_com::{CoTaskMemPWSTR,Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler};
        use super::native_browser_devtools::bounded_callback;
        let Some(view)=app.get_webview(&target.resource.label) else {return (PanelInputOutcome::NotDispatched,false,false);};
        let object=match super::native_browser_editor::resolve(app,&target).await {Ok(v)=>v,Err(_)=>return (PanelInputOutcome::NotDispatched,false,false)};
        let (key,code,vk)=match keys[0].as_str() {
            "home"=>("Home","Home",36),"end"=>("End","End",35),"tab"=>("Tab","Tab",9),
            "enter"=>("Enter","Enter",13),"escape"=>("Escape","Escape",27),_=>unreachable!(),
        };
        let (sender,receiver)=tokio::sync::oneshot::channel();let sender=Arc::new(Mutex::new(Some(sender)));
        let app_ui=app.clone();let resource=target.resource.clone();
        let (method,params)=ReadMethod::FocusedTargetState(object.clone()).request();
        let queued=view.with_webview(move |platform| {
            let finish=|value| {if let Ok(mut slot)=sender.lock() {if let Some(sender)=slot.take() {let _=sender.send(value);}}};
            if now()>=expires_ms || super::browser_panel::input_resource(&app_ui).as_ref()!=Some(&target.resource) {
                finish((PanelInputOutcome::NotDispatched,false,false));return;
            }
            let core=match unsafe {platform.controller().CoreWebView2()} {Ok(v)=>v,Err(_)=>{finish((PanelInputOutcome::NotDispatched,false,false));return;}};
            let callback_core=core.clone();let callback_sender=sender.clone();
            let handler:ICoreWebView2CallDevToolsProtocolMethodCompletedHandler=bounded_callback::Handler(Box::new(move |status,raw| {
                let finish=|value| {if let Ok(mut slot)=callback_sender.lock() {if let Some(sender)=slot.take() {let _=sender.send(value);}}};
                let actual=if status.is_ok() {unsafe {bounded_callback::read(raw)}.ok()
                    .and_then(|raw|serde_json::from_str(&raw).ok()).and_then(|raw|state(&raw).ok())} else {None};
                if now()>=expires_ms || super::browser_panel::input_resource(&app_ui).as_ref()!=Some(&target.resource)
                    || actual.is_none() || actual!=target.editor {finish((PanelInputOutcome::NotDispatched,false,false));return Ok(());}
                let phases=Arc::new(Mutex::new((None::<bool>,None::<bool>)));
                let make_ack=|pressed:bool| {
                    let phases=phases.clone();let sender=callback_sender.clone();
                    let ack:ICoreWebView2CallDevToolsProtocolMethodCompletedHandler=bounded_callback::Handler(Box::new(move |status,raw| {
                        let confirmed=status.is_ok() && unsafe {bounded_callback::read(raw)}.is_ok_and(|raw|
                            serde_json::from_str::<Value>(&raw).is_ok_and(|v|v.as_object().is_some_and(|v|v.is_empty())));
                        if let Ok(mut phase)=phases.lock() {
                            let slot=if pressed {&mut phase.0} else {&mut phase.1};if slot.is_none() {*slot=Some(confirmed);}
                            if let (Some(down),Some(up))=*phase {if let Ok(mut sender)=sender.lock() {if let Some(sender)=sender.take() {
                                let _=sender.send((if down&&up {PanelInputOutcome::Released} else {PanelInputOutcome::ReleaseUnknown},down,up));
                            }}}
                        }
                        Ok(())
                    })).into();ack
                };
                let down=make_ack(true);let up=make_ack(false);let method=CoTaskMemPWSTR::from("Input.dispatchKeyEvent");
                let params=CoTaskMemPWSTR::from(serde_json::json!({"type":"rawKeyDown","key":key,"code":code,"windowsVirtualKeyCode":vk,"modifiers":0}).to_string().as_str());
                let down_queued=unsafe {callback_core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*params.as_ref().as_pcwstr(),&down)}.is_ok();
                // 不等待按下回执；同一 UI 闭包总是尝试释放，包括按下入队失败时。
                let params=CoTaskMemPWSTR::from(serde_json::json!({"type":"keyUp","key":key,"code":code,"windowsVirtualKeyCode":vk,"modifiers":0}).to_string().as_str());
                let up_queued=unsafe {callback_core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*params.as_ref().as_pcwstr(),&up)}.is_ok();
                if !down_queued||!up_queued {finish((PanelInputOutcome::ReleaseUnknown,false,false));}
                Ok(())
            })).into();
            let method=CoTaskMemPWSTR::from(method);let params=CoTaskMemPWSTR::from(params.as_str());
            if unsafe {core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*params.as_ref().as_pcwstr(),&handler)}.is_err() {finish((PanelInputOutcome::NotDispatched,false,false));}
        });
        let outcome=if queued.is_err() {(PanelInputOutcome::ReleaseUnknown,false,false)} else {
            tokio::time::timeout(std::time::Duration::from_secs(3),receiver).await.ok().and_then(Result::ok).unwrap_or((PanelInputOutcome::ReleaseUnknown,false,false))
        };
        let _=native_browser_devtools::read(app,&resource,ReadMethod::ReleaseEditor(object)).await;
        outcome
    }
    #[cfg(not(windows))]
    {let _=(app,target,keys,expires_ms);(PanelInputOutcome::NotDispatched,false,false)}
}
