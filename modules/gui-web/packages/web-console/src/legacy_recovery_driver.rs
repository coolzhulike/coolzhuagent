//! RD4-03 驱动层：把 A-1.6 的 **R1–R9** 顺序串成一条生产恢复入口。
//!
//! 每一步都只调用**已按资格/事实**设计的入口：协调器（跨进程锁 + epoch）、按资格的事件与
//! 阶段推进、RD4-01 的 `converge_legacy_run`（同事务写控制终态 + 收敛事实）。
//!
//! 本驱动坚持三条裁决口径：
//! 1. **不得自证**：`recovery_control_authority` 与 `new_input_intake_paused` 的值来自
//!    协调器**真实做过的事**（持有跨进程锁、已把该资源隔离），不是调用方的声明；
//! 2. **阻断优先**：只要资源评估为 `Uncertain`，就**建立真实事故并保持隔离**，R9 不开放新输入；
//! 3. **未知不等于没有**：owner 解析为 `Unknown` 时**拒绝写终态**（保持隔离），而不是当作"无 owner"。

use std::path::Path;

use runtime::{
    IncidentState as RuntimeIncidentState, InputSafetyIncident, InputSafetyResourceScope,
    LegacyCuRunConvergenceEvidence, LegacyResourceScope, LegacyRunHistoricalOutcome,
    LegacyRunInputResourceState, RecoveryDisposition, RecoveryStage,
};

use crate::computer_use_store::{
    ComputerUseRunStore, LegacyRunConvergenceOutcome, LegacyUnconvergedRun,
};
use crate::legacy_recovery::{
    assess_legacy_run_resource, resolve_legacy_run_owner, LegacyResourceAssessment,
    LegacyRunOwner,
};
use crate::input_safety_store::{
    CoordinatorError, InputSafetyCoordinator, InputSafetyStoreError, RecoveryControlGuard,
};

/// 一次遗留运行恢复的最终去向。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyRecoveryDisposition {
    /// 已收敛（写入控制终态 + 收敛事实）。
    Converged { convergence_id: i64 },
    /// 同一次收敛已存在：幂等，未写第二份。
    AlreadyConverged { convergence_id: i64 },
    /// 本次**没有重放**：该操作的处置已是终态（PR-01／P0-1），重放只会重复同一结论。
    AlreadySettled { disposition: RecoveryDisposition },
    /// **在任何写入之前**拒绝（缺前提/缺检查），资源保持隔离。
    RefusedBeforeWrite { code: String, message: String },
    /// 规则拒绝（已有真实终态 / 提交候选 / 原 revision 变了 / 冲突 / 操作 ID 复用）。
    RefusedByRule { message: String },
}

/// 恢复一次的结果（含到达的阶段，便于审计"崩溃在哪一步"）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyRecoveryReport {
    pub call_id: String,
    pub stage: RecoveryStage,
    pub disposition: LegacyRecoveryDisposition,
    pub owner: LegacyRunOwner,
    pub assessment: LegacyResourceAssessment,
    /// R9 之后是否**真的**开放了新输入（本驱动对遗留运行的默认结论是"否"）。
    pub new_input_reopened: bool,
}

/// 驱动输入。
pub(crate) struct LegacyRecoveryInputs<'a> {
    pub session_db_path: &'a Path,
    /// 输入安全库根（生产由 launcher 注入；测试传临时目录）。
    pub input_safety_root: &'a Path,
    /// 跨进程协调范围（生产用 `recovery_coordination_scope()`）。
    pub coordination_scope: &'a str,
    /// 调用方**已经持有**的协调器（同一资源范围只允许一个协调者）。
    ///
    /// 启动路径必须复用它：若每个遗留运行各起一个协调器，即使协调范围字符串不同，
    /// 它们改的仍是**同一个资源 scope** 的 epoch，会互相顶掉资格（现场实测踩到过）。
    pub coordinator: Option<&'a InputSafetyCoordinator>,
    pub resource_scope: &'a InputSafetyResourceScope,
    pub run: &'a LegacyUnconvergedRun,
    pub recovery_operation_id: &'a str,
    pub recovery_service_instance: &'a str,
    /// 来源数据库标识（**不是**补造的 workspace id）。
    pub source_database_identity: &'a str,
    /// **提交候选检查**：必须由能读事实日志的调用方提供。
    ///
    /// `None` ⇒ 本驱动**拒绝收敛**（`commit_candidate_check_missing`）——因为
    /// "本次没查到提交候选"必须是**一次真实检查的结论**，不能靠缺省值凑成 `false`。
    pub observed_commit_candidate: Option<bool>,
    /// 可选的独立安全检查（拿不出来 ⇒ 资源评估必为 `Uncertain`）。
    pub independent_safety_check: Option<&'a runtime::CurrentResourceSafetyCheck>,
}

/// 幂等推进阶段：**已达标就跳过**，未达标才按资格推进。
///
/// 为什么需要幂等：恢复是"重启后重放"。上次中断在半途的操作，其 stage 已落盘到中途
/// （例如 `r5`），重放时驱动仍从 R2 起逐条请求——若直接调
/// [`InputSafetyStore::advance_recovery_operation_authorized`]，store 的"不得跳步或回退"
/// 守门会把**正常重放**判成回退而拒绝，使启动路径每次都失败、资源永久保持阻断
/// （现场实测：遗留操作停在 `r1`/`r5` 时命中）。
/// 这里只负责"跳过已达标的阶段"；阶段本身的推进仍由 store 按持有资格校验。
/// 结账：写入**终态处置**（PR-01／P0-1）。与 `advance` 同例——按持有资格写。
///
/// 为什么要结账：被拒绝的恢复若一直挂在"未结账"上，一个无法判定的遗留运行就能让资源
/// **永久隔离**（台账 §B-80）。结账把"这件事怎么结的"落成事实，同时把它移出"待对账"。
fn settle(
    coordinator: &InputSafetyCoordinator,
    operation_id: &str,
    disposition: RecoveryDisposition,
    reason: &str,
) -> Result<(), CoordinatorError> {
    coordinator
        .store()
        .settle_recovery_operation_authorized(operation_id, coordinator.control(), disposition, reason)
        .map(|_| ())
        .map_err(CoordinatorError::Store)
}

fn advance(
    coordinator: &InputSafetyCoordinator,
    operation_id: &str,
    next: RecoveryStage,
) -> Result<(), CoordinatorError> {
    let current = coordinator
        .store()
        .recovery_operation(operation_id)
        .map_err(CoordinatorError::Store)?
        .ok_or_else(|| {
            CoordinatorError::Store(InputSafetyStoreError::Sqlite("恢复操作不存在".to_string()))
        })?;
    if current.stage.order() >= next.order() {
        return Ok(());
    }
    coordinator
        .store()
        .advance_recovery_operation_authorized(operation_id, coordinator.control(), next)
        .map_err(CoordinatorError::Store)
}

/// 执行一次完整的 R1–R9 恢复。
pub(crate) fn run_legacy_recovery(
    inputs: &LegacyRecoveryInputs<'_>,
) -> Result<LegacyRecoveryReport, CoordinatorError> {
    let run = inputs.run;

    // ---- R1：取得同 scope 唯一恢复协调权（跨进程锁 + epoch + RecoveryOperationStarted）----
    //
    // 调用方已持有协调器时**复用它**（同一资源范围只允许一个协调者）；否则自行取得。
    let owned_coordinator;
    let coordinator = match inputs.coordinator {
        Some(existing) => existing,
        None => {
            owned_coordinator = InputSafetyCoordinator::begin(
                inputs.input_safety_root,
                inputs.resource_scope,
                inputs.recovery_operation_id,
                &["converge_legacy_cu_run"],
                std::time::Duration::from_secs(2),
            )?;
            &owned_coordinator
        }
    };
    let guard: &RecoveryControlGuard = coordinator.control();
    // 无论自有还是**复用**协调器，都要确保"本次运行"的操作已登记并绑定当前资格：
    // 协调器自带的登记用的是它自己的 operation id；复用时不补这一步，
    // 后续按资格推进阶段会报"恢复操作不存在"（测试抓到过）。这里用幂等登记，
    // 已存在的操作只会被推进 stage/committed，不会被换绑。
    coordinator
        .store()
        .put_recovery_operation(&runtime::InputSafetyRecoveryOperation {
            recovery_operation_id: inputs.recovery_operation_id.to_string(),
            coordinator_instance_id: guard.coordinator_id().to_string(),
            recovery_epoch: guard.epoch(),
            gate_revision: guard.epoch(),
            scope: inputs.resource_scope.clone(),
            source_database_identity: inputs.source_database_identity.to_string(),
            candidate_run_ids: vec![run.call_id.clone()],
            allowed_operations: vec!["converge_legacy_cu_run".to_string()],
            stage: RecoveryStage::CoordinationAcquired,
            recorded_at_unix_ms: unix_now_ms(),
            committed: false,
            disposition: RecoveryDisposition::Pending,
        })
        .map_err(CoordinatorError::Store)?;
    // **重启后必须重新取得协调权才能继续**（裁决 §2.2）：旧运行遗留的操作行仍绑在已失效的
    // epoch／协调者上，而 `put_recovery_operation` 刻意**只允许推进 stage/committed、不允许换绑**。
    // 因此这里按**当前**资格重新绑定；否则后续按资格推进阶段会报"未持有当前恢复资格"
    // （现场实测：持有 epoch 1、当前 5 ⇒ 每次启动都在 R2 失败，资源永久保持阻断）。
    if let Some(existing) = coordinator
        .store()
        .recovery_operation(inputs.recovery_operation_id)
        .map_err(CoordinatorError::Store)?
    {
        let bound_elsewhere = existing.recovery_epoch != guard.epoch()
            || existing.coordinator_instance_id != guard.coordinator_id();
        if bound_elsewhere && !existing.committed {
            coordinator
                .store()
                .rebind_recovery_operation_authorized(inputs.recovery_operation_id, guard)
                .map_err(CoordinatorError::Store)?;
        }
        // **已结账的操作不再重放**（PR-01／P0-1）：结账是结论（例如"需要人工复核"）。重放只会
        // 重复做出同一拒绝、反复建立相同阻断，并把"等人工"伪装成"在办"。这里如实报告上次的
        // 结账结果，把待办交给人工通道；资源是否放行由阻断事实与放行决定继续表达。
        if existing.is_settled() {
            let store = ComputerUseRunStore::open(inputs.session_db_path).map_err(|error| {
                CoordinatorError::Store(InputSafetyStoreError::Sqlite(error.to_string()))
            })?;
            return Ok(LegacyRecoveryReport {
                call_id: run.call_id.clone(),
                stage: existing.stage,
                disposition: LegacyRecoveryDisposition::AlreadySettled {
                    disposition: existing.disposition,
                },
                owner: resolve_legacy_run_owner(inputs.session_db_path, &run.session_id, &run.turn_id),
                assessment: assess_legacy_run_resource(
                    &store,
                    &run.session_id,
                    &run.turn_id,
                    inputs.independent_safety_check,
                ),
                new_input_reopened: false,
            });
        }
    }

    // ---- R2：持久化恢复意图 + **关闭新输入接纳**（收紧方向不需要资格，但由协调者执行）----
    coordinator
        .store()
        .isolate_resource(
            inputs.resource_scope,
            "遗留运行恢复：先关闸再收敛",
            Some(guard.coordinator_id()),
            guard.epoch(),
        )
        .map_err(CoordinatorError::Store)?;
    advance(&coordinator, inputs.recovery_operation_id, RecoveryStage::IntentPersistedAndIntakeClosed)?;

    // ---- R3：登记在途（本阶段可如实登记的就是"受影响的运行集合"）----
    advance(&coordinator, inputs.recovery_operation_id, RecoveryStage::InFlightRegistered)?;

    // ---- R4：旧执行者/宿主状态核查（遗留运行早于本实例，本进程内无在途执行者）----
    let interactive_scope = windows_process_guard::current_interactive_session_scope()
        .unwrap_or_else(|_| "unknown-session".to_string());
    advance(&coordinator, inputs.recovery_operation_id, RecoveryStage::ExecutorStopRequested)?;

    // ---- R5：为未确认风险建立**真实**事故 + 阻断，并取得**已验证**引用 ----
    let store = ComputerUseRunStore::open(inputs.session_db_path)
        .map_err(|error| CoordinatorError::Store(InputSafetyStoreError::Sqlite(error.to_string())))?;
    let assessment = assess_legacy_run_resource(
        &store,
        &run.session_id,
        &run.turn_id,
        inputs.independent_safety_check,
    );
    let run_ref = format!("{}#{}", inputs.source_database_identity, run.call_id);
    let incident_id = format!("legacy-run-{}", run.call_id);
    let block_id = format!("legacy-block-{}", run.call_id);
    // **只有资源不确定时**才建立事故与阻断：安全时开阻断本身就是错的
    // （裁决 §2.5：资源安全已独立证明时不应建立不存在的释放事故）。
    let mut blocking_refs: Vec<String> = Vec::new();
    if !assessment.is_safe() {
    coordinator
        .store()
        .establish_incident_authorized(
            &InputSafetyIncident {
                incident_id: incident_id.clone(),
                scope: inputs.resource_scope.clone(),
                reason: format!("遗留 CU 运行 {} 未收尾：{}", run.call_id, assessment.describe()),
                original_run_ref: Some(run_ref.clone()),
                state: RuntimeIncidentState::Pending,
                created_at_unix_ms: unix_now_ms(),
                resolved_at_unix_ms: None,
                evidence_refs: Vec::new(),
            },
            guard,
        )
        .map_err(CoordinatorError::Store)?;
    coordinator
        .store()
        .open_resource_block(&block_id, inputs.resource_scope, "legacy_recovery", &incident_id)
        .map_err(CoordinatorError::Store)?;
    let revision = coordinator
        .store()
        .resource_state(inputs.resource_scope)
        .map_err(CoordinatorError::Store)?
        .revision;
    let raw_ref = format!("{}#{incident_id}", coordinator.store().store_id().as_str());
    // **在同一服务内完成真实核查**后才把它作为阻断引用交给收敛（裁决 §1.5 允许的形态）。
    coordinator
        .store()
        .verify_blocking_ref(&raw_ref, inputs.resource_scope, revision, Some(&run_ref))
        .map_err(|rejection| {
            CoordinatorError::Store(InputSafetyStoreError::Sqlite(format!(
                "阻断引用核查失败（{}）：{rejection}",
                rejection.code()
            )))
        })?;
        blocking_refs.push(raw_ref.clone());
    }
    advance(&coordinator, inputs.recovery_operation_id, RecoveryStage::IncidentEstablished)?;

    // ---- R6：按**真实关系**核对 owner；未知 ⇒ 拒绝写终态并保持隔离 ----
    let owner = resolve_legacy_run_owner(inputs.session_db_path, &run.session_id, &run.turn_id);
    if let LegacyRunOwner::Unknown { reason } = owner {
        // 决策（P0-1 情况 2）：owner **永久未知**不得停在 pending、也不得改判成"已收敛"，
        // 而是 `HumanReviewRequired`——资源**仍然隔离**，但待办明确归属到人（可查询、可放行）。
        settle(
            &coordinator,
            inputs.recovery_operation_id,
            RecoveryDisposition::HumanReviewRequired,
            &format!("owner 关系未知（{}）：请人工核对并决定是否放行", reason.code()),
        )?;
        return Ok(LegacyRecoveryReport {
            call_id: run.call_id.clone(),
            stage: RecoveryStage::IncidentEstablished,
            disposition: LegacyRecoveryDisposition::RefusedBeforeWrite {
                code: reason.code().to_string(),
                message: format!(
                    "owner 关系未知（{}）：未知不是\"没有 owner\"，因此不写终态；资源保持隔离",
                    reason.code()
                ),
            },
            owner,
            assessment,
            new_input_reopened: false,
        });
    }
    let owner_checks = match &owner {
        LegacyRunOwner::Resolved { runtime_run_id, kind, owner_id } => vec![format!(
            "runtime_run={runtime_run_id} kind={kind} owner={owner_id:?}"
        )],
        LegacyRunOwner::Unknown { reason } => vec![format!("owner_unknown={}", reason.code())],
    };
    advance(&coordinator, inputs.recovery_operation_id, RecoveryStage::RunRelationChecked)?;

    // ---- R7：同事务提交已裁定终态与收敛事实 ----
    let Some(observed_commit_candidate) = inputs.observed_commit_candidate else {
        // **刻意不结账**：这不是"结论"，而是"这次尝试不完整"（调用方漏了必填的真实检查）。
        // 结账会把它伪装成已处置，还会让后续带齐检查的重放因"已结账"而不再执行。
        // 因此保持 `Pending`——它在"待对账"里如实表现为**待补检查**，而不是永久隔离的既成事实。
        return Ok(LegacyRecoveryReport {
            call_id: run.call_id.clone(),
            stage: RecoveryStage::RunRelationChecked,
            disposition: LegacyRecoveryDisposition::RefusedBeforeWrite {
                code: "commit_candidate_check_missing".to_string(),
                message: "缺少\"是否存在待确认终态提交\"的**真实检查**：不得用缺省值凑成 false，故拒绝收敛".to_string(),
            },
            owner,
            assessment,
            new_input_reopened: false,
        });
    };
    let evidence = LegacyCuRunConvergenceEvidence {
        owner_checks,
        executor_checks: vec![
            format!("interactive_session_scope={interactive_scope}"),
            "本进程内无该遗留运行的在途执行者（其早于本实例）".to_string(),
        ],
        existing_receipts: Vec::new(),
        commit_candidates: observed_commit_candidate
            .then(|| format!("caller-check:{run_ref}"))
            .into_iter()
            .collect(),
        resource_incidents: blocking_refs.clone(),
    };
    let historical_outcome = LegacyRunHistoricalOutcome::Unknown;
    let input_resource = LegacyRunInputResourceState {
        old_executor_may_be_present: Some(false),
        unconfirmed_release_obligations: 0,
        blocking_event_refs: blocking_refs.clone(),
        safe_for_new_input: assessment.is_safe(),
    };
    let operated_at_ms = unix_now_ms();
    // 契约要求**始终**提供"本次建立的安全检查记录"。它记录的是"本次在哪个**当前候选 scope**
    // 做过检查"（基准明确为 CurrentMayBeAffectedNotHistoricalScope，**不是**历史 scope），
    // 而"是否安全"由 `input_resource.safe_for_new_input` 承担——因此提供记录**不等于**宣称安全。
    let owned_check = runtime::CurrentResourceSafetyCheck {
        basis: runtime::CurrentResourceCandidateBasis::CurrentMayBeAffectedNotHistoricalScope,
        session_id: run.session_id.clone(),
        turn_id: run.turn_id.clone(),
        checked_at_unix_ms: operated_at_ms,
    };
    let check_record = inputs.independent_safety_check.unwrap_or(&owned_check);
    let outcome = store
        .converge_legacy_run(&crate::computer_use_store::LegacyRunConvergenceRequest {
            source_database_identity: inputs.source_database_identity,
            source_database_identity_registered_now: false,
            call_id: &run.call_id,
            expected_original_state: &run.state,
            expected_original_state_version: run.state_version,
            observed_commit_candidate,
            recovery_operation_id: inputs.recovery_operation_id,
            recovery_service_instance: inputs.recovery_service_instance,
            // 这两个值来自协调器**真实做过的事**（持有跨进程锁 + 已隔离该资源），
            // 不是调用方声明：guard 只能由协调器产生。
            recovery_control_authority: guard.coordinator_id(),
            new_input_intake_paused: true,
            operated_at_ms,
            reconciled_at_ms: operated_at_ms,
            resource_blocking_refs: &blocking_refs,
            evidence: &evidence,
            historical_outcome: &historical_outcome,
            input_resource: &input_resource,
            historical_resource_scope: &LegacyResourceScope::Unrecorded,
            current_resource_safety_check: check_record,
        })
        .map_err(|error| CoordinatorError::Store(InputSafetyStoreError::Sqlite(error.to_string())))?;

    let disposition = match outcome {
        LegacyRunConvergenceOutcome::Converged { convergence_id, .. } => {
            advance(&coordinator, inputs.recovery_operation_id, RecoveryStage::TerminalCommitted)?;
            LegacyRecoveryDisposition::Converged { convergence_id }
        }
        LegacyRunConvergenceOutcome::AlreadyConverged { convergence_id } => {
            LegacyRecoveryDisposition::AlreadyConverged { convergence_id }
        }
        LegacyRunConvergenceOutcome::Refused(refusal) => {
            // 规则拒绝是**有依据的结论**（已有真实终态／存在提交候选／revision 变了／冲突／
            // 操作 ID 复用）：机器已判定不得写终态 ⇒ `KeptIsolated`（资源继续隔离，无人工待办）。
            settle(
                &coordinator,
                inputs.recovery_operation_id,
                RecoveryDisposition::KeptIsolated,
                &format!("规则拒绝，按裁决保持隔离：{refusal:?}"),
            )?;
            LegacyRecoveryDisposition::RefusedByRule {
                message: format!("{refusal:?}"),
            }
        }
        LegacyRunConvergenceOutcome::PreconditionUnmet(precondition) => {
            // 前提不满足（例如原状态/revision 已被改动）：同样是有依据的结论 ⇒ 保持隔离并结账。
            settle(
                &coordinator,
                inputs.recovery_operation_id,
                RecoveryDisposition::KeptIsolated,
                &format!("收敛前提不满足，按裁决保持隔离：{precondition:?}"),
            )?;
            LegacyRecoveryDisposition::RefusedBeforeWrite {
                code: "convergence_precondition_unmet".to_string(),
                message: format!("{precondition:?}"),
            }
        }
    };

    // ---- R8/R9：阶段提交；**只在**独立证明安全且无未关闭阻断时才开放新输入 ----
    let mut new_input_reopened = false;
    if matches!(disposition, LegacyRecoveryDisposition::Converged { .. }) {
        advance(&coordinator, inputs.recovery_operation_id, RecoveryStage::StageCommitted)?;
        if assessment.is_safe() {
            let reopened = coordinator
                .store()
                .reopen_new_input_authorized(inputs.resource_scope, guard);
            match reopened {
                Ok(state) => {
                    new_input_reopened = state.accepts_new_input;
                    let _ = advance(
                        &coordinator,
                        inputs.recovery_operation_id,
                        RecoveryStage::Reopened,
                    );
                }
                // 有未关闭阻断 ⇒ 保持隔离（**不是**失败，是裁决要求的保守结果）。
                Err(_) => new_input_reopened = false,
            }
        }
    }

    // **结账放在阶段阶梯之后**（PR-01／P0-1）：R8（`stage_committed`）由 store 置
    // `committed = true`，而阶段推进要求"未提交"才能前进。若在收敛分支里提前结账，
    // R8 会被 store 的"不得跳步或回退"守门挡下（现场实测：
    // `当前 r7_terminal_committed → 请求 r8_stage_committed`）。因此顺序是：
    // 先走完阶段 → 再结账。
    if matches!(
        disposition,
        LegacyRecoveryDisposition::Converged { .. } | LegacyRecoveryDisposition::AlreadyConverged { .. }
    ) {
        settle(
            &coordinator,
            inputs.recovery_operation_id,
            RecoveryDisposition::Recovered,
            "已收敛：控制终态与收敛事实已落库",
        )?;
    }

    Ok(LegacyRecoveryReport {
        call_id: run.call_id.clone(),
        stage: if new_input_reopened {
            RecoveryStage::Reopened
        } else if matches!(disposition, LegacyRecoveryDisposition::Converged { .. }) {
            RecoveryStage::StageCommitted
        } else {
            RecoveryStage::IncidentEstablished
        },
        disposition,
        owner,
        assessment,
        new_input_reopened,
    })
}

fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_use_store::{seed_legacy_unrecorded_run_for_test, ComputerUseRunStore};

    fn setup() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
        let directory = tempfile::TempDir::new().expect("tempdir");
        let session_db = directory.path().join("web-sessions.sqlite3");
        let safety_root = directory.path().join("input-safety");
        {
            let connection = crate::open_session_connection(&session_db).expect("open");
            crate::initialize_session_schema(&connection).expect("schema");
        }
        (directory, session_db, safety_root)
    }

    fn scope() -> InputSafetyResourceScope {
        InputSafetyResourceScope::parse("windows-session-recovery").expect("scope")
    }

    /// 每个用例的临时目录给出唯一协调范围，避免测试触碰真实交互会话的输入锁。
    fn isolated_coordination_scope(safety_root: &Path) -> String {
        format!("legacy-recovery-test|{}", safety_root.display())
    }

    fn run_with_isolated_coordinator(
        inputs: &LegacyRecoveryInputs<'_>,
    ) -> Result<LegacyRecoveryReport, CoordinatorError> {
        assert!(inputs.coordinator.is_none(), "夹具只替换未注入的协调器");
        let coordinator = InputSafetyCoordinator::begin_with_coordination_scope(
            inputs.input_safety_root,
            &isolated_coordination_scope(inputs.input_safety_root),
            inputs.resource_scope,
            inputs.recovery_operation_id,
            &["converge_legacy_cu_run"],
            std::time::Duration::from_secs(2),
        )?;
        let report = run_legacy_recovery(&LegacyRecoveryInputs {
            session_db_path: inputs.session_db_path,
            input_safety_root: inputs.input_safety_root,
            coordination_scope: inputs.coordination_scope,
            coordinator: Some(&coordinator),
            resource_scope: inputs.resource_scope,
            run: inputs.run,
            recovery_operation_id: inputs.recovery_operation_id,
            recovery_service_instance: inputs.recovery_service_instance,
            source_database_identity: inputs.source_database_identity,
            observed_commit_candidate: inputs.observed_commit_candidate,
            independent_safety_check: inputs.independent_safety_check,
        });
        coordinator.relinquish().expect("测试协调器必须释放");
        report
    }

    /// 重放：上次中断留下的操作（绑在**已失效**的 epoch 上）必须被重新绑定并继续。
    ///
    /// 现场背景：驱动按 `run.call_id` 命名操作 id，而 `put_recovery_operation` 刻意只推进
    /// stage/committed、**不换绑**；旧操作因此仍绑在旧 epoch 上，后续按资格推进阶段报
    /// "未持有当前恢复资格（持有 epoch 1，当前 5）"⇒ 启动路径每次失败、资源永久阻断。
    #[test]
    fn replay_rebinds_an_operation_left_by_a_previous_run() {
        let (_directory, session_db, safety_root) = setup();
        crate::create_chat_runtime_run_sqlite(
            &session_db,
            "run-chat-replay",
            "claim",
            "ws-0123456789abcdef",
            Some("session-1"),
            "room-1",
            "turn-1",
        )
        .expect("seed runtime run");
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        seed_legacy_unrecorded_run_for_test(&store, "legacy-cu-replay", "session-1", "turn-1", true);
        drop(store);
        let run = ComputerUseRunStore::open(&session_db)
            .expect("store")
            .legacy_unconverged_runs()
            .expect("candidates")
            .into_iter()
            .find(|run| run.call_id == "legacy-cu-replay")
            .expect("候选存在");

        // 模拟"上次启动中断"：取得资格 → 登记操作 → **释放资格**，操作行留在旧 epoch 上且未提交。
        let previous = InputSafetyCoordinator::begin_with_coordination_scope(
            &safety_root,
            &isolated_coordination_scope(&safety_root),
            &scope(),
            "recovery-legacy-replay",
            &["converge_legacy_cu_run"],
            std::time::Duration::from_secs(2),
        )
        .expect("previous coordinator");
        let previous_epoch = previous.control().epoch();
        previous.relinquish().expect("release previous epoch");

        // 断言时须仍持有新资格；本例在检查 epoch 后才显式释放。
        let current_coordinator = InputSafetyCoordinator::begin_with_coordination_scope(
            &safety_root,
            &isolated_coordination_scope(&safety_root),
            &scope(),
            "recovery-legacy-replay",
            &["converge_legacy_cu_run"],
            std::time::Duration::from_secs(2),
        )
        .expect("current coordinator");
        let report = run_legacy_recovery(&LegacyRecoveryInputs {
            session_db_path: &session_db,
            input_safety_root: &safety_root,
            coordinator: Some(&current_coordinator),
            coordination_scope: "windows-session-recovery|physical-input-resource",
            resource_scope: &scope(),
            run: &run,
            recovery_operation_id: "recovery-legacy-replay",
            recovery_service_instance: "recovery-service-test",
            source_database_identity: "session-db:default",
            observed_commit_candidate: Some(false),
            independent_safety_check: None,
        })
        .expect("重放必须成功（不得报未持有当前恢复资格）");
        assert!(
            matches!(report.disposition, LegacyRecoveryDisposition::Converged { .. }),
            "重放应当完成收敛：{:?}",
            report.disposition
        );

        let safety = crate::input_safety_store::InputSafetyStore::open_at(&safety_root).expect("safety");
        let operation = safety
            .recovery_operation("recovery-legacy-replay")
            .expect("read")
            .expect("exists");
        let current = safety
            .current_recovery_epoch(&scope())
            .expect("read epoch")
            .expect("本次运行必须持有资格");
        assert!(
            current.epoch > previous_epoch,
            "重启必须取得**新** epoch（旧 {} → 新 {}）",
            previous_epoch,
            current.epoch
        );
        assert_eq!(
            operation.recovery_epoch, current.epoch,
            "重放必须把旧操作重新绑定到当前 epoch"
        );
        current_coordinator.relinquish().expect("release current epoch");
    }

    /// **Recovery-P0-T1（验收）**：遗留运行 owner 永久未知 ⇒
    /// ① 资源仍被阻断；② 处置是 `HumanReviewRequired`（**不是**一直 pending）；
    /// ③ 待对账归零（"永久隔离"的账挂不上去）。
    #[test]
    fn p0_t1_unknown_owner_settles_as_human_review_and_stays_blocked() {
        let (_directory, session_db, safety_root) = setup();
        // **刻意不建任何 owner 关系** ⇒ `resolve_legacy_run_owner` 必为 Unknown。
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        seed_legacy_unrecorded_run_for_test(&store, "legacy-cu-unknown-owner", "session-x", "turn-x", true);
        drop(store);
        let run = ComputerUseRunStore::open(&session_db)
            .expect("store")
            .legacy_unconverged_runs()
            .expect("candidates")
            .into_iter()
            .find(|run| run.call_id == "legacy-cu-unknown-owner")
            .expect("候选存在");

        let report = run_with_isolated_coordinator(&LegacyRecoveryInputs {
            session_db_path: &session_db,
            input_safety_root: &safety_root,
            coordinator: None,
            coordination_scope: "windows-session-recovery|physical-input-resource",
            resource_scope: &scope(),
            run: &run,
            recovery_operation_id: "recovery-p0-t1",
            recovery_service_instance: "recovery-service-test",
            source_database_identity: "session-db:default",
            observed_commit_candidate: Some(false),
            independent_safety_check: None,
        })
        .expect("驱动必须给出结账结果，而不是失败");
        assert!(
            matches!(report.disposition, LegacyRecoveryDisposition::RefusedBeforeWrite { .. }),
            "owner 未知必须拒绝写终态：{:?}",
            report.disposition
        );
        assert!(!report.new_input_reopened, "资源必须保持隔离");

        let safety = crate::input_safety_store::InputSafetyStore::open_at(&safety_root).expect("safety");
        let operation = safety
            .recovery_operation("recovery-p0-t1")
            .expect("read")
            .expect("exists");
        assert_eq!(
            operation.disposition,
            RecoveryDisposition::HumanReviewRequired,
            "owner 永久未知必须落到'待人工复核'"
        );
        assert!(operation.committed, "必须结账——不得永久挂在 pending");
        assert!(
            safety
                .unsettled_recovery_operations(&scope())
                .expect("unsettled")
                .is_empty(),
            "结账后不得再算待对账"
        );
        assert!(
            safety.has_open_resource_block(&scope()).expect("blocked"),
            "阻断必须仍然存在：结账是'怎么结的'，不是'已放行'"
        );
        assert!(
            safety
                .human_review_required_operations()
                .expect("human")
                .iter()
                .any(|operation| operation.recovery_operation_id == "recovery-p0-t1"),
            "待办必须能在'待人工复核'里查到"
        );
    }

    /// **PR-01**：已结账的操作**不重放**——第二遍如实报 `AlreadySettled`，且不重复建阻断。
    #[test]
    fn settled_operation_is_reported_instead_of_being_replayed() {
        let (_directory, session_db, safety_root) = setup();
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        seed_legacy_unrecorded_run_for_test(&store, "legacy-cu-settled", "session-x", "turn-x", true);
        drop(store);
        let run = ComputerUseRunStore::open(&session_db)
            .expect("store")
            .legacy_unconverged_runs()
            .expect("candidates")
            .into_iter()
            .find(|run| run.call_id == "legacy-cu-settled")
            .expect("候选存在");
        fn inputs_for<'a>(
            session_db: &'a Path,
            safety_root: &'a Path,
            scope: &'a InputSafetyResourceScope,
            run: &'a crate::computer_use_store::LegacyUnconvergedRun,
        ) -> LegacyRecoveryInputs<'a> {
            LegacyRecoveryInputs {
                session_db_path: session_db,
                input_safety_root: safety_root,
                coordinator: None,
                coordination_scope: "windows-session-recovery|physical-input-resource",
                resource_scope: scope,
                run,
                recovery_operation_id: "recovery-settled-replay",
                recovery_service_instance: "recovery-service-test",
                source_database_identity: "session-db:default",
                observed_commit_candidate: Some(false),
                independent_safety_check: None,
            }
        }
        let first = run_with_isolated_coordinator(&inputs_for(&session_db, &safety_root, &scope(), &run)).expect("首次");
        assert!(matches!(first.disposition, LegacyRecoveryDisposition::RefusedBeforeWrite { .. }));

        let safety = crate::input_safety_store::InputSafetyStore::open_at(&safety_root).expect("safety");
        let blocks_after_first = safety.open_block_count(&scope()).expect("blocks");
        drop(safety);

        let second = run_with_isolated_coordinator(&inputs_for(&session_db, &safety_root, &scope(), &run)).expect("第二遍");
        match second.disposition {
            LegacyRecoveryDisposition::AlreadySettled { disposition } => {
                assert_eq!(disposition, RecoveryDisposition::HumanReviewRequired);
            }
            other => panic!("已结账的操作必须如实报 AlreadySettled：{other:?}"),
        }
        let safety = crate::input_safety_store::InputSafetyStore::open_at(&safety_root).expect("safety");
        assert_eq!(
            safety.open_block_count(&scope()).expect("blocks"),
            blocks_after_first,
            "重放不得重复建立阻断"
        );
    }

    /// R1–R9 端到端：**资源不确定 ⇒ 建事故 + 保持隔离**，但控制终态与收敛事实**必须落库**。
    #[test]
    fn driver_converges_legacy_run_and_keeps_resource_isolated() {
        let (_directory, session_db, safety_root) = setup();
        // owner 关系真实存在（chat_turn 运行行）。
        crate::create_chat_runtime_run_sqlite(
            &session_db,
            "run-chat-legacy",
            "claim",
            "ws-0123456789abcdef",
            Some("session-1"),
            "room-1",
            "turn-1",
        )
        .expect("seed runtime run");
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        seed_legacy_unrecorded_run_for_test(&store, "legacy-cu-1", "session-1", "turn-1", true);
        drop(store);

        let run = ComputerUseRunStore::open(&session_db)
            .expect("store")
            .legacy_unconverged_runs()
            .expect("candidates")
            .into_iter()
            .find(|run| run.call_id == "legacy-cu-1")
            .expect("候选存在");

        let report = run_with_isolated_coordinator(&LegacyRecoveryInputs {
            session_db_path: &session_db,
            input_safety_root: &safety_root,
            coordinator: None,
            coordination_scope: "windows-session-recovery|physical-input-resource",
            resource_scope: &scope(),
            run: &run,
            recovery_operation_id: "recovery-legacy-1",
            recovery_service_instance: "recovery-service-test",
            source_database_identity: "session-db:default",
            observed_commit_candidate: Some(false),
            independent_safety_check: None,
        })
        .expect("driver");

        assert!(
            matches!(report.disposition, LegacyRecoveryDisposition::Converged { .. }),
            "应当收敛：{:?}",
            report.disposition
        );
        assert!(
            !report.new_input_reopened,
            "资源不确定（缺独立证明）⇒ **不得**开放新输入"
        );
        assert!(report.assessment.describe().contains("input blocked"));
        assert_eq!(report.stage, RecoveryStage::StageCommitted);

        // 控制终态与收敛事实确实落库；资源仍隔离；阻断仍开着。
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        assert!(
            store.latest_legacy_run_convergence("legacy-cu-1").expect("read").is_some(),
            "收敛事实必须落库"
        );
        assert!(
            store.legacy_unconverged_runs().expect("scan").iter().all(|run| run.call_id != "legacy-cu-1"),
            "已收敛的运行不得再出现在未收敛候选里（控制维度已终止）"
        );
        let safety = crate::input_safety_store::InputSafetyStore::open_at(&safety_root).expect("safety");
        assert!(
            safety.has_open_resource_block(&scope()).expect("block"),
            "资源不确定时阻断必须仍然开着"
        );
        assert!(
            !safety.resource_state(&scope()).expect("state").accepts_new_input,
            "不得接受新输入"
        );
    }

    /// owner 关系**未知** ⇒ 在任何写入之前拒绝，且资源保持隔离（未知不是"没有 owner"）。
    #[test]
    fn driver_refuses_when_owner_relation_is_unknown() {
        let (_directory, session_db, safety_root) = setup();
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        seed_legacy_unrecorded_run_for_test(&store, "legacy-cu-1", "session-1", "turn-1", true);
        drop(store);
        let run = ComputerUseRunStore::open(&session_db)
            .expect("store")
            .legacy_unconverged_runs()
            .expect("candidates")
            .into_iter()
            .next()
            .expect("候选存在");

        let report = run_with_isolated_coordinator(&LegacyRecoveryInputs {
            session_db_path: &session_db,
            input_safety_root: &safety_root,
            coordinator: None,
            coordination_scope: "windows-session-recovery-owner|physical-input-resource",
            resource_scope: &scope(),
            run: &run,
            recovery_operation_id: "recovery-legacy-2",
            recovery_service_instance: "recovery-service-test",
            source_database_identity: "session-db:default",
            observed_commit_candidate: Some(false),
            independent_safety_check: None,
        })
        .expect("driver");

        match &report.disposition {
            LegacyRecoveryDisposition::RefusedBeforeWrite { code, .. } => {
                assert_eq!(code, "legacy_run_owner_no_mapping");
            }
            other => panic!("必须因 owner 未知而拒写：{other:?}"),
        }
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        assert!(
            store.latest_legacy_run_convergence("legacy-cu-1").expect("read").is_none(),
            "拒绝时必须**一个字节都不写**"
        );
    }

    /// 缺"提交候选检查" ⇒ 拒绝收敛（不得用缺省值凑成 false）。
    #[test]
    fn driver_refuses_without_a_real_commit_candidate_check() {
        let (_directory, session_db, safety_root) = setup();
        crate::create_chat_runtime_run_sqlite(
            &session_db,
            "run-chat-legacy",
            "claim",
            "ws-0123456789abcdef",
            Some("session-1"),
            "room-1",
            "turn-1",
        )
        .expect("seed runtime run");
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        seed_legacy_unrecorded_run_for_test(&store, "legacy-cu-1", "session-1", "turn-1", true);
        drop(store);
        let run = ComputerUseRunStore::open(&session_db)
            .expect("store")
            .legacy_unconverged_runs()
            .expect("candidates")
            .into_iter()
            .next()
            .expect("候选存在");

        let report = run_with_isolated_coordinator(&LegacyRecoveryInputs {
            session_db_path: &session_db,
            input_safety_root: &safety_root,
            coordinator: None,
            coordination_scope: "windows-session-recovery-check|physical-input-resource",
            resource_scope: &scope(),
            run: &run,
            recovery_operation_id: "recovery-legacy-3",
            recovery_service_instance: "recovery-service-test",
            source_database_identity: "session-db:default",
            observed_commit_candidate: None,
            independent_safety_check: None,
        })
        .expect("driver");
        match &report.disposition {
            LegacyRecoveryDisposition::RefusedBeforeWrite { code, .. } => {
                assert_eq!(code, "commit_candidate_check_missing");
            }
            other => panic!("必须因缺检查而拒写：{other:?}"),
        }
        let store = ComputerUseRunStore::open(&session_db).expect("store");
        assert!(store.latest_legacy_run_convergence("legacy-cu-1").expect("read").is_none());
    }
}
