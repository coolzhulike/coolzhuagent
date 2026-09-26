//! **执行许可门**（8.2c，§B-124 裁决 §五／§八批准范围）。
//!
//! 把 §七 的执行顺序收成一个可测的单元，**供执行器在逐动作 fail-closed 点调用**：
//!
//! ```text
//! 1. 生成 ExecutionAttemptId（执行接纳层）
//! 2. 读当前 InputSafetyResourceState → gate_revision / recovery_epoch
//! 3. （authority 校验由调用方在既有准入处完成，本模块不重复）
//! 4. 创建 InputPermit，绑定 attempt + gate_revision + recovery_epoch
//! 5. 提交许可（同一短事务）
//! 6. **物理输入前复核**：current gate/epoch == permit 的 gate/epoch
//! 7. consume（单次状态转换）
//! ```
//!
//! ## 设计要点
//!
//! - **身份来自执行接纳层**，不由本模块或存储层编造（§七 明令"禁止先创建 permit 然后补 attempt"）。
//! - **只绑两个真实维度**：`gate_revision` 与 `recovery_epoch`。`policy_revision` 已按 §B-124 删除
//!   ——本库没有"安全政策版本"的权威来源。
//! - **复核不是重复**：第 2 步读到的值用于**建许可**，第 6 步在**物理输入之前**再读一次并与许可比对；
//!   两次读取之间若发生关闸或恢复换代，复核必然失败 ⇒ 拒绝输入。
//! - 全过程**不跨等待持有事务**：每步各自是短事务（由存储适配器保证）。

use std::path::{Path, PathBuf};

use runtime::{ExecutionAttemptId, InputPermit, InputPermitState, InputSafetyResourceScope};

use crate::input_permit_store::{
    PermitConsumptionBudget, PermitExecutorBinding, PermitStoreError,
};
use crate::input_safety_store::InputSafetyStore;

/// 许可门的拒绝：**可分辨**，且每条都对应"零物理输入"。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PermitGateRefusal {
    /// 宿主未注入输入安全库根：不得自行推导路径（与既有口径一致）。
    RootNotInjected,
    /// 资源 scope 不可解析。
    ScopeUnavailable { reason: String },
    /// 库层失败（含写锁竞争——**不当作成功**）。
    Store(String),
    /// 执行尝试身份本身非法（例如缺少观察代次或尝试序号为 0）。
    InvalidAttempt { field: &'static str, reason: &'static str },
    /// 物理输入前复核失败：gate 或 epoch 已变（或消费被存储拒绝）。
    StaleOrRefused(String),
}

impl PermitGateRefusal {
    #[must_use]
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::RootNotInjected => "input_safety_root_not_injected",
            Self::ScopeUnavailable { .. } => "input_safety_scope_unavailable",
            Self::Store(_) => "input_permit_store_failed",
            Self::InvalidAttempt { .. } => "invalid_execution_attempt",
            Self::StaleOrRefused(_) => "input_permit_stale_or_refused",
        }
    }

    #[must_use]
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::RootNotInjected => {
                "宿主未注入输入安全状态根：按裁决不得自行推导路径，故拒绝输入".to_string()
            }
            Self::ScopeUnavailable { reason } => {
                format!("无法确定输入资源 scope：{reason}")
            }
            Self::Store(detail) => format!("输入安全库操作失败：{detail}"),
            Self::InvalidAttempt { field, reason } => {
                format!("执行尝试身份字段 {field} 无效：{reason}")
            }
            Self::StaleOrRefused(detail) => {
                format!("许可复核或消费失败（拒绝输入）：{detail}")
            }
        }
    }
}

/// 已签发、待消费的执行许可（第 4–5 步的产物）。
///
/// 它**自述**自己绑定到的 gate/epoch，因此第 6 步的复核是对"许可说的"与"现在是什么"做比对，
/// 而不是拿刚读到的值自己和自己比。
#[derive(Debug, Clone)]
pub(crate) struct IssuedExecutionPermit {
    pub permit_id: String,
    pub attempt_key: String,
    pub gate_revision: u64,
    pub recovery_epoch: u64,
    /// **已核实的执行者实例 id**（`None` = 尚未就绪）。
    ///
    /// 按 2026-09-26 裁决"先补 8.3c 生产者、再接 consume"：在真实执行者身份可生产之前，
    /// 本字段保持 `None`，且 `recheck_and_consume` 会据此**明确拒绝**——
    /// 绝不用尝试身份或任何相似字符串顶替（与 B-121／B-124 同源的原则）。
    pub executor_instance_id: Option<String>,
    root: PathBuf,
    scope: InputSafetyResourceScope,
}

impl IssuedExecutionPermit {
    /// **物理输入前的复核 + 消费**（第 6–7 步）。成功返回许可的终态。
    ///
    /// 复核失败 ⇒ 拒绝（调用方必须放弃这次输入）；消费失败同样拒绝。
    pub(crate) fn recheck_and_consume(&self) -> Result<InputPermitState, PermitGateRefusal> {
        let store = InputSafetyStore::open_at(&self.root)
            .map_err(|error| PermitGateRefusal::Store(error.to_string()))?;
        let current = store
            .resource_state(&self.scope)
            .map_err(|error| PermitGateRefusal::Store(error.to_string()))?;
        // 这两条是"复核"的实质：把**许可自述的**绑定与**此刻的**真实状态比对。
        if current.revision != self.gate_revision {
            return Err(PermitGateRefusal::StaleOrRefused(format!(
                "gate 已变化（许可 {} → 当前 {}）",
                self.gate_revision, current.revision
            )));
        }
        if current.recovery_epoch != self.recovery_epoch {
            return Err(PermitGateRefusal::StaleOrRefused(format!(
                "恢复 epoch 已换代（许可 {} → 当前 {}）",
                self.recovery_epoch, current.recovery_epoch
            )));
        }
        // 执行者身份：**尚未就绪就必须拒绝**（不猜、不自绑）。
        let Some(executor_instance_id) = self.executor_instance_id.as_deref() else {
            return Err(PermitGateRefusal::StaleOrRefused(
                "执行者实例身份尚未就绪：按裁决先补 8.3c 生产者，再开放许可消费".to_string(),
            ));
        };
        let budget = PermitConsumptionBudget {
            gate_revision: current.revision,
            held_epoch: current.recovery_epoch,
        };
        store
            .permit_store()
            .consume_permit(
                &self.permit_id,
                budget,
                PermitExecutorBinding::Verified(executor_instance_id),
                0,
            )
            .map_err(|error| match error {
                PermitStoreError::StaleGateRevision { .. }
                | PermitStoreError::StaleRecoveryEpoch { .. }
                | PermitStoreError::PermitAlreadyConsumed { .. }
                | PermitStoreError::PermitExecutorMismatch { .. }
                | PermitStoreError::PermitStale { .. }
                | PermitStoreError::IllegalTransition(_) => {
                    PermitGateRefusal::StaleOrRefused(error.to_string())
                }
                other => PermitGateRefusal::Store(other.to_string()),
            })
    }
}

/// 许可门：**签发**一侧（第 1–5 步）。
pub(crate) struct PermitGate;

impl PermitGate {
    /// 为一次执行尝试签发许可。
    ///
    /// `observation_generation`／`step_identity`／`attempt_sequence` **必须来自执行接纳层**
    /// （本函数不接受"从动作内容反推"的调用方式；身份不由内容决定，这是 §B-121 的结论）。
    pub(crate) fn issue(
        root: Option<&Path>,
        scope: &InputSafetyResourceScope,
        action_id: &str,
        frozen_action_digest: &str,
        observation_generation: u64,
        step_identity: &str,
        attempt_sequence: u64,
        now_unix_ms: u64,
    ) -> Result<IssuedExecutionPermit, PermitGateRefusal> {
        let root = root.ok_or(PermitGateRefusal::RootNotInjected)?;
        let attempt = ExecutionAttemptId::new(
            action_id,
            observation_generation,
            step_identity,
            attempt_sequence,
        )
        .map_err(|error| PermitGateRefusal::InvalidAttempt {
            field: error.field,
            reason: error.reason,
        })?;
        let store = InputSafetyStore::open_at(root)
            .map_err(|error| PermitGateRefusal::Store(error.to_string()))?;
        // 第 2 步：读**当前**资源状态，取两个真实维度。
        let state = store
            .resource_state(scope)
            .map_err(|error| PermitGateRefusal::Store(error.to_string()))?;
        let permit = InputPermit {
            permit_id: format!("permit-{}", attempt.stable_key()),
            action_id: action_id.to_string(),
            execution_attempt_id: attempt.clone(),
            scope: scope.clone(),
            execution_context_ref: step_identity.to_string(),
            frozen_action_digest: frozen_action_digest.to_string(),
            gate_revision: state.revision,
            issued_owner_id: "executor".to_string(),
            issued_epoch: state.recovery_epoch,
            // 期限由调用方在接线时按阶段预算给；单元层不假装知道它。
            expires_at_unix_ms: now_unix_ms.saturating_add(30_000),
            executor_instance_id: None,
            state: InputPermitState::PendingActivation,
            revision: 1,
            revocation_reason: None,
        };
        let permits = store.permit_store();
        match permits.register_pending(&permit, InputPermitState::PendingActivation, now_unix_ms) {
            Ok(()) => {}
            // 同一次尝试已登记 ⇒ 幂等复用（不新建、不重新执行）。
            Err(PermitStoreError::ConditionNotMet { .. }) => {
                let existing = permits
                    .load_permit(&permit.permit_id)
                    .map_err(|error| PermitGateRefusal::Store(error.to_string()))?;
                if existing.frozen_action_digest != frozen_action_digest {
                    return Err(PermitGateRefusal::StaleOrRefused(
                        "同一次执行尝试的内容摘要与已登记许可不一致".to_string(),
                    ));
                }
            }
            Err(other) => return Err(PermitGateRefusal::Store(other.to_string())),
        }
        Ok(IssuedExecutionPermit {
            permit_id: permit.permit_id,
            attempt_key: attempt.stable_key(),
            gate_revision: state.revision,
            recovery_epoch: state.recovery_epoch,
            // 8.3c 生产者接线前**保持 None**：许可能签发、但**不能**消费（fail-closed）。
            executor_instance_id: None,
            root: root.to_path_buf(),
            scope: scope.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_safety_store::InputSafetyStore;
    use runtime::ResourceSafetyState;

    fn root() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("tempdir")
    }

    fn scope() -> InputSafetyResourceScope {
        InputSafetyResourceScope::parse("windows-session-permit-gate").expect("scope")
    }

    /// 在一张**真实**库上准备资源状态（隔离、revision/epoch 可控）。
    fn seed_store(directory: &Path, revision: u64, epoch: u64) {
        let store = InputSafetyStore::open_at(directory).expect("open");
        let scope = scope();
        store
            .put_resource_state(&runtime::InputSafetyResourceState {
                scope: scope.clone(),
                state: ResourceSafetyState::Isolated,
                revision,
                coordinator_instance_id: None,
                recovery_epoch: epoch,
                accepts_new_input: false,
            })
            .expect("put resource state");
    }

    /// 签发可用；但在 8.3c 生产者接线前，**消费必须失败**（执行者身份未就绪）。
    ///
    /// 这条是"先补 8.3c、再接 consume"的可执行形式：宁可拒绝，也不用尝试身份自绑。
    #[test]
    fn issue_works_but_consume_is_refused_until_the_executor_identity_exists() {
        let directory = root();
        seed_store(directory.path(), 7, 3);
        let issued = PermitGate::issue(
            Some(directory.path()),
            &scope(),
            "click:uia-x:hash1",
            "digest-1",
            41,
            "step-7",
            1,
            1_000,
        )
        .expect("签发可用（签发不需要执行者）");
        assert_eq!((issued.gate_revision, issued.recovery_epoch), (7, 3));
        assert_eq!(issued.executor_instance_id, None, "生产者接线前必须保持 None");
        let refusal = issued
            .recheck_and_consume()
            .expect_err("执行者身份未就绪时必须拒绝消费");
        assert_eq!(refusal.code(), "input_permit_stale_or_refused");
        assert!(
            refusal.reason().contains("执行者实例身份尚未就绪"),
            "{}",
            refusal.reason()
        );
        // 许可仍是待激活（拒绝不等于放过，也没有被"顺手绑定"）。
        let store = InputSafetyStore::open_at(directory.path()).expect("open");
        assert_eq!(
            store.permit_store().load_permit(&issued.permit_id).expect("load").state,
            InputPermitState::PendingActivation
        );
    }

    /// §七 的核心：**物理输入前复核**能抓住"签发之后、输入之前"的关闸。
    #[test]
    fn recheck_catches_a_gate_change_that_happened_after_issuing() {
        let directory = root();
        seed_store(directory.path(), 7, 3);
        let issued = PermitGate::issue(
            Some(directory.path()),
            &scope(),
            "click:uia-x:hash1",
            "digest-1",
            41,
            "step-7",
            1,
            1_000,
        )
        .expect("签发");
        // 签发之后发生关闸：revision 7 → 11。
        seed_store(directory.path(), 11, 3);
        let refusal = issued
            .recheck_and_consume()
            .expect_err("gate 变化必须拒绝输入");
        assert_eq!(refusal.code(), "input_permit_stale_or_refused");
        assert!(refusal.reason().contains("gate 已变化"), "{}", refusal.reason());
    }

    /// 未注入库根 ⇒ fail-closed（不推导路径）。
    #[test]
    fn missing_root_is_refused_without_guessing_a_path() {
        let refusal = PermitGate::issue(
            None,
            &scope(),
            "click:uia-x:hash1",
            "digest-1",
            41,
            "step-7",
            1,
            1_000,
        )
        .expect_err("未注入库根必须拒绝");
        assert_eq!(refusal.code(), "input_safety_root_not_injected");
    }

    /// 执行尝试身份非法（尝试序号 0）⇒ 拒绝，而不是补一个默认值。
    #[test]
    fn invalid_attempt_identity_is_refused_instead_of_defaulted() {
        let directory = root();
        seed_store(directory.path(), 7, 3);
        let refusal = PermitGate::issue(
            Some(directory.path()),
            &scope(),
            "click:uia-x:hash1",
            "digest-1",
            0, // 观察代次 0：没有观察上下文
            "step-7",
            1,
            1_000,
        )
        .expect_err("观察代次为 0 必须拒绝");
        assert_eq!(refusal.code(), "invalid_execution_attempt");
    }
}
