//! 原生输入的宿主授权：长存储操作在 lease 临界区外，一次通知在短临界区内。
use std::{cell::{Cell, RefCell}, collections::{BTreeMap, HashSet}, path::PathBuf, sync::{Mutex, OnceLock}};

use computer_use::prepared_input::{NativeInputAuthorization, NativeInputCompletion, NativeInputPermit, PreparedNativeInput};
use runtime::{ExecutionAttemptId, ExecutorInstanceState, ExecutorObservation, ExecutorRegistration,
    InputReleaseStatus, InputSafetyResourceScope, ProcessInstanceEvidence};
use windows_process_guard::{InputDispatchGuard, ScopedInputOwnership};

use crate::{input_permit_gate::PermitGate, input_safety_store::{coordinator_instance_identity, InputSafetyStore}};

fn memory_blocks() -> &'static Mutex<HashSet<String>> {
    static BLOCKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    BLOCKS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// 持久面板沿用既有输入阻断/人工恢复，不另造浏览器安全状态机。
pub(super) fn isolate_panel_unknown(root:&std::path::Path,scope:&InputSafetyResourceScope,request:&str,reason:&str) -> Result<(),String> {
    if let Ok(mut blocks)=memory_blocks().lock() { blocks.insert(scope.as_str().into()); }
    let store=InputSafetyStore::open_at(root).map_err(|e|e.to_string())?;
    let state=store.resource_state(scope).map_err(|e|e.to_string())?;
    store.open_resource_block(&format!("native-panel-{request}"),scope,"native_executor_outcome_unknown",request).map_err(|e|e.to_string())?;
    store.isolate_resource(scope,reason,Some(&coordinator_instance_identity()),state.recovery_epoch).map_err(|e|e.to_string())?;
    Ok(())
}

/// 存储故障也不能放行；锁中毒按未知阻断处理，不恢复默认允许。
pub(crate) fn memory_input_allowed(scope: &InputSafetyResourceScope) -> bool {
    memory_blocks().lock().map(|blocks| !blocks.contains(scope.as_str())).unwrap_or(false)
}

/// 仅由人工恢复成功的入口显式调用；读取失败、未恢复或仍有事故阻断均不清除。
pub(crate) fn clear_memory_block_after_recovery(root: &std::path::Path, scope: &InputSafetyResourceScope) -> Result<(), String> {
    let store = InputSafetyStore::open_at(root).map_err(|e| e.to_string())?;
    let state = store.resource_state(scope).map_err(|e| e.to_string())?;
    if state.state != runtime::ResourceSafetyState::Safe || !state.accepts_new_input
        || store.has_open_resource_block(scope).map_err(|e| e.to_string())? {
        return Err("输入资源仍未完成恢复，不能清除本进程阻断".into());
    }
    memory_blocks().lock().map_err(|_| "输入阻断锁已损坏")?.remove(scope.as_str());
    Ok(())
}

/// 这些字段来自已接纳的动作与真实观察；不从 helper 的自述重建业务身份。
pub(crate) struct NativeInputAuthorizationContext {
    pub root: PathBuf,
    pub scope: InputSafetyResourceScope,
    pub owner_id: String,
    pub action_id: String,
    pub frozen_action_digest: String,
    pub observation_generation: u64,
    pub step_identity: String,
    pub deadline_unix_ms: u64,
}

struct RegisteredHelper {
    process: ProcessInstanceEvidence,
    executor_id: String,
    permit_id: Option<String>,
    attempt_id: String,
    deadline_unix_ms: u64,
    dispatch_attempted: bool,
    notified: bool,
    consumed: bool,
}

pub(crate) struct HostNativeInputAuthorization<'a, 'broker> {
    context: NativeInputAuthorizationContext,
    lease: &'a ScopedInputOwnership<'broker>,
    cancelled: &'a dyn Fn() -> bool,
    coordinator: String,
    next_attempt: Cell<u64>,
    helpers: RefCell<BTreeMap<String, RegisteredHelper>>,
}

impl<'a, 'broker> HostNativeInputAuthorization<'a, 'broker> {
    pub(crate) fn new(
        context: NativeInputAuthorizationContext,
        lease: &'a ScopedInputOwnership<'broker>,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Result<Self, String> {
        if context.owner_id != lease.owner_id() || context.scope.as_str() != lease.scope() || !lease.is_current() {
            return Err("原生输入 owner 与实际持有的 lease 不符".into());
        }
        if context.deadline_unix_ms <= crate::unix_timestamp_millis() {
            return Err("本次动作输入期限已经耗尽".into());
        }
        ExecutionAttemptId::new(&context.action_id, context.observation_generation, &context.step_identity, 1)
            .map_err(|e| e.to_string())?;
        if context.frozen_action_digest.trim().is_empty() { return Err("缺少冻结动作摘要".into()); }
        Ok(Self { context, lease, cancelled,
            coordinator: coordinator_instance_identity(), next_attempt: Cell::new(1), helpers: RefCell::new(BTreeMap::new()) })
    }

    fn isolate(&self, request_id: &str, reason: &str) -> Result<(), String> {
        // 即使磁盘不可写，也先阻断本进程的后续动作；此处不持有 dispatch 临界区。
        if let Ok(mut blocks) = memory_blocks().lock() { blocks.insert(self.context.scope.as_str().into()); }
        let store = InputSafetyStore::open_at(&self.context.root).map_err(|e| e.to_string())?;
        let state = store.resource_state(&self.context.scope).map_err(|e| e.to_string())?;
        store.open_resource_block(&format!("native-helper-{request_id}"), &self.context.scope,
            "native_executor_outcome_unknown", request_id).map_err(|e| e.to_string())?;
        store.append_event(runtime::InputSafetyEvent {kind:runtime::InputSafetyEventKind::ResourceStateChanged,
            scope:Some(self.context.scope.clone()),subject_id:Some(request_id.into()),detail:reason.into(),
            recorded_at_unix_ms:crate::unix_timestamp_millis()}).map_err(|e|e.to_string())?;
        store.isolate_resource(&self.context.scope, reason, Some(&self.coordinator), state.recovery_epoch)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn reconcile(&self, request_id: &str, helper: &RegisteredHelper, completion: Option<&NativeInputCompletion>) -> Result<(), String> {
        let identity_matches = completion.and_then(|c| c.process.as_ref()) == Some(&helper.process);
        let exited = identity_matches && completion.is_some_and(|c| c.process_exit_confirmed);
        let final_received = identity_matches && completion.is_some_and(|c| c.trusted_final);
        let released = identity_matches && completion.is_some_and(|c|
            matches!(c.input_release, InputReleaseStatus::Released | InputReleaseStatus::NotNeeded));
        let notified_matches = completion.is_some_and(|c| c.execute_notified == helper.notified);
        // 未发 execute 的屏障证明可以结束零输入尝试，无须伪造 helper Final。
        let known_zero = exited && !helper.notified && notified_matches
            && completion.is_some_and(|c| c.input_release == InputReleaseStatus::NotNeeded);
        let settled = exited && (final_received || known_zero) && released && notified_matches;
        let now = crate::unix_timestamp_millis();
        let result = (|| -> Result<(), String> {
            let store = InputSafetyStore::open_at(&self.context.root).map_err(|e| e.to_string())?;
            store.executor_store().record_native_completion(&helper.executor_id, helper.process, exited, now)
                .map_err(|e| e.to_string())?;
            if let Some(permit_id) = &helper.permit_id {
                // 消费提交返回错误也可能需要重读核对，不能只相信进程内标志。
                let current = store.permit_store().load_permit(permit_id).map_err(|e| e.to_string())?.state;
                if current.crossed_dispatch_boundary() {
                    store.permit_store().record_native_completion(permit_id, (final_received || known_zero) && notified_matches, now)
                        .map_err(|e| e.to_string())?;
                } else if current == runtime::InputPermitState::PendingActivation {
                    store.permit_store().revoke(permit_id, "helper 结束前未获得消费资格", now).map_err(|e| e.to_string())?;
                }
            }
            Ok(())
        })();
        if !settled || result.is_err() {
            let reason = result.as_ref().err().map(String::as_str).unwrap_or("原生 helper 的最终回执、实例退出或输入释放未确认");
            let isolation = self.isolate(request_id, reason);
            if let Err(error) = result { return Err(error); }
            isolation?;
            return Err(reason.to_string());
        }
        Ok(())
    }
}

impl NativeInputAuthorization for HostNativeInputAuthorization<'_, '_> {
    fn authorize(&self, prepared: &PreparedNativeInput) -> Result<NativeInputPermit, String> {
        let now = crate::unix_timestamp_millis();
        let deadline = self.context.deadline_unix_ms.min(prepared.deadline_unix_ms);
        if (self.cancelled)() || !memory_input_allowed(&self.context.scope) || !self.lease.is_current() || now >= deadline {
            return Err("原生输入已取消、阻断、失去 owner 或期限耗尽".into());
        }
        if prepared.request_id.trim().is_empty() || prepared.nonce.trim().is_empty()
            || prepared.ready_at_unix_ms == 0 || prepared.ready_at_unix_ms > now
            || prepared.protocol_version != runtime::TWO_PHASE_HELPER_PROTOCOL_VERSION || !prepared.supervision_bound
            || !prepared.process.is_identifying() {
            return Err("helper READY、监督关系或进程身份不完整".into());
        }
        prepared.helper.validate_structure().map_err(|e| e.to_string())?;
        if self.helpers.borrow().contains_key(&prepared.request_id) { return Err("同一 helper 不得重复申请执行资格".into()); }
        // core 从原始 Child 捕获；此处重新读取 OS 的 PID+创建时间，绝不把字符串当实例证据。
        let current = windows_process_guard::capture_live_process_identity(prepared.process.pid).map_err(|e| e.to_string())?;
        if current.creation_time_filetime() != prepared.process.creation_time_filetime
            || current.image_path() != Some(prepared.helper.host_process_path.as_str()) {
            return Err("helper 的 OS 实例或实际程序路径已经变化".into());
        }
        let sequence = self.next_attempt.get();
        self.next_attempt.set(sequence.checked_add(1).ok_or("helper 尝试序号耗尽")?);
        let attempt = ExecutionAttemptId::new(&self.context.action_id, self.context.observation_generation,
            &self.context.step_identity, sequence).map_err(|e| e.to_string())?;
        let executor_id = format!("native-{}-{}-{}", prepared.process.pid, prepared.process.creation_time_filetime, prepared.request_id);
        let registration = ExecutorRegistration {
            executor_instance_id: executor_id.clone(), launch_operation_id: prepared.request_id.clone(),
            coordinator_instance_id: self.coordinator.clone(), scope: self.context.scope.clone(),
            host_launch_instance: self.coordinator.clone(), action_id: self.context.action_id.clone(),
            pid: prepared.process.pid, creation_time_100ns: Some(prepared.process.creation_time_filetime), user_session: None,
            helper: prepared.helper.clone(), protocol_version: prepared.protocol_version, supervision_bound: true,
            state: ExecutorInstanceState::RegisteredPendingVerification, revision: 1,
        };
        let store = InputSafetyStore::open_at(&self.context.root).map_err(|e| e.to_string())?;
        store.executor_store().register_launch_intent(&registration, now).map_err(|e| e.to_string())?;
        // 一经登记就跟踪到 completed/Drop；后续任何失败都不能遗弃这个已启动的 helper。
        self.helpers.borrow_mut().insert(prepared.request_id.clone(), RegisteredHelper {
            process: prepared.process, executor_id: executor_id.clone(), permit_id: None,
            attempt_id: attempt.stable_key(), deadline_unix_ms: deadline, dispatch_attempted: false, notified: false, consumed: false,
        });
        store.executor_store().record_instance_evidence(&executor_id, ExecutorObservation::MatchesAndAlive,
            Some(prepared.process.creation_time_filetime), true, now).map_err(|e| e.to_string())?;
        let issued = PermitGate::issue_bound(&self.context.root, &self.context.scope, &attempt,
            &self.context.frozen_action_digest, &self.context.owner_id, &executor_id, deadline, crate::unix_timestamp_millis())
            .map_err(|e| e.reason())?;
        self.helpers.borrow_mut().get_mut(&prepared.request_id).expect("已登记 helper").permit_id = Some(issued.permit_id.clone());
        issued.recheck_and_consume().map_err(|e| e.reason())?;
        self.helpers.borrow_mut().get_mut(&prepared.request_id).expect("已登记 helper").consumed = true;
        Ok(NativeInputPermit { permit_id: issued.permit_id, attempt_id: issued.attempt_key, executor_instance_id: executor_id,
            expires_at_unix_ms: deadline })
    }

    fn dispatch(&self, prepared: &PreparedNativeInput, permit: &NativeInputPermit,
        notify: &mut dyn FnMut() -> Result<(), String>) -> Result<(), String> {
        let mut helpers = self.helpers.borrow_mut();
        let helper = helpers.get_mut(&prepared.request_id).ok_or("本次 helper 未登记")?;
        if !helper.consumed || helper.dispatch_attempted || helper.process != prepared.process
            || helper.permit_id.as_deref() != Some(&permit.permit_id) || helper.attempt_id != permit.attempt_id
            || helper.executor_id != permit.executor_instance_id || helper.deadline_unix_ms != permit.expires_at_unix_ms {
            return Err("本次输入许可不匹配或已经通知过".into());
        }
        helper.dispatch_attempted = true;
        let allowed = || !(self.cancelled)() && memory_input_allowed(&self.context.scope) && crate::unix_timestamp_millis() < helper.deadline_unix_ms;
        self.lease.dispatch_if_current(&[InputDispatchGuard::new("cancel_deadline_and_resource", &allowed)], || {
            notify()?;
            Ok::<_, String>(())
        }).map_err(|e| e.to_string())??;
        helper.notified = true;
        Ok(())
    }

    fn completed(&self, completion: &NativeInputCompletion) -> Result<(), String> {
        // core 可能在 READY 前失败；没有登记也没有许可，原始 outcome 仍由调用方处理。
        let Some(helper) = self.helpers.borrow_mut().remove(&completion.request_id) else { return Ok(()); };
        self.reconcile(&completion.request_id, &helper, Some(completion))
    }
}

impl Drop for HostNativeInputAuthorization<'_, '_> {
    fn drop(&mut self) {
        let unfinished = std::mem::take(self.helpers.get_mut());
        for (request_id, helper) in unfinished {
            let _ = self.reconcile(&request_id, &helper, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{process::Command, time::Duration};
    use runtime::InputPermitState;
    use windows_process_guard::{ChildProcessJob, InteractiveInputLeaseBroker};

    /// 真正启动无输入功能的 PowerShell；只验证宿主协议，不对用户桌面执行键鼠。
    fn harmless_process() -> (std::process::Child, ChildProcessJob, PreparedNativeInput) {
        use sha2::{Digest, Sha256};
        let script = "Start-Sleep -Seconds 60";
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", script]);
        let (child, job) = ChildProcessJob::spawn_managed(&mut command).expect("托管子进程");
        let identity = windows_process_guard::capture_child_process_identity(&child).expect("原始 child 身份");
        let now = crate::unix_timestamp_millis();
        let request_id = format!("port-test-{}-{}", child.id(), identity.creation_time_filetime());
        let prepared = PreparedNativeInput {
            process: ProcessInstanceEvidence { pid: child.id(), creation_time_filetime: identity.creation_time_filetime() },
            helper: runtime::HelperIdentity { host_process_path: identity.image_path().expect("实际程序路径").into(),
                script_or_program_digest: format!("sha256:{:x}", Sha256::digest(script.as_bytes())) },
            request_id: request_id.clone(), nonce: request_id, ready_at_unix_ms: now,
            deadline_unix_ms: now + 30_000, protocol_version: runtime::TWO_PHASE_HELPER_PROTOCOL_VERSION,
            supervision_bound: true,
        };
        (child, job, prepared)
    }

    fn context(root: &std::path::Path, scope: InputSafetyResourceScope) -> NativeInputAuthorizationContext {
        let store = InputSafetyStore::open_at(root).expect("临时库");
        store.connection_for_test().execute(
            "INSERT INTO input_safety_resource_state(scope,state,revision,coordinator_instance_id,recovery_epoch,accepts_new_input,updated_at_unix_ms)
             VALUES (?1,'safe',1,NULL,1,1,1)", [scope.as_str()]).expect("仅测试准备已恢复资源");
        NativeInputAuthorizationContext { root: root.into(), scope, owner_id: "host-port-test-owner".into(),
            action_id: "click-test-action-1".into(), frozen_action_digest: "sha256:port-test-action".into(),
            observation_generation: 1, step_identity: "call-port-test:step-1".into(),
            deadline_unix_ms: crate::unix_timestamp_millis() + 30_000 }
    }

    #[test]
    fn host_port_cancel_after_consume_never_notifies_and_finishes_from_zero_input_barrier() {
        let root = tempfile::tempdir().expect("temp");
        let (mut child, job, mut prepared) = harmless_process();
        let scope = InputSafetyResourceScope::parse(&format!("port-cancel-{}", child.id())).expect("scope");
        let broker = InteractiveInputLeaseBroker::default();
        let lease = ScopedInputOwnership::acquire(&broker, scope.as_str(), "host-port-test-owner", Duration::ZERO).expect("lease");
        let cancelled = Cell::new(false);
        let cancellation = || cancelled.get();
        let authorizer = HostNativeInputAuthorization::new(context(root.path(), scope.clone()), &lease, &cancellation).expect("host");
        prepared.process.creation_time_filetime += 1;
        assert!(authorizer.authorize(&prepared).is_err(), "同 PID 的错误创建时间不得获得资格");
        prepared.process.creation_time_filetime -= 1;
        let permit = authorizer.authorize(&prepared).expect("真实实例可登记消费");
        cancelled.set(true);
        let mut notify_count = 0;
        let mut notify = || { notify_count += 1; Ok(()) };
        assert!(authorizer.dispatch(&prepared, &permit, &mut notify).is_err());
        assert!(authorizer.dispatch(&prepared, &permit, &mut notify).is_err(), "第二次也不得尝试通知");
        assert_eq!(notify_count, 0);
        drop(job);
        child.wait().expect("确认原进程退出");
        authorizer.completed(&NativeInputCompletion { request_id: prepared.request_id.clone(), process: Some(prepared.process),
            execute_notified: false, process_exit_confirmed: true, input_release: InputReleaseStatus::NotNeeded,
            trusted_final: false, error: Some("取消发生在通知前".into()) }).expect("真实零输入屏障可结账");
        let store = InputSafetyStore::open_at(root.path()).expect("store");
        assert_eq!(store.permit_store().load_permit(&permit.permit_id).expect("permit").state, InputPermitState::Finished);
        assert_eq!(store.executor_store().load_executor(&permit.executor_instance_id).expect("executor").state, ExecutorInstanceState::ExitedConfirmed);
        assert!(memory_input_allowed(&scope));
    }

    #[test]
    fn host_port_consumed_without_final_is_unknown_and_blocks_until_explicit_recovery() {
        let root = tempfile::tempdir().expect("temp");
        let (mut child, job, prepared) = harmless_process();
        let scope = InputSafetyResourceScope::parse(&format!("port-unknown-{}", child.id())).expect("scope");
        let broker = InteractiveInputLeaseBroker::default();
        let lease = ScopedInputOwnership::acquire(&broker, scope.as_str(), "host-port-test-owner", Duration::ZERO).expect("lease");
        let cancelled = || false;
        let authorizer = HostNativeInputAuthorization::new(context(root.path(), scope.clone()), &lease, &cancelled).expect("host");
        let permit = authorizer.authorize(&prepared).expect("authorize");
        let mut count = 0;
        let mut notify = || { count += 1; Ok(()) };
        authorizer.dispatch(&prepared, &permit, &mut notify).expect("受控通知");
        assert!(authorizer.dispatch(&prepared, &permit, &mut notify).is_err());
        assert_eq!(count, 1);
        drop(job);
        child.wait().expect("确认退出");
        assert!(authorizer.completed(&NativeInputCompletion { request_id: prepared.request_id.clone(), process: Some(prepared.process),
            execute_notified: true, process_exit_confirmed: true, input_release: InputReleaseStatus::Unknown,
            trusted_final: false, error: Some("通知后缺少可信最终回执".into()) }).is_err());
        let store = InputSafetyStore::open_at(root.path()).expect("store");
        assert_eq!(store.permit_store().load_permit(&permit.permit_id).expect("permit").state, InputPermitState::OutcomeUnknown);
        assert!(store.has_open_resource_block(&scope).expect("blocks"));
        assert!(!memory_input_allowed(&scope));
        assert!(clear_memory_block_after_recovery(root.path(), &scope).is_err());
        // 仅模拟独立人工恢复已经完成，不能由 authorize 或重新打开窗口自动清除。
        store.connection_for_test().execute("UPDATE input_safety_resource_blocks SET state='closed' WHERE scope=?1", [scope.as_str()]).expect("恢复fixture");
        store.connection_for_test().execute("UPDATE input_safety_resource_state SET state='safe',accepts_new_input=1 WHERE scope=?1", [scope.as_str()]).expect("恢复fixture");
        assert!(!memory_input_allowed(&scope), "磁盘Safe不会自动解除进程内阻断");
        clear_memory_block_after_recovery(root.path(), &scope).expect("显式恢复");
        assert!(memory_input_allowed(&scope));
    }
}
