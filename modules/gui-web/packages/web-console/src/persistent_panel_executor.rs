//! 既有输入安全库中的持久宿主分支；不放宽短命helper的READY/Job/退出契约。
use native_browser_protocol::{HostIdentity, PanelResource};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use crate::input_permit_store::{ExecutorStore, PermitStore, PermitStoreError};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="snake_case")]
pub(super) enum InputExecutorKind { EphemeralHelper, PersistentNativePanel }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PanelExecutorBinding {
    pub kind: InputExecutorKind,
    pub executor_instance_id: String,
    pub host_id: String,
    pub process: HostIdentity,
    pub resource: PanelResource,
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{ExecutionAttemptId, InputPermit, InputPermitState, InputSafetyResourceScope};
    use crate::input_safety_store::InputSafetyStore;

    fn binding() -> PanelExecutorBinding {
        PanelExecutorBinding {kind:InputExecutorKind::PersistentNativePanel,executor_instance_id:"1".repeat(32),
            host_id:format!("native-{}", "2".repeat(32)),process:HostIdentity {instance_id:"3".repeat(32),boot_id:"2".repeat(32),
                pid:123,creation_time_filetime:456,canonical_executable:"C:/test/coolzhu-tauri-shell.exe".into()},
            resource:PanelResource {workspace_path:"C:/test/project".into(),room_id:"room-1".into(),label:"browser-panel-1".into(),generation:1,navigation_revision:1}}
    }
    fn permit(executor: &PanelExecutorBinding,scope: &InputSafetyResourceScope,step: &str) -> (InputPermit,PanelPermitBinding) {
        let attempt = ExecutionAttemptId::new("click-action",1,step,1).unwrap();
        let permit = InputPermit {permit_id:format!("permit-{}",attempt.stable_key()),action_id:"click-action".into(),
            execution_attempt_id:attempt.clone(),scope:scope.clone(),execution_context_ref:step.into(),
            frozen_action_digest:"sha256:test-click".into(),gate_revision:1,issued_owner_id:"test-owner".into(),
            issued_epoch:1,expires_at_unix_ms:1000,executor_instance_id:Some(executor.executor_instance_id.clone()),
            state:InputPermitState::PendingActivation,revision:1,revocation_reason:None};
        let binding = PanelPermitBinding {executor:executor.clone(),attempt_key:attempt.stable_key(),ticket_id:"4".repeat(32),
            observation_id:"5".repeat(32),document_token:"6".repeat(32),node_id:"7".repeat(32),input_kind:Default::default()};
        (permit,binding)
    }
    fn store() -> (tempfile::TempDir,InputSafetyStore,InputSafetyResourceScope) {
        let root = tempfile::Builder::new().prefix("panel-store-").tempdir_in("tmp").unwrap();
        let store = InputSafetyStore::open_at(root.path()).unwrap();
        let scope = InputSafetyResourceScope::parse("windows-session-panel-store-test").unwrap();
        store.connection_for_test().execute("INSERT INTO input_safety_resource_state(scope,state,revision,recovery_epoch,accepts_new_input,updated_at_unix_ms) VALUES(?1,'safe',1,1,1,1)", [scope.as_str()]).unwrap();
        (root,store,scope)
    }

    #[test]
    fn panel_instance_has_separate_completion_semantics_and_one_permit_per_attempt() {
        let (_root,store,scope) = store();
        let executor = binding();
        assert!(executor.valid_shape());
        // 只验证数据库契约；生产登记额外调用真实OS核验，此处不对桌面发送输入。
        store.executor_store().register_verified_panel(&executor,&scope,"test-coordinator",1).unwrap();
        assert_eq!(store.executor_store().panel_binding(&executor.executor_instance_id).unwrap(),executor);
        assert!(store.executor_store().load_executor(&executor.executor_instance_id).is_err());
        let supervised: i64 = store.connection_for_test().query_row("SELECT supervision_bound FROM input_safety_executors WHERE executor_instance_id=?1", [&executor.executor_instance_id],|row| row.get(0)).unwrap();
        assert_eq!(supervised,0,"持久宿主不能假装获得helper的Job监督");
        for step in ["cu-test:step-1","cu-test:step-2"] {
            let (permit,binding) = permit(&executor,&scope,step);
            store.permit_store().register_pending_bound(&permit,InputPermitState::PendingActivation,2,Some(&binding)).unwrap();
            assert_eq!(store.permit_store().consume_verified_panel_permit(&permit.permit_id,&binding,1,1,3).unwrap(),InputPermitState::DispatchCommitted);
            assert!(store.permit_store().consume_verified_panel_permit(&permit.permit_id,&binding,1,1,4).is_err());
            store.permit_store().record_native_completion(&permit.permit_id,true,5).unwrap();
            assert!(store.executor_store().panel_binding(&executor.executor_instance_id).is_ok(),"动作结束不伪造宿主退出");
        }
        let process = runtime::ProcessInstanceEvidence {pid:123,creation_time_filetime:456};
        assert!(store.executor_store().record_native_completion(&executor.executor_instance_id,process,true,6).is_err(),"helper退出回执不能结账持久宿主");
    }

    #[test]
    fn panel_permit_rejects_missing_or_replaced_binding_expiry_and_changed_gate() {
        let (_root,store,scope) = store();
        let executor = binding();
        store.executor_store().register_verified_panel(&executor,&scope,"test-coordinator",1).unwrap();
        let (permit,binding) = permit(&executor,&scope,"cu-test:step-1");
        store.permit_store().register_pending_bound(&permit,InputPermitState::PendingActivation,2,Some(&binding)).unwrap();
        let mut replacement = executor.clone(); replacement.resource.generation = 2; replacement.resource.label = "browser-panel-2".into();
        assert!(store.executor_store().register_verified_panel(&replacement,&scope,"test-coordinator",2).is_err());
        let mut wrong = binding.clone(); wrong.node_id = "8".repeat(32);
        assert!(store.permit_store().consume_verified_panel_permit(&permit.permit_id,&wrong,1,1,3).is_err());
        assert!(store.permit_store().consume_verified_panel_permit(&permit.permit_id,&binding,2,1,3).is_err());
        assert!(store.permit_store().consume_verified_panel_permit(&permit.permit_id,&binding,1,2,3).is_err());
        assert!(store.permit_store().consume_verified_panel_permit(&permit.permit_id,&binding,1,1,1000).is_err());
        store.connection_for_test().execute("UPDATE input_safety_resource_state SET revision=2",[]).unwrap();
        assert!(store.permit_store().consume_verified_panel_permit(&permit.permit_id,&binding,1,1,3).is_err());
        assert_eq!(store.permit_store().load_permit(&permit.permit_id).unwrap().state,InputPermitState::PendingActivation);
    }
}
impl PanelExecutorBinding {
    pub(super) fn valid_shape(&self) -> bool {
        self.kind == InputExecutorKind::PersistentNativePanel
            && native_browser_protocol::opaque_id(&self.executor_instance_id)
            && self.host_id == format!("native-{}", self.process.boot_id)
            && self.process.valid_shape() && self.resource.valid_shape()
    }
}

/// 绑定已预检的一次尝试；节点引用不构成输入授权。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PanelPermitBinding {
    pub executor: PanelExecutorBinding,
    pub attempt_key: String,
    pub ticket_id: String,
    pub observation_id: String,
    pub document_token: String,
    pub node_id: String,
    /// 旧click记录保持原含义；新回执不能把wheel ACK当成mouse release。
    #[serde(default)]
    pub input_kind: native_browser_protocol::PanelInputKind,
}
impl PanelPermitBinding {
    pub(super) fn valid_shape(&self) -> bool {
        self.executor.valid_shape() && !self.attempt_key.is_empty() && self.attempt_key.len() <= 2048
            && !self.attempt_key.chars().any(char::is_control)
            && [&self.ticket_id,&self.observation_id,&self.document_token,&self.node_id]
                .into_iter().all(|id| native_browser_protocol::opaque_id(id))
    }
}

/// 随既有安全库迁移事务一起提交；旧执行者明确保持ephemeral_helper，旧事实不回填面板身份。
pub(super) fn ensure_v4_objects(connection: &Connection) -> rusqlite::Result<()> {
    for (table,column,definition) in [
        ("input_safety_executors","executor_kind","executor_kind TEXT NOT NULL DEFAULT 'ephemeral_helper'"),
        ("input_safety_executors","persistent_binding_json","persistent_binding_json TEXT"),
        ("input_safety_permits","persistent_binding_json","persistent_binding_json TEXT"),
    ] {
        let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
        let names = statement.query_map([], |row| row.get::<_,String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if !names.iter().any(|name| name == column) {
            connection.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {definition}"))?;
        }
    }
    Ok(())
}
fn sql(error: rusqlite::Error) -> PermitStoreError { PermitStoreError::Sqlite(error.to_string()) }
fn refused(what: &'static str) -> PermitStoreError { PermitStoreError::ConditionNotMet {what} }
fn json(value: &impl Serialize) -> Result<String,PermitStoreError> {
    serde_json::to_string(value).map_err(|error| PermitStoreError::Sqlite(error.to_string()))
}

impl ExecutorStore<'_> {
    /// OS核验在登记与消费时各做一次。登记不会把未知或隔离资源恢复为Safe。
    pub(super) fn register_panel(
        &self, binding: &PanelExecutorBinding, scope: &runtime::InputSafetyResourceScope,
        coordinator: &str, now: u64,
    ) -> Result<(),PermitStoreError> {
        if !binding.valid_shape() || coordinator.is_empty() || coordinator.chars().any(char::is_control) {
            return Err(refused("持久宿主身份不完整"));
        }
        crate::native_browser_host::verify_process(&binding.process).map_err(|_| refused("持久宿主OS实例核验失败"))?;
        self.register_verified_panel(binding,scope,coordinator,now)
    }

    fn register_verified_panel(
        &self,binding: &PanelExecutorBinding,scope: &runtime::InputSafetyResourceScope,coordinator: &str,now: u64,
    ) -> Result<(),PermitStoreError> {
        let encoded = json(binding)?;
        self.connection.execute_batch("BEGIN IMMEDIATE").map_err(sql)?;
        let result = (|| {
            let existing: Option<(String,String,String)> = self.connection.query_row(
                "SELECT executor_kind,scope,COALESCE(persistent_binding_json,'') FROM input_safety_executors WHERE executor_instance_id=?1",
                [&binding.executor_instance_id],|row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(sql)?;
            if let Some((kind,old_scope,old_binding)) = existing {
                if kind != "persistent_native_panel" || old_scope != scope.as_str() || old_binding != encoded {
                    return Err(refused("持久宿主实例ID不能绑定另一个进程或面板"));
                }
                // 相同身份不新建资格，不恢复已被收紧的登记状态。
                return Ok(());
            }
            self.connection.execute(
                "INSERT INTO input_safety_executors(executor_instance_id,launch_operation_id,coordinator_instance_id,scope,
                 host_launch_instance,action_id,pid,creation_time_100ns,user_session,host_process_path,script_or_program_digest,
                 protocol_version,supervision_bound,state,revision,updated_at_unix_ms,executor_kind,persistent_binding_json)
                 VALUES(?1,?2,?3,?4,?2,'',?5,?6,NULL,?7,'',1,0,'registered_pending_verification',1,?8,'persistent_native_panel',?9)",
                rusqlite::params![binding.executor_instance_id,binding.process.boot_id,coordinator,scope.as_str(),
                    i64::from(binding.process.pid),binding.process.creation_time_filetime as i64,
                    binding.process.canonical_executable,now as i64,encoded]).map_err(sql)?;
            // supervision_bound=0不冒充Job；该类型只由认证实例+动作回执监督，不使用helper退出结账。
            self.connection.execute(
                "UPDATE input_safety_executors SET state='verified_alive',revision=revision+1 WHERE executor_instance_id=?1 AND executor_kind='persistent_native_panel'",
                [&binding.executor_instance_id]).map_err(sql)?;
            Ok(())
        })();
        match result { Ok(()) => self.connection.execute_batch("COMMIT").map_err(sql),
            Err(error) => { let _ = self.connection.execute_batch("ROLLBACK"); Err(error) } }
    }

    pub(super) fn panel_binding(&self,instance: &str) -> Result<PanelExecutorBinding,PermitStoreError> {
        let raw: Option<String> = self.connection.query_row(
            "SELECT persistent_binding_json FROM input_safety_executors WHERE executor_instance_id=?1 AND executor_kind='persistent_native_panel' AND state='verified_alive'",
            [instance],|row| row.get(0)).optional().map_err(sql)?;
        let binding: PanelExecutorBinding = serde_json::from_str(&raw.ok_or_else(|| refused("缺少已核验的持久宿主登记"))?)
            .map_err(|_| refused("持久宿主登记损坏"))?;
        if !binding.valid_shape() { return Err(refused("持久宿主登记损坏")); }
        Ok(binding)
    }
}

impl PermitStore<'_> {
    /// 仅消费一次安全授权。父取消/CU Pending→Dispatching须在会话库独立裁决。
    pub(super) fn consume_panel_permit(
        &self,permit_id: &str,expected: &PanelPermitBinding,gate_revision: u64,recovery_epoch: u64,now: u64,
    ) -> Result<runtime::InputPermitState,PermitStoreError> {
        if !expected.valid_shape() { return Err(refused("持久面板许可绑定不完整")); }
        crate::native_browser_host::verify_process(&expected.executor.process).map_err(|_| refused("消费前OS实例已失效"))?;
        self.consume_verified_panel_permit(permit_id,expected,gate_revision,recovery_epoch,now)
    }

    fn consume_verified_panel_permit(
        &self,permit_id: &str,expected: &PanelPermitBinding,gate_revision: u64,recovery_epoch: u64,now: u64,
    ) -> Result<runtime::InputPermitState,PermitStoreError> {
        let executor_json = json(&expected.executor)?;
        let permit_json = json(expected)?;
        self.in_immediate_transaction("consume_panel_permit",|connection| {
            let blocked: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM input_safety_resource_blocks b JOIN input_safety_permits p ON p.scope=b.scope WHERE p.permit_id=?1 AND b.state='open')",
                [permit_id],|row| row.get(0)).map_err(sql)?;
            if blocked { return Err(refused("资源仍存在输入阻断")); }
            let changed = connection.execute(
                "UPDATE input_safety_permits SET state='dispatch_committed',revision=revision+1,updated_at_unix_ms=?1
                 WHERE permit_id=?2 AND state='pending_activation' AND expires_at_unix_ms>?1
                   AND gate_revision=?3 AND issued_epoch=?4 AND persistent_binding_json=?5
                   AND execution_attempt_id=?6 AND executor_instance_id=?7
                   AND EXISTS(SELECT 1 FROM input_safety_resource_state r WHERE r.scope=input_safety_permits.scope
                     AND r.state='safe' AND r.accepts_new_input=1 AND r.revision=?3 AND r.recovery_epoch=?4)
                   AND EXISTS(SELECT 1 FROM input_safety_executors e WHERE e.executor_instance_id=?7
                     AND e.scope=input_safety_permits.scope AND e.executor_kind='persistent_native_panel'
                     AND e.state='verified_alive' AND e.persistent_binding_json=?8 AND e.pid=?9 AND e.creation_time_100ns=?10)",
                rusqlite::params![now as i64,permit_id,gate_revision as i64,recovery_epoch as i64,permit_json,
                    expected.attempt_key,expected.executor.executor_instance_id,executor_json,
                    i64::from(expected.executor.process.pid),expected.executor.process.creation_time_filetime as i64]).map_err(sql)?;
            if changed != 1 { return Err(refused("面板许可已消费或进程、资源、gate/epoch绑定失效")); }
            Ok(runtime::InputPermitState::DispatchCommitted)
        })
    }
}
