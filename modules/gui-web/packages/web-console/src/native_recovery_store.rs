//! 原生恢复的快照与原子提交，复用现有输入安全表，不增加授权 schema。
use crate::input_safety_store::{InputSafetyStore, RecoveryControlGuard};
use runtime::{InputSafetyResourceScope, ProcessInstanceEvidence, ReleaseIsolationDecision};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RecoveryBlock {
    pub block_id: String,
    pub reason: String,
    pub opened_at_unix_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_safety_store::InputSafetyCoordinator;
    #[test]
    fn native_recovery_cannot_acquire_while_actual_input_lease_is_held() {
        assert_eq!(
            crate::input_safety_store::recovery_coordination_scope().unwrap(),
            crate::input_safety_store::physical_input_resource_scope()
                .unwrap()
                .as_str()
        );
        let root = tempfile::tempdir().unwrap();
        let scope = scope(root.path());
        let broker = windows_process_guard::InteractiveInputLeaseBroker::default();
        let lease = windows_process_guard::ScopedInputOwnership::acquire(
            &broker,
            scope.as_str(),
            "actual-input-owner",
            std::time::Duration::ZERO,
        )
        .unwrap();
        let blocked = InputSafetyCoordinator::begin_with_coordination_scope(
            root.path(),
            scope.as_str(),
            &scope,
            "recovery-while-running",
            &["release_isolation"],
            std::time::Duration::from_millis(30),
        );
        assert!(
            matches!(
                blocked,
                Err(crate::input_safety_store::CoordinatorError::Busy { .. })
            ),
            "同一物理输入尚在使用时恢复必须被真实内核锁拒绝"
        );
        drop(lease);
        let _ready = coordinator(root.path(), &scope);
    }
    fn scope(root: &std::path::Path) -> InputSafetyResourceScope {
        InputSafetyResourceScope::parse(&format!(
            "native-recovery-{}",
            root.file_name().unwrap().to_string_lossy()
        ))
        .unwrap()
    }
    fn coordinator(
        root: &std::path::Path,
        scope: &InputSafetyResourceScope,
    ) -> InputSafetyCoordinator {
        InputSafetyCoordinator::begin_with_coordination_scope(
            root,
            scope.as_str(),
            scope,
            "native-recovery-test",
            &["release_isolation", "open_new_input"],
            std::time::Duration::from_secs(1),
        )
        .unwrap()
    }
    fn decision(scope: &InputSafetyResourceScope) -> ReleaseIsolationDecision {
        ReleaseIsolationDecision {
            decision_id: "native-decision".into(),
            scope: scope.clone(),
            operator: "真实管道已核实的测试操作者".into(),
            reason: "接受未确认结果，确认执行者已退出".into(),
            evidence_refs: vec![],
            acknowledged_block_ids: vec!["block".into()],
            acknowledged_run_ids: vec![],
            release_epoch: 0,
            coordinator_instance_id: String::new(),
            decided_at_unix_ms: 0,
        }
    }
    fn seed(root: &std::path::Path, scope: &InputSafetyResourceScope) -> InputSafetyStore {
        let store = InputSafetyStore::open_at(root).unwrap();
        store
            .open_resource_block("block", scope, "test", "故障注入：执行结果没有最终回执")
            .unwrap();
        store
            .isolate_resource(scope, "故障注入隔离", None, 0)
            .unwrap();
        store
    }
    #[test]
    fn native_recovery_stale_cross_scope_and_expired_do_not_close_blocks() {
        let root = tempfile::tempdir().unwrap();
        let scope = scope(root.path());
        let store = seed(root.path(), &scope);
        let expected = store.native_recovery_store().snapshot(&scope).unwrap();
        let coordinator = coordinator(root.path(), &scope);
        let now = crate::unix_timestamp_millis();
        let decision = decision(&scope);
        assert!(coordinator
            .store()
            .native_recovery_store()
            .commit_release(&expected, &decision, coordinator.control(), &[], now)
            .is_err());
        let mut other = decision.clone();
        other.scope = InputSafetyResourceScope::parse("other-resource").unwrap();
        assert!(coordinator
            .store()
            .native_recovery_store()
            .commit_release(&expected, &other, coordinator.control(), &[], now + 90_000)
            .is_err());
        store
            .open_resource_block("new-block", &scope, "test", "确认期间新增阻断")
            .unwrap();
        assert!(coordinator
            .store()
            .native_recovery_store()
            .commit_release(
                &expected,
                &decision,
                coordinator.control(),
                &[],
                now + 90_000
            )
            .is_err());
        assert_eq!(
            store.unacknowledged_open_block_ids(&scope).unwrap().len(),
            2
        );
        assert!(store.latest_release_decision(&scope).unwrap().is_none());
    }
    #[test]
    fn native_recovery_inflight_requires_exit_and_preserves_unknown_outcome() {
        for kind in ["ephemeral_helper", "persistent_native_panel"] {
            inflight_requires_exit_and_preserves_unknown_outcome(kind);
        }
    }
    fn inflight_requires_exit_and_preserves_unknown_outcome(kind: &str) {
        let root = tempfile::tempdir().unwrap();
        let scope = scope(root.path());
        let store = seed(root.path(), &scope);
        let process = ProcessInstanceEvidence {
            pid: 4242,
            creation_time_filetime: 1234567,
        };
        let executor = runtime::ExecutorRegistration {
            executor_instance_id: "executor".into(),
            launch_operation_id: "launch".into(),
            coordinator_instance_id: "host".into(),
            scope: scope.clone(),
            host_launch_instance: "host".into(),
            action_id: "action".into(),
            pid: process.pid,
            creation_time_100ns: Some(process.creation_time_filetime),
            user_session: None,
            helper: runtime::HelperIdentity {
                host_process_path: "test-helper.exe".into(),
                script_or_program_digest: "sha256:test".into(),
            },
            protocol_version: 2,
            supervision_bound: true,
            state: runtime::ExecutorInstanceState::RegisteredPendingVerification,
            revision: 1,
        };
        store
            .executor_store()
            .register_launch_intent(&executor, 1)
            .unwrap();
        store
            .executor_store()
            .record_instance_evidence(
                "executor",
                runtime::ExecutorObservation::MatchesAndAlive,
                Some(process.creation_time_filetime),
                true,
                2,
            )
            .unwrap();
        let permit = runtime::InputPermit {
            permit_id: "permit".into(),
            action_id: "action".into(),
            execution_attempt_id: runtime::ExecutionAttemptId::new("action", 1, "step", 1).unwrap(),
            scope: scope.clone(),
            execution_context_ref: "call".into(),
            frozen_action_digest: "sha256:action".into(),
            gate_revision: 1,
            issued_owner_id: "owner".into(),
            issued_epoch: 0,
            expires_at_unix_ms: 10_000,
            executor_instance_id: Some("executor".into()),
            state: runtime::InputPermitState::PendingActivation,
            revision: 1,
            revocation_reason: None,
        };
        store
            .permit_store()
            .register_pending(&permit, runtime::InputPermitState::PendingActivation, 1)
            .unwrap();
        // 故障注入：模拟宿主崩溃在消费后、完成回执前。断言由真实事务实现负责。
        // 两种执行者均复用原事务回归；这里仅注入持久记录类别，不宣称实际OS已退出。
        store.connection_for_test().execute("UPDATE input_safety_executors SET executor_kind=?1 WHERE executor_instance_id='executor'",[kind]).unwrap();
        store.connection_for_test().execute("UPDATE input_safety_permits SET state='dispatch_committed' WHERE permit_id='permit'",[]).unwrap();
        store
            .native_recovery_store()
            .quarantine_unsettled_on_startup(&scope)
            .unwrap();
        let expected = store.native_recovery_store().snapshot(&scope).unwrap();
        let coordinator = coordinator(root.path(), &scope);
        let mut decision = decision(&scope);
        decision.acknowledged_block_ids =
            expected.blocks.iter().map(|b| b.block_id.clone()).collect();
        let expiry = crate::unix_timestamp_millis() + 90_000;
        assert!(
            coordinator
                .store()
                .native_recovery_store()
                .commit_release(&expected, &decision, coordinator.control(), &[], expiry)
                .is_err(),
            "仍在途不得凭人工确认跳过退出事实"
        );
        assert!(store.has_open_resource_block(&scope).unwrap());
        let mut failing = decision.clone();
        failing.acknowledged_block_ids.push("不存在的阻断".into());
        assert!(coordinator
            .store()
            .native_recovery_store()
            .commit_release(
                &expected,
                &failing,
                coordinator.control(),
                &[("executor".into(), process)],
                expiry
            )
            .is_err());
        assert_eq!(
            store.permit_store().load_permit("permit").unwrap().state,
            runtime::InputPermitState::DispatchCommitted,
            "事务中途出错不得只提交退出或许可状态"
        );
        assert_eq!(
            store.unacknowledged_open_block_ids(&scope).unwrap().len(),
            3,
            "逐条关闭中途出错也全部回滚"
        );
        let result = coordinator
            .store()
            .native_recovery_store()
            .commit_release(
                &expected,
                &decision,
                coordinator.control(),
                &[("executor".into(), process)],
                expiry,
            )
            .unwrap();
        assert_eq!(result.acknowledged_block_ids.len(), 3);
        assert!(!store.has_open_resource_block(&scope).unwrap());
        assert_eq!(
            store.permit_store().load_permit("permit").unwrap().state,
            runtime::InputPermitState::OutcomeUnknown
        );
        assert_eq!(
            store.connection_for_test().query_row("SELECT state FROM input_safety_executors WHERE executor_instance_id='executor'", [], |row| row.get::<_,String>(0)).unwrap(),
            "exited_confirmed"
        );
        assert!(
            store
                .native_recovery_store()
                .snapshot(&scope)
                .unwrap()
                .permits[0]
                .unknown_accepted
        );
        store
            .native_recovery_store()
            .quarantine_unsettled_on_startup(&scope)
            .unwrap();
        assert!(
            !store.has_open_resource_block(&scope).unwrap(),
            "明确接受过的同一未知修订在重启时不重新隔离"
        );
        assert!(
            coordinator
                .store()
                .native_recovery_store()
                .commit_release(
                    &expected,
                    &decision,
                    coordinator.control(),
                    &[("executor".into(), process)],
                    expiry
                )
                .is_err(),
            "已提交确认不可再次提交"
        );
        // 同一真实结果字段为 Unknown、没有接受事件，相当于回执落库后/建 block 前崩溃窗口。
        store
            .connection_for_test()
            .execute(
                "DELETE FROM input_safety_events WHERE detail LIKE 'native-permit-risk-accepted:%'",
                [],
            )
            .unwrap();
        store
            .native_recovery_store()
            .quarantine_unsettled_on_startup(&scope)
            .unwrap();
        assert!(
            store.has_open_resource_block(&scope).unwrap(),
            "未被人工接受的 Unknown 即使 executor 已退出也必须隔离"
        );
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RecoveryExecutor {
    pub id: String,
    pub pid: u32,
    pub created: Option<u64>,
    pub revision: u64,
    pub state: String,
    pub kind: crate::persistent_panel_executor::InputExecutorKind,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RecoveryPermit {
    pub id: String,
    pub executor: Option<String>,
    pub revision: u64,
    pub state: String,
    pub unknown_accepted: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RecoverySnapshot {
    pub scope: String,
    pub gate_revision: u64,
    pub recovery_epoch: u64,
    pub resource_epoch: u64,
    pub blocks: Vec<RecoveryBlock>,
    pub permits: Vec<RecoveryPermit>,
    pub executors: Vec<RecoveryExecutor>,
}
impl RecoverySnapshot {
    pub(crate) fn has_unsettled_execution(&self) -> bool {
        self.executors
            .iter()
            .any(|executor| executor.state != "exited_confirmed")
            || self.permits.iter().any(|permit| {
                matches!(permit.state.as_str(), "dispatch_committed" | "executing")
                    || (permit.state == "outcome_unknown" && !permit.unknown_accepted)
            })
    }
}

pub(crate) struct NativeRecoveryStore<'a> {
    store: &'a InputSafetyStore,
    connection: &'a Connection,
}
impl<'a> NativeRecoveryStore<'a> {
    pub(crate) fn new(store: &'a InputSafetyStore, connection: &'a Connection) -> Self {
        Self { store, connection }
    }

    fn read_snapshot(&self, scope: &InputSafetyResourceScope) -> Result<RecoverySnapshot, String> {
        let resource = self
            .store
            .resource_state(scope)
            .map_err(|e| e.to_string())?;
        let mut statement = self.connection.prepare("SELECT block_id,source_kind,source_ref,opened_at_unix_ms,
            COALESCE((SELECT reason FROM input_safety_incidents i WHERE i.scope=b.scope AND i.incident_id=b.source_ref),
                     (SELECT detail FROM input_safety_events e WHERE e.scope=b.scope AND e.subject_id=b.source_ref ORDER BY event_seq DESC LIMIT 1))
            FROM input_safety_resource_blocks b WHERE scope=?1 AND state='open' ORDER BY block_id").map_err(|e| e.to_string())?;
        let blocks = statement
            .query_map([scope.as_str()], |r| {
                let kind = r.get::<_, String>(1)?;
                let reference = r.get::<_, String>(2)?;
                Ok(RecoveryBlock {
                    block_id: r.get(0)?,
                    reason: r
                        .get::<_, Option<String>>(4)?
                        .unwrap_or_else(|| match kind.as_str() {
                            "native_executor_outcome_unknown" => {
                                format!("原生输入结果或按键释放未确认，原始请求：{reference}")
                            }
                            "native_restart_unsettled" => {
                                format!("上次进程结束时仍有未结账执行者或在途输入许可：{reference}")
                            }
                            _ => format!("{kind}：{reference}"),
                        }),
                    opened_at_unix_ms: r.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let mut statement = self.connection.prepare("SELECT permit_id,executor_instance_id,revision,state,
            EXISTS(SELECT 1 FROM input_safety_events e JOIN input_safety_release_decisions d ON d.decision_id=e.subject_id AND d.scope=e.scope
                   WHERE e.scope=p.scope AND e.kind='recovery_stage_advanced' AND e.detail='native-permit-risk-accepted:'||p.permit_id||':revision:'||p.revision)
            FROM input_safety_permits p WHERE scope=?1 AND state NOT IN ('finished','revoked') ORDER BY permit_id").map_err(|e| e.to_string())?;
        let permits = statement
            .query_map([scope.as_str()], |r| {
                Ok(RecoveryPermit {
                    id: r.get(0)?,
                    executor: r.get(1)?,
                    revision: r.get(2)?,
                    state: r.get(3)?,
                    unknown_accepted: r.get(4)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        // 持久宿主不以进程退出结算动作。已结清全部许可时不能仅因宿主常驻而判成在途输入；
        // 未结许可仍纳入快照，未知/已消费动作保留隔离和人工核验，helper口径保持原样。
        let mut statement = self.connection.prepare("SELECT executor_instance_id,pid,creation_time_100ns,revision,state,executor_kind FROM input_safety_executors e WHERE scope=?1 AND
            ((executor_kind!='persistent_native_panel' AND state!='exited_confirmed') OR EXISTS(SELECT 1 FROM input_safety_permits p WHERE p.executor_instance_id=e.executor_instance_id AND p.scope=e.scope AND p.state NOT IN ('finished','revoked'))) ORDER BY executor_instance_id").map_err(|e| e.to_string())?;
        let executors = statement
            .query_map([scope.as_str()], |r| {
                Ok(RecoveryExecutor {
                    id: r.get(0)?,
                    pid: r.get(1)?,
                    created: r.get(2)?,
                    revision: r.get(3)?,
                    state: r.get(4)?,
                    kind: match r.get::<_,String>(5)?.as_str() {
                        "ephemeral_helper" => crate::persistent_panel_executor::InputExecutorKind::EphemeralHelper,
                        "persistent_native_panel" => crate::persistent_panel_executor::InputExecutorKind::PersistentNativePanel,
                        _ => return Err(rusqlite::Error::InvalidQuery),
                    },
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let recovery_epoch = self
            .connection
            .query_row(
                "SELECT epoch FROM input_safety_ownership_epochs WHERE scope=?1",
                [scope.as_str()],
                |r| r.get::<_, u64>(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or(0);
        Ok(RecoverySnapshot {
            scope: scope.as_str().into(),
            gate_revision: resource.revision,
            recovery_epoch,
            resource_epoch: resource.recovery_epoch,
            blocks,
            permits,
            executors,
        })
    }

    pub(crate) fn snapshot(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<RecoverySnapshot, String> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|e| e.to_string())?;
        let snapshot = self.read_snapshot(scope)?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(snapshot)
    }

    /// 启动时只收紧，不因为新表为空或上次来不及建事故就自动开放。
    pub(crate) fn quarantine_unsettled_on_startup(
        &self,
        scope: &InputSafetyResourceScope,
    ) -> Result<(), String> {
        let snapshot = self.snapshot(scope)?;
        let mut unsettled = snapshot
            .executors
            .iter()
            .filter(|e| e.state != "exited_confirmed")
            .map(|e| ("executor", e.id.as_str(), e.revision))
            .collect::<Vec<_>>();
        unsettled.extend(
            snapshot
                .permits
                .iter()
                .filter(|p| {
                    matches!(p.state.as_str(), "dispatch_committed" | "executing")
                        || (p.state == "outcome_unknown" && !p.unknown_accepted)
                })
                .map(|p| ("permit", p.id.as_str(), p.revision)),
        );
        if unsettled.is_empty() {
            return Ok(());
        }
        for (kind, id, revision) in unsettled {
            self.store
                .open_resource_block(
                    &format!("restart-unsettled-{kind}-{id}-v{revision}"),
                    scope,
                    "native_restart_unsettled",
                    id,
                )
                .map_err(|e| e.to_string())?;
        }
        self.store
            .isolate_resource(
                scope,
                "启动时发现上次未结账的原生执行者或在途许可",
                None,
                snapshot.resource_epoch,
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// proof 已在宿主内存原子消费；同一写事务复核版本、收紧未结许可、记录放行。
    pub(crate) fn commit_release(
        &self,
        expected: &RecoverySnapshot,
        decision: &ReleaseIsolationDecision,
        guard: &RecoveryControlGuard,
        confirmed_exits: &[(String, ProcessInstanceEvidence)],
        expires: u64,
    ) -> Result<ReleaseIsolationDecision, String> {
        self.connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;
        let result = (|| {
            if crate::unix_timestamp_millis() >= expires {
                return Err("恢复确认已过期，请重新申请".into());
            }
            // 本次 Coordinator 唯一允许的变化是取得下一代恢复资格；其它恢复经过亦拒绝。
            if expected.recovery_epoch.checked_add(1) != Some(guard.epoch()) {
                return Err("恢复资格代次已变化，请重新确认".into());
            }
            let mut expected_now = expected.clone();
            expected_now.recovery_epoch = guard.epoch();
            if self.read_snapshot(&decision.scope)? != expected_now {
                return Err("资源状态或阻断集合已变化，请重新确认".into());
            }
            for executor in &expected.executors {
                if executor.state == "exited_confirmed" {
                    continue;
                }
                let evidence = confirmed_exits
                    .iter()
                    .find(|(id, _)| id == &executor.id)
                    .map(|(_, p)| p)
                    .ok_or("仍有执行者未确认退出，不得开放输入")?;
                let store = self.store.executor_store();
                match executor.kind {
                    crate::persistent_panel_executor::InputExecutorKind::EphemeralHelper => store.record_native_completion(
                        &executor.id,*evidence,true,crate::unix_timestamp_millis()),
                    crate::persistent_panel_executor::InputExecutorKind::PersistentNativePanel => store.record_verified_panel_exit(
                        &executor.id,*evidence,crate::unix_timestamp_millis()),
                }.map_err(|e| e.to_string())?;
            }
            let mut accepted_permits = Vec::new();
            for permit in &expected.permits {
                if permit.state != "pending_activation" {
                    let executor_id = permit
                        .executor
                        .as_ref()
                        .ok_or("在途许可缺少执行者归属，需先核查")?;
                    if !expected.executors.iter().any(|e| {
                        &e.id == executor_id
                            && (e.state == "exited_confirmed"
                                || confirmed_exits.iter().any(|(id, _)| id == executor_id))
                    }) {
                        return Err("在途许可的执行者未确认退出".into());
                    }
                }
                let next = match permit.state.as_str() {
                    "pending_activation" => "revoked",
                    "dispatch_committed" | "executing" => {
                        accepted_permits.push((permit.id.clone(), permit.revision + 1));
                        "outcome_unknown"
                    }
                    "outcome_unknown" => {
                        accepted_permits.push((permit.id.clone(), permit.revision));
                        continue;
                    }
                    _ => return Err("许可状态无法判定".into()),
                };
                self.connection.execute("UPDATE input_safety_permits SET state=?1,revision=revision+1,updated_at_unix_ms=?2 WHERE permit_id=?3 AND revision=?4",
                    params![next,crate::unix_timestamp_millis() as i64,permit.id,permit.revision as i64]).map_err(|e|e.to_string())?;
            }
            // 不把未知结果改成功；此决定只接受当前风险并解除逐条展示过的阻断。
            let recorded = self
                .store
                .release_isolation_authorized(decision, guard)
                .map_err(|e| e.to_string())?;
            for (id, revision) in accepted_permits {
                self.store
                    .append_event(runtime::InputSafetyEvent {
                        kind: runtime::InputSafetyEventKind::RecoveryStageAdvanced,
                        scope: Some(decision.scope.clone()),
                        subject_id: Some(recorded.decision_id.clone()),
                        detail: format!("native-permit-risk-accepted:{id}:revision:{revision}"),
                        recorded_at_unix_ms: recorded.decided_at_unix_ms,
                    })
                    .map_err(|e| e.to_string())?;
            }
            Ok(recorded)
        })();
        match result {
            Ok(value) => {
                self.connection
                    .execute_batch("COMMIT")
                    .map_err(|e| e.to_string())?;
                Ok(value)
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }
}
