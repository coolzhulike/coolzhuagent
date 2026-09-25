//! 动作回执的**证据元数据**契约（RPR-04c 第 8 项）。
//!
//! 回执回答"发生了什么"，本模块回答"**谁说的、原始记录在哪、是不是推断出来的**"。
//! 只有把依据一起落盘，"输入前拒绝"与"已发送"才不会被读成同一个东西。
//!
//! # 为什么证据不挂在 `ActionReceipt` 上（兼容性事实，不是偏好）
//!
//! `ActionReceipt` 是**结构体字面量**被生产的（`computer-use-core` 的
//! `input_stroke.rs` / `contracts.rs` / `controller.rs`，web-console 的
//! `browser_bridge.rs` / `computer_use_executor.rs` 各有字面量构造点）。
//! Rust 的结构体字面量必须写全所有字段，因此**给 `ActionReceipt` 加任何字段都会让
//! 那些文件编译失败**——而本轮不允许改动它们。于是：
//!
//! - 证据元数据做成**同层的事实字段**：`ActionFact { identity, receipt, evidence }`
//!   （`crate::fact_store` 的 `ActionFact` / `FactLogRecord::ActionReceipt`），
//!   与回执一起原子落盘，读写两侧都校验两者一致；
//! - `ActionReceipt` 的**线格式与字段表本轮零改动**（旧 JSON 原样可读，生产者不需要改）。
//!
//! # 生产接入约定（接线时按此照做，不是展示标签）
//!
//! 1. **写入点**：宿主在写动作事实时必须走 `FactStore::record_action_fact`
//!    （`crate::fact_store`），它会对证据做两轮校验：
//!    `ActionEvidence::validate()`（依据与它自带的原始引用是否自洽）与
//!    `ActionEvidence::validate_against(receipt)`（证据与回执是否矛盾，例如
//!    DOM 协议响应不得声称鼠标已释放）。校验不过 → **拒绝写入**，
//!    不允许靠贴错标签让事实通过。
//! 2. **生产方必须填的字段**（按依据来源）：
//!    | 依据 | 必填证据 |
//!    | --- | --- |
//!    | `HostPreInputRejection`（宿主机输入前拒绝） | `action_id` + `raw_error_ref` |
//!    | `NativeHelperReceipt`（原生 helper 回执） | `action_id` + `request_id` |
//!    | `BrowserProtocolResponse`（浏览器协议响应） | `action_id` + `request_id` + `raw_response_ref` |
//!    | `HeuristicInference`（启发式推断） | `action_id` + `inference_rule_version` |
//!    | `Unknown`（不知道依据） | `action_id`（且不得断言输入已发送/已释放） |
//! 3. **消费点**：`FactSnapshot::latest_action_evidence` / `action_evidence_history`
//!    可读回证据；`FactSnapshot::blocks_further_input` 会因身份异常挡住后续声明输入资格的动作。
//! 4. **不得只填一个展示标签**：`surface` / `basis` 由**产生事实的那条通路**填写
//!    （宿主机拒绝点填 `HostPreInputRejection`，helper 退出路径填 `NativeHelperReceipt`，
//!    浏览器桥的协议响应填 `BrowserProtocolResponse`）；拿不到依据时**必须**填
//!    `Unknown` 并让 `raw_*_ref` 为空，不得挑一个"看起来像"的依据。
//!
//! 本轮**未接线**（与 `fact_store.rs` 同一现状）：没有任何请求入口在调用这些类型；
//! 上面第 1–4 条是接线时的强制约定。

use serde::{Deserialize, Serialize};

use crate::run_contract::{
    is_placeholder_identity_value, ActionReceipt, InputDelivery, InputReleaseStatus,
    RunContractError,
};

/// 动作发生的目标表面。
///
/// 与 `computer-use-core` 的 `ComputerUseSurface` 口径对齐（桌面 / 浏览器），
/// 但本 crate 不依赖该 crate，因此在这里独立定义；`Unknown` 表示**不知道**，
/// 不是"默认桌面"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionSurface {
    Desktop,
    Browser,
    Cli,
    /// 明确的未知：不得被读成上面任何一种。
    Unknown,
}

impl ActionSurface {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Browser => "browser",
            Self::Cli => "cli",
            Self::Unknown => "unknown",
        }
    }
}

/// 回执事实的**依据来源**：这条回执是谁说的。
///
/// 未知变体在反序列化时**报错**而不是落到 `Unknown`：`Unknown` 是"明确的未知事实"，
/// 不是兜底桶——把不认识的依据读成未知仍是编造，把不认识的依据读成已知更糟。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceBasis {
    /// 宿主机在**输入开始之前**就拒绝（动作从未获得输入资格）。
    HostPreInputRejection,
    /// 原生 helper 的退出事实（受控进程自己报告走了哪些步骤）。
    NativeHelperReceipt,
    /// 浏览器协议（扩展 / CDP）响应报文。
    BrowserProtocolResponse,
    /// 启发式推断：由观察间接推出，必须有规则版本，且不得断言强事实。
    HeuristicInference,
    /// 不知道依据。
    Unknown,
}

impl EvidenceBasis {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostPreInputRejection => "host_pre_input_rejection",
            Self::NativeHelperReceipt => "native_helper_receipt",
            Self::BrowserProtocolResponse => "browser_protocol_response",
            Self::HeuristicInference => "heuristic_inference",
            Self::Unknown => "unknown",
        }
    }

    /// 该依据是否允许断言"输入已释放"。
    ///
    /// 只有**原生 helper 自己的退出事实**可以：DOM/协议路径没有物理按键释放义务
    /// （裁决：DOM 操作不应伪造鼠标释放成功，没有释放义务的路径标"不适用"）。
    #[must_use]
    pub const fn may_assert_release(self) -> bool {
        matches!(self, Self::NativeHelperReceipt)
    }

    /// 该依据是否**只能**表达不确定的事实（推断 / 未知）。
    #[must_use]
    pub const fn is_indirect(self) -> bool {
        matches!(self, Self::HeuristicInference | Self::Unknown)
    }
}

/// 原始证据引用的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceReferenceKind {
    /// 原始错误（异常、退出码、拒绝原因所在的原始记录）。
    Error,
    /// 原始响应（协议报文、helper 回执原文）。
    Response,
    /// 原始日志行。
    Log,
    /// 原始产物（截图、文件、导出件）。
    Artifact,
}

/// 指向**真实存在**的原始错误 / 响应 / 日志 / 产物的引用。
///
/// `reference` 是"去哪里找原文"的定位符（例如事实存储的行号、报文的 request id、
/// 产物路径），**不是**把原文抄一遍。空的或用占位值填的引用不算引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceReference {
    pub kind: EvidenceReferenceKind,
    pub reference: String,
    /// 原文摘要（可选）；摘要缺失不影响引用的有效性。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

impl EvidenceReference {
    #[must_use]
    pub fn new(kind: EvidenceReferenceKind, reference: impl Into<String>) -> Self {
        Self {
            kind,
            reference: reference.into(),
            digest: None,
        }
    }

    fn validate(&self, field: &str) -> Result<(), RunContractError> {
        if self.reference.trim().is_empty() {
            return Err(RunContractError::invalid_action_evidence(format!(
                "{field} 的引用不能为空"
            )));
        }
        if is_placeholder_identity_value(&self.reference) {
            return Err(RunContractError::placeholder_action_evidence(format!(
                "{field} 的引用是占位值（{}），不是可追溯的原始记录",
                self.reference
            )));
        }
        if let Some(digest) = &self.digest {
            if digest.trim().is_empty() || is_placeholder_identity_value(digest) {
                return Err(RunContractError::placeholder_action_evidence(format!(
                    "{field} 的摘要为空或为占位值"
                )));
            }
        }
        Ok(())
    }
}

/// 一条动作事实的证据元数据。
///
/// 与 `ActionReceipt` **同层**（见模块文档"为什么证据不挂在 `ActionReceipt` 上"）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionEvidence {
    /// 对应回执的 `action_id`；必须与回执一致（写入时校验）。
    pub action_id: String,
    pub surface: ActionSurface,
    pub basis: EvidenceBasis,
    /// 对应 `request_id`：产生这条事实的那次请求 / 调用的标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    /// 原始错误引用（输入前拒绝、helper 失败等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_error_ref: Option<EvidenceReference>,
    /// 原始响应引用（协议报文、helper 回执原文）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_response_ref: Option<EvidenceReference>,
    /// 推断规则版本：只有启发式推断才允许（也才必须）携带。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_rule_version: Option<String>,
}

impl ActionEvidence {
    /// 依据自校验：引用是否可追溯、该依据要求的原始引用是否齐全。
    pub fn validate(&self) -> Result<(), RunContractError> {
        if self.action_id.trim().is_empty() {
            return Err(RunContractError::invalid_identity("action_id"));
        }
        if is_placeholder_identity_value(&self.action_id) {
            return Err(RunContractError::placeholder_action_evidence(format!(
                "证据的 action_id 是占位值（{}）",
                self.action_id
            )));
        }
        if let Some(request_id) = &self.request_id {
            if request_id.trim().is_empty() || is_placeholder_identity_value(request_id) {
                return Err(RunContractError::placeholder_action_evidence(
                    "证据的 request_id 为空或为占位值",
                ));
            }
        }
        if let Some(error_ref) = &self.raw_error_ref {
            error_ref.validate("raw_error_ref")?;
        }
        if let Some(response_ref) = &self.raw_response_ref {
            response_ref.validate("raw_response_ref")?;
        }
        if let Some(version) = &self.inference_rule_version {
            if version.trim().is_empty() || is_placeholder_identity_value(version) {
                return Err(RunContractError::placeholder_action_evidence(
                    "证据的 inference_rule_version 为空或为占位值",
                ));
            }
            // 推断规则版本只属于启发式推断：贴在别的依据上等于给"直接证据"镀金。
            if self.basis != EvidenceBasis::HeuristicInference {
                return Err(RunContractError::invalid_action_evidence(
                    "inference_rule_version 只适用于 heuristic_inference 依据",
                ));
            }
        }

        match self.basis {
            EvidenceBasis::HostPreInputRejection => {
                if self.raw_error_ref.is_none() {
                    return Err(RunContractError::invalid_action_evidence(
                        "输入前拒绝必须给原始错误引用（raw_error_ref）：拒绝理由要可追溯",
                    ));
                }
            }
            EvidenceBasis::NativeHelperReceipt => {
                if self.request_id.is_none() {
                    return Err(RunContractError::invalid_action_evidence(
                        "原生 helper 回执必须给 request_id：哪一次受控调用产生的",
                    ));
                }
            }
            EvidenceBasis::BrowserProtocolResponse => {
                if self.request_id.is_none() {
                    return Err(RunContractError::invalid_action_evidence(
                        "浏览器协议响应必须给 request_id：哪一次协议报文",
                    ));
                }
                if self.raw_response_ref.is_none() {
                    return Err(RunContractError::invalid_action_evidence(
                        "浏览器协议响应必须给原始响应引用（raw_response_ref）",
                    ));
                }
            }
            EvidenceBasis::HeuristicInference => {
                if self.inference_rule_version.is_none() {
                    return Err(RunContractError::invalid_action_evidence(
                        "启发式推断必须给 inference_rule_version：推断结论要能按规则版本复核",
                    ));
                }
            }
            // 明确的未知：不要求引用（本来就没有），但下面禁止它断言任何强事实。
            EvidenceBasis::Unknown => {}
        }
        Ok(())
    }

    /// 证据与本条回执是否一致：`action_id` 与"依据能支撑多强的结论"都要对得上。
    ///
    /// 这是"证据不是展示标签"的关键一步：依据弱却断言强 → 拒绝写入。
    pub fn validate_against(&self, receipt: &ActionReceipt) -> Result<(), RunContractError> {
        if self.action_id != receipt.action_id {
            return Err(RunContractError::invalid_action_evidence(format!(
                "证据的 action_id（{}）与回执的 action_id（{}）不一致",
                self.action_id, receipt.action_id
            )));
        }

        // 释放事实只能由原生 helper 的退出事实支撑（裁决：DOM 操作不应伪造鼠标释放成功）。
        if !self.basis.may_assert_release() && receipt.input_release == InputReleaseStatus::Released {
            return Err(RunContractError::invalid_action_evidence(format!(
                "依据 {} 不得断言输入已释放（只有原生 helper 回执可以）；没有物理按键释放义务的路径应为 not_needed",
                self.basis.as_str()
            )));
        }

        if self.basis == EvidenceBasis::HostPreInputRejection {
            // 输入前拒绝：回执必须是**可证明的未发送**，且从未有过释放义务。
            if receipt.input_delivery != InputDelivery::NotSent
                || receipt.may_have_started_input()
                || receipt.path_completed == Some(true)
                || receipt.confirmed_point_count.unwrap_or(0) > 0
                || receipt.input_release != InputReleaseStatus::NotNeeded
            {
                return Err(RunContractError::invalid_action_evidence(
                    "host_pre_input_rejection 依据要求回执是「输入前未发送且无释放义务」的事实",
                ));
            }
        }

        if self.basis.is_indirect() {
            // 推断 / 未知不得断言强事实：不得说"已发送"、"已释放"、"路径走完"、"非部分"。
            if receipt.input_delivery == InputDelivery::Sent
                || receipt.input_release == InputReleaseStatus::Released
                || receipt.path_completed == Some(true)
                || receipt.partial == Some(false)
            {
                return Err(RunContractError::invalid_action_evidence(format!(
                    "依据 {} 是间接证据，不得断言输入已发送 / 已释放 / 路径完成 / 非部分；这些结论必须留作未知",
                    self.basis.as_str()
                )));
            }
        }
        Ok(())
    }

    /// 是否属于间接证据（推断 / 未知）——消费方据此选择更保守的解读。
    #[must_use]
    pub const fn is_indirect(&self) -> bool {
        self.basis.is_indirect()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionEvidence, ActionSurface, EvidenceBasis, EvidenceReference, EvidenceReferenceKind,
    };
    use crate::run_contract::{
        ActionReceipt, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus,
    };

    fn receipt(
        action_id: &str,
        input_delivery: InputDelivery,
        path_completed: Option<bool>,
        partial: Option<bool>,
        confirmed_points: Option<u32>,
        input_release: InputReleaseStatus,
    ) -> ActionReceipt {
        ActionReceipt {
            action_id: action_id.to_string(),
            input_delivery,
            partial,
            path_completed,
            confirmed_point_count: confirmed_points,
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release,
        }
    }

    fn sample(action_id: &str, basis: EvidenceBasis) -> ActionEvidence {
        ActionEvidence {
            action_id: action_id.to_string(),
            surface: ActionSurface::Browser,
            basis,
            request_id: None,
            raw_error_ref: None,
            raw_response_ref: None,
            inference_rule_version: None,
        }
    }

    /// 每种依据都要求它自己的原始引用：缺了就是"只有标签、没有证据"。
    #[test]
    fn every_basis_requires_its_own_raw_reference() {
        let mut rejection = sample("action-1", EvidenceBasis::HostPreInputRejection);
        assert!(rejection.validate().is_err(), "输入前拒绝必须给原始错误引用");
        rejection.raw_error_ref = Some(EvidenceReference::new(
            EvidenceReferenceKind::Error,
            "web-sessions.sqlite3#computer_use_events/1821",
        ));
        assertion_ok(rejection.validate());

        let helper = sample("action-1", EvidenceBasis::NativeHelperReceipt);
        assert!(helper.validate().is_err(), "helper 回执必须给 request_id");

        let protocol = sample("action-1", EvidenceBasis::BrowserProtocolResponse);
        assert!(protocol.validate().is_err(), "协议响应必须给 request_id");

        let heuristic = sample("action-1", EvidenceBasis::HeuristicInference);
        assert!(heuristic.validate().is_err(), "推断必须给规则版本");

        // 明确的未知不要求引用（本来就没有），但也不许断言强事实（见下一个用例）。
        assertion_ok(sample("action-1", EvidenceBasis::Unknown).validate());
    }

    /// 占位引用不算引用；推断规则版本不得贴在直接证据上。
    #[test]
    fn placeholder_references_and_misplaced_rule_versions_are_rejected() {
        let mut rejection = sample("action-1", EvidenceBasis::HostPreInputRejection);
        rejection.raw_error_ref = Some(EvidenceReference::new(EvidenceReferenceKind::Error, "n/a"));
        assert!(rejection.validate().is_err(), "占位引用必须被拒绝");

        let mut helper = sample("action-1", EvidenceBasis::NativeHelperReceipt);
        helper.request_id = Some("unknown".to_string());
        assert!(helper.validate().is_err(), "占位 request_id 必须被拒绝");

        helper.request_id = Some("helper-call-7".to_string());
        helper.inference_rule_version = Some("rule-v1".to_string());
        assert!(
            helper.validate().is_err(),
            "推断规则版本只属于启发式推断，贴到 helper 回执上是镀金"
        );
    }

    /// DOM / 协议路径不得伪造鼠标释放（裁决第 19 条）。
    #[test]
    fn protocol_evidence_never_claims_a_release_it_does_not_own() {
        let released = receipt(
            "action-1",
            InputDelivery::Sent,
            None,
            Some(false),
            None,
            InputReleaseStatus::Released,
        );
        for basis in [
            EvidenceBasis::BrowserProtocolResponse,
            EvidenceBasis::HeuristicInference,
            EvidenceBasis::Unknown,
            EvidenceBasis::HostPreInputRejection,
        ] {
            let mut evidence = sample("action-1", basis);
            evidence.request_id = Some("req-1".to_string());
            evidence.raw_response_ref = Some(EvidenceReference::new(
                EvidenceReferenceKind::Response,
                "browser_bridge#dispatch/9",
            ));
            evidence.raw_error_ref = Some(EvidenceReference::new(
                EvidenceReferenceKind::Error,
                "browser_bridge#dispatch/9",
            ));
            evidence.inference_rule_version =
                (basis == EvidenceBasis::HeuristicInference).then(|| "rule-v2".to_string());
            assert!(
                evidence.validate_against(&released).is_err(),
                "{} 不得断言输入已释放",
                basis.as_str()
            );
        }

        // 原生 helper 事实可以（它真的按了键也真的松了）。
        let helper_evidence = ActionEvidence {
            request_id: Some("helper-call-7".to_string()),
            ..sample("action-1", EvidenceBasis::NativeHelperReceipt)
        };
        assertion_ok(helper_evidence.validate_against(&released));
    }

    /// 推断 / 未知不得断言"已发送 / 路径完成 / 非部分"。
    #[test]
    fn indirect_basis_cannot_assert_certainty() {
        let certain = receipt(
            "action-1",
            InputDelivery::Sent,
            Some(true),
            Some(false),
            Some(4),
            InputReleaseStatus::NotNeeded,
        );
        for basis in [EvidenceBasis::HeuristicInference, EvidenceBasis::Unknown] {
            let mut evidence = sample("action-1", basis);
            evidence.inference_rule_version =
                (basis == EvidenceBasis::HeuristicInference).then(|| "rule-v2".to_string());
            assert!(
                evidence.validate_against(&certain).is_err(),
                "{} 是间接证据，不得断言强事实",
                basis.as_str()
            );
        }

        // 同样的回执换成原生 helper 事实是允许的。
        let helper_evidence = ActionEvidence {
            request_id: Some("helper-call-7".to_string()),
            ..sample("action-1", EvidenceBasis::NativeHelperReceipt)
        };
        assertion_ok(helper_evidence.validate_against(&certain));
    }

    /// 输入前拒绝依据必须配"未发送且无释放义务"的回执，否则矛盾。
    #[test]
    fn pre_input_rejection_basis_requires_a_pre_input_receipt() {
        let mut evidence = sample("action-1", EvidenceBasis::HostPreInputRejection);
        evidence.raw_error_ref = Some(EvidenceReference::new(
            EvidenceReferenceKind::Error,
            "input_gate#reject/3",
        ));
        let not_sent = receipt(
            "action-1",
            InputDelivery::NotSent,
            Some(false),
            Some(false),
            Some(0),
            InputReleaseStatus::NotNeeded,
        );
        assertion_ok(evidence.validate_against(&not_sent));

        let sent = receipt(
            "action-1",
            InputDelivery::Sent,
            None,
            Some(false),
            None,
            InputReleaseStatus::NotNeeded,
        );
        assert!(
            evidence.validate_against(&sent).is_err(),
            "已经发送的输入不能标成「输入前拒绝」"
        );
    }

    /// 证据与回执的 `action_id` 必须一致。
    #[test]
    fn evidence_must_belong_to_the_receipt_it_describes() {
        let evidence = sample("action-2", EvidenceBasis::Unknown);
        assert!(evidence.validate().is_ok(), "示例证据自身必须自洽");
        let other = receipt(
            "action-1",
            InputDelivery::MayHaveBeenSent,
            None,
            None,
            None,
            InputReleaseStatus::Unknown,
        );
        assert!(evidence.validate_against(&other).is_err());
    }

    fn assertion_ok(result: Result<(), crate::run_contract::RunContractError>) {
        if let Err(error) = result {
            panic!("应当通过：{}：{}", error.code, error.message);
        }
    }
}
