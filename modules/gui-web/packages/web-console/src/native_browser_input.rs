//! 认证宿主的类型化输入传输；不签发授权，不裁决任务成功，不以当前网页覆盖旧动作回执。
use std::{sync::{Mutex,OnceLock},time::{Duration,Instant}};
use axum::{extract::DefaultBodyLimit,http::{HeaderMap,StatusCode},routing::post,Json,Router};
use native_browser_protocol::{PanelInputRequest,PanelInputReply,PanelResource,INPUT_PATH,MAX_INPUT_BYTES};

struct Pending { host_id:String,request:PanelInputRequest,delivered:bool,deadline:Instant,sender:std::sync::mpsc::Sender<PanelInputReply> }
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
    let mut value=pending().lock().ok()?;
    stage_delivery(&mut value,host_id,resource)
}

fn stage_delivery(value:&mut Option<Pending>,host_id:&str,resource:Option<&PanelResource>) -> Option<PanelInputRequest> {
    let request=value.as_ref()?;
    // 等待线程到期后可能尚未获调度清理；宿主领取也必须遵守同一期限。
    if Instant::now()>=request.deadline { return None; }
    if request.delivered || request.host_id!=host_id { return None; }
    if resource!=Some(&request.request.resource) {
        // 同一认证宿主已撤销资源，且请求从未领取；此锁是零交付屏障。
        // 已领取的请求绝不走这里，仍须等待真实释放回执，不能靠关闭推导释放。
        let request=value.take()?;
        let mut reply=PanelInputReply {host_id:request.host_id,request_id:request.request.request_id,
            resource:request.request.resource,outcome:native_browser_protocol::PanelInputOutcome::NotDispatched,
            ticket_id:None,expires_at_unix_ms:None,attempt_id:None,executor_instance_id:None,
            down_confirmed:false,up_confirmed:false,navigation:None,error:Some("native_browser_resource_changed".into())};
        use native_browser_protocol::PanelInputCommand;
        match request.request.command {
            PanelInputCommand::ExecuteClick {ticket_id,attempt_id,executor_instance_id,..}
            | PanelInputCommand::ExecuteKeys {ticket_id,attempt_id,executor_instance_id,..}
            | PanelInputCommand::ExecuteScroll {ticket_id,attempt_id,executor_instance_id,..}
            | PanelInputCommand::ExecuteText {ticket_id,attempt_id,executor_instance_id,..}
            | PanelInputCommand::ExecuteNavigate {ticket_id,attempt_id,executor_instance_id,..} => {
                reply.ticket_id=Some(ticket_id);reply.attempt_id=Some(attempt_id);reply.executor_instance_id=Some(executor_instance_id);
            },
            _ => {},
        }
        let _=request.sender.send(reply);
        return None;
    }
    let request=value.as_mut()?;
    request.delivered=true;
    Some(request.request.clone())
}
async fn receive(headers:HeaderMap,Json(reply):Json<PanelInputReply>) -> Result<StatusCode,StatusCode> {
    if !crate::native_browser_host::authenticated(&headers) { return Err(StatusCode::UNAUTHORIZED); }
    if !reply.valid_shape() { return Err(StatusCode::BAD_REQUEST); }
    let mut value=pending().lock().map_err(|_|StatusCode::SERVICE_UNAVAILABLE)?;
    settle_reply(&mut value,reply)
}
fn settle_reply(value:&mut Option<Pending>,reply:PanelInputReply) -> Result<StatusCode,StatusCode> {
    let request=value.as_ref().ok_or(StatusCode::CONFLICT)?;
    // 迟到回执不能利用Guard尚未清理的窗口追认成功，也不能清掉其它请求。
    if Instant::now()>=request.deadline { return Err(StatusCode::CONFLICT); }
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
        | native_browser_protocol::PanelInputCommand::PrepareKeys {..}
        | native_browser_protocol::PanelInputCommand::PrepareText {..} | native_browser_protocol::PanelInputCommand::PrepareNavigate {..} =>
            matches!(reply.outcome,native_browser_protocol::PanelInputOutcome::Prepared|native_browser_protocol::PanelInputOutcome::NotDispatched)
                && reply.attempt_id.is_none() && reply.executor_instance_id.is_none(),
        native_browser_protocol::PanelInputCommand::ExecuteClick {ticket_id,attempt_id,executor_instance_id,..}
        | native_browser_protocol::PanelInputCommand::ExecuteKeys {ticket_id,attempt_id,executor_instance_id,..} =>
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
    let start=Instant::now();let limit=remaining.min(Duration::from_secs(8));
    {
        let mut value=pending().lock().map_err(|_|"native_input_unavailable")?;
        if value.is_some() { return Err("native_input_busy".into()); }
        *value=Some(Pending {host_id:host_id.into(),request,delivered:false,deadline:start+limit,sender});
    }
    let _guard=Guard(id);
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
    fn keys_require_actual_release_instead_of_no_held_ack() {
        let (_,_,mut reply)=pending_click(Instant::now()+Duration::from_secs(8));
        let command=PanelInputCommand::ExecuteKeys {
            target:PanelClickTarget {observation_id:"1".repeat(32),document_token:"2".repeat(32),node_id:"3".repeat(32)},
            keys:vec!["end".into()],ticket_id:"4".repeat(32),permit_id:"permit-1".into(),attempt_id:"attempt-1".into(),
            executor_instance_id:"5".repeat(32),expires_at_unix_ms:100,
        };
        assert!(phase_matches(&command,&reply));
        reply.outcome=PanelInputOutcome::Acknowledged;reply.down_confirmed=false;reply.up_confirmed=false;
        assert!(reply.valid_shape());
        assert!(!phase_matches(&command,&reply),"无按住输入的ACK不能冒充按键释放");
        assert!(valid_input_keys(&["end".into()]));
        assert!(!valid_input_keys(&["ctrl".into(),"a".into()]));
        assert!(!valid_input_keys(&["meta".into()]));
    }

    #[test]
    fn resource_withdrawal_proves_zero_delivery_only_before_host_receives_request() {
        let resource=PanelResource {workspace_path:"workspace".into(),room_id:"room-1".into(),label:"browser-panel-1".into(),generation:1,navigation_revision:1};
        let command=PanelInputCommand::ExecuteClick {target:PanelClickTarget {observation_id:"1".repeat(32),document_token:"2".repeat(32),node_id:"3".repeat(32)},
            ticket_id:"4".repeat(32),permit_id:"permit-1".into(),attempt_id:"attempt-1".into(),executor_instance_id:"5".repeat(32),expires_at_unix_ms:100};
        let request=PanelInputRequest {request_id:"6".repeat(32),resource:resource.clone(),command:command.clone()};
        let (sender,receiver)=std::sync::mpsc::channel();
        let mut pending=Some(Pending {host_id:"native-host-0123456789".into(),request:request.clone(),delivered:false,deadline:Instant::now()+Duration::from_secs(8),sender});
        assert!(stage_delivery(&mut pending,"native-other-0123456789",None).is_none());
        assert!(pending.is_some() && receiver.try_recv().is_err(),"其它宿主不能撤销原动作");
        assert!(stage_delivery(&mut pending,"native-host-0123456789",None).is_none());
        let reply=receiver.try_recv().expect("未领取请求必须立即以零交付事实结账");
        assert!(reply.valid_shape() && phase_matches(&command,&reply));
        assert_eq!(reply.outcome,PanelInputOutcome::NotDispatched);
        assert_eq!(reply.resource,resource);
        assert!(!reply.down_confirmed && !reply.up_confirmed && pending.is_none());

        let (sender,receiver)=std::sync::mpsc::channel();
        let mut pending=Some(Pending {host_id:"native-host-0123456789".into(),request,delivered:false,deadline:Instant::now()+Duration::from_secs(8),sender});
        assert!(stage_delivery(&mut pending,"native-host-0123456789",Some(&resource)).is_some());
        assert!(stage_delivery(&mut pending,"native-host-0123456789",None).is_none());
        assert!(pending.as_ref().unwrap().delivered && receiver.try_recv().is_err(),"领取后关闭不能伪造未派发或释放回执");
    }

    fn pending_click(deadline:Instant) -> (Option<Pending>,std::sync::mpsc::Receiver<PanelInputReply>,PanelInputReply) {
        let resource=PanelResource {workspace_path:"workspace".into(),room_id:"room-1".into(),label:"browser-panel-1".into(),generation:1,navigation_revision:1};
        let command=PanelInputCommand::ExecuteClick {
            target:PanelClickTarget {observation_id:"1".repeat(32),document_token:"2".repeat(32),node_id:"3".repeat(32)},
            ticket_id:"4".repeat(32),permit_id:"permit-1".into(),attempt_id:"attempt-1".into(),
            executor_instance_id:"5".repeat(32),expires_at_unix_ms:100,
        };
        let request=PanelInputRequest {request_id:"6".repeat(32),resource:resource.clone(),command};
        let reply=PanelInputReply {host_id:"native-host-0123456789".into(),request_id:request.request_id.clone(),resource,
            outcome:PanelInputOutcome::Released,ticket_id:Some("4".repeat(32)),expires_at_unix_ms:None,
            attempt_id:Some("attempt-1".into()),executor_instance_id:Some("5".repeat(32)),
            down_confirmed:true,up_confirmed:true,navigation:None,error:None};
        assert!(request.valid_shape() && reply.valid_shape());
        let (sender,receiver)=std::sync::mpsc::channel();
        (Some(Pending {host_id:reply.host_id.clone(),request,delivered:false,deadline,sender}),receiver,reply)
    }

    #[test]
    fn expired_request_cannot_be_delivered_before_waiter_cleanup() {
        // 模拟等待线程已到期但尚未获调度执行Guard；不用sleep制造不稳定竞争。
        let (mut pending,receiver,reply)=pending_click(Instant::now());
        assert!(stage_delivery(&mut pending,&reply.host_id,Some(&reply.resource)).is_none(),"到期请求不能因等待线程尚未清理而被宿主领取");
        assert!(!pending.as_ref().unwrap().delivered);
        assert!(receiver.try_recv().is_err(),"传输层不能伪造释放回执");
    }

    #[test]
    fn expired_release_receipt_cannot_settle_before_waiter_cleanup() {
        let (mut pending,receiver,reply)=pending_click(Instant::now());
        pending.as_mut().unwrap().delivered=true;
        assert_eq!(settle_reply(&mut pending,reply),Err(StatusCode::CONFLICT),"超过本请求回执期限，即使Guard尚未清理也不能追认成功");
        assert!(pending.is_some() && receiver.try_recv().is_err());
    }

    #[test]
    fn original_release_receipt_survives_close_and_navigation() {
        for navigation in [false,true] {
            let (mut pending,receiver,reply)=pending_click(Instant::now()+Duration::from_secs(8));
            assert!(stage_delivery(&mut pending,&reply.host_id,Some(&reply.resource)).is_some());
            let mut next=reply.resource.clone();next.navigation_revision+=1;
            let current=if navigation {Some(&next)} else {None};
            assert!(stage_delivery(&mut pending,&reply.host_id,current).is_none());
            assert!(receiver.try_recv().is_err(),"领取后撤销资源不能冒充零交付");
            assert_eq!(settle_reply(&mut pending,reply.clone()),Ok(StatusCode::NO_CONTENT));
            assert_eq!(receiver.try_recv().unwrap().resource,reply.resource,"原动作按原资源结账");
            assert_eq!(settle_reply(&mut pending,reply),Err(StatusCode::CONFLICT),"重复回执不得再次结算");
        }
    }

    #[test]
    fn late_receipt_cannot_consume_replacement_request() {
        let (mut pending,receiver,old)=pending_click(Instant::now()+Duration::from_secs(8));
        let active=pending.as_mut().unwrap();
        active.request.request_id="7".repeat(32);
        active.request.resource.navigation_revision+=1;
        active.delivered=true;
        let mut current=old.clone();current.request_id=active.request.request_id.clone();current.resource=active.request.resource.clone();
        let mut stale_revision=current.clone();stale_revision.resource=old.resource.clone();
        let mut wrong_attempt=current.clone();wrong_attempt.attempt_id=Some("previous-attempt".into());
        let mut wrong_executor=current.clone();wrong_executor.executor_instance_id=Some("8".repeat(32));
        for rejected in [old,stale_revision,wrong_attempt,wrong_executor] {
            assert_eq!(settle_reply(&mut pending,rejected),Err(StatusCode::CONFLICT));
            assert!(pending.is_some() && receiver.try_recv().is_err(),"旧回执不能吞掉新请求或串用执行资格");
        }
        assert_eq!(settle_reply(&mut pending,current),Ok(StatusCode::NO_CONTENT));
        assert_eq!(receiver.try_recv().unwrap().resource.navigation_revision,2);
    }

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
