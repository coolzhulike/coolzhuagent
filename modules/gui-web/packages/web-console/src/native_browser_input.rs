//! 认证宿主的类型化输入传输；不签发授权，不裁决任务成功，不以当前网页覆盖旧动作回执。
use std::{sync::{Mutex,OnceLock},time::{Duration,Instant}};
use axum::{extract::DefaultBodyLimit,http::{HeaderMap,StatusCode},routing::post,Json,Router};
use native_browser_protocol::{PanelInputRequest,PanelInputReply,PanelResource,INPUT_PATH,MAX_INPUT_BYTES};

struct Pending { host_id:String,request:PanelInputRequest,delivered:bool,sender:std::sync::mpsc::Sender<PanelInputReply> }
fn pending() -> &'static Mutex<Option<Pending>> {
    static VALUE:OnceLock<Mutex<Option<Pending>>>=OnceLock::new();VALUE.get_or_init(||Mutex::new(None))
}
struct Guard(String);
impl Drop for Guard {
    fn drop(&mut self) { if let Ok(mut value)=pending().lock() {
        if value.as_ref().is_some_and(|v| v.request.request_id==self.0) { *value=None; }
    } }
}
pub(super) fn routes() -> Router {
    Router::new().route(INPUT_PATH,post(receive).layer(DefaultBodyLimit::max(MAX_INPUT_BYTES)))
}
pub(super) fn deliver(host_id:&str,resource:Option<&PanelResource>) -> Option<PanelInputRequest> {
    let mut value=pending().lock().ok()?;let value=value.as_mut()?;
    if value.delivered || value.host_id!=host_id || resource!=Some(&value.request.resource) { return None; }
    value.delivered=true;Some(value.request.clone())
}
async fn receive(headers:HeaderMap,Json(reply):Json<PanelInputReply>) -> Result<StatusCode,StatusCode> {
    if !crate::native_browser_host::authenticated(&headers) { return Err(StatusCode::UNAUTHORIZED); }
    if !reply.valid_shape() { return Err(StatusCode::BAD_REQUEST); }
    let mut value=pending().lock().map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    let request=value.as_ref().ok_or(StatusCode::CONFLICT)?;
    // 导航/关闭不能抹掉原动作释放回执；只接受确切已派发请求对应的认证宿主回执。
    if !request.delivered || request.host_id!=reply.host_id || request.request.request_id!=reply.request_id
        || request.request.resource!=reply.resource { return Err(StatusCode::CONFLICT); }
    // 回执阶段和执行实例也必须与原始请求相符，不能把预检回执用于执行结算。
    if !phase_matches(&request.request.command,&reply) { return Err(StatusCode::CONFLICT); }
    value.take().ok_or(StatusCode::CONFLICT)?.sender.send(reply).map_err(|_|StatusCode::CONFLICT)?;
    Ok(StatusCode::NO_CONTENT)
}

fn phase_matches(command:&native_browser_protocol::PanelInputCommand,reply:&PanelInputReply) -> bool {
    match command {
        native_browser_protocol::PanelInputCommand::PrepareClick {..} | native_browser_protocol::PanelInputCommand::PrepareScroll {..}
        | native_browser_protocol::PanelInputCommand::PrepareText {..} | native_browser_protocol::PanelInputCommand::PrepareNavigate {..} =>
            matches!(reply.outcome,native_browser_protocol::PanelInputOutcome::Prepared|native_browser_protocol::PanelInputOutcome::NotDispatched)
                && reply.attempt_id.is_none() && reply.executor_instance_id.is_none(),
        native_browser_protocol::PanelInputCommand::ExecuteClick {ticket_id,attempt_id,executor_instance_id,..} =>
            matches!(reply.outcome,native_browser_protocol::PanelInputOutcome::Released|native_browser_protocol::PanelInputOutcome::ReleaseUnknown|native_browser_protocol::PanelInputOutcome::NotDispatched)
                && reply.ticket_id.as_deref()==Some(ticket_id.as_str())
                && reply.attempt_id.as_deref()==Some(attempt_id.as_str())
                && reply.executor_instance_id.as_deref()==Some(executor_instance_id.as_str()),
        native_browser_protocol::PanelInputCommand::ExecuteScroll {ticket_id,attempt_id,executor_instance_id,..}
        | native_browser_protocol::PanelInputCommand::ExecuteText {ticket_id,attempt_id,executor_instance_id,..}
        | native_browser_protocol::PanelInputCommand::ExecuteNavigate {ticket_id,attempt_id,executor_instance_id,..} =>
            matches!(reply.outcome,native_browser_protocol::PanelInputOutcome::Acknowledged|native_browser_protocol::PanelInputOutcome::DispatchUnknown|native_browser_protocol::PanelInputOutcome::NotDispatched)
                && !reply.down_confirmed && !reply.up_confirmed
                && reply.ticket_id.as_deref()==Some(ticket_id.as_str())
                && reply.attempt_id.as_deref()==Some(attempt_id.as_str())
                && reply.executor_instance_id.as_deref()==Some(executor_instance_id.as_str())
                && match command {
                    native_browser_protocol::PanelInputCommand::ExecuteNavigate {url,..} => {
                        if reply.outcome == native_browser_protocol::PanelInputOutcome::Acknowledged {
                            reply.navigation.as_ref().is_some_and(|nav| nav.url == *url && nav.matches_source(&reply.resource))
                        } else { reply.navigation.is_none() }
                    },
                    _ => reply.navigation.is_none(),
                },
    }
}
pub(super) fn request(host_id:&str,request:PanelInputRequest,remaining:Duration,cancelled:Option<&dyn Fn()->bool>) -> Result<PanelInputReply,String> {
    if !request.valid_shape() { return Err("native_input_invalid".into()); }
    let (sender,receiver)=std::sync::mpsc::channel();let id=request.request_id.clone();
    {
        let mut value=pending().lock().map_err(|_|"native_input_unavailable")?;
        if value.is_some() { return Err("native_input_busy".into()); }
        *value=Some(Pending {host_id:host_id.into(),request,delivered:false,sender});
    }
    let _guard=Guard(id);let start=Instant::now();let limit=remaining.min(Duration::from_secs(8));
    loop {
        if cancelled.is_some_and(|check|check()) { return Err("native_input_cancelled".into()); }
        if start.elapsed()>=limit { return Err("native_input_receipt_timeout".into()); }
        match receiver.recv_timeout((limit-start.elapsed().min(limit)).min(Duration::from_millis(50))) {
            Ok(reply)=>return Ok(reply),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>{},
            Err(_)=>return Err("native_input_receipt_missing".into()),
        }
    }
}
pub(super) fn random_id() -> Result<String,String> {
    let mut bytes=[0u8;16];getrandom::fill(&mut bytes).map_err(|_|"native_input_unavailable")?;
    Ok(bytes.iter().map(|b|format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use native_browser_protocol::*;

    #[test]
    fn wheel_ack_cannot_settle_click_or_claim_mouse_release() {
        let target=PanelClickTarget {observation_id:"1".repeat(32),document_token:"2".repeat(32),node_id:"3".repeat(32)};
        let resource=PanelResource {workspace_path:"workspace".into(),room_id:"room-1".into(),label:"browser-panel-1".into(),generation:1,navigation_revision:1};
        let click=PanelInputCommand::ExecuteClick {target:target.clone(),ticket_id:"4".repeat(32),permit_id:"permit-1".into(),
            attempt_id:"attempt-1".into(),executor_instance_id:"5".repeat(32),expires_at_unix_ms:100};
        let scroll=PanelInputCommand::ExecuteScroll {target,direction:ScrollDirection::Down,amount:1,ticket_id:"4".repeat(32),permit_id:"permit-1".into(),
            attempt_id:"attempt-1".into(),executor_instance_id:"5".repeat(32),expires_at_unix_ms:100};
        let mut reply=PanelInputReply {host_id:"native-host-0123456789".into(),request_id:"6".repeat(32),resource,
            outcome:PanelInputOutcome::Acknowledged,ticket_id:Some("4".repeat(32)),expires_at_unix_ms:None,
            attempt_id:Some("attempt-1".into()),executor_instance_id:Some("5".repeat(32)),down_confirmed:false,up_confirmed:false,navigation:None,error:None};
        assert!(reply.valid_shape() && phase_matches(&scroll,&reply));
        assert!(!phase_matches(&click,&reply));
        let nav=PanelInputCommand::ExecuteNavigate {target:PanelClickTarget {observation_id:"1".repeat(32),document_token:"2".repeat(32),node_id:"3".repeat(32)},
            url:"https://example.invalid/next".into(),ticket_id:"4".repeat(32),permit_id:"permit-1".into(),attempt_id:"attempt-1".into(),executor_instance_id:"5".repeat(32),expires_at_unix_ms:100};
        assert!(!phase_matches(&nav,&reply),"普通ACK不能伪造导航目标资源");
        let mut destination=reply.resource.clone();destination.navigation_revision+=1;
        reply.navigation=Some(PanelNavigationReceipt {destination,url:"https://example.invalid/next".into()});
        assert!(reply.valid_shape() && phase_matches(&nav,&reply));
        assert!(!phase_matches(&scroll,&reply),"导航回执不能借用于滚动");
        reply.navigation.as_mut().unwrap().destination.room_id="other-room".into();
        assert!(!reply.valid_shape() && !phase_matches(&nav,&reply));
        reply.navigation=None;
        reply.up_confirmed=true;
        assert!(!reply.valid_shape() && !phase_matches(&scroll,&reply));
        reply.up_confirmed=false;
        reply.attempt_id=Some("other-attempt".into());
        assert!(!phase_matches(&scroll,&reply));
        reply.attempt_id=Some("attempt-1".into());
        reply.outcome=PanelInputOutcome::DispatchUnknown;
        reply.error=Some("native_input_not_confirmed".into());
        assert!(reply.valid_shape() && phase_matches(&scroll,&reply));
        assert!(!phase_matches(&click,&reply));
    }
}
