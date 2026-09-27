//! 开放路径与恢复驱动触发点（第八轮 §2.5 的"独立证明才开放"）。
//!
//! 为什么需要它：入口接线（§B-70）之后，CU 的正式输入入口**只在共享库说"该资源接受新输入"
//! 时才放行**。但"谁把它开放"不能由输入入口自己宣布——那正是裁决禁止的自证。因此开放必须由
//! **一次独立评估 + 协调资格**完成：
//!
//! ```text
//! 启动独立评估（无未决阻断 / 无待收敛遗留运行 / 无待对账恢复操作）
//!     ↓ 全部满足
//! 取得协调资格（跨进程锁 + epoch）
//!     ↓
//! 按资格开放新输入（reopen_new_input_authorized）
//! ```
//!
//! 任一项不满足 ⇒ **保持隔离**并把原因如实报出（不是失败，是保守结果）；库根未注入 ⇒
//! 报告"未注入"并跳过（此时正式输入本来就 fail-closed，**不得**回退到自造路径）。

use std::path::Path;

use crate::computer_use_store::ComputerUseRunStore;
use runtime::InputSafetyResourceScope;

use crate::input_safety_store::{
    input_safety_state_root, recovery_coordination_scope, CoordinatorError, InputSafetyCoordinator,
    InputSafetyStore, InputSafetyStoreError,
};
use crate::legacy_recovery_driver::{
    run_legacy_recovery, LegacyRecoveryDisposition, LegacyRecoveryInputs,
};

/// 启动路径协调器**自身**的登记操作 ID。
///
/// 它必须和开放结果一起结账：否则每次启动都会留下一条"尚未提交"的操作，下一次启动就会
/// 因此拒绝开放——与"被拒绝的恢复永不结账"同源的挂账缺陷（PR-01／P0-1）。
const STARTUP_INPUT_SAFETY_OPERATION_ID: &str = "startup-input-safety";

/// 开放前的独立评估结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpeningAssessment {
    pub resource_scope: String,
    /// **未获人工放行**的开启阻断数（只有它们构成拒绝理由）。
    pub open_blocks: usize,
    /// 该资源上尚未提交（`Pending`）的恢复操作数。
    pub pending_recovery_operations: usize,
    /// 尚未收敛、且**未被放行决定接受**的遗留 CU 运行数（它们必须先被恢复）。
    pub legacy_unconverged_runs: usize,
    /// 已被人工放行决定**接受**的遗留运行数（operator 明确承担其风险；仍如实计数、不再挡路）。
    pub acknowledged_runs: usize,
    /// 处置为"需要人工复核"的恢复操作数（这是"等人"，不是"等机器"）。
    pub human_review_required: usize,
    /// 不满足"可开放"的原因（`None` = 评估通过）。
    pub refusal: Option<String>,
}

impl OpeningAssessment {
    #[must_use]
    pub(crate) fn is_safe(&self) -> bool {
        self.refusal.is_none()
    }

    #[must_use]
    pub(crate) fn describe(&self) -> String {
        match &self.refusal {
            None => format!(
                "评估通过（未决阻断 0 / 待对账恢复 0 / 待收敛遗留运行 0，scope={}）",
                self.resource_scope
            ),
            Some(reason) => format!(
                "保持隔离：{reason}（未获放行阻断 {} / 待对账恢复 {} / 待收敛遗留运行 {} / \
                 已获人工放行 {} / 待人工复核 {}）",
                self.open_blocks,
                self.pending_recovery_operations,
                self.legacy_unconverged_runs,
                self.acknowledged_runs,
                self.human_review_required
            ),
        }
    }
}

/// 开放路径的最终去向。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpeningOutcome {
    /// 已按资格开放（`epoch` 为开放时的协调 epoch）。
    Opened { epoch: u64 },
    /// 评估不通过 ⇒ 保持隔离（附原因）。
    KeptIsolated { assessment: OpeningAssessment },
    /// 库根未注入：跳过（正式输入本来就 fail-closed，**不得**自造路径）。
    SkippedRootNotInjected,
    /// 库不可用 ⇒ 保持隔离（附原因）。
    StoreUnavailable { code: &'static str, message: String },
}

/// **独立评估**：判断该资源现在是否可以开放新输入。
///
/// 三项都必须成立：没有未关闭阻断、没有该资源上的待对账恢复操作、没有待收敛的遗留运行。
pub(crate) fn assess_input_resource(
    safety_root: &Path,
    session_db_path: &Path,
    resource_scope: &InputSafetyResourceScope,
) -> Result<OpeningAssessment, InputSafetyStoreError> {
    let store = InputSafetyStore::open_at(safety_root)?;
    // 阻断分两类：**未获人工放行**的（真的挡路）与被最新放行决定覆盖的（operator 已承担）。
    // 只有前者构成拒绝理由——否则"人工放行"就成了一句空话（PR-01／P0-1）。
    let open_blocks = store.unacknowledged_open_block_ids(resource_scope)?.len();
    #[cfg(windows)]
    let has_unsettled_native_execution = {
        let snapshot=store.native_recovery_store().snapshot(resource_scope).map_err(InputSafetyStoreError::Sqlite)?;
        snapshot.has_unsettled_execution()
    };
    #[cfg(not(windows))]
    let has_unsettled_native_execution=false;
    let mut pending_recovery_operations = 0usize;
    for outcome in store.reconcile_recovery_operations_on_startup()? {
        // **只把 `NeedsReacquire` 算作"待对账"**：`StillAuthorized` 表示该操作正由**活着的**
        // 协调者处理中——在启动路径里那就是本次调用自己。把它计入"待对账"，会让启动路径的
        // 开放分支**永不可达**（现场实测：`pending=3` 的三条里就有一条是本次协调器的登记）。
        let crate::input_safety_store::RecoveryReconcileOutcome::NeedsReacquire {
            recovery_operation_id,
            ..
        } = &outcome
        else {
            continue;
        };
        if let Some(operation) = store.recovery_operation(recovery_operation_id)? {
            if operation.scope == *resource_scope && !operation.committed {
                pending_recovery_operations += 1;
            }
        }
    }
    // 放行决定"接受"的遗留运行不再计入挡路项：operator 已在署名/理由/证据下落定了判断，
    // 系统**不得**用"还有待收敛遗留运行"把它再次挡回去（那就等于没有放行通道）。
    let acknowledged_runs = store.acknowledged_run_ids(resource_scope)?;
    let legacy_unconverged_runs = if session_db_path.exists() {
        ComputerUseRunStore::open(session_db_path)
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?
            .legacy_unconverged_runs()
            .map_err(|error| InputSafetyStoreError::Sqlite(error.to_string()))?
            .into_iter()
            .filter(|run| !acknowledged_runs.contains(&run.call_id))
            .count()
    } else {
        0
    };
    let acknowledged_runs = acknowledged_runs.len();
    let human_review_required = store.human_review_required_operations()?.len();
    let refusal = if has_unsettled_native_execution {
        Some("仍有未确认退出的原生执行者或在途输入许可".to_string())
    } else if open_blocks > 0 {
        Some("该资源仍有未获放行的阻断事实".to_string())
    } else if pending_recovery_operations > 0 {
        Some("该资源上仍有尚未提交的恢复操作".to_string())
    } else if legacy_unconverged_runs > 0 {
        Some("仍有待收敛的遗留 CU 运行（必须先完成恢复或由人工放行）".to_string())
    } else {
        None
    };
    Ok(OpeningAssessment {
        resource_scope: resource_scope.as_str().to_string(),
        open_blocks,
        pending_recovery_operations,
        legacy_unconverged_runs,
        acknowledged_runs,
        human_review_required,
        refusal,
    })
}

/// 评估 + （通过时）按资格开放。
pub(crate) fn assess_and_open_input_resource(
    safety_root: &Path,
    session_db_path: &Path,
    resource_scope: &InputSafetyResourceScope,
    coordination_scope: &str,
) -> Result<OpeningOutcome, CoordinatorError> {
    let assessment = assess_input_resource(safety_root, session_db_path, resource_scope)
        .map_err(CoordinatorError::Store)?;
    if !assessment.is_safe() {
        return Ok(OpeningOutcome::KeptIsolated { assessment });
    }
    let coordinator = InputSafetyCoordinator::begin_with_coordination_scope(
        safety_root,
        coordination_scope,
        resource_scope,
        "startup-open-input",
        &["open_new_input"],
        std::time::Duration::from_secs(2),
    )?;
    #[cfg(windows)]
    coordinator.store().native_recovery_store().quarantine_unsettled_on_startup(resource_scope)
        .map_err(|error|CoordinatorError::Store(InputSafetyStoreError::Sqlite(error)))?;
    let state = coordinator
        .store()
        .reopen_new_input_authorized(resource_scope, coordinator.control())
        .map_err(CoordinatorError::Store)?;
    debug_assert!(state.accepts_new_input);
    // 与启动路径同口径：本入口自身的登记操作也要结账，不得留下永久"待对账"。
    coordinator
        .store()
        .settle_recovery_operation_authorized(
            "startup-open-input",
            coordinator.control(),
            runtime::RecoveryDisposition::Recovered,
            "开放路径自身的操作结账（已按资格开放）",
        )
        .map_err(CoordinatorError::Store)?;
    let epoch = coordinator.control().epoch();
    // 开放后**主动释放**排他资格：正常输入不依赖恢复锁常驻（资格失效后不得再改安全状态，
    // 而"已经开放"这一事实已持久化在资源状态里）。
    coordinator
        .store()
        .release_recovery_epoch(&crate::input_safety_store::OwnedRecoveryEpoch {
            scope: resource_scope.clone(),
            epoch,
            coordinator_instance_id: coordinator.control().coordinator_id().to_string(),
        })
        .map_err(CoordinatorError::Store)?;
    Ok(OpeningOutcome::Opened { epoch })
}

/// 启动触发点：驱动遗留运行恢复，然后尝试开放该资源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupInputSafetyReport {
    pub candidates: usize,
    pub converged: usize,
    pub refused: usize,
    /// 本次**未重放**（该操作已结账）：它们不做事、也不算拒绝（PR-01／P0-1）。
    pub settled: usize,
    pub opening: OpeningOutcome,
}

/// 在启动序列里调用（**排在既有的 orphan 恢复之后**）。
///
/// 库根未注入时返回 `SkippedRootNotInjected`：不报错、不自造路径，正式输入保持 fail-closed。
pub(crate) fn run_startup_input_safety(
    session_db_path: &Path,
    recovery_service_instance: &str,
    source_database_identity: &str,
) -> Result<StartupInputSafetyReport, CoordinatorError> {
    let Some(safety_root) = input_safety_state_root() else {
        return Ok(StartupInputSafetyReport {
            candidates: 0,
            converged: 0,
            refused: 0,
            settled: 0,
            opening: OpeningOutcome::SkippedRootNotInjected,
        });
    };
    let resource_scope = crate::input_safety_store::physical_input_resource_scope()
        .map_err(|refusal| CoordinatorError::Store(InputSafetyStoreError::Sqlite(refusal.reason())))?;
    let coordination_scope = recovery_coordination_scope().map_err(CoordinatorError::Store)?;

    // 先驱动遗留运行恢复（每个候选一条；库不存在时为零）。
    let candidates = if session_db_path.exists() {
        ComputerUseRunStore::open(session_db_path)
            .map_err(|error| CoordinatorError::Store(InputSafetyStoreError::Sqlite(error.to_string())))?
            .legacy_unconverged_runs()
            .map_err(|error| CoordinatorError::Store(InputSafetyStoreError::Sqlite(error.to_string())))?
    } else {
        Vec::new()
    };
    // **同一资源范围只允许一个协调者**：整个启动路径共用一个协调器与一个协调范围，
    // 不再给每个遗留运行/开放步骤各起一个（否则 epoch 会互相顶掉，现场实测踩到过）。
    let coordinator = InputSafetyCoordinator::begin_with_coordination_scope(
        &safety_root,
        &coordination_scope,
        &resource_scope,
        STARTUP_INPUT_SAFETY_OPERATION_ID,
        &["converge_legacy_cu_run", "open_new_input"],
        std::time::Duration::from_secs(2),
    )?;
    let mut converged = 0usize;
    #[cfg(windows)]
    coordinator.store().native_recovery_store().quarantine_unsettled_on_startup(&resource_scope)
        .map_err(|error|CoordinatorError::Store(InputSafetyStoreError::Sqlite(error)))?;
    let mut refused = 0usize;
    let mut settled = 0usize;
    for run in &candidates {
        let report = run_legacy_recovery(&LegacyRecoveryInputs {
            session_db_path,
            input_safety_root: &safety_root,
            coordinator: Some(&coordinator),
            coordination_scope: &coordination_scope,
            resource_scope: &resource_scope,
            run,
            recovery_operation_id: &format!("startup-recovery-{}", run.call_id),
            recovery_service_instance,
            source_database_identity,
            // 提交候选检查走**事实日志的真实读取**：已有该运行的终态控制事实 ⇒ 判为有候选，
            // 收敛会被拒绝（不覆盖真实终态）；没有 ⇒ `false`，允许写已裁定终态。
            observed_commit_candidate: Some(crate::legacy_recovery::has_terminal_commit_candidate(
                session_db_path,
                &run.call_id,
            )),
            independent_safety_check: None,
        })?;
        match report.disposition {
            LegacyRecoveryDisposition::Converged { .. }
            | LegacyRecoveryDisposition::AlreadyConverged { .. } => converged += 1,
            LegacyRecoveryDisposition::AlreadySettled { .. } => settled += 1,
            _ => refused += 1,
        }
    }

    // 再评估并（可能）开放。
    // 评估与开放同样**复用**上面的协调器（不再另起一个、也不给范围加后缀）。
    let opening = match assess_input_resource(&safety_root, session_db_path, &resource_scope) {
        Ok(assessment) if assessment.is_safe() => {
            match coordinator
                .store()
                .reopen_new_input_authorized(&resource_scope, coordinator.control())
            {
                Ok(state) => {
                    debug_assert!(state.accepts_new_input);
                    OpeningOutcome::Opened {
                        epoch: coordinator.control().epoch(),
                    }
                }
                Err(error) => OpeningOutcome::StoreUnavailable {
                    code: error.code(),
                    message: error.to_string(),
                },
            }
        }
        Ok(assessment) => OpeningOutcome::KeptIsolated { assessment },
        Err(error) => OpeningOutcome::StoreUnavailable {
            code: error.code(),
            message: error.to_string(),
        },
    };
    // **本次协调器自身的登记也要结账**（PR-01／P0-1）：否则它会永远留在"待对账"里，
    // 让下一次启动看到"仍有尚未提交的恢复操作"而永久拒绝开放。
    //
    // 处置口径与 `opening` 一致：开放了就是 `Recovered`；仍是隔离就是 `KeptIsolated`
    // （机器已判定，且底层遗留运行的"待人工复核"另有各自的操作承担）。
    // 根未注入／库不可用时**保持 pending**：那时连结账都写不进去，如实挂着才是诚实的。
    let own_disposition = match &opening {
        OpeningOutcome::Opened { .. } => Some(runtime::RecoveryDisposition::Recovered),
        OpeningOutcome::KeptIsolated { .. } => Some(runtime::RecoveryDisposition::KeptIsolated),
        OpeningOutcome::SkippedRootNotInjected | OpeningOutcome::StoreUnavailable { .. } => None,
    };
    if let Some(disposition) = own_disposition {
        if let Err(error) = coordinator.store().settle_recovery_operation_authorized(
            STARTUP_INPUT_SAFETY_OPERATION_ID,
            coordinator.control(),
            disposition,
            "启动路径自身的评估/开放操作结账",
        ) {
            eprintln!("[input-safety] 启动路径自身操作结账失败（不影响已表达的 opening 结果）：{error}");
        }
    }

    // 收尾：**一次性**释放协调资格。释放失败只如实记进日志口径（`opening` 已表达真实结果），
    // 不再把整个启动路径判为失败——避免"资源其实已开放、却报告保持阻断"的误导性状态。
    if let Err(error) = coordinator.store().release_recovery_epoch(
        &crate::input_safety_store::OwnedRecoveryEpoch {
            scope: resource_scope.clone(),
            epoch: coordinator.control().epoch(),
            coordinator_instance_id: coordinator.control().coordinator_id().to_string(),
        },
    ) {
        eprintln!("[input-safety] 启动路径收尾释放资格失败（不影响已表达的 opening 结果）：{error}");
    }
    Ok(StartupInputSafetyReport {
        candidates: candidates.len(),
        converged,
        refused,
        settled,
        opening,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::computer_use_store::seed_legacy_unrecorded_run_for_test;

    /// 作用域内改写**库根**进程环境变量（RPR-01b）。
    ///
    /// 统一走 `crate::test_env`：它既**恢复**原值（Drop 不受提前返回/panic 影响），
    /// 又**持有全 crate 共用的进程环境锁**——因为进程环境是全局的，而测试是多线程并行跑的，
    /// 只恢复不加锁时，同进程的并发用例仍可能读到别人中途设定的库根。
    ///
    /// 本文件原先自带一份只恢复、不加锁的 `ScopedEnvVar`；它已由共享守卫取代。
    fn root_env() -> &'static str {
        crate::input_safety_store::INPUT_SAFETY_STATE_ROOT_ENV
    }

    fn set_root(value: impl AsRef<std::ffi::OsStr>) -> crate::test_env::ScopedEnvVar {
        crate::test_env::set(root_env(), Some(value))
    }

    fn remove_root() -> crate::test_env::ScopedEnvVar {
        crate::test_env::remove(root_env())
    }

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
        InputSafetyResourceScope::parse("windows-session-opening").expect("scope")
    }

    /// 干净资源 ⇒ 评估通过并**按资格开放**（开放后输入入口才可能放行）。
    #[test]
    fn clean_resource_is_opened_by_the_evaluation_path() {
        let (_directory, session_db, safety_root) = setup();
        let outcome = assess_and_open_input_resource(
            &safety_root,
            &session_db,
            &scope(),
            "windows-session-opening|test-open",
        )
        .expect("opening");
        assert!(matches!(outcome, OpeningOutcome::Opened { .. }), "{outcome:?}");
        let store = InputSafetyStore::open_at(&safety_root).expect("store");
        assert!(
            store.resource_state(&scope()).expect("state").accepts_new_input,
            "评估通过后资源必须接受新输入"
        );
    }

    /// **活着**协调者的在办操作不算"待对账"：否则启动路径的开放分支**永不可达**。
    ///
    /// 现场背景：启动路径先取协调资格（登记一条未提交操作）再评估，而评估曾把
    /// `StillAuthorized`（正在被活着的持有者处理）也计入 `pending_recovery_operations`，
    /// 于是每次启动都得到"仍有尚未提交的恢复操作"而永远保持隔离。
    #[test]
    fn operations_held_by_a_live_coordinator_are_not_pending_reconciliation() {
        let (_directory, session_db, safety_root) = setup();
        let coordinator = InputSafetyCoordinator::begin_with_coordination_scope(
            &safety_root,
            "windows-session-opening|test-still-authorized",
            &scope(),
            "self-operation",
            &["open_new_input"],
            std::time::Duration::from_secs(2),
        )
        .expect("coordinator");
        // 协调器已登记一条未提交（r1）操作，且持有者**活着**。
        let assessment = assess_input_resource(&safety_root, &session_db, &scope()).expect("assess");
        assert_eq!(
            assessment.pending_recovery_operations, 0,
            "活着协调者的在办操作不是待对账：{assessment:?}"
        );
        assert!(assessment.is_safe(), "干净资源必须评估通过：{assessment:?}");
        // 开放分支确实可达（这正是启动路径的形态：同一协调器直接按资格开放）。
        let state = coordinator
            .store()
            .reopen_new_input_authorized(&scope(), coordinator.control())
            .expect("按资格开放");
        assert!(state.accepts_new_input, "评估通过后必须能开放新输入");
    }

    /// 有未关闭阻断 ⇒ **保持隔离**（不是失败，是保守结果），且不得开放。
    #[test]
    fn open_block_keeps_the_resource_isolated() {
        let (_directory, session_db, safety_root) = setup();
        let store = InputSafetyStore::open_at(&safety_root).expect("store");
        store
            .open_resource_block("block-1", &scope(), "legacy_recovery", "incident-1")
            .expect("open block");
        drop(store);

        let outcome = assess_and_open_input_resource(
            &safety_root,
            &session_db,
            &scope(),
            "windows-session-opening|test-blocked",
        )
        .expect("opening");
        match outcome {
            OpeningOutcome::KeptIsolated { assessment } => {
                assert_eq!(assessment.open_blocks, 1);
                assert!(assessment.refusal.as_deref().unwrap_or("").contains("阻断"));
            }
            other => panic!("必须保持隔离：{other:?}"),
        }
        let store = InputSafetyStore::open_at(&safety_root).expect("store");
        assert!(!store.resource_state(&scope()).expect("state").accepts_new_input);
    }

    /// 有待收敛的遗留运行 ⇒ 保持隔离（必须先完成恢复）。
    #[test]
    fn pending_legacy_runs_keep_the_resource_isolated() {
        let (_directory, session_db, safety_root) = setup();
        let cu_store = ComputerUseRunStore::open(&session_db).expect("cu store");
        seed_legacy_unrecorded_run_for_test(&cu_store, "legacy-cu-1", "session-1", "turn-1", true);
        drop(cu_store);

        let opening = assess_and_open_input_resource(
            &safety_root,
            &session_db,
            &scope(),
            "windows-session-opening|test-legacy",
        )
        .expect("opening");
        match opening {
            OpeningOutcome::KeptIsolated { assessment } => {
                assert_eq!(assessment.legacy_unconverged_runs, 1);
            }
            other => panic!("必须保持隔离：{other:?}"),
        }
    }

    /// **PR-01／P0-1**：没有人工放行 ⇒ 阻断与遗留运行都挡路；放行之后 ⇒ 评估通过并可开放。
    ///
    /// 这是"永久隔离"出口的**端到端**断言：不是"改个字段就算放行"，而是
    /// "放行决定（人事）+ 独立评估（机器）"两者都成立，资源才真的能被重新接受输入。
    #[test]
    fn release_decision_is_what_makes_the_resource_openable() {
        let (_directory, session_db, safety_root) = setup();
        let cu_store = ComputerUseRunStore::open(&session_db).expect("cu store");
        seed_legacy_unrecorded_run_for_test(&cu_store, "legacy-cu-1", "session-1", "turn-1", true);
        drop(cu_store);
        let store = InputSafetyStore::open_at(&safety_root).expect("store");
        store
            .open_resource_block("legacy-block-legacy-cu-1", &scope(), "legacy_recovery", "incident-1")
            .expect("block");
        drop(store);

        // ① 未放行：阻断挡路（遗留运行也挡路）。
        let before = assess_input_resource(&safety_root, &session_db, &scope()).expect("assess");
        assert!(!before.is_safe(), "有未获放行的阻断时必须保持隔离：{before:?}");
        assert_eq!(before.open_blocks, 1);
        assert_eq!(before.legacy_unconverged_runs, 1);

        // ② 人工放行：逐条声明阻断 + 明确接受该遗留运行（承担其风险）。
        let coordinator = InputSafetyCoordinator::begin_with_coordination_scope(
            &safety_root,
            "windows-session-opening|test-release",
            &scope(),
            "release-test",
            &["release_isolation", "open_new_input"],
            std::time::Duration::from_secs(2),
        )
        .expect("coordinator");
        coordinator
            .store()
            .release_isolation_authorized(
                &runtime::ReleaseIsolationDecision {
                    decision_id: "release-opening-1".to_string(),
                    scope: scope(),
                    operator: "ops-zhang".to_string(),
                    reason: "已人工核对旧执行者不在场，接受该遗留运行的风险".to_string(),
                    evidence_refs: vec!["manual-check-2026-09-25".to_string()],
                    acknowledged_block_ids: vec!["legacy-block-legacy-cu-1".to_string()],
                    acknowledged_run_ids: vec!["legacy-cu-1".to_string()],
                    release_epoch: 0,
                    coordinator_instance_id: String::new(),
                    decided_at_unix_ms: 0,
                },
                coordinator.control(),
            )
            .expect("放行");

        // ③ 放行之后：评估通过（阻断已解除、遗留运行已被接受），且**按资格**开放成功。
        let after = assess_input_resource(&safety_root, &session_db, &scope()).expect("assess");
        assert!(after.is_safe(), "放行之后评估必须通过：{after:?}");
        assert_eq!(after.open_blocks, 0);
        assert_eq!(after.acknowledged_runs, 1, "被接受的遗留运行要如实计数，不谎报为 0");
        let state = coordinator
            .store()
            .reopen_new_input_authorized(&scope(), coordinator.control())
            .expect("按资格开放");
        assert!(state.accepts_new_input, "放行 + 独立评估通过 ⇒ 资源重新接受输入");
    }

    /// 启动触发点：库根未注入 ⇒ 如实地"跳过"，**不自造路径**（正式输入继续 fail-closed）。
    #[test]
    fn startup_skips_without_an_injected_root() {
        // **串行守卫**：本用例临时**移除**进程级库根，并行时会让别的用例读到"未注入"（现场实测：与 CU 入口用例并行时，入口用例偶发
        // 报 `input_safety_resource_not_accepting_new_input`）。
        let _guard = crate::tests::config_test_guard();
        let (_directory, session_db, _safety_root) = setup();
        let _root = remove_root();
        let report = run_startup_input_safety(&session_db, "test-instance", "session-db:default")
            .expect("report");
        assert_eq!(report.opening, OpeningOutcome::SkippedRootNotInjected);
        assert_eq!(report.candidates, 0);
    }

    /// 启动触发点：有待收敛运行 ⇒ 驱动**收敛**它（事实日志无终态事实），
    /// 但资源**仍然保持隔离**——因为该运行缺独立安全检查，恢复期为它建立了真实阻断。
    #[test]
    fn startup_drives_candidates_and_keeps_the_resource_isolated() {
        // **串行守卫**：本用例把进程级库根指向自己的临时目录，并行时会让别的用例读写错库（现场实测：与 CU 入口用例并行时，入口用例偶发
        // 报 `input_safety_resource_not_accepting_new_input`）。
        let _guard = crate::tests::config_test_guard();
        let (_directory, session_db, safety_root) = setup();
        // 真实 owner 关系必须存在：否则驱动会以 `legacy_run_owner_no_mapping` 拒写（用例 5 的规则）。
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
        let cu_store = ComputerUseRunStore::open(&session_db).expect("cu store");
        seed_legacy_unrecorded_run_for_test(&cu_store, "legacy-cu-1", "session-1", "turn-1", true);
        drop(cu_store);
        let _root = set_root(safety_root.as_os_str());
        let report = run_startup_input_safety(&session_db, "test-instance", "session-db:default")
            .expect("report");
        assert_eq!(report.candidates, 1);
        assert_eq!(
            report.converged, 1,
            "事实日志里没有该运行的终态控制事实 ⇒ 允许写已裁定终态"
        );
        assert_eq!(report.refused, 0);
        match &report.opening {
            OpeningOutcome::KeptIsolated { assessment } => {
                assert!(
                    assessment.refusal.as_deref().unwrap_or("").contains("阻断"),
                    "收敛后仍应有未关闭阻断 ⇒ 保持隔离：{assessment:?}"
                );
            }
            other => panic!("必须保持隔离：{other:?}"),
        }
    }
}
