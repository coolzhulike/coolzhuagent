//! 持久WebView宿主的单次点击；预检票据不授予输入权限，按下后必须尝试释放。
use std::{collections::HashMap, sync::{Mutex, OnceLock}, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use native_browser_protocol::{PanelClickTarget, PanelInputCommand, PanelInputOutcome, PanelInputReply, PanelInputRequest};
use tauri::{AppHandle, Manager};
use super::native_browser_target::{self, VerifiedTarget};

struct Ticket { target: PanelClickTarget, verified: VerifiedTarget, expiry: Instant, expires_ms: u64 }
static IN_FLIGHT:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);
struct Flight;
impl Drop for Flight {fn drop(&mut self) {IN_FLIGHT.store(false,std::sync::atomic::Ordering::SeqCst);}}
fn tickets() -> &'static Mutex<HashMap<String,Ticket>> {
    static VALUE: OnceLock<Mutex<HashMap<String,Ticket>>> = OnceLock::new();
    VALUE.get_or_init(|| Mutex::new(HashMap::new()))
}
fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis().min(u128::from(u64::MAX)) as u64 }
fn random_id() -> Result<String,String> {
    let mut bytes = [0u8;16]; getrandom::fill(&mut bytes).map_err(|_| "native_input_unavailable")?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

pub(super) async fn handle(app: &AppHandle, host_id: String, request: PanelInputRequest) -> PanelInputReply {
    let mut reply = PanelInputReply {host_id,request_id:request.request_id.clone(),resource:request.resource.clone(),
        outcome:PanelInputOutcome::NotDispatched,ticket_id:None,expires_at_unix_ms:None,
        attempt_id:None,executor_instance_id:None,down_confirmed:false,up_confirmed:false,error:None};
    if !request.valid_shape() { reply.error=Some("native_input_invalid".into()); return reply; }
    match &request.command {
        PanelInputCommand::PrepareClick {target} => {
            let result = native_browser_target::verify(app,&request.resource,&target.observation_id,&target.document_token,&target.node_id).await;
            match result {
                Ok(verified) => {
                    let result = (|| {
                        let id = random_id()?;
                        let expires_ms = now().saturating_add(2000);
                        let mut cache = tickets().lock().map_err(|_| "native_input_unavailable")?;
                        cache.retain(|_,ticket| ticket.expiry > Instant::now());
                        if cache.len() >= 4 { return Err("native_input_busy".to_string()); }
                        cache.insert(id.clone(),Ticket {target:target.clone(),verified,expiry:Instant::now()+Duration::from_secs(2),expires_ms});
                        Ok((id,expires_ms))
                    })();
                    match result {
                        Ok((id,expiry)) => { reply.outcome=PanelInputOutcome::Prepared;reply.ticket_id=Some(id);reply.expires_at_unix_ms=Some(expiry); },
                        Err(error) => reply.error=Some(error),
                    }
                },
                Err(error) => reply.error=Some(error),
            }
        },
        PanelInputCommand::ExecuteClick {target,ticket_id,expires_at_unix_ms,attempt_id,executor_instance_id,..} => {
            reply.ticket_id=Some(ticket_id.clone());reply.attempt_id=Some(attempt_id.clone());reply.executor_instance_id=Some(executor_instance_id.clone());
            if IN_FLIGHT.compare_exchange(false,true,std::sync::atomic::Ordering::SeqCst,std::sync::atomic::Ordering::SeqCst).is_err() {
                reply.error=Some("native_input_busy".into());return reply;
            }
            let _flight=Flight;
            // 先取走，一次性消费；失配、过期或重检失败均不能复活此票据。
            let ticket = tickets().lock().ok().and_then(|mut cache| cache.remove(ticket_id));
            let Some(ticket) = ticket else { reply.error=Some("native_input_ticket_missing".into());return reply; };
            if ticket.expiry <= Instant::now() || now() >= *expires_at_unix_ms || *expires_at_unix_ms > ticket.expires_ms
                || ticket.target != *target || ticket.verified.resource != request.resource {
                reply.error=Some("native_input_ticket_stale".into());return reply;
            }
            let checked = native_browser_target::verify(app,&request.resource,&target.observation_id,&target.document_token,&target.node_id).await;
            let verified = match checked {
                Ok(value) if value.document == ticket.verified.document && value.node.backend_node == ticket.verified.node.backend_node
                    && value.x == ticket.verified.x && value.y == ticket.verified.y => value,
                Ok(_) => { reply.error=Some("native_browser_node_changed".into());return reply; },
                Err(error) => { reply.error=Some(error);return reply; },
            };
            let (outcome,down,up) = click(app,verified,*expires_at_unix_ms).await;
            let _ = super::native_browser_nodes::retire();
            reply.outcome=outcome;reply.down_confirmed=down;reply.up_confirmed=up;
            if outcome != PanelInputOutcome::Released { reply.error=Some("native_input_not_confirmed".into()); }
        },
    }
    reply
}

/// 按下/释放使用同一个原始controller。导航或取消不能截断释放；COM入队不等于回执成功。
async fn click(app: &AppHandle, target: VerifiedTarget, expires_ms: u64) -> (PanelInputOutcome,bool,bool) {
    if now() >= expires_ms || super::browser_panel::input_resource(app).as_ref() != Some(&target.resource) {
        return (PanelInputOutcome::NotDispatched,false,false);
    }
    #[cfg(windows)]
    {
        use webview2_com::{CoTaskMemPWSTR, Microsoft::Web::WebView2::Win32::ICoreWebView2CallDevToolsProtocolMethodCompletedHandler};
        use super::native_browser_devtools::bounded_callback;
        let Some(view) = app.get_webview(&target.resource.label) else { return (PanelInputOutcome::NotDispatched,false,false); };
        let (sender,receiver) = tokio::sync::oneshot::channel();
        let sender = std::sync::Arc::new(Mutex::new(Some(sender)));
        let sender_outside = sender.clone();
        let app_ui=app.clone();
        let queued = view.with_webview(move |platform| {
            let finish = |value| { if let Ok(mut sender)=sender.lock() { if let Some(sender)=sender.take() { let _=sender.send(value); } } };
            if now() >= expires_ms || super::browser_panel::input_resource(&app_ui).as_ref() != Some(&target.resource) {
                finish((PanelInputOutcome::NotDispatched,false,false));return;
            }
            let core = match unsafe { platform.controller().CoreWebView2() } {
                Ok(core)=>core,Err(_)=>{finish((PanelInputOutcome::NotDispatched,false,false));return;}
            };
            let phases=std::sync::Arc::new(Mutex::new((None::<bool>,None::<bool>)));
            let make_handler=|pressed:bool| {
                let phases=phases.clone();let sender=sender.clone();
                let handler:ICoreWebView2CallDevToolsProtocolMethodCompletedHandler=bounded_callback::Handler(Box::new(move |status,text| {
                    let confirmed=status.is_ok() && unsafe {bounded_callback::read(text)}.is_ok_and(|raw|
                        serde_json::from_str::<serde_json::Value>(&raw).is_ok_and(|v|v.as_object().is_some_and(|v|v.is_empty())));
                    if let Ok(mut phase)=phases.lock() {
                        let slot=if pressed {&mut phase.0} else {&mut phase.1};
                        if slot.is_none() { *slot=Some(confirmed); }
                        if let (Some(down),Some(up))=*phase { if let Ok(mut sender)=sender.lock() { if let Some(sender)=sender.take() {
                            let _=sender.send((if down && up {PanelInputOutcome::Released} else {PanelInputOutcome::ReleaseUnknown},down,up));
                        } } }
                    }
                    Ok(())
                })).into();handler
            };
            let down_handler=make_handler(true);let up_handler=make_handler(false);
            let release_params=serde_json::json!({"type":"mouseReleased","x":target.x,"y":target.y,"button":"left","buttons":0,"clickCount":1}).to_string();
            let method=CoTaskMemPWSTR::from("Input.dispatchMouseEvent");
            let press_params=serde_json::json!({"type":"mousePressed","x":target.x,"y":target.y,"button":"left","buttons":1,"clickCount":1}).to_string();
            let parameters=CoTaskMemPWSTR::from(press_params.as_str());
            let down_queued=unsafe {core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*parameters.as_ref().as_pcwstr(),&down_handler)}.is_ok();
            // 同一UI闭包按顺序入队两条命令；绝不等待down回调才排队up，丢回调也必须尝试释放。
            let parameters=CoTaskMemPWSTR::from(release_params.as_str());
            let up_queued=unsafe {core.CallDevToolsProtocolMethod(*method.as_ref().as_pcwstr(),*parameters.as_ref().as_pcwstr(),&up_handler)}.is_ok();
            if !down_queued || !up_queued { finish((PanelInputOutcome::ReleaseUnknown,false,false)); }
        });
        if queued.is_err() {
            // UI闭包是否已经执行无法证明，按未知处理，不能自动重放。
            drop(sender_outside);return (PanelInputOutcome::ReleaseUnknown,false,false);
        }
        tokio::time::timeout(Duration::from_secs(3),receiver).await.ok().and_then(Result::ok)
            .unwrap_or((PanelInputOutcome::ReleaseUnknown,false,false))
    }
    #[cfg(not(windows))]
    { (PanelInputOutcome::NotDispatched,false,false) }
}
