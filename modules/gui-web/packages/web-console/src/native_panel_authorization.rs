//! 持久面板的业务授权和动作结算；helper分支保持原有READY/Job/退出条件。
use std::cell::RefCell;
use computer_use::prepared_input::{NativeInputAuthorization,NativeInputPermit,PreparedNativeInput,PanelInputPreparation};
use native_browser_protocol::{PanelInputReply,PanelInputOutcome};
use runtime::{ExecutionAttemptId,InputPermit,InputPermitState};
use windows_process_guard::{ScopedInputOwnership,InputDispatchGuard};
use crate::{native_input_authorization::NativeInputAuthorizationContext,input_safety_store::InputSafetyStore,
    persistent_panel_executor::{InputExecutorKind,PanelExecutorBinding,PanelPermitBinding}};

struct Claimed { permit:String,binding:PanelPermitBinding }
pub(super) struct HostPanelAuthorization<'a,'broker> {
    context:NativeInputAuthorizationContext,parent:&'a crate::FrozenParentContext,
    store:&'a crate::computer_use_store::ComputerUseRunStore,call_id:&'a str,index:u64,
    lease:&'a ScopedInputOwnership<'broker>,cancelled:&'a dyn Fn()->bool,claimed:RefCell<Option<Claimed>>,
}
impl<'a,'broker> HostPanelAuthorization<'a,'broker> {
    pub(super) fn new(context:NativeInputAuthorizationContext,parent:&'a crate::FrozenParentContext,
        store:&'a crate::computer_use_store::ComputerUseRunStore,call_id:&'a str,index:u64,
        lease:&'a ScopedInputOwnership<'broker>,cancelled:&'a dyn Fn()->bool) -> Result<Self,String> {
        if !lease.is_current() || lease.owner_id()!=context.owner_id || lease.scope()!=context.scope.as_str() {
            return Err("面板输入所有者与实际lease不符".into());
        }
        Ok(Self {context,parent,store,call_id,index,lease,cancelled,claimed:RefCell::new(None)})
    }
    fn settle(&self,claimed:&Claimed,reply:Option<&PanelInputReply>) -> Result<(),String> {
        let known=reply.is_some_and(|r|r.valid_shape() && r.host_id==claimed.binding.executor.host_id
            && r.ticket_id.as_deref()==Some(claimed.binding.ticket_id.as_str())
            && r.attempt_id.as_deref()==Some(claimed.binding.attempt_key.as_str())
            && r.executor_instance_id.as_deref()==Some(claimed.binding.executor.executor_instance_id.as_str())
            && r.resource==claimed.binding.executor.resource && matches!(r.outcome,PanelInputOutcome::Released|PanelInputOutcome::NotDispatched));
        let result=(|| {
            let store=InputSafetyStore::open_at(&self.context.root).map_err(|e|e.to_string())?;
            store.permit_store().record_native_completion(&claimed.permit,known,crate::unix_timestamp_millis()).map_err(|e|e.to_string())?;
            let phase=match reply.filter(|_|known).map(|r|r.outcome) {
                Some(PanelInputOutcome::Released)=>"released",
                Some(PanelInputOutcome::NotDispatched)=>"aborted_before_input",
                _=>"release_unknown",
            };
            self.store.settle_panel_attempt(self.call_id,self.index,&claimed.binding.ticket_id,phase)?;
            store.append_event(runtime::InputSafetyEvent {kind:runtime::InputSafetyEventKind::ResourceStateChanged,
                scope:Some(self.context.scope.clone()),subject_id:Some(claimed.permit.clone()),
                detail:format!("持久面板动作回执：{phase}；不以宿主退出结算，也不推导网页目标达成"),recorded_at_unix_ms:crate::unix_timestamp_millis()}).map_err(|e|e.to_string())
        })();
        if !known || result.is_err() {
            let _=crate::native_input_authorization::isolate_panel_unknown(&self.context.root,&self.context.scope,
                &claimed.binding.ticket_id,"持久面板释放或结算未确认，禁止自动重放");
            return Err(result.err().unwrap_or_else(||"持久面板动作释放未确认".into()));
        }
        result
    }
}
impl NativeInputAuthorization for HostPanelAuthorization<'_,'_> {
    fn authorize(&self,_:&PreparedNativeInput)->Result<NativeInputPermit,String> {Err("持久面板授权不能用于短命helper".into())}
    fn dispatch(&self,_:&PreparedNativeInput,_:&NativeInputPermit,_:&mut dyn FnMut()->Result<(),String>)->Result<(),String> {Err("持久面板不发送helper execute".into())}
    fn claim_panel(&self,p:&PanelInputPreparation)->Result<NativeInputPermit,String> {
        if self.claimed.borrow().is_some() || (self.cancelled)() || !crate::native_input_authorization::memory_input_allowed(&self.context.scope) {
            return Err("动作已申请、取消或输入资源已阻断".into());
        }
        let (host,process)=crate::native_browser_host::input_process(self.parent,&p.resource)?;
        let now=crate::unix_timestamp_millis();let expiry=p.expires_at_unix_ms.min(self.context.deadline_unix_ms);
        if host!=p.host_id || process!=p.process || expiry<=now || expiry>now.saturating_add(2000)
            || !p.target.valid_shape() || !native_browser_protocol::opaque_id(&p.ticket_id) { return Err("面板票据身份或期限不符".into()); }
        use sha2::{Digest,Sha256};
        let resource_json=serde_json::to_vec(&p.resource).map_err(|e|e.to_string())?;
        let mut hash=Sha256::new();hash.update(process.instance_id.as_bytes());hash.update(resource_json);
        let executor_id=format!("{:x}",hash.finalize())[..32].to_string();
        let executor=PanelExecutorBinding {kind:InputExecutorKind::PersistentNativePanel,executor_instance_id:executor_id.clone(),host_id:host,process,resource:p.resource.clone()};
        let attempt=ExecutionAttemptId::new(&self.context.action_id,self.context.observation_generation,&self.context.step_identity,1).map_err(|e|e.to_string())?;
        let binding=PanelPermitBinding {executor,attempt_key:attempt.stable_key(),ticket_id:p.ticket_id.clone(),observation_id:p.target.observation_id.clone(),document_token:p.target.document_token.clone(),node_id:p.target.node_id.clone()};
        let store=InputSafetyStore::open_at(&self.context.root).map_err(|e|e.to_string())?;
        store.executor_store().register_panel(&binding.executor,&self.context.scope,&crate::input_safety_store::coordinator_instance_identity(),now).map_err(|e|e.to_string())?;
        let state=store.resource_state(&self.context.scope).map_err(|e|e.to_string())?;
        if !state.accepts_new_input || state.state!=runtime::ResourceSafetyState::Safe { return Err("输入资源未允许新动作".into()); }
        let permit=InputPermit {permit_id:format!("permit-{}",attempt.stable_key()),action_id:self.context.action_id.clone(),execution_attempt_id:attempt.clone(),
            scope:self.context.scope.clone(),execution_context_ref:self.context.step_identity.clone(),frozen_action_digest:self.context.frozen_action_digest.clone(),
            gate_revision:state.revision,issued_owner_id:self.context.owner_id.clone(),issued_epoch:state.recovery_epoch,expires_at_unix_ms:expiry,
            executor_instance_id:Some(executor_id.clone()),state:InputPermitState::PendingActivation,revision:1,revocation_reason:None};
        store.permit_store().register_pending_bound(&permit,InputPermitState::PendingActivation,now,Some(&binding)).map_err(|e|e.to_string())?;
        let encoded=serde_json::to_string(&binding).map_err(|e|e.to_string())?;
        self.store.prepare_panel_attempt(self.call_id,self.index,&p.ticket_id,&encoded)?;
        let allowed=|| !(self.cancelled)() && crate::native_input_authorization::memory_input_allowed(&self.context.scope) && crate::unix_timestamp_millis()<expiry;
        let claim=self.lease.dispatch_if_current(&[InputDispatchGuard::new("panel_cancel_and_expiry",&allowed)],|| {
            crate::native_browser_host::input_process(self.parent,&p.resource)?;
            // 第一提交只消费安全资格；第二提交才裁决父停止与本步派发。二者不是跨库原子事务。
            store.permit_store().consume_panel_permit(&permit.permit_id,&binding,state.revision,state.recovery_epoch,crate::unix_timestamp_millis()).map_err(|e|e.to_string())?;
            self.store.claim_panel_attempt(self.parent,self.call_id,self.index,&p.ticket_id,&encoded,expiry,self.cancelled)
        }).map_err(|e|e.to_string()).and_then(|r|r);
        if let Err(error)=claim {
            let consumed=store.permit_store().load_permit(&permit.permit_id).map_err(|e|e.to_string())?.state.crossed_dispatch_boundary();
            if consumed { store.permit_store().record_native_completion(&permit.permit_id,true,crate::unix_timestamp_millis()).map_err(|e|e.to_string())?; }
            else { store.permit_store().revoke(&permit.permit_id,"授权后未输入",crate::unix_timestamp_millis()).map_err(|e|e.to_string())?; }
            self.store.settle_panel_attempt(self.call_id,self.index,&p.ticket_id,"aborted_before_input")?;
            return Err(error);
        }
        *self.claimed.borrow_mut()=Some(Claimed {permit:permit.permit_id.clone(),binding});
        Ok(NativeInputPermit {permit_id:permit.permit_id,attempt_id:attempt.stable_key(),executor_instance_id:executor_id,expires_at_unix_ms:expiry})
    }
    fn completed_panel(&self,reply:Option<&PanelInputReply>)->Result<(),String> {
        let claimed=self.claimed.borrow_mut().take().ok_or("没有本次面板派发资格")?;
        self.settle(&claimed,reply)
    }
}
impl Drop for HostPanelAuthorization<'_,'_> {
    fn drop(&mut self) { if let Some(claimed)=self.claimed.get_mut().take() { let _=self.settle(&claimed,None); } }
}
