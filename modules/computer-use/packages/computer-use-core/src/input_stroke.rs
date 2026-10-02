//! 受控原生笔画：固定 helper 只接受窗口身份和数值路径，不提供脚本执行入口。
use super::{
    cleanup_release_status_of, derive_after_cleanup, derive_before_cleanup, derive_delivery_facts,
    derive_input_status, derive_release_obligation, validate_receipt_for_write, DeliveryFacts,
    HelperFactPhase, InputStatus,
    HelperFactRead, HelperFactView, ReleaseDerivationInputs, ReleaseObligationState,
    HELPER_FACT_PROTOCOL_V1, HELPER_FACT_PROTOCOL_V2,
};
#[cfg(windows)]
use super::HelperPipeReadersAdmission;
use super::{HelperCleanupReservation, MousePoint};
use crate::cleanup::{
    unix_ms, CleanupDeadline, CleanupPolicy, CleanupReleaseStatus, HelperCleanupFacts,
};
use runtime::{ActionReceipt, EffectStatus, GoalVerdict, InputDelivery, InputReleaseStatus};
use crate::prepared_input::{NativeInputAuthorization, NativeInputCompletion, PreparedInputSession, SupervisedHelperChild};
use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// 输入安全的取消/收尾策略：**唯一**集中定义点。
///
/// 三个数值是**本轮裁决给出的待验默认值，不是实测时延**（协作退出 ≤ 2 秒、
/// 自动收尾总窗口 ≤ 4 秒、独立释放等待 ≤ 2 秒）。任何地方都不得各自调大它们。
#[must_use]
fn input_cleanup_policy() -> CleanupPolicy {
    CleanupPolicy::default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StrokeWindow {
    pub handle: isize,
    pub process_id: u32,
    pub rect: [i32; 4],
    pub dpi: u32,
}

pub fn validate_stroke(
    window: StrokeWindow,
    bounds: [i32; 4],
    points: &[MousePoint],
    duration_ms: u64,
) -> Result<(), String> {
    let valid_rect = |r: [i32; 4]| {
        r[2] > 0 && r[3] > 0 && r[0].checked_add(r[2]).is_some() && r[1].checked_add(r[3]).is_some()
    };
    if window.handle == 0
        || window.process_id == 0
        || window.dpi == 0
        || !valid_rect(window.rect)
        || !valid_rect(bounds)
    {
        return Err("invalid_stroke_bounds: 窗口身份或边界无效".into());
    }
    if bounds[0] < window.rect[0]
        || bounds[1] < window.rect[1]
        || i64::from(bounds[0]) + i64::from(bounds[2])
            > i64::from(window.rect[0]) + i64::from(window.rect[2])
        || i64::from(bounds[1]) + i64::from(bounds[3])
            > i64::from(window.rect[1]) + i64::from(window.rect[3])
    {
        return Err("invalid_stroke_bounds: 画布超出已观察窗口".into());
    }
    if !(2..=256).contains(&points.len()) || duration_ms > 5_000 {
        return Err("invalid_stroke_path: 笔画需要 2–256 点且耗时不超过 5000ms".into());
    }
    if points.iter().any(|p| {
        p.x < bounds[0]
            || p.y < bounds[1]
            || p.x >= bounds[0] + bounds[2]
            || p.y >= bounds[1] + bounds[3]
    }) {
        return Err("invalid_stroke_path: 笔画坐标超出画布".into());
    }
    Ok(())
}

/// 受控 helper 自己报告的输入事实（进度记录）。
///
/// 这些值只可能来自受信 helper 的进度文件（`Engine` 在每一步被确认之后写入，写入后回读校验），
/// 与模型生成的动作 JSON 无关。缺文件/损坏一律表示"没有事实"，不得被解读成"未发送"。
///
/// CU-F01 补充的最小标记（v2）：
/// - `protocol` / `request_id`：这份记录属于哪一版协议、哪一次请求；
/// - `phase`：这条记录是过程快照还是**收尾之后的最终事实**；
/// - `cursor_moved`：光标是否已经移动过——没有它，"零注入"与"什么都没做"无法区分。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HelperInputFacts {
    /// 记录声明的协议版本（旧记录 = [`HELPER_FACT_PROTOCOL_V1`]）。
    pub protocol: u32,
    /// 本次请求的身份；旧记录为 `None`，因此无法核对归属。
    pub request_id: Option<String>,
    /// 记录所处的阶段；只有 [`HelperFactPhase::Final`] 才算收尾已封闭。
    pub phase: HelperFactPhase,
    /// 是否确认移动过光标；`None` = 旧记录无从核对。
    pub cursor_moved: Option<bool>,
    /// helper 确认已注入的路径点数：起点在"按下确认"之后计入，未按下时恒为 0。
    pub injected_points: u32,
    /// helper 是否确认按下过左键；它决定是否存在释放义务。
    pub button_down: bool,
    /// helper 是否确认走完整条路径。
    pub path_completed: bool,
    /// `None` = helper 没有确认释放结果。
    pub released: Option<bool>,
}

/// v1 进度记录（没有版本／阶段／身份标记）。仍可读取，但**不能**当证明。
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportedV1 {
    injected_points: u32,
    button_down: bool,
    path_completed: bool,
    #[serde(default)]
    released: Option<bool>,
}

/// v2 进度记录：在 v1 之上补最小必要的版本／身份／阶段／完整性标记。
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportedV2 {
    protocol: u32,
    request_id: String,
    phase: String,
    cursor_moved: bool,
    injected_points: u32,
    button_down: bool,
    path_completed: bool,
    #[serde(default)]
    released: Option<bool>,
}

impl HelperInputFacts {
    /// 读取 helper 进度文件内容。
    ///
    /// 严格解析：缺键、多键、类型不符、取值超出受控上限、JSON 损坏或协议/身份不符
    /// 都**不是**"零输入"，一律返回 [`HelperFactRead::Rejected`] 并带上原因。
    ///
    /// 旧版（v1）记录仍可读取，但它的 `phase` 是 `LegacyUnverifiable`、
    /// `cursor_moved` 是 `None`，因此**永远不能**充当"零输入"证明：版本兼容 +
    /// 异常呈现，而不是把旧记录丢掉或者当证据用。
    #[must_use]
    pub fn read(raw: &[u8], expected_request_id: &str) -> HelperFactRead<Self> {
        let value: serde_json::Value = match serde_json::from_slice(raw) {
            Ok(value) => value,
            Err(error) => {
                return HelperFactRead::Rejected {
                    anomaly: format!("进度记录不是合法 JSON：{error}"),
                }
            }
        };
        match serde_json::from_value::<ReportedV2>(value.clone()) {
            Ok(reported) => Self::from_v2(reported, expected_request_id),
            Err(v2_error) => match serde_json::from_value::<ReportedV1>(value) {
                Ok(reported) => Self::from_v1(reported),
                Err(v1_error) => HelperFactRead::Rejected {
                    anomaly: format!(
                        "进度记录不符合任何已知协议：v2（{v2_error}）；v1（{v1_error}）"
                    ),
                },
            },
        }
    }

    fn from_v1(reported: ReportedV1) -> HelperFactRead<Self> {
        let facts = Self {
            protocol: HELPER_FACT_PROTOCOL_V1,
            request_id: None,
            phase: HelperFactPhase::LegacyUnverifiable,
            cursor_moved: None,
            injected_points: reported.injected_points,
            button_down: reported.button_down,
            path_completed: reported.path_completed,
            released: reported.released,
        };
        match facts.check_integrity() {
            Ok(()) => HelperFactRead::Trusted(facts),
            Err(anomaly) => HelperFactRead::Rejected { anomaly },
        }
    }

    fn from_v2(reported: ReportedV2, expected_request_id: &str) -> HelperFactRead<Self> {
        if reported.protocol != HELPER_FACT_PROTOCOL_V2 {
            return HelperFactRead::Rejected {
                anomaly: format!(
                    "协议版本不符：记录声明 protocol={}，本进程只认 {} 与缺失版本键的 v1",
                    reported.protocol, HELPER_FACT_PROTOCOL_V2
                ),
            };
        }
        if reported.request_id != expected_request_id {
            // 身份不合＝这份事实不属于本动作的这次请求，绝不能被接纳。
            return HelperFactRead::Rejected {
                anomaly: format!(
                    "记录不属于本次请求：request_id={:?}，期望 {:?}",
                    reported.request_id, expected_request_id
                ),
            };
        }
        let Some(phase) = HelperFactPhase::from_marker(&reported.phase) else {
            return HelperFactRead::Rejected {
                anomaly: format!("阶段标记无法识别：phase={:?}", reported.phase),
            };
        };
        let facts = Self {
            protocol: reported.protocol,
            request_id: Some(reported.request_id),
            phase,
            cursor_moved: Some(reported.cursor_moved),
            injected_points: reported.injected_points,
            button_down: reported.button_down,
            path_completed: reported.path_completed,
            released: reported.released,
        };
        if let Err(anomaly) = facts.check_v2_integrity() {
            return HelperFactRead::Rejected { anomaly };
        }
        HelperFactRead::Trusted(facts)
    }

    /// v1／v2 共同的自洽性检查（与旧实现逐条一致）。
    fn check_integrity(&self) -> Result<(), String> {
        if self.injected_points > MAX_STROKE_POINTS {
            return Err(format!(
                "已注入点数 {} 超出上限 {MAX_STROKE_POINTS}",
                self.injected_points
            ));
        }
        if self.injected_points > 0 && !self.button_down {
            // "注入过点却没按下按键"在 helper 的契约里不可能出现，只能是伪造或损坏。
            return Err("注入过点却没按下按键，只能是伪造或损坏".to_string());
        }
        if self.injected_points == 0 && self.path_completed {
            return Err("零注入却报告路径走完，只能是伪造或损坏".to_string());
        }
        Ok(())
    }

    /// v2 额外的阶段／完整性检查：过程快照不得冒充最终事实，反之亦然。
    fn check_v2_integrity(&self) -> Result<(), String> {
        self.check_integrity()?;
        // 起点阶段：任何 Move / Down / 注入都还没发生。
        if self.phase == HelperFactPhase::PreInput
            && (self.injected_points > 0
                || self.button_down
                || self.path_completed
                || self.cursor_moved == Some(true))
        {
            return Err("起点阶段的记录却已经移动光标／注入／按下：阶段与事实自相矛盾"
                .to_string());
        }
        // 点数只可能在"按下确认"之后才计入，因此"移动过光标 + 零注入"是合法的
        // （路径第 0 点移动成功、随后校验失败），而"没移动过光标却注入了点"不可能。
        if self.cursor_moved == Some(false) && self.injected_points > 0 {
            return Err("没有移动过光标却报告已注入路径点：阶段与事实自相矛盾".to_string());
        }
        Ok(())
    }

    /// 投影到两条路径共用的释放事实视图。
    #[must_use]
    pub fn release_view(&self) -> HelperFactView<'_> {
        HelperFactView {
            phase: self.phase,
            cursor_moved: self.cursor_moved,
            injected: self.injected_points,
            ever_pressed: self.button_down,
            still_holding: self.button_down && self.released != Some(true),
            sequence_completed: self.path_completed,
            released: self.released,
            request_id: self.request_id.as_deref(),
        }
    }
}

/// 路径点数上限（与 `validate_stroke` 的上限一致）。
const MAX_STROKE_POINTS: u32 = 256;


/// 读取 helper 进度文件；不存在或不可读都表示"没有记录"。
fn read_helper_facts(path: &Path, expected_request_id: &str) -> HelperFactRead<HelperInputFacts> {
    match std::fs::read(path) {
        Ok(raw) => HelperInputFacts::read(&raw, expected_request_id),
        Err(_) => HelperFactRead::Missing,
    }
}

/// 受控笔画失败的分类（错误码 + 是否可重试）。分类只看错误文本，不看事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrokeFailureKind {
    /// helper 明确报释放失败，或 helper 退出后补发释放仍失败。
    ReleaseUnconfirmed,
    Cancelled,
    /// helper 明确报告输入许可到期；与人工取消分开，不能继续重放。
    DeadlineExceeded,
    Stale,
    /// 被本进程强杀，或非零退出且没有给出任何原因。
    HelperLost,
    Failed,
}

impl StrokeFailureKind {
    /// 与 `computer_use_desktop_bridge` 既有的 `classify_stroke_failure` 完全一致。
    #[must_use]
    pub fn classify(message: &str) -> Self {
        if message.contains("mouse_release_failed") || message.contains("input_release_unconfirmed") {
            Self::ReleaseUnconfirmed
        } else if message.contains("stroke_cancelled: permit expired") {
            Self::DeadlineExceeded
        } else if message.contains("stroke_cancelled") {
            Self::Cancelled
        } else if message.contains("stale_observation") {
            Self::Stale
        } else if message.contains("helper_lost") {
            Self::HelperLost
        } else {
            Self::Failed
        }
    }

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ReleaseUnconfirmed => "mouse_release_failed",
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::Stale => "stale_observation",
            Self::HelperLost => "helper_lost",
            Self::Failed => "input_failed",
        }
    }

    /// 只有**确认释放**（或根本没有释放义务）的失败才允许继续；
    /// "释放未确认"必须隔离，`retryable` 只是分类信息，不是重放授权。
    #[must_use]
    pub const fn retryable(self) -> bool {
        !matches!(self, Self::ReleaseUnconfirmed | Self::Cancelled | Self::DeadlineExceeded)
    }
}

/// 受控笔画失败：保留既有错误文本，并附带 helper 已报告过的输入事实。
///
/// 两个维度**分开**存放，互不覆盖：
/// - `message` / [`StrokeFailure::kind`]：**原始执行原因**（只看 helper 自己的报告文本）；
/// - `release_state` / `cleanup`：**安全收尾结果**（无需释放 / 已结清 / 未确认）。
///
/// 投递事实与释放义务都由**同一份**推导产出（`input` 模块的唯一实现点），
/// 在构造时算好并留存，避免各处再写第二套判断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrokeFailure {
    pub message: String,
    pub helper_process: Option<runtime::ProcessInstanceEvidence>,
    /// helper 是否已收到请求、可能已经注入过输入。`false` 表示**可以证明**输入前就失败了。
    pub input_possible: bool,
    /// 进度记录的读取结果：可信事实 / 不可信（带原因）/ 没有记录。
    pub fact_read: HelperFactRead<HelperInputFacts>,
    /// helper 自己报告过释放失败（受信报告）。
    pub helper_reported_release_failure: bool,
    /// 执行者已确认静止。
    pub stillness_confirmed: bool,
    /// 投递事实（推导结果，不是再次计算的现场判断）。
    pub delivery: DeliveryFacts,
    /// CU-01：由 `delivery` 派生的粗粒度输入状态（none／partial／complete／unknown）。
    pub input_status: InputStatus,
    /// 释放义务的**最终**状态（已把本次收尾的结果合并进去）。
    pub release_state: ReleaseObligationState,
    /// 有界收尾的事实；`None` = 这次失败没有进入取消/异常收尾。
    pub cleanup: Option<HelperCleanupFacts>,
}

impl StrokeFailure {
    /// 输入开始之前就失败：没有 helper 请求，也就没有输入。
    fn before_input(message: impl Into<String>) -> Self {
        Self::derive(
            message.into(),
            false,
            HelperFactRead::Missing,
            false,
            true,
            false,
        )
    }

    /// helper 已收到请求之后失败：可能已注入，事实按 helper 报告为准。
    ///
    /// 事实之外的推导标志取默认值（helper 没报释放失败、执行者已静止、本进程没补发）。
    /// 生产路径统一走 `run_helper` 的私有构造，这个入口供外部（如桥接层与其单测）
    /// 按"给定事实"复现同一条推导链。
    #[must_use]
    pub fn after_input(
        message: impl Into<String>,
        fact_read: HelperFactRead<HelperInputFacts>,
    ) -> Self {
        Self::derive(message.into(), true, fact_read, false, true, false)
    }

    /// 唯一的事实推导入口（裁决 §2 的正推顺序：先事实，再义务，最后回执）。
    fn derive(
        message: String,
        input_possible: bool,
        fact_read: HelperFactRead<HelperInputFacts>,
        helper_reported_release_failure: bool,
        stillness_confirmed: bool,
        independent_release_confirmed: bool,
    ) -> Self {
        let inputs = StrokeFailure::derivation_inputs_for(
            StrokeFailureKind::classify(&message),
            input_possible,
            // 构造器默认按"笔画路径"处理；其余模式由 `run_helper` 走带模式的入口。
            true,
            &fact_read,
            helper_reported_release_failure,
            stillness_confirmed,
            independent_release_confirmed,
        );
        let delivery = derive_delivery_facts(&inputs);
        let release_state = derive_release_obligation(&inputs);
        // CU-01：粗粒度输入状态由事实层派生出（唯一判定点在 `derive_input_status`），
        // 报告与界面读它即可，不必各自把 delivery/partial/点数再拼一遍。
        let input_status = derive_input_status(&delivery);
        Self {
            message,
            helper_process: None,
            input_possible,
            fact_read,
            helper_reported_release_failure,
            stillness_confirmed,
            delivery,
            input_status,
            release_state,
            cleanup: None,
        }
    }

    fn derivation_inputs_for<'a>(
        kind: StrokeFailureKind,
        input_possible: bool,
        stroke_mode: bool,
        fact_read: &'a HelperFactRead<HelperInputFacts>,
        helper_reported_release_failure: bool,
        stillness_confirmed: bool,
        independent_release_confirmed: bool,
    ) -> ReleaseDerivationInputs<'a> {
        ReleaseDerivationInputs {
            kind,
            input_possible,
            path_action: stroke_mode,
            // 笔画路径不预留义务：义务完全由 helper 的事实决定。截图、独立释放这类
            // 不注入按键的模式在**机制上**不可能留下按住状态，是一条正面事实。
            obligation_mechanism_leaves_nothing: !stroke_mode,
            // 归属由读取时的身份核对保证（不合的记录在 read 里就已被判为不可信）。
            facts_belong_to_this_action: true,
            facts: fact_read.trusted().map(HelperInputFacts::release_view),
            fact_anomaly: fact_read.anomaly(),
            helper_reported_release_failure,
            stillness_confirmed,
            independent_release_confirmed,
        }
    }

    /// 本次失败（含已合并的收尾结果）的推导输入。
    #[must_use]
    pub fn derivation_inputs(&self) -> ReleaseDerivationInputs<'_> {
        let independent_release_confirmed = self
            .cleanup
            .is_some_and(|cleanup| cleanup.independent_release_confirmed);
        StrokeFailure::derivation_inputs_for(
            self.kind(),
            self.input_possible,
            true,
            &self.fact_read,
            self.helper_reported_release_failure,
            self.stillness_confirmed,
            independent_release_confirmed,
        )
    }

    /// 附加有界收尾的事实，并按收尾结果**合并**释放义务状态，不改动原始分类。
    fn with_cleanup(mut self, cleanup: HelperCleanupFacts) -> Self {
        self.cleanup = Some(cleanup);
        let inputs = self.derivation_inputs();
        self.release_state = derive_after_cleanup(&inputs, cleanup.independent_release_confirmed);
        self
    }

    /// 逐字复用旧实现：原始执行原因只看错误文本。
    #[must_use]
    pub fn kind(&self) -> StrokeFailureKind {
        StrokeFailureKind::classify(&self.message)
    }

    /// 可信事实；没有记录或记录不可信时返回 `None`。
    #[must_use]
    pub fn facts(&self) -> Option<&HelperInputFacts> {
        self.fact_read.trusted()
    }

    /// 记录不可信时的原因（协议／身份／完整性／冲突）。
    #[must_use]
    pub fn fact_anomaly(&self) -> Option<&str> {
        self.fact_read.anomaly()
    }

    /// 释放义务的最终五态。
    #[must_use]
    pub const fn release_state(&self) -> ReleaseObligationState {
        self.release_state
    }

    /// 只读访问收尾事实；这次失败没有进入取消/异常收尾时返回 `None`。
    #[must_use]
    pub fn cleanup(&self) -> Option<&HelperCleanupFacts> {
        self.cleanup.as_ref()
    }
}

impl std::fmt::Display for StrokeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StrokeFailure {}

/// helper 成功退出后可以确认的事实。
///
/// 受控 helper 的契约是：任何一步失败都会 `throw` 并以非零状态退出，`finally` 一定会
/// 尝试 `Up()`。因此"正常退出"本身就确认了整条路径走完、按键已释放——这不是从
/// `input_sent` 推导，而是 helper 自己的退出语义（由 `MockChecks` 的事件顺序与释放保证覆盖）。
///
/// 可信的终态记录优先（它带阶段与光标移动事实）；只有记录缺失或未声明走完时，
/// 才按退出语义补一条**明确标注**为最终事实的合成记录。
fn confirmed_success_facts(
    points: usize,
    request_id: &str,
    reported: HelperFactRead<HelperInputFacts>,
) -> Result<HelperInputFacts, StrokeFailure> {
    match reported {
        HelperFactRead::Trusted(facts) if facts.path_completed => Ok(facts),
        // 记录缺失或未声明走完：按 helper 自身的退出语义补一条**明确标注**为最终事实的记录。
        HelperFactRead::Trusted(_) | HelperFactRead::Missing => Ok(HelperInputFacts {
            protocol: HELPER_FACT_PROTOCOL_V2,
            request_id: Some(request_id.to_string()),
            phase: HelperFactPhase::Final,
            cursor_moved: Some(true),
            injected_points: u32::try_from(points).unwrap_or(0),
            button_down: true,
            path_completed: true,
            released: Some(true),
        }),
        // 有记录却读不得：绝不合成"完整成功"的事实，保留异常并交上层处理。
        HelperFactRead::Rejected { anomaly } => Err(StrokeFailure::derive(
            format!("input_release_unconfirmed: 笔画进度记录不可信，无法确认动作结果（{anomaly}）"),
            true,
            HelperFactRead::Rejected { anomaly },
            false,
            true,
            false,
        )),
    }
}

/// 受控输入开始之前就被拒绝：明确的"未发送"事实（不得留给上层猜）。
#[must_use]
pub fn pre_input_receipt(action_id: &str, is_path: bool) -> ActionReceipt {
    ActionReceipt {
        action_id: action_id.to_string(),
        input_delivery: InputDelivery::NotSent,
        partial: Some(false),
        path_completed: if is_path { Some(false) } else { None },
        confirmed_point_count: if is_path { Some(0) } else { None },
        effect: EffectStatus::NotObserved,
        goal_verdict: GoalVerdict::NotChecked,
        input_release: InputReleaseStatus::NotNeeded,
    }
}

/// 非路径动作已确认完成：输入已发出，动作本身没有残留释放义务。
#[must_use]
pub fn sent_receipt(action_id: &str) -> ActionReceipt {
    ActionReceipt {
        action_id: action_id.to_string(),
        input_delivery: InputDelivery::Sent,
        partial: Some(false),
        path_completed: None,
        confirmed_point_count: None,
        effect: EffectStatus::NotObserved,
        goal_verdict: GoalVerdict::NotChecked,
        input_release: InputReleaseStatus::NotNeeded,
    }
}

/// 同一动作里已确认发出前一段输入、后一段失败（例如点到但没输入文字）。
#[must_use]
pub fn partial_input_receipt(action_id: &str) -> ActionReceipt {
    ActionReceipt {
        action_id: action_id.to_string(),
        input_delivery: InputDelivery::Sent,
        partial: Some(true),
        path_completed: None,
        confirmed_point_count: None,
        effect: EffectStatus::NotObserved,
        goal_verdict: GoalVerdict::NotChecked,
        input_release: InputReleaseStatus::NotNeeded,
    }
}

/// 失败路径的回执：直接落**已经推导好的**投递事实与释放义务，不在这里再判一遍。
///
/// - 可信的"零输入"证明成立 → 明确的未发送 + `input_release = NotNeeded`
///   （**不再**被"释放未确认"这类文本类别覆盖，见裁决 §5）；
/// - 已注入但未走完 → 保留部分输入与释放义务；
/// - 路径完成但释放失败 → 分别记录"路径完成"与"释放未完成"；
/// - 失联/记录不可信/没有任何事实 → 维持未知（可能已发送 + 释放未知），不推断零输入。
#[must_use]
pub fn helper_failure_receipt(action_id: &str, failure: &StrokeFailure) -> ActionReceipt {
    let receipt = ActionReceipt {
        action_id: action_id.to_string(),
        input_delivery: failure.delivery.input_delivery,
        partial: failure.delivery.partial,
        path_completed: failure.delivery.path_completed,
        confirmed_point_count: failure.delivery.confirmed_point_count,
        effect: EffectStatus::NotObserved,
        goal_verdict: GoalVerdict::NotChecked,
        input_release: failure.release_state.release_status(),
    };
    debug_assert!(
        validate_receipt_for_write(&receipt).is_ok(),
        "helper 事实必须能构成可写入的自洽回执：{receipt:?}"
    );
    receipt
}

/// 成功路径的回执：helper 已确认走完整条路径并释放。
#[must_use]
pub fn helper_success_receipt(action_id: &str, facts: HelperInputFacts) -> ActionReceipt {
    let receipt = ActionReceipt {
        action_id: action_id.to_string(),
        input_delivery: InputDelivery::Sent,
        partial: Some(!facts.path_completed),
        path_completed: Some(facts.path_completed),
        confirmed_point_count: Some(facts.injected_points),
        effect: EffectStatus::NotObserved,
        goal_verdict: GoalVerdict::NotChecked,
        input_release: if facts.released == Some(true) {
            InputReleaseStatus::Released
        } else {
            InputReleaseStatus::Unknown
        },
    };
    debug_assert!(
        validate_receipt_for_write(&receipt).is_ok(),
        "成功路径事实必须能构成可写入的自洽回执：{receipt:?}"
    );
    receipt
}

/// 取消会通知 helper；helper 的 finally 总会尝试释放鼠标，Escape 也可取消。
pub fn controlled_drag_path(
    window: StrokeWindow,
    bounds: [i32; 4],
    points: &[MousePoint],
    duration_ms: u64,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<HelperInputFacts, StrokeFailure> {
    controlled_drag_path_impl(window, bounds, points, duration_ms, timeout, cancelled, None).map(|outcome| outcome.facts)
}

#[derive(Debug)]
pub struct NativeStrokeOutcome {
    pub facts: HelperInputFacts,
    pub helper_process: Option<runtime::ProcessInstanceEvidence>,
    pub process_exit_confirmed: bool,
}

pub fn controlled_drag_path_authorized(
    window: StrokeWindow,
    bounds: [i32; 4],
    points: &[MousePoint],
    duration_ms: u64,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
    authorization: &dyn NativeInputAuthorization,
) -> Result<NativeStrokeOutcome, StrokeFailure> {
    controlled_drag_path_impl(window, bounds, points, duration_ms, timeout, cancelled, Some(authorization))
}

fn controlled_drag_path_impl(
    window: StrokeWindow,
    bounds: [i32; 4],
    points: &[MousePoint],
    duration_ms: u64,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
    authorization: Option<&dyn NativeInputAuthorization>,
) -> Result<NativeStrokeOutcome, StrokeFailure> {
    validate_stroke(window, bounds, points, duration_ms).map_err(StrokeFailure::before_input)?;
    if cancelled() {
        return Err(StrokeFailure::before_input("stroke_cancelled: 输入开始前已取消"));
    }
    if !cfg!(test) && authorization.is_none() {
        return Err(StrokeFailure::before_input("input_authorization_required: 桌面笔画尚未绑定宿主执行许可"));
    }
    let run = run_helper(
        serde_json::json!({"mode":"stroke", "window":window, "bounds":bounds, "points":points, "duration_ms":duration_ms}),
        timeout,
        cancelled,
        StrokeRunCapacity::OrdinaryAction,
        authorization,
    )?;
    let facts = confirmed_success_facts(points.len(), &run.request_id, run.fact_read)
        .map_err(|mut failure| { failure.helper_process = run.helper_process; failure })?;
    Ok(NativeStrokeOutcome { facts, helper_process: run.helper_process, process_exit_confirmed: true })
}

const MAX_CAPTURE_PIXELS: i64 = 16_777_216;
// 32 位像素的 PNG（含编码开销）限定在 80 MiB；大图走临时文件，不占通用 stdout 缓冲。
const MAX_CAPTURE_PNG_BYTES: u64 = 80 * 1024 * 1024;

struct CaptureSpool {
    directory: PathBuf,
    image: PathBuf,
}

impl CaptureSpool {
    fn new() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let nonce = format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        );
        let directory = std::env::temp_dir().join(format!("coolzhu-capture-{nonce}"));
        std::fs::create_dir(&directory)
            .map_err(|error| format!("无法建立截图临时目录: {error}"))?;
        let image = directory.join("image.png");
        Ok(Self { directory, image })
    }

    fn cleanup(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.image) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("截图临时文件清理失败: {error}")),
        }
        std::fs::remove_dir(&self.directory)
            .map_err(|error| format!("截图临时目录清理失败: {error}"))
    }
}

impl Drop for CaptureSpool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.image);
        let _ = std::fs::remove_dir(&self.directory);
    }
}

/// 只截取当前已绑定窗口的真实屏幕像素，不绘制或导入任何图片。
///
/// 截图模式不注入任何输入，因此不存在需要保留的输入事实；失败只保留文本。
pub fn capture_window_image(
    window: StrokeWindow,
    timeout: Duration,
) -> Result<serde_json::Value, String> {
    if window.rect[2] <= 0
        || window.rect[3] <= 0
        || i64::from(window.rect[2]) * i64::from(window.rect[3]) > MAX_CAPTURE_PIXELS
    {
        return Err("窗口截图尺寸无效或超过 1600 万像素".into());
    }
    let spool = CaptureSpool::new()?;
    let run = run_helper(
        serde_json::json!({"mode":"capture", "window":window, "capture_file":spool.image}),
        timeout,
        &|| false,
        StrokeRunCapacity::OrdinaryAction,
        None,
    ).map_err(|failure| failure.message);
    // run_helper 的所有出口均已等到 helper 退出（异常出口会先 kill + wait），
    // 此时才能读回和删除可能含桌面内容的临时文件。
    let result = run.and_then(|run| {
        if run.pipe.is_some_and(|facts| facts.truncated) {
            return Err(format!("窗口截图回执被管道截断（缺口：{:?}）", run.pipe));
        }
        // 截图结果优先用**增量协议记录**（不依赖 EOF）：孙进程持有管道写端时，
        // "等整段文本"会拿不到结果，而已经成行到达的记录仍然可用。
        if let Some(record) = run.capture_record {
            return hydrate_capture_record(record, &spool.image, window);
        }
        // 没有可解析的完整记录：如实报缺口，**不**把"没有记录"当成"零尺寸截图"。
        let pipe = run
            .pipe
            .map(|facts| format!("{facts:?}"))
            .unwrap_or_else(|| "无管道事实".to_string());
        let record = serde_json::from_str(run.output.trim()).map_err(|error| {
            format!("窗口截图结果无效（缺口：{pipe}）: {error}")
        })?;
        hydrate_capture_record(record, &spool.image, window)
    });
    let cleanup = spool.cleanup();
    match (result, cleanup) {
        (_, Err(error)) => Err(error),
        (result, Ok(())) => result,
    }
}

fn hydrate_capture_record(
    mut record: serde_json::Value,
    image_path: &Path,
    window: StrokeWindow,
) -> Result<serde_json::Value, String> {
    let width = record["width"].as_i64().ok_or("截图回执缺少宽度")?;
    let height = record["height"].as_i64().ok_or("截图回执缺少高度")?;
    let length = record["bytes_len"].as_u64().ok_or("截图回执缺少字节数")?;
    let expected_hash = record["sha256"].as_str().ok_or("截图回执缺少 SHA-256")?;
    let screen_rect = record["screen_rect"].as_array().ok_or("截图回执缺少屏幕边界")?;
    let client_rect = record["client_rect"].as_array().ok_or("截图回执缺少客户区边界")?;
    if width <= 0 || height <= 0 || width > i64::from(window.rect[2])
        || height > i64::from(window.rect[3]) || width * height > MAX_CAPTURE_PIXELS
        || length == 0 || length > MAX_CAPTURE_PNG_BYTES
        || expected_hash.len() != 64 || !expected_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        || screen_rect.len() != 4 || screen_rect[2].as_i64() != Some(width)
        || screen_rect[3].as_i64() != Some(height)
        || !screen_rect.iter().all(|part| part.as_i64().is_some())
        || client_rect.len() != 4 || !client_rect.iter().all(|part| part.as_i64().is_some())
    {
        return Err("截图回执尺寸、边界或摘要无效".into());
    }
    let file = std::fs::File::open(image_path)
        .map_err(|error| format!("截图临时文件不可读: {error}"))?;
    let mut bytes = Vec::new();
    file.take(MAX_CAPTURE_PNG_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("截图临时文件读取失败: {error}"))?;
    if bytes.len() as u64 != length || bytes.len() < 24
        || &bytes[..8] != b"\x89PNG\r\n\x1a\n"
        || &bytes[12..16] != b"IHDR"
        || u32::from_be_bytes(bytes[16..20].try_into().unwrap_or_default()) as i64 != width
        || u32::from_be_bytes(bytes[20..24].try_into().unwrap_or_default()) as i64 != height
    {
        return Err("截图临时文件长度或 PNG 尺寸与回执不符".into());
    }
    let actual_hash = format!("{:x}", Sha256::digest(&bytes));
    if !actual_hash.eq_ignore_ascii_case(expected_hash) {
        return Err("截图临时文件 SHA-256 与回执不符".into());
    }
    record["data_url"] = serde_json::json!(format!("data:image/png;base64,{}", BASE64_STANDARD.encode(&bytes)));
    Ok(record)
}

/// 一次受控笔画 helper 运行的读取器容量来源（§C-21 选②）。
///
/// 与原生输入路径同一套语义：**普通动作**在 `spawn` 之前一次性预留
/// "本 helper 两条流 + 可能的一次清理（独立释放）"；**清理运行**转用上层已经预留的
/// 那一份，不再竞争普通容量，也不再预留下一层清理额度。
#[derive(Debug)]
enum StrokeRunCapacity {
    OrdinaryAction,
    CleanupRun(HelperCleanupReservation),
}

/// 一次受控 helper 调用的成功结果。
#[derive(Debug)]
struct HelperRun {
    helper_process: Option<runtime::ProcessInstanceEvidence>,
    output: String,
    /// 本次请求的身份（helper 必须在进度记录里原样回写它）。
    request_id: String,
    /// 进度记录的读取结果：可信事实 / 不可信（带原因）/ 没有记录。
    fact_read: HelperFactRead<HelperInputFacts>,
    /// 从**已经收到**的协议记录里增量解析出的截图结果（不依赖 EOF）；
    /// 没有可解析的记录时为 `None`。
    capture_record: Option<serde_json::Value>,
    /// 管道收尾事实：读取线程是否**核实**结束、有没有证据缺口。
    pipe: Option<super::PipeSupervisionFacts>,
}

/// 从"已经收到的协议记录（完整行）"里取截图结果：最后一条能解析成 JSON 的记录。
///
/// 只认**已成行**的记录：末尾的半行可能是被截断的半个回执，不能当成功回执用。
fn capture_record_from_lines(lines: &[String]) -> Option<serde_json::Value> {
    lines
        .iter()
        .rev()
        .find_map(|line| serde_json::from_str(line).ok())
}

/// 受监督地运行受控 helper（Windows 实现）。
///
/// 等待链只有三段，且每段都有界：**进程结束状态**（`try_wait`）、
/// **协作退出/强杀**（`min(2 秒, 剩余收尾时间)`）、**管道收尾**（`min(诊断片长, 剩余收尾时间)`，
/// 只核实读取线程是否结束，不等待任何孙进程）。
#[cfg(windows)]
fn run_helper(
    mut request: serde_json::Value,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
    capacity: StrokeRunCapacity,
    authorization: Option<&dyn NativeInputAuthorization>,
) -> Result<HelperRun, StrokeFailure> {
    if !cfg!(windows) {
        return Err(StrokeFailure::before_input("受控桌面输入仅支持 Windows"));
    }
    // 测试专用（生产构建不编译）：受控笔画同样占用读取器容量，先过进程级测试闸门，
    // 避免并行用例互相挤爆读取器容量（普通动作按"两条流 + 一次清理"计）。
    #[cfg(test)]
    let _test_pipe_slot = super::test_pipe_reader_capacity_slot(super::NATIVE_RUN_READER_UNITS);
    // ------------------------------------------------------------------
    // 接纳顺序（§C-21 选②）：资格校验 → **计算最大 reader 需求** → **一次性预留** →
    // 创建并监督 helper → 创建读取器 → 最终输入前检查 → 允许业务输入。
    //
    // 容量不足时**连进程都不创建**；普通动作连"可能的一次清理（独立释放）"所需的容量
    // 一起预留，清理运行**转用**它（不重新竞争普通容量、不递归预留下一层）。
    let (mut admission, cleanup_reservation) = match capacity {
        StrokeRunCapacity::OrdinaryAction => {
            let mut admission = HelperPipeReadersAdmission::acquire_with_cleanup_reserve()
                .map_err(|denial| {
                    StrokeFailure::before_input(format!(
                        "受控桌面输入 helper 读取器容量不足（未创建进程、未产生输入）: {denial}"
                    ))
                })?;
            let cleanup = admission.take_cleanup_reservation();
            (admission, cleanup)
        }
        StrokeRunCapacity::CleanupRun(reserved) => (reserved.into_admission(), None),
    };
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    // 请求身份：宿主生成、随请求下发、由 helper 原样回写，用于核对"记录属于本动作"。
    let request_id = format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let cancel_file = std::env::temp_dir().join(format!("coolzhu-stroke-cancel-{request_id}"));
    let mut prepared = authorization.map(|_| PreparedInputSession::new(&request_id, timeout));
    // 进度文件只承载 helper 自己写出的输入事实；删除发生在读完事实之后。
    let progress_file = cancel_file.with_extension("progress.json");
    request["request_id"] = serde_json::json!(request_id);
    request["cancel_file"] = serde_json::json!(cancel_file.to_string_lossy());
    request["progress_file"] = serde_json::json!(progress_file.to_string_lossy());
    if let Some(prepared) = prepared.as_ref() { prepared.apply_request(&mut request); }
    let script = format!("$ErrorActionPreference='Stop'; $OutputEncoding=[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false); Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'\n{}\n'@\n{}", include_str!("input_stroke_native.cs"), HELPER_ENTRY);
    let mut command = Command::new("powershell.exe");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let (child, job) = windows_process_guard::ChildProcessJob::spawn_managed(&mut command)
        .map_err(|error| StrokeFailure::before_input(format!("无法启动受控输入 helper: {error}")))?;
    let mut child = SupervisedHelperChild::new(child, job, &cancel_file);
    let helper_identity = windows_process_guard::capture_child_process_identity(&child).ok();
    let helper_process = helper_identity.as_ref().map(|identity| runtime::ProcessInstanceEvidence {
        pid: identity.pid(), creation_time_filetime: identity.creation_time_filetime(),
    });
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(StrokeFailure::before_input("helper 标准管道不可用"));
    };
    // 读取线程由原生监督器持有（`windows-process-guard` 的有界管道读取器）：
    // 只看"已经可读"的字节，**不发出无界阻塞的读取**，因此读取线程的退出条件
    // 与"孙进程是否仍持有管道写端"无关。旧实现的无界 `join()` 正是在这里被拖住：
    // 被强杀的 helper 留下的孙进程（`Add-Type` 的编译器）继续持有写端 ⇒ 读取线程拿不到 EOF。
    // 两条流从**凭证**里各带走 1 个单位：容量已在本函数开头（spawn 之前）预留。
    let out_reader =
        match super::helper_pipes::supervise_stdout_from(&mut admission, "stroke-helper-stdout", stdout) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(StrokeFailure::before_input(error));
            }
        };
    let err_reader =
        match super::helper_pipes::supervise_stderr_from(&mut admission, "stroke-helper-stderr", stderr) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                // out_reader 未被消费：它的 Drop 会把未核实的读取转入有上限的残留登记，不失管。
                return Err(StrokeFailure::before_input(error));
            }
        };
    if let Err(error) = stdin.write_all(request.to_string().as_bytes()) {
        let _ = child.kill();
        let _ = child.wait();
        drop(stdin);
        // 有界收尾两个读取线程：读到多少算多少，**不等待孙进程自然退出**。
        let pipes = super::helper_pipes::drain(out_reader, err_reader, None, input_cleanup_policy());
        let _ = std::fs::remove_file(&progress_file);
        if request["mode"] == "stroke" {
            // 请求没送到 helper（写入不完整 ⇒ helper 连 JSON 都解析不出）：这是**可证明**的
            // 零输入，因此**不**启动任何补发；收尾事实如实写"无需释放"。
            // 旧实现无条件补发一次独立释放——那正是"在结构事实之外额外发 UP"的做法，
            // 而"未按下时多发 UP 无害"不能当作共享桌面上的普遍安全假设。
            let policy = input_cleanup_policy();
            let deadline = CleanupDeadline::establish(policy, unix_ms());
            let wait = deadline.independent_release_wait_at(Instant::now());
            let finished_at_ms = unix_ms();
            let cleanup = HelperCleanupFacts {
                stopped_new_input_at_ms: deadline.started_at_unix_ms(),
                cleanup_started_at_ms: deadline.started_at_unix_ms(),
                cleanup_finished_at_ms: finished_at_ms,
                cooperative_exit_waited_ms: 0,
                forced_kill: true,
                independent_release_issued: false,
                independent_release_wait_ms: wait.as_millis().min(u128::from(u64::MAX)) as u64,
                independent_release_confirmed: false,
                independent_release_skipped_window_expired: false,
                release: CleanupReleaseStatus::NotNeeded,
                pipe: Some(pipes.facts),
            };
            return Err(StrokeFailure::before_input(format!(
                "输入 helper 请求失败: {error}"
            ))
            .with_cleanup(cleanup));
        }
        return Err(StrokeFailure::before_input(format!(
            "输入 helper 请求失败: {error}"
        )));
    }
    drop(stdin);
    // 请求已经送到 helper：从这里开始不能再声称"零输入"。
    let started = Instant::now();
    let policy = input_cleanup_policy();
    // 第一次进入取消/异常收尾时建立**唯一**的收尾截止时间；
    // 之后循环里再看到取消/到期信号时复用它，**不得刷新**（否则收尾窗口可以被续期）。
    let mut cleanup_deadline: Option<CleanupDeadline> = None;
    let mut cancellation_at = None;
    let mut cooperative_exit_waited = Duration::ZERO;
    let mut forced_kill = false;
    // 截图模式的协议记录：轮询期间增量收下（不依赖 EOF），收尾后再补一次。
    let capture_mode = request["mode"] == "capture";
    let mut capture_record: Option<serde_json::Value> = None;
    let mut authorization_error = None;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                forced_kill = true;
                break Err(error.to_string());
            }
        }
        // 增量协议记录（**不依赖 EOF**）：截图模式的成功回执是一条压缩 JSON 记录，
        // 只要它已经成行到达，就先收下——即使孙进程仍持有管道写端、读取线程还没见到 EOF。
        if authorization_error.is_none() {
            if let (Some(prepared), Some(authorization)) = (prepared.as_mut(), authorization) {
                if let Err(error) = prepared.poll(authorization, helper_identity.as_ref(), &script, cancelled, &mut child) {
                    authorization_error = Some(error);
                }
            }
        }
        if capture_record.is_none() && capture_mode {
            let snapshot = super::helper_pipes::snapshot_text(&out_reader);
            capture_record = capture_record_from_lines(&super::complete_lines(snapshot.as_bytes()));
        }
        if cancellation_at.is_none() && (authorization_error.is_some() || cancelled() || started.elapsed() >= timeout) {
            // 独立哨兵文件只承载取消信号，不接收任何用户代码或命令。
            let _ = std::fs::write(&cancel_file, b"cancel");
            cancellation_at = Some(Instant::now());
            // 业务期限到期/收到取消只意味着"停止接纳新业务输入"：
            // 收尾窗口从这一刻起算，且只建立一次。
            let fixed = CleanupDeadline::fixed(&mut cleanup_deadline, policy, unix_ms());
            cleanup_deadline = Some(fixed);
        }
        if let (Some(at), Some(deadline)) = (cancellation_at, cleanup_deadline) {
            let now = Instant::now();
            cooperative_exit_waited = now.saturating_duration_since(at);
            // 协作退出等待 = min(2 秒, 剩余收尾时间)。helper 一旦自己退出，
            // 上面的 try_wait 会先一步结束循环——没有输入或释放义务时不强制等满。
            if at.elapsed() >= deadline.cooperative_exit_grace_at(now) {
                let _ = child.kill();
                forced_kill = true;
                break child.wait().map_err(|error| error.to_string());
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    // 有界收尾两个读取线程：**不等待孙进程自然退出**，只做有界等待并核实读取线程结束。
    // 等待额度取 `min(诊断片长, 剩余收尾时间)`，与协作退出／独立释放共同消耗同一个窗口。
    // 旧实现在这里是**对两个读取线程的无界 join**（等待它们读到 EOF），
    // 孙进程持管道时"四秒收尾"因此在机制上失效。
    let pipes = super::helper_pipes::drain(out_reader, err_reader, cleanup_deadline, policy);
    // 收尾之后仍没有增量记录时，从**已经收到**的字节里再解析一次（同样不依赖 EOF）。
    if capture_record.is_none() && capture_mode {
        capture_record = capture_record_from_lines(&pipes.stdout_records);
    }
    let pipe_facts = pipes.facts;
    let output = pipes.stdout.as_str();
    let errors_text = pipes.stderr.as_str();
    // 事实必须在删除进度文件之前取出；读取结果区分"没有记录"与"记录不可信"。
    let fact_read = read_helper_facts(&progress_file, &request_id);
    let _ = std::fs::remove_file(&progress_file);
    let _ = std::fs::remove_file(&cancel_file);
    let helper_reported_release_failure = errors_text.contains("mouse_release_failed");
    let helper_exited_ok = matches!(&status, Ok(status) if status.success());
    let helper_reported_failure = !errors_text.trim().is_empty();
    let stroke_mode = request["mode"] == "stroke";
    // 笔画路径总是把子进程收起（`wait` 给出结束状态）；读不出结束状态就不算确认静止。
    let exit_confirmed = matches!(&status, Ok(_));
    // ---------------------------------------------------------------------
    // 正推顺序（裁决 §2）：收集原始结果与回执 → 校验身份/协议/完整性 → 推导投递事实
    // → 推导释放义务 → 有义务或可能有义务时执行受限收尾 → 合并收尾事实 → 生成回执与分类。
    // ---------------------------------------------------------------------
    // 原始执行原因只看 helper 自己的报告与进程事实：补发结果属于另一个维度，不得覆盖它。
    let cause = StrokeFailureCause::classify(
        helper_reported_release_failure,
        cancellation_at.is_some(),
        forced_kill,
        helper_exited_ok,
        is_helper_lost(forced_kill, helper_exited_ok, helper_reported_failure),
    );
    let input_possible = prepared.as_ref().is_none_or(|prepared| prepared.notified);
    let derivation_inputs = StrokeFailure::derivation_inputs_for(
        cause.kind(),
        input_possible,
        stroke_mode,
        &fact_read,
        helper_reported_release_failure,
        exit_confirmed,
        false,
    );
    let before_cleanup = derive_before_cleanup(&derivation_inputs);
    let release_needed = needs_emergency_release(
        stroke_mode,
        before_cleanup,
        exit_confirmed,
        forced_kill,
        helper_exited_ok,
    );
    // 独立（补发）释放**最多一次**，等待上限 = min(2 秒, 剩余收尾时间)。
    if cleanup_deadline.is_none() && (forced_kill || release_needed) {
        // "需收尾错误"同样属于第一次进入收尾：在这里固定收尾截止时间。
        cleanup_deadline = Some(CleanupDeadline::establish(policy, unix_ms()));
    }
    let mut independent_release_issued = false;
    let mut independent_release_wait = Duration::ZERO;
    let mut independent_release_confirmed = false;
    let mut independent_release_skipped_window_expired = false;
    if release_needed {
        let wait = cleanup_deadline.map_or(policy.independent_release_wait_cap(), |deadline| {
            deadline.independent_release_wait_at(Instant::now())
        });
        if wait.is_zero() {
            // 收尾窗口已到期：不延长、不重试循环，直接按"未确认释放"隔离。
            independent_release_skipped_window_expired = true;
        } else {
            independent_release_issued = true;
            independent_release_wait = wait;
            // 独立释放**转用**接纳时已经预留的清理容量（不重新竞争普通容量）。
            match emergency_release(wait, cleanup_reservation) {
                Ok(()) => independent_release_confirmed = true,
                // 补发失败**不**改写原始执行原因，也不在这里拼一条清理错误串：
                // 释放义务的最终状态（未结清 ⇒ 外部按安全阻断）由收尾事实表达，
                // 分类与恢复建议由调用链按两个维度合并（见 `computer_use_desktop_bridge`）。
                Err(_) => {}
            }
        }
    }
    // 合并收尾事实，得到**最终**释放义务状态（唯一推导点）。
    let release_state = derive_after_cleanup(&derivation_inputs, independent_release_confirmed);
    // 收尾报告：只记录能确认的事实；确认不了就明确写未确认。
    // 收尾报告里的释放状态与回执同源（同一个推导），不再各写一套条件。
    let release = cleanup_release_status_of(release_state.release_status());
    let cleanup_facts = cleanup_deadline.map(|deadline| HelperCleanupFacts {
        stopped_new_input_at_ms: deadline.started_at_unix_ms(),
        cleanup_started_at_ms: deadline.started_at_unix_ms(),
        cleanup_finished_at_ms: unix_ms(),
        cooperative_exit_waited_ms: cooperative_exit_waited
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
        forced_kill,
        independent_release_issued,
        independent_release_wait_ms: independent_release_wait
            .as_millis()
            .min(u128::from(u64::MAX)) as u64,
        independent_release_confirmed,
        independent_release_skipped_window_expired,
        release,
        // 管道收尾事实：与协议事实分开记录。未核实结束只记监督/I/O 故障，
        // **不**据此改写释放义务（不伪造"释放未知"）。
        pipe: Some(pipe_facts),
    });
    if let Some(authorization) = authorization {
        if let Err(error) = authorization.completed(&NativeInputCompletion {
            request_id: request_id.clone(), process: helper_process,
            execute_notified: prepared.as_ref().is_some_and(|prepared| prepared.notified),
            process_exit_confirmed: exit_confirmed,
            input_release: release_state.release_status(),
            trusted_final: fact_read.trusted().is_some_and(|facts| facts.phase == HelperFactPhase::Final),
            error: authorization_error.clone().or_else(|| cause.message(&errors_text, stroke_mode)),
        }) { authorization_error = Some(format!("输入完成对账失败: {error}")); }
    }
    if let Some(message) = authorization_error.map(|error| format!("input_authorization_failed: {error}"))
        .or_else(|| cause.message(&errors_text, stroke_mode)) {
        let mut failure = StrokeFailure::derive(
            message,
            input_possible,
            fact_read,
            helper_reported_release_failure,
            exit_confirmed,
            independent_release_confirmed,
        );
        failure.helper_process = helper_process;
        return Err(match cleanup_facts {
            Some(cleanup) => failure.with_cleanup(cleanup),
            None => failure,
        });
    }
    // 没有失败：返回这次运行的输出与事实（成功路径的事实确认由调用方负责）。
    // `output` 只作诊断用途保留（已收到的字节按 UTF-8 宽松解码）；
    // 截图结果优先用**增量**协议记录，不用"等 EOF 才拿得到"的整段文本。
    Ok(HelperRun {
        helper_process,
        output: output.to_string(),
        request_id,
        fact_read,
        capture_record,
        pipe: Some(pipe_facts),
    })
}

/// 非 Windows：受控桌面输入不可用（不会走到任何进程/管道路径）。
#[cfg(not(windows))]
fn run_helper(
    _request: serde_json::Value,
    _timeout: Duration,
    _cancelled: &dyn Fn() -> bool,
    _capacity: StrokeRunCapacity,
    _authorization: Option<&dyn NativeInputAuthorization>,
) -> Result<HelperRun, StrokeFailure> {
    Err(StrokeFailure::before_input("受控桌面输入仅支持 Windows"))
}

/// 原始执行原因：只看 helper 自己的报告与进程事实（补发结果属于"安全收尾"维度）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrokeFailureCause {
    /// helper 自己报告释放失败。
    ReleaseReportedFailed,
    /// 已取消或超时。
    Cancelled,
    /// 执行者失联。
    HelperLost,
    /// 其它非零退出。
    Failed,
    /// 没有失败。
    None,
}

impl StrokeFailureCause {
    fn classify(
        release_reported_failed: bool,
        cancelled: bool,
        _forced_kill: bool,
        helper_exited_ok: bool,
        helper_lost: bool,
    ) -> Self {
        if release_reported_failed {
            Self::ReleaseReportedFailed
        } else if cancelled {
            Self::Cancelled
        } else if helper_lost {
            Self::HelperLost
        } else if !helper_exited_ok {
            Self::Failed
        } else {
            Self::None
        }
    }

    /// 推导用的分类（`None` = 没有失败，按"非失联"处理）。
    #[must_use]
    const fn kind(self) -> StrokeFailureKind {
        match self {
            Self::ReleaseReportedFailed => StrokeFailureKind::ReleaseUnconfirmed,
            Self::Cancelled => StrokeFailureKind::Cancelled,
            Self::HelperLost => StrokeFailureKind::HelperLost,
            Self::Failed | Self::None => StrokeFailureKind::Failed,
        }
    }

    /// 失败文本；没有失败时返回 `None`。
    fn message(self, stderr: &str, stroke_mode: bool) -> Option<String> {
        match self {
            Self::None => None,
            Self::ReleaseReportedFailed => {
                Some(format!("mouse_release_failed: {}", stderr.trim()))
            }
            Self::Cancelled => Some("stroke_cancelled: 已取消或超时".to_string()),
            Self::HelperLost => Some(
                "helper_lost: 受控输入 helper 未正常结束且未给出原因".to_string(),
            ),
            Self::Failed => Some(format!(
                "{}: {}",
                if stroke_mode {
                    "受控桌面操作失败"
                } else {
                    "受控输入 helper 失败"
                },
                stderr.trim()
            )),
        }
    }
}

/// 是否需要由本进程独立补发一次释放（不依赖 helper 的 `finally`）。
///
/// 判定**消费事实推导出的五态**，不再自己拼字段条件：
/// - 已证明不存在（"从未输入／从未按下"的可信证明）→ **不补发**：不给可证明零输入
///   的运行发多余 UP；
/// - 已产生且已结清 → 不重复补发；
/// - 已产生且未结清 → 在既定安全条件（被强杀 / 以非零状态退出）下补发**一次**；
/// - 可能存在 / 证据冲突 → 先确认旧执行者静止，满足条件才补发，否则隔离；
/// - 释放通道自身（`mode = release`）永不递归补发。
///
/// "helper 自己报过释放失败"不再作为**跳过**补发的理由：旧实现会因此完全放弃收尾，
/// 让"明确失败"变成"什么都不做"。它由五态表达为未结清／证据冲突，仍然最多一次。
fn needs_emergency_release(
    is_stroke: bool,
    state: ReleaseObligationState,
    stillness_confirmed: bool,
    forced_kill: bool,
    helper_exited_ok: bool,
) -> bool {
    if !is_stroke || !state.requires_controlled_cleanup() {
        return false;
    }
    if state.requires_stillness_before_cleanup() && !stillness_confirmed {
        return false;
    }
    forced_kill || !helper_exited_ok
}

/// 是否属于"执行者失联"。
///
/// 注意非零退出**不等于**失联：helper 主动报错时同样是 `throw` 后非零退出，
/// 并把原因写进 stderr——那属于"报告了失败"，应保留原有的分类（stale/取消/泛化失败）。
/// 因此只有两种情形算失联：被本进程强杀，或非零退出却**没有给出任何原因**。
fn is_helper_lost(forced_kill: bool, helper_exited_ok: bool, helper_reported_failure: bool) -> bool {
    forced_kill || (!helper_exited_ok && !helper_reported_failure)
}

// 单测观测点：本线程"独立（补发）释放"的启动次数与"是否强制失败"。
//
// **只有单测**写它。生产逻辑除了在 emergency_release 里累加计数、并按注入值让这次
// 补发直接失败之外，不读也不依赖它；任何生产判定都不会因为它的存在而改变。
#[cfg(test)]
thread_local! {
    static RELEASE_PROBE: std::cell::Cell<(u32, u8)> = const { std::cell::Cell::new((0, 0)) };
}

/// 补发通道的注入模式：0 = 真实执行（默认）/ 必失败 / 必成功。
#[cfg(test)]
const RELEASE_PROBE_FORCE_FAILURE: u8 = 1;
#[cfg(test)]
const RELEASE_PROBE_FORCE_SUCCESS: u8 = 2;

/// 重置本线程的补发观测点。
#[cfg(test)]
fn release_probe_reset(mode: u8) {
    RELEASE_PROBE.with(|probe| probe.set((0, mode)));
}

/// 本线程已经启动过多少次独立（补发）释放。
#[cfg(test)]
fn release_probe_count() -> u32 {
    RELEASE_PROBE.with(|probe| probe.get().0)
}

/// 记一次补发尝试，返回这次使用的注入模式。
#[cfg(test)]
fn release_probe_record_attempt() -> u8 {
    RELEASE_PROBE.with(|probe| {
        let (count, mode) = probe.get();
        probe.set((count + 1, mode));
        mode
    })
}

/// 独立（补发）释放：**最多一次**，等待上限由调用方按 `min(2 秒, 剩余收尾时间)` 给出。
///
/// 只释放左键，绝不移动光标；递归仅在 stroke 模式发生，此分支为 release。
/// 这里**不承诺释放成功**：失败会把"未确认释放"如实返回，交上层隔离。
///
/// 它是**真实的清理动作**（`SafetyCleanup`）：结果作为独立收尾事实记录，
/// 关联它试图结清的原动作；它自己不会执行 Down，因此**不**表示旧义务已结清——
/// 结清与否只看释放义务状态，不看"这次 UP 有没有发出去"。
fn emergency_release(
    timeout: Duration,
    reserved: Option<HelperCleanupReservation>,
) -> Result<(), StrokeFailure> {
    #[cfg(test)]
    match release_probe_record_attempt() {
        RELEASE_PROBE_FORCE_FAILURE => {
            return Err(StrokeFailure::before_input(
                "mouse_release_failed: 独立释放被单测注入为必失败",
            ))
        }
        RELEASE_PROBE_FORCE_SUCCESS => return Ok(()),
        _ => {}
    }
    // 清理运行只允许"转用上层预留"这一条容量来源：没有预留就不去抢普通容量。
    let capacity = match reserved {
        Some(reserved) => StrokeRunCapacity::CleanupRun(reserved),
        None => {
            return Err(StrokeFailure::before_input(
                "cleanup_reader_capacity_unavailable: 独立释放没有可用的清理预留",
            ))
        }
    };
    run_helper(
        serde_json::json!({"mode":"release"}),
        timeout,
        &|| false,
        capacity,
        None,
    )
        .map(|_| ())
        .map_err(|failure| StrokeFailure {
            // 独立释放失败不注入路径输入，但内层事实（如果有）不得被丢弃。
            message: format!("mouse_release_failed: 独立释放失败: {}", failure.message),
            helper_process: failure.helper_process,
            input_possible: failure.input_possible,
            fact_read: failure.fact_read,
            helper_reported_release_failure: failure.helper_reported_release_failure,
            stillness_confirmed: failure.stillness_confirmed,
            delivery: failure.delivery,
            // 派生视图随事实一起传递：不得在此重算（唯一判定点在 `derive_input_status`）。
            input_status: failure.input_status,
            release_state: failure.release_state,
            cleanup: failure.cleanup,
        })
}

const HELPER_ENTRY: &str = r#"
$r=[Console]::In.ReadToEnd()|ConvertFrom-Json
if($r.mode -eq 'release') { [CoolzhuStroke.Native]::EmergencyRelease(); 'released'; exit }
$progress=$null
if($r.progress_file){ $progress=[CoolzhuStroke.FileProgress]::new([string]$r.progress_file,[string]$r.request_id) }
if($r.two_phase_helper){
 $q=[string]$r.ready_file;$m=[string]$r.permit_file;$n=[string]$r.two_phase_nonce
 function DenyStrokeInput($e){if($progress){$progress.Report('final',$false,0,$false,$false,$true)};throw $e}
 if(!$q -or !$m -or !$n -or !$r.input_deadline_unix_ms){DenyStrokeInput 'rejected_permit: missing handshake fields'}
 [IO.File]::WriteAllText($q+'.tmp',(@{type='ready';helper_protocol_version=1;nonce=$n;pid=$PID;timestamp_unix_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()}|ConvertTo-Json -Compress));[IO.File]::Move($q+'.tmp',$q)
 while($true){
  if((Test-Path -LiteralPath ([string]$r.cancel_file)) -or [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() -ge [long]$r.input_deadline_unix_ms){DenyStrokeInput 'stroke_cancelled: awaiting permit'}
  if(Test-Path -LiteralPath $m){
   try{$v=[IO.File]::ReadAllText($m)|ConvertFrom-Json}catch{DenyStrokeInput 'rejected_permit: invalid JSON'}
   if($v.type -cne 'execute' -or $v.nonce -cne $n -or [string]::IsNullOrWhiteSpace($v.permit_id) -or [string]::IsNullOrWhiteSpace($v.attempt_id) -or [string]::IsNullOrWhiteSpace($v.executor_instance_id)){DenyStrokeInput 'rejected_permit: invalid session'}
   if(!$v.expires_at_unix_ms){DenyStrokeInput 'rejected_permit: missing expiry'}
   [CoolzhuStroke.Deadline]::At=[Math]::Min([long]$r.input_deadline_unix_ms,[long]$v.expires_at_unix_ms);[CoolzhuStroke.Deadline]::Check()
   break
  };Start-Sleep -Milli 20
 }
}
if($r.mock_scenario){ [CoolzhuStroke.MockChecks]::RunScenario([string]$r.mock_scenario,[string]$r.progress_file,[string]$r.request_id); 'released'; exit }
$w=$r.window
$native=[CoolzhuStroke.Native]::new([long]$w.handle,[uint32]$w.process_id,[int[]]$w.rect,[uint32]$w.dpi,[string]$r.cancel_file)
if($r.mode -eq 'capture') { $native.Capture([string]$r.capture_file)|ConvertTo-Json -Compress; exit }
if($r.mode -ne 'stroke') { throw 'unsupported helper mode' }
$points=@($r.points|ForEach-Object{[CoolzhuStroke.Point]::new([int]$_.x,[int]$_.y)})
[CoolzhuStroke.Engine]::RunWithProgress($native,[CoolzhuStroke.Point[]]$points,[int[]]$r.bounds,[int]$r.duration_ms,$progress)
'released'
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComputerUseRetryOwner;

    #[test]
    #[cfg(windows)]
    fn capture_receipt_transfers_png_larger_than_pipe_limit() {
        let mut pixels = vec![0_u8; 512 * 512 * 4];
        let mut random = 0x1234_5678_u32;
        for byte in &mut pixels {
            random ^= random << 13;
            random ^= random >> 17;
            random ^= random << 5;
            *byte = random as u8;
        }
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, 512, 512);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().expect("PNG 头");
            writer.write_image_data(&pixels).expect("PNG 像素");
        }
        assert!(png_bytes.len() > 256 * 1024);
        let spool = CaptureSpool::new().expect("截图临时目录");
        std::fs::write(&spool.image, &png_bytes).expect("落盘 PNG");
        let metadata = serde_json::json!({
            "width":512,"height":512,"screen_rect":[0,0,512,512],
            "client_rect":[0,0,512,512],"bytes_len":png_bytes.len(),
            "sha256":format!("{:x}", Sha256::digest(&png_bytes)),
        });
        let stdout = format!("{metadata}\n");
        assert!(stdout.len() < 256 * 1024);
        let record = capture_record_from_lines(&super::super::complete_lines(stdout.as_bytes()))
            .expect("完整、无需 EOF 的元数据行");
        let window = StrokeWindow { handle: 1, process_id: 1, rect: [0,0,512,512], dpi: 96 };
        let image = hydrate_capture_record(record, &spool.image, window).expect("完整截图回执");
        let encoded = image["data_url"].as_str().unwrap().strip_prefix("data:image/png;base64,").unwrap();
        assert_eq!(BASE64_STANDARD.decode(encoded).unwrap(), png_bytes);
        drop(spool);
    }

    #[test]
    #[cfg(windows)]
    fn prepared_stroke_binding_waits_for_authorization_and_returns_same_identity() {
        use crate::prepared_input::{NativeInputPermit, PreparedNativeInput};
        struct Port { deny: bool, seen: std::cell::RefCell<Option<runtime::ProcessInstanceEvidence>> }
        impl NativeInputAuthorization for Port {
            fn authorize(&self, prepared: &PreparedNativeInput) -> Result<NativeInputPermit, String> {
                self.seen.replace(Some(prepared.process));
                if self.deny { return Err("笔画授权拒绝".into()) }
                Ok(NativeInputPermit { permit_id: "stroke-permit".into(), attempt_id: "stroke-attempt".into(), executor_instance_id: "stroke-executor".into(), expires_at_unix_ms: prepared.deadline_unix_ms })
            }
            fn dispatch(&self, _: &PreparedNativeInput, _: &NativeInputPermit, notify: &mut dyn FnMut() -> Result<(), String>) -> Result<(), String> { notify() }
            fn completed(&self, completion: &NativeInputCompletion) -> Result<(), String> {
                assert_eq!(completion.process, *self.seen.borrow());
                assert!(completion.process_exit_confirmed);
                assert!(completion.trusted_final);
                assert_eq!(completion.execute_notified, !self.deny);
                Ok(())
            }
        }
        for deny in [true, false] {
            let port = Port { deny, seen: Default::default() };
            let result = run_helper(serde_json::json!({"mode":"stroke","mock_scenario":"success"}), Duration::from_secs(20), &|| false, StrokeRunCapacity::OrdinaryAction, Some(&port));
            let identity = port.seen.borrow().expect("真实笔画 helper 必须先 READY");
            if deny {
                let failure = result.expect_err("未经授权不能画线");
                assert!(!failure.input_possible);
                assert!(failure.message.contains("笔画授权拒绝"));
                assert_eq!(failure.helper_process, Some(identity));
                assert_eq!(failure.release_state.release_status(), InputReleaseStatus::NotNeeded);
            } else {
                let run = result.expect("获准的内存笔画应成功");
                assert_eq!(run.helper_process, Some(identity));
                assert_eq!(run.fact_read.trusted().unwrap().injected_points, 3);
            }
        }
    }

    fn window() -> StrokeWindow {
        StrokeWindow {
            handle: 1,
            process_id: 2,
            rect: [-100, 50, 500, 300],
            dpi: 144,
        }
    }

    /// v2 最终事实的最小构造（`phase = final`，收尾已封闭）。
    fn final_facts(
        cursor_moved: bool,
        injected_points: u32,
        button_down: bool,
        path_completed: bool,
        released: Option<bool>,
    ) -> HelperInputFacts {
        HelperInputFacts {
            protocol: HELPER_FACT_PROTOCOL_V2,
            request_id: Some("req-1".to_string()),
            phase: HelperFactPhase::Final,
            cursor_moved: Some(cursor_moved),
            injected_points,
            button_down,
            path_completed,
            released,
        }
    }

    fn trusted(facts: HelperInputFacts) -> HelperFactRead<HelperInputFacts> {
        HelperFactRead::Trusted(facts)
    }

    /// 用生产读取器读一段 v2 JSON（请求身份固定为 `req-1`）。
    fn read_v2(json: &str) -> HelperFactRead<HelperInputFacts> {
        HelperInputFacts::read(json.as_bytes(), "req-1")
    }

    #[test]
    fn stroke_bounds_reject_window_escape_overflow_and_invalid_lengths() {
        let points = [MousePoint { x: 0, y: 100 }, MousePoint { x: 100, y: 200 }];
        assert!(validate_stroke(window(), [-50, 75, 200, 200], &points, 100).is_ok());
        assert!(validate_stroke(window(), [-101, 75, 200, 200], &points, 100).is_err());
        assert!(validate_stroke(window(), [i32::MAX, 0, 2, 2], &points, 100).is_err());
        assert!(validate_stroke(window(), [-50, 75, 100, 200], &points, 100).is_err());
        assert!(validate_stroke(window(), [-50, 75, 200, 200], &points, 5001).is_err());
        assert!(validate_stroke(window(), [-50, 75, 200, 200], &points[..1], 100).is_err());
    }

    #[test]
    fn stroke_cancel_before_start_never_launches_backend() {
        let result = controlled_drag_path(
            window(),
            window().rect,
            &[MousePoint { x: 0, y: 100 }, MousePoint { x: 100, y: 200 }],
            0,
            Duration::from_secs(1),
            &|| true,
        );
        let failure = result.expect_err("输入前的取消必须失败");
        assert!(failure.message.contains("stroke_cancelled"));
        // helper 从未启动：这是"能证明输入前失败"，回执必须写未发送。
        assert!(!failure.input_possible);
        assert!(failure.facts().is_none());
        assert_eq!(failure.release_state(), ReleaseObligationState::ProvenAbsent);
        let receipt = helper_failure_receipt("act-1", &failure);
        receipt.validate().expect("输入前失败的回执必须自洽");
        validate_receipt_for_write(&receipt).expect("输入前失败的回执必须可写入");
        assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.partial, Some(false));
        assert_eq!(receipt.path_completed, Some(false));
        assert_eq!(receipt.confirmed_point_count, Some(0));
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
    }

    #[test]
    #[cfg(windows)]
    fn stroke_native_engine_mock_guarantees_release_on_failure_and_cancel() {
        // 运行与生产相同的 C# Engine，但注入纯内存驱动；不调用 Native/User32。
        let script=format!("$ErrorActionPreference='Stop'; Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'\n{}\n'@\n[CoolzhuStroke.MockChecks]::Run()",include_str!("input_stroke_native.cs"));
        let result = super::super::run_powershell(&script, Duration::from_secs(45)).unwrap();
        assert_eq!(result.trim(), "mock-path-cancel-failure-release:ok");
    }

    /// RPR-04b 回归（路径第 N 点后失败）：真实 helper 引擎自己写出进度文件，
    /// 生产解析器读回它——不依赖测试拼装的 partial 对象。
    #[test]
    #[cfg(windows)]
    fn real_helper_engine_writes_partial_path_facts_the_production_parser_reads() {
        let directory = std::env::temp_dir().join("coolzhu-stroke-facts-test");
        std::fs::create_dir_all(&directory).expect("临时目录可用");
        let partial_file = directory.join(format!("partial-{}.json", std::process::id()));
        let completed_file = directory.join(format!("completed-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&partial_file);
        let _ = std::fs::remove_file(&completed_file);
        let source = include_str!("input_stroke_native.cs");

        // 场景一：真实 Engine 在第 2 个点失败（第 1 点已注入、按键已确认释放）。
        let path = partial_file.to_string_lossy().replace('\\', "/");
        let script = format!(
            "$ErrorActionPreference='Stop'; Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'\n{source}\n'@\n[CoolzhuStroke.MockChecks]::RunProgressCheck('{path}')"
        );
        let _ = super::super::run_powershell(&script, Duration::from_secs(45)).unwrap();
        let raw = std::fs::read(&partial_file).expect("helper 必须写出进度文件");
        let facts = HelperInputFacts::read(&raw, "mock-request")
            .trusted()
            .cloned()
            .expect("生产解析器必须能读回 helper 的事实");
        assert_eq!(facts.injected_points, 1, "已注入的点必须来自 helper 自己的报告");
        assert!(facts.button_down);
        assert!(!facts.path_completed);
        assert_eq!(facts.released, Some(true));
        assert_eq!(facts.phase, HelperFactPhase::Final, "收尾记录必须声明 final");
        assert_eq!(facts.cursor_moved, Some(true), "光标移动过必须如实记录");
        assert_eq!(facts.protocol, HELPER_FACT_PROTOCOL_V2);

        // 失败事实落成回执：部分输入保留，且不得升格成完整路径。
        let failure = StrokeFailure::after_input("stale_observation: 窗口已变化", trusted(facts));
        let receipt = helper_failure_receipt("act-1", &failure);
        receipt.validate().expect("真实 helper 事实构成的回执必须自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::Sent);
        assert_eq!(receipt.partial, Some(true));
        assert_eq!(receipt.path_completed, Some(false));
        assert_eq!(receipt.confirmed_point_count, Some(1));
        assert_eq!(receipt.input_release, InputReleaseStatus::Released);

        // 场景二：整条路径完成。
        let path = completed_file.to_string_lossy().replace('\\', "/");
        let script = format!(
            "$ErrorActionPreference='Stop'; Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'\n{source}\n'@\n[CoolzhuStroke.MockChecks]::RunCompletedProgressCheck('{path}')"
        );
        let _ = super::super::run_powershell(&script, Duration::from_secs(45)).unwrap();
        let raw = std::fs::read(&completed_file).expect("helper 必须写出进度文件");
        let facts = HelperInputFacts::read(&raw, "mock-request")
            .trusted()
            .cloned()
            .expect("生产解析器必须能读回 helper 的事实");
        assert_eq!(facts.injected_points, 3);
        assert!(facts.path_completed);
        assert_eq!(facts.released, Some(true));
        let receipt = helper_success_receipt("act-1", facts);
        receipt.validate().expect("成功回执必须自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::Sent);
        assert_eq!(receipt.partial, Some(false));
        assert_eq!(receipt.path_completed, Some(true));
        assert_eq!(receipt.confirmed_point_count, Some(3));
        assert_eq!(receipt.input_release, InputReleaseStatus::Released);
        let _ = std::fs::remove_file(&partial_file);
        let _ = std::fs::remove_file(&completed_file);
    }

    /// T03：只有"起点快照 + 之后被强杀"的落盘事实（`phase = pre_input`，收尾从未封闭）。
    ///
    /// 起点快照写在任何 Move/Down **之前**，它不覆盖之后可能发生的事；因此
    /// **不得**被推导成完整零输入，也**不得**触发任何自动重放。
    #[test]
    #[cfg(windows)]
    fn t03_pre_input_snapshot_after_a_kill_is_not_a_complete_zero_input_proof() {
        let directory = std::env::temp_dir().join("coolzhu-stroke-preinput-test");
        std::fs::create_dir_all(&directory).expect("临时目录可用");
        let file = directory.join(format!("pre-input-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let source = include_str!("input_stroke_native.cs");
        let path = file.to_string_lossy().replace('\\', "/");
        let script = format!(
            "$ErrorActionPreference='Stop'; Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'\n{source}\n'@\n[CoolzhuStroke.MockChecks]::RunPreInputKillCheck('{path}')"
        );
        let _ = super::super::run_powershell(&script, Duration::from_secs(45)).unwrap();
        let raw = std::fs::read(&file).expect("helper 必须写出起点事实");
        let facts = HelperInputFacts::read(&raw, "mock-request")
            .trusted()
            .cloned()
            .expect("起点记录本身是可信记录");
        assert_eq!(facts.phase, HelperFactPhase::PreInput);
        assert_eq!(facts.cursor_moved, Some(false));
        assert_eq!(facts.injected_points, 0);

        // 被强杀：起点快照看起来是"零"，但它不是证明。
        let failure = StrokeFailure::after_input("helper_lost: 未正常结束", trusted(facts));
        assert_eq!(
            failure.release_state(),
            ReleaseObligationState::Possible,
            "未封闭的记录只能推出'可能存在义务'"
        );
        let receipt = helper_failure_receipt("act-pre", &failure);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
        assert_eq!(receipt.partial, None);
        assert_eq!(receipt.path_completed, None);
        assert_eq!(receipt.confirmed_point_count, None);
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
        // 没有任何"自动重放"的授权：不可重试的错误码 + 回执显示输入可能已发出。
        let error = crate::ComputerUseError::new(
            "helper_lost",
            failure.message.clone(),
            stroke_failure_kind_retryable(failure.kind()),
            ComputerUseRetryOwner::None,
        )
        .with_receipt(receipt);
        assert!(error.receipt_shows_input_may_have_been_sent("act-pre"));
        let _ = std::fs::remove_file(&file);
    }

    fn stroke_failure_kind_retryable(kind: StrokeFailureKind) -> bool {
        kind.retryable()
    }

    /// T01（端到端，真实子进程 + 真实脚本 + 真实 Engine，**不注入任何输入**）：
    /// 伪窗口身份在第 0 步就自报失败，最终记录证明"光标没动过、没按下、没注入"，
    /// 因此回执必须是确定的 `Stale + NotSent + NotNeeded`，且**不**启动任何补发。
    ///
    /// `Move/Down/Up` 的零调用由 helper 自己的记录给出：
    /// `cursor_moved = false`（Move 没跑）、`button_down = false` 且 `released = null`
    /// （`armed` 从未成立 ⇒ Down/Up 都没跑）。
    #[test]
    #[cfg(windows)]
    fn t01_stale_identity_failure_is_deterministic_stale_not_sent_not_needed() {
        release_probe_reset(RELEASE_PROBE_FORCE_SUCCESS);
        // handle/pid 都不是当前前台窗口：Native.Check() 会先于任何输入抛出 stale_observation。
        let window = StrokeWindow {
            handle: 1,
            process_id: 4,
            rect: [0, 0, 400, 300],
            dpi: 96,
        };
        let points = [MousePoint { x: 10, y: 10 }, MousePoint { x: 200, y: 200 }];
        // 业务期限给足：helper 自己会在身份校验处立刻失败，这里的时间只用来避免
        // "主机先超时并发出取消"与"helper 自己报 stale"在并发负载下抢跑。
        let failure = controlled_drag_path(
            window,
            window.rect,
            &points,
            20,
            Duration::from_secs(30),
            &|| false,
        )
        .expect_err("身份不匹配的窗口必须失败");

        // 请求确实送到了真实 helper，因此不能声称"零输入"；事实必须来自 helper 自己。
        assert!(failure.input_possible);
        assert_eq!(failure.kind(), StrokeFailureKind::Stale);
        let facts = failure.facts().expect("真实 helper 必须写出收尾事实");
        assert_eq!(facts.phase, HelperFactPhase::Final, "收尾后必须留下最终事实");
        assert_eq!(facts.cursor_moved, Some(false), "Move 从未被调用");
        assert_eq!(facts.injected_points, 0, "Down 从未被调用");
        assert!(!facts.button_down);
        assert!(!facts.path_completed);
        assert_eq!(facts.released, None, "armed 从未成立 ⇒ Up 从未被调用");
        assert_eq!(failure.release_state(), ReleaseObligationState::ProvenAbsent);

        let receipt = helper_failure_receipt("act-real", &failure);
        receipt.validate().expect("真实 helper 事实构成的回执必须自洽");
        validate_receipt_for_write(&receipt).expect("零输入证明的回执必须可写入");
        assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.partial, Some(false));
        assert_eq!(receipt.path_completed, Some(false));
        assert_eq!(receipt.confirmed_point_count, Some(0));
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
        // 零输入证明成立 ⇒ 根本没有启动独立（补发）释放。
        assert_eq!(release_probe_count(), 0, "不需要补发时不得启动独立释放");
        // 这种回执不得阻断控制器的一次重新观察（输入前失败可以重新观察后重规划）。
        let error = crate::ComputerUseError::recoverable("stale_observation", failure.message.clone())
            .with_receipt(receipt);
        assert!(!error.receipt_shows_input_may_have_been_sent("act-real"));
    }

    /// T01（helper 侧，确定性）：身份校验在**任何输入之前**就失败。
    ///
    /// 这条用例直接钉住"`Move/Down/Up` 零调用"：helper 自己的内存驱动会记录每一次
    /// Move/Down/Up，driver 计数器与事件表必须都是空的；最终记录必须够格当零输入证明。
    #[test]
    #[cfg(windows)]
    fn t01_identity_failure_calls_no_move_down_or_up_and_leaves_a_proof() {
        release_probe_reset(RELEASE_PROBE_FORCE_FAILURE);
        let failure = run_helper(
            serde_json::json!({"mode":"stroke", "mock_scenario":"identity_failure"}),
            Duration::from_secs(20),
            &|| false,
            StrokeRunCapacity::OrdinaryAction,
            None,
        )
        .expect_err("身份校验失败必须失败");
        assert_eq!(failure.kind(), StrokeFailureKind::Stale, "必须保持确定性 Stale");
        let facts = failure.facts().expect("收尾后的最终事实必须留下");
        assert_eq!(facts.phase, HelperFactPhase::Final);
        assert_eq!(facts.cursor_moved, Some(false), "Move 一次都没调用");
        assert_eq!(facts.injected_points, 0);
        assert!(!facts.button_down, "Down 一次都没调用");
        assert_eq!(facts.released, None, "armed 从未成立 ⇒ Up 一次都没调用");
        let receipt = helper_failure_receipt("act-t01", &failure);
        validate_receipt_for_write(&receipt).expect("可写入");
        assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);
        assert_eq!(
            release_probe_count(),
            0,
            "零输入证明成立时不得启动独立释放（即使它被注入为必失败）"
        );
    }

    /// T02：把"独立释放必失败"作为**故障注入**。零输入证明成立时这个通道**根本不被调用**，
    /// 原始分类（stale_observation）不受影响；作为对照，义务未结清时它会被调用**一次**。
    #[test]
    #[cfg(windows)]
    fn t02_a_broken_independent_release_is_never_invoked_when_zero_input_is_proven() {
        release_probe_reset(RELEASE_PROBE_FORCE_FAILURE);
        let window = StrokeWindow {
            handle: 1,
            process_id: 4,
            rect: [0, 0, 400, 300],
            dpi: 96,
        };
        let points = [MousePoint { x: 10, y: 10 }, MousePoint { x: 200, y: 200 }];
        // 业务期限给足：helper 自己会在身份校验处立刻失败，这里的时间只用来避免
        // "主机先超时并发出取消"与"helper 自己报 stale"在并发负载下抢跑。
        let failure = controlled_drag_path(
            window,
            window.rect,
            &points,
            20,
            Duration::from_secs(30),
            &|| false,
        )
        .expect_err("身份不匹配的窗口必须失败");

        assert_eq!(release_probe_count(), 0, "证明成立时不得触碰补发通道");
        assert_eq!(
            failure.kind(),
            StrokeFailureKind::Stale,
            "原始分类不得因为补发通道坏掉而改变"
        );
        assert_eq!(failure.release_state(), ReleaseObligationState::ProvenAbsent);
        assert!(failure.cleanup().is_none(), "没有进入收尾就不该留下收尾事实");
        assert!(!failure.message.contains("input_release_unconfirmed"));
    }

    /// T04：缺字段／坏协议／错误请求身份都**不是**"零输入"。异常必须被保留，
    /// 绝不能靠默认零值填成 `NotSent`。
    #[test]
    fn t04_unreadable_records_are_never_filled_with_zero_defaults() {
        // 缺字段／空内容。
        assert!(
            read_v2(r#"{"injected_points":1,"button_down":true}"#).is_rejected(),
            "缺字段必须读成'有记录但不可信'"
        );
        assert!(HelperInputFacts::read(b"", "req-1").is_rejected());
        // 坏 JSON / 多字段 / 越界 / 自相矛盾。
        for raw in [
            "not json",
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":1,"button_down":true,"path_completed":false,"released":true,"extra":1}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":300,"button_down":true,"path_completed":true,"released":true}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":2,"button_down":false,"path_completed":false,"released":null}"#,
            r#"{"protocol":9,"request_id":"req-1","phase":"final","cursor_moved":false,"injected_points":0,"button_down":false,"path_completed":false,"released":null}"#,
            r#"{"protocol":2,"request_id":"someone-else","phase":"final","cursor_moved":false,"injected_points":0,"button_down":false,"path_completed":false,"released":null}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"nonsense","cursor_moved":false,"injected_points":0,"button_down":false,"path_completed":false,"released":null}"#,
        ] {
            let read = read_v2(raw);
            assert!(read.is_rejected(), "必须保留异常而不是当成事实：{raw}");
            assert!(
                read.anomaly().is_some_and(|anomaly| !anomaly.is_empty()),
                "异常必须带原因：{raw}"
            );
        }

        // 不可信记录 → 维持未知（可能已发送 + 释放未知），绝不写未发送。
        let failure = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            read_v2(r#"{"protocol":2,"request_id":"someone-else","phase":"final","cursor_moved":false,"injected_points":0,"button_down":false,"path_completed":false,"released":null}"#),
        );
        assert_eq!(failure.release_state(), ReleaseObligationState::EvidenceConflict);
        let receipt = helper_failure_receipt("act-1", &failure);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
        assert_eq!(receipt.partial, None);
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
        assert!(failure.fact_anomaly().is_some(), "异常必须随失败一起上抛");
    }

    /// T05：Down 已发生但路径点数为零 —— 仍必须登记释放义务。
    #[test]
    fn t05_a_pressed_button_still_registers_a_release_duty_with_zero_injected_points() {
        // helper 在"按下确认"与"写点"之间被强杀：按下过、点数还是 0、没有释放结论。
        let facts = final_facts(true, 0, true, false, None);
        let failure = StrokeFailure::after_input("helper_lost: 未正常结束", trusted(facts));
        assert_eq!(failure.release_state(), ReleaseObligationState::Unsettled);
        let receipt = helper_failure_receipt("act-5", &failure);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
        assert_eq!(receipt.partial, Some(true));
        assert_eq!(receipt.path_completed, Some(false));
        assert_eq!(receipt.confirmed_point_count, Some(0));
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
        assert!(
            !failure.release_state().is_proven_absent(),
            "按下过的记录永远不能被读成'没有义务'"
        );
    }

    /// T06：`button_down` 是**单调**的"曾按下"标志（Up 不会把它翻回 false）。
    /// 一份"已经走到路径中途、却声称从未按下"的记录是**冲突**，不得被解释为"从未按下"；
    /// 而"按下过且已抬起"的记录必须结清（不是"没有义务"）。
    #[test]
    fn t06_a_press_flag_is_never_read_as_never_pressed() {
        // 先 Down 后 Up 的真实记录：button_down 仍为 true、released=true。
        let pressed_and_released = final_facts(true, 3, true, true, Some(true));
        assert!(pressed_and_released.button_down, "Up 不会把'曾按下'翻回 false");
        let released_failure = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            trusted(pressed_and_released),
        );
        assert_eq!(
            released_failure.release_state(),
            ReleaseObligationState::Settled,
            "已产生的义务已经结清，而不是'从未产生'"
        );
        assert_eq!(
            helper_failure_receipt("act-6", &released_failure).input_release,
            InputReleaseStatus::Released
        );

        // 自相矛盾：走了 2 个点却声称从未按下（只能是伪造/损坏）→ 保留冲突。
        let contradictory = read_v2(
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":2,"button_down":false,"path_completed":false,"released":true}"#,
        );
        assert!(contradictory.is_rejected(), "矛盾记录不得被接纳");
        let failure = StrokeFailure::after_input("helper_lost: 未正常结束", contradictory);
        assert_eq!(failure.release_state(), ReleaseObligationState::EvidenceConflict);
        let receipt = helper_failure_receipt("act-6", &failure);
        assert_ne!(
            receipt.input_delivery,
            InputDelivery::NotSent,
            "冲突证据不得被算成未发送"
        );
        assert_ne!(receipt.input_release, InputReleaseStatus::NotNeeded);
    }

    /// T07：helper / 补发通道**明确报告释放失败**时，经统一协调**最多一次**受控补发。
    ///
    /// 旧行为是"明确失败 ⇒ 完全跳过收尾"；这里钉住归一后的语义：一次尝试、结论如实记录，
    /// 原始原因保留。
    #[test]
    #[cfg(windows)]
    fn t07_an_explicit_release_failure_gets_exactly_one_controlled_cleanup() {
        // ① 补发通道坏掉：只尝试一次，义务仍未结清，外部按安全阻断。
        release_probe_reset(RELEASE_PROBE_FORCE_FAILURE);
        let failure = run_release_failed_stroke();
        assert_eq!(release_probe_count(), 1, "最多一次受控补发");
        assert_eq!(
            failure.kind(),
            StrokeFailureKind::ReleaseUnconfirmed,
            "helper 明确报告释放失败这个原始原因必须保留"
        );
        assert_eq!(
            failure.release_state(),
            ReleaseObligationState::Unsettled,
            "补发失败 ⇒ 义务仍未结清"
        );
        assert!(
            failure.release_state().carries_unsettled_duty(),
            "未结清 ⇒ 外部行为必须按安全阻断"
        );
        let cleanup = failure.cleanup().expect("进入过收尾必须留下收尾事实");
        assert!(cleanup.independent_release_issued);
        assert!(!cleanup.independent_release_confirmed);
        assert_eq!(cleanup.release, CleanupReleaseStatus::Unconfirmed);

        // ② 补发成功：义务结清，不再重复补发。
        release_probe_reset(RELEASE_PROBE_FORCE_SUCCESS);
        let failure = run_release_failed_stroke();
        assert_eq!(release_probe_count(), 1, "最多一次受控补发");
        assert_eq!(
            failure.kind(),
            StrokeFailureKind::ReleaseUnconfirmed,
            "原始原因必须保留"
        );
        assert_eq!(failure.release_state(), ReleaseObligationState::Settled);
        let cleanup = failure.cleanup().expect("进入过收尾必须留下收尾事实");
        assert!(cleanup.independent_release_issued);
        assert!(cleanup.independent_release_confirmed);
        assert_eq!(cleanup.release, CleanupReleaseStatus::Confirmed);
    }

    /// 真实 helper（真实 PowerShell + 真实 C# Engine，**内存驱动、零注入**）：
    /// "路径中途失败 + helper 自己的 `Up()` 也失败" ⇒ helper 明确报告释放失败。
    ///
    /// 这条用例覆盖的是**生产编排**：收到 helper 的受信报告之后，仍然要走一次受控收尾，
    /// 而不是因为"已经明确失败"就完全放弃。补发次数由线程注观测。
    #[cfg(windows)]
    fn run_release_failed_stroke() -> StrokeFailure {
        run_helper(
            serde_json::json!({"mode":"stroke", "mock_scenario":"release_failure"}),
            Duration::from_secs(20),
            &|| false,
            StrokeRunCapacity::OrdinaryAction,
            None,
        )
        .expect_err("helper 报告释放失败时必须失败")
    }

    /// T08：原动作"零输入"与"独立清理已派发"是**两个分别存在的事实**：
    /// 回执只反映本动作的输入事实，清理事实单独记录，两者不互相改写。
    #[test]
    fn t08_the_zero_input_fact_and_a_dispatched_cleanup_are_recorded_separately() {
        // 本动作的事实：收尾已封闭、没移动光标、没按下、零注入。
        let mut failure = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            trusted(final_facts(false, 0, false, false, None)),
        );
        assert_eq!(helper_failure_receipt("act-8", &failure).input_delivery, InputDelivery::NotSent);
        // 独立清理是真实清理动作：它被派发过，但不改变本动作的输入事实。
        failure = failure.with_cleanup(HelperCleanupFacts {
            pipe: None,
            stopped_new_input_at_ms: 10,
            cleanup_started_at_ms: 10,
            cleanup_finished_at_ms: 40,
            cooperative_exit_waited_ms: 0,
            forced_kill: true,
            independent_release_issued: true,
            independent_release_wait_ms: 30,
            independent_release_confirmed: false,
            independent_release_skipped_window_expired: false,
            release: CleanupReleaseStatus::Unconfirmed,
        });
        let receipt = helper_failure_receipt("act-8", &failure);
        receipt.validate().expect("自洽");
        assert_eq!(
            receipt.input_delivery,
            InputDelivery::NotSent,
            "清理动作不得把本动作改写成已发送"
        );
        assert_eq!(
            receipt.input_release,
            InputReleaseStatus::NotNeeded,
            "本动作的释放义务仍是无义务"
        );
        let cleanup = failure.cleanup().expect("清理事实必须保留");
        assert!(cleanup.independent_release_issued);
        assert_eq!(cleanup.release, CleanupReleaseStatus::Unconfirmed);
    }

    /// T09：新动作的 `NotSent + NotNeeded` **不解除**别的动作遗留的隔离。
    #[test]
    fn t09_a_new_not_needed_receipt_never_clears_another_actions_incident() {
        // 上一个动作留下的未解决隔离（释放未确认 ⇒ 隔离）。
        let previous = HelperCleanupFacts {
            pipe: None,
            stopped_new_input_at_ms: 1,
            cleanup_started_at_ms: 1,
            cleanup_finished_at_ms: 9,
            cooperative_exit_waited_ms: 8,
            forced_kill: true,
            independent_release_issued: true,
            independent_release_wait_ms: 8,
            independent_release_confirmed: false,
            independent_release_skipped_window_expired: false,
            release: CleanupReleaseStatus::Unconfirmed,
        };
        assert!(previous.release.quarantines());

        // 新动作：伪身份、输入前失败 ⇒ NotSent + NotNeeded。
        let failure = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            trusted(final_facts(false, 0, false, false, None)),
        );
        let receipt = helper_failure_receipt("act-new", &failure);
        assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);

        // 新结论只能挂在它自己的动作身份上：别的动作既不能借它"证明零输入"，
        // 也不能因此解除隔离。
        let error = crate::ComputerUseError::recoverable("stale_observation", failure.message.clone())
            .with_receipt(receipt)
            .with_cleanup(previous);
        assert!(error.receipt_matches("act-new"));
        assert!(
            !error.receipt_matches("act-old"),
            "新动作的回执不属于旧动作"
        );
        assert!(
            error.receipt_shows_input_may_have_been_sent("act-old"),
            "对旧动作而言新回执是身份不合的异常，必须保守处理"
        );
        assert!(
            error.cleanup().is_some_and(|cleanup| cleanup.release.quarantines()),
            "旧动作的未解决隔离必须原样保留"
        );
    }

    /// T10：`NotSent + input_release != NotNeeded` 的**新写入**必须被拒绝；
    /// 但"旧记录仍可读取与复核"——版本兼容不是把历史记录丢掉。
    #[test]
    fn t10_not_sent_with_an_unsettled_release_is_rejected_for_write_but_readable() {
        let rejected = ActionReceipt {
            action_id: "act-10".to_string(),
            input_delivery: InputDelivery::NotSent,
            partial: Some(false),
            path_completed: Some(false),
            confirmed_point_count: Some(0),
            effect: EffectStatus::NotObserved,
            goal_verdict: GoalVerdict::NotChecked,
            input_release: InputReleaseStatus::Unknown,
        };
        assert!(
            rejected.validate().is_ok(),
            "结构层不检查这一条（core-runtime 只查字段一致）"
        );
        let error = validate_receipt_for_write(&rejected).expect_err("新写入必须被拒绝");
        assert!(error.contains("释放义务"), "拒绝原因必须说清楚：{error}");

        // 可信零输入证明成立时，构造侧固定写 NotNeeded，不会被"释放未确认"文本类别覆盖。
        let proven = StrokeFailure::after_input(
            "mouse_release_failed: helper 报了释放失败但记录证明从未按下",
            trusted(final_facts(false, 0, false, false, None)),
        );
        let receipt = helper_failure_receipt("act-10", &proven);
        validate_receipt_for_write(&receipt).expect("零输入证明的回执必须可写入");
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);

        // 旧记录（历史 JSON）仍可读取：serde 版本兼容不动，异常由消费者保守呈现。
        let legacy = serde_json::json!({
            "action_id": "act-legacy",
            "input_delivery": "not_sent",
            "partial": false,
            "path_completed": false,
            "confirmed_point_count": 0,
            "effect": "not_observed",
            "goal_verdict": "not_checked",
            "input_release": "unknown"
        });
        let decoded: ActionReceipt =
            serde_json::from_value(legacy).expect("旧记录必须仍可读取");
        assert_eq!(decoded.input_release, InputReleaseStatus::Unknown);
        let error = crate::ComputerUseError::recoverable("stale_observation", "旧记录")
            .with_receipt(decoded);
        assert!(
            error.receipt_shows_input_may_have_been_sent("act-legacy"),
            "旧异常记录必须按'可能已发送'复核，而不是被当成零输入证明"
        );
    }

    /// 进度记录的解析必须是严格的：任何缺失/多余/越界/自相矛盾都**不是**事实。
    #[test]
    fn helper_progress_parsing_is_strict_and_never_guesses() {
        // 空内容／坏 JSON：都是"有记录但不可信"，不是一个默认事实。
        for raw in ["", "not json"] {
            let read = HelperInputFacts::read(raw.as_bytes(), "req-1");
            assert!(
                matches!(read, HelperFactRead::Rejected { .. } | HelperFactRead::Missing),
                "不得把读不出的字节当成事实：{raw:?}"
            );
        }
        // 缺字段 / 多字段 / 越界 / 自相矛盾：都是"有记录但不可信"。
        for raw in [
            r#"{"injected_points":1,"button_down":true}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":1,"button_down":true,"path_completed":false,"released":null,"extra":1}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":300,"button_down":true,"path_completed":true,"released":true}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":2,"button_down":false,"path_completed":false,"released":null}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":false,"injected_points":3,"button_down":true,"path_completed":false,"released":true}"#,
            r#"{"protocol":2,"request_id":"req-1","phase":"pre_input","cursor_moved":true,"injected_points":0,"button_down":false,"path_completed":false,"released":null}"#,
        ] {
            assert!(
                HelperInputFacts::read(raw.as_bytes(), "req-1").is_rejected(),
                "不得把无法核对的字节拼成看起来可信的事实：{raw}"
            );
        }
        // 可信的 v2 最终事实。
        assert_eq!(
            HelperInputFacts::read(
                br#"{"protocol":2,"request_id":"req-1","phase":"final","cursor_moved":true,"injected_points":1,"button_down":true,"path_completed":false,"released":null}"#,
                "req-1"
            ),
            HelperFactRead::Trusted(HelperInputFacts {
                protocol: HELPER_FACT_PROTOCOL_V2,
                request_id: Some("req-1".to_string()),
                phase: HelperFactPhase::Final,
                cursor_moved: Some(true),
                injected_points: 1,
                button_down: true,
                path_completed: false,
                released: None,
            })
        );
    }

    /// 旧版（v1，无版本/阶段/身份键）记录仍可**读取**，但它永远不能充当零输入证明。
    #[test]
    fn legacy_v1_records_stay_readable_but_can_never_prove_zero_input() {
        let read = HelperInputFacts::read(
            br#"{"injected_points":0,"button_down":false,"path_completed":false,"released":null}"#,
            "req-1",
        );
        let facts = read.trusted().cloned().expect("旧记录必须仍可读取");
        assert_eq!(facts.protocol, HELPER_FACT_PROTOCOL_V1);
        assert_eq!(facts.phase, HelperFactPhase::LegacyUnverifiable);
        assert_eq!(facts.cursor_moved, None, "旧记录无从核对光标是否移动过");
        let failure = StrokeFailure::after_input("stale_observation: 窗口已变化", trusted(facts));
        assert_eq!(
            failure.release_state(),
            ReleaseObligationState::Possible,
            "旧记录不得被当成完整的零输入证明"
        );
        let receipt = helper_failure_receipt("act-v1", &failure);
        assert_ne!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);
    }

    /// RPR-04b 回归（helper 失联 / 未确认释放）：事实与分类共同决定回执，
    /// 已知的部分输入必须保留，未知的部分不得升格成完整或已释放。
    #[test]
    fn helper_failure_receipt_keeps_known_facts_and_never_overclaims() {
        // 路径第 2 点后失败：按下过按键、只注入 1 点、已确认释放。
        let partial = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            trusted(final_facts(true, 1, true, false, Some(true))),
        );
        let receipt = helper_failure_receipt("act-9", &partial);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.action_id, "act-9");
        assert_eq!(receipt.input_delivery, InputDelivery::Sent);
        assert_eq!(receipt.partial, Some(true));
        assert_eq!(receipt.path_completed, Some(false));
        assert_eq!(receipt.confirmed_point_count, Some(1));
        assert_eq!(receipt.input_release, InputReleaseStatus::Released);

        // helper 失联且没有任何事实：维持未知，不推断零输入，也不写路径字段。
        let lost_without_facts =
            StrokeFailure::after_input("helper_lost: 未正常结束", HelperFactRead::Missing);
        let receipt = helper_failure_receipt("act-9", &lost_without_facts);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
        assert_eq!(receipt.partial, None);
        assert_eq!(receipt.path_completed, None);
        assert_eq!(receipt.confirmed_point_count, None);
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);

        // helper 失联但已报告过部分注入：保留已知的点，不得升格为完整。
        let lost_with_facts = StrokeFailure::after_input(
            "helper_lost: 未正常结束",
            trusted(final_facts(true, 5, true, false, None)),
        );
        let receipt = helper_failure_receipt("act-9", &lost_with_facts);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::MayHaveBeenSent);
        assert_eq!(receipt.partial, Some(true));
        assert_eq!(receipt.path_completed, Some(false));
        assert_eq!(receipt.confirmed_point_count, Some(5));

        // 路径完成但释放失败：两个事实必须分开记录，不得合并成"整段没执行"。
        let release_failed = StrokeFailure::after_input(
            "mouse_release_failed: Up() 失败",
            trusted(final_facts(true, 4, true, true, Some(false))),
        );
        let receipt = helper_failure_receipt("act-9", &release_failed);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::Sent);
        assert_eq!(receipt.partial, Some(false));
        assert_eq!(receipt.path_completed, Some(true));
        assert_eq!(receipt.confirmed_point_count, Some(4));
        assert_eq!(receipt.input_release, InputReleaseStatus::Unknown);

        // 按下之前就失败（最终事实、光标没动过、零注入）：明确未发送、无释放义务。
        let before_down = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            trusted(final_facts(false, 0, false, false, None)),
        );
        let receipt = helper_failure_receipt("act-9", &before_down);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::NotSent);
        assert_eq!(receipt.confirmed_point_count, Some(0));
        assert_eq!(receipt.input_release, InputReleaseStatus::NotNeeded);

        // 光标移动过但没按下：不是 NotSent（`injected_points = 0` 不覆盖路径开始前的移动）。
        let moved_without_press = StrokeFailure::after_input(
            "stale_observation: 窗口已变化",
            trusted(final_facts(true, 0, false, false, Some(true))),
        );
        let receipt = helper_failure_receipt("act-9", &moved_without_press);
        receipt.validate().expect("自洽");
        assert_eq!(
            receipt.input_delivery,
            InputDelivery::MayHaveBeenSent,
            "移动过光标就不能声称'什么都没发生'"
        );
        assert_eq!(receipt.confirmed_point_count, Some(0));
        assert_eq!(receipt.path_completed, Some(false));

        // 取消：只保留 helper 自己确认过的注入点。
        let cancelled = StrokeFailure::after_input(
            "stroke_cancelled: 已取消或超时",
            trusted(final_facts(true, 2, true, false, Some(true))),
        );
        let receipt = helper_failure_receipt("act-9", &cancelled);
        receipt.validate().expect("自洽");
        assert_eq!(receipt.input_delivery, InputDelivery::Sent);
        assert_eq!(receipt.confirmed_point_count, Some(2));
        assert_eq!(receipt.input_release, InputReleaseStatus::Released);
    }

    /// 分类必须与既有行为逐字一致，且"释放未确认"与"取消"都不可重试。
    #[test]
    fn stroke_failure_kind_matches_the_existing_classification() {
        for (message, code, retryable) in [
            ("mouse_release_failed: Up() 失败", "mouse_release_failed", false),
            (
                "input_release_unconfirmed: helper 退出后补发释放失败: x",
                "mouse_release_failed",
                false,
            ),
            ("stroke_cancelled: 已取消或超时", "cancelled", false),
            ("stroke_cancelled: permit expired", "deadline_exceeded", false),
            (
                "stroke_cancelled: permit expired; mouse_release_failed: Up() 失败",
                "mouse_release_failed",
                false,
            ),
            ("stale_observation: 窗口已变化", "stale_observation", true),
            ("helper_lost: 未正常结束且未给出原因", "helper_lost", true),
            ("受控桌面操作失败: boom", "input_failed", true),
        ] {
            let kind = StrokeFailureKind::classify(message);
            assert_eq!(kind.code(), code, "{message}");
            assert_eq!(kind.retryable(), retryable, "{message}");
        }
    }

    /// RPR-11c：收尾策略只在一个地方定义，且**不再**用固定大超时补发释放。
    #[test]
    fn cleanup_policy_is_defined_once_and_has_no_fixed_large_release_timeout() {
        let policy = input_cleanup_policy();
        // 三个数值是待验默认值：协作退出 2 秒 / 总窗口 4 秒 / 独立释放 2 秒。
        assert_eq!(policy.cooperative_exit_grace(), Duration::from_secs(2));
        assert_eq!(policy.cleanup_window(), Duration::from_secs(4));
        assert_eq!(policy.independent_release_wait_cap(), Duration::from_secs(2));

        let source = include_str!("input_stroke.rs");
        // 注意：断言文本本身也在被检视的源码里，因此待查串必须运行时拼出来。
        let fixed_six_second_timeout = format!("Duration::from_secs({})", 3 * 2);
        assert!(
            !source.contains(&fixed_six_second_timeout),
            "补发释放不得再用固定 6 秒：等待上限必须来自 min(2 秒, 剩余收尾时间)"
        );
    }

    /// RPR-11c：独立（补发）释放**最多一次**，等待上限 = `min(2 秒, 剩余收尾时间)`；
    /// 四秒窗口结束就隔离，不延长、不重试循环。
    #[test]
    fn independent_release_wait_is_bounded_by_the_remaining_cleanup_window() {
        let policy = input_cleanup_policy();
        let start = Instant::now();
        let deadline = CleanupDeadline::establish_at(policy, 0, start);

        // 立刻补发：拿到完整上限 2 秒。
        assert_eq!(
            deadline.independent_release_wait_at(start),
            Duration::from_secs(2)
        );
        // 协作退出已经用掉 3 秒：独立释放只剩 1 秒，二者相加不超 4 秒总窗口。
        let later = start + Duration::from_secs(3);
        assert_eq!(
            deadline.independent_release_wait_at(later),
            Duration::from_secs(1)
        );
        // 窗口到期：不再尝试补发（不延长、不重试循环）。
        assert_eq!(
            deadline.independent_release_wait_at(start + Duration::from_secs(4)),
            Duration::ZERO
        );
        // 协作退出 + 独立释放共同消耗同一个窗口，不会各自再领一份完整额度。
        assert!(
            deadline.cooperative_exit_grace_at(start)
                + deadline.independent_release_wait_at(start + Duration::from_secs(2))
                <= policy.cleanup_window()
        );
    }

    /// RPR-11c／CU-F01：重放机制只能存在于"停止等待"层面——补发释放的调用点**唯一**，
    /// 且第一次进入取消/异常收尾时固定的收尾窗口不会被后续信号刷新。
    #[test]
    fn cleanup_window_is_fixed_once_and_the_release_has_a_single_call_site() {
        let source = include_str!("input_stroke.rs");
        // 边界取**列 0** 的 `#[cfg(test)]`（测试模块）：函数体里现在也有缩进的
        // 测试专用属性，按裸串切会把函数体截断。
        // 用空格拼接：下面的检查只看片段是否出现，不依赖换行。
        let helper: String = source
            .split("fn run_helper(")
            .nth(1)
            .expect("run_helper")
            .lines()
            .take_while(|line| !line.starts_with("#[cfg(test)]"))
            .collect::<Vec<_>>()
            .join(" ");
        // 只在 helper 自己的循环里统计真正的补发调用（`needs_emergency_release` 只是判定）。
        // CU-F01 归一后只剩一处：请求写入失败的分支已经不再补发（那时可证明零输入）。
        // §C-21 之后这一处多带一个参数（**转用**接纳时预留的清理容量），调用点数量不变。
        let issue_release = format!("emergency_release({}, cleanup_reservation)", "wait");
        assert_eq!(
            helper.matches(issue_release.as_str()).count(),
            1,
            "补发释放只允许一个调用点：多一处就是'多层各补一次'"
        );
        assert!(
            helper.contains("HelperPipeReadersAdmission::acquire_with_cleanup_reserve()")
                && helper.contains("StrokeRunCapacity::CleanupRun(reserved) => (reserved.into_admission(), None)"),
            "普通动作在 spawn 之前一次性预留'两条流 + 一次清理'；清理运行**转用**它\
             （不重新竞争普通容量，也**不**递归预留下一层清理额度）"
        );
        assert!(
            helper.contains("cleanup_deadline = Some(fixed)"),
            "取消/到期只固定收尾窗口，不用重复信号刷新它"
        );

        let policy = input_cleanup_policy();
        let mut slot = None;
        let first = CleanupDeadline::fixed(&mut slot, policy, 1_000);
        let repeated = CleanupDeadline::fixed(&mut slot, policy, 9_999);
        assert_eq!(
            repeated.deadline_at(),
            first.deadline_at(),
            "重复的取消/到期信号不得把收尾窗口续期"
        );
        assert_eq!(repeated.started_at_unix_ms(), 1_000);
    }

    /// CU-F01 §1／§3：释放义务判定必须**消费事实**。
    ///
    /// - 已证明不存在（可信的"零输入/未按下"证明）⇒ 不启动补发；
    /// - 已产生且已结清 ⇒ 不重复补发；
    /// - 已产生且未结清、可能存在、证据冲突 ⇒ 在既定安全条件下最多一次。
    #[test]
    fn release_cleanup_consumes_the_derived_obligation_state() {
        use ReleaseObligationState::{EvidenceConflict, Possible, ProvenAbsent, Settled, Unsettled};
        // 非笔画模式（释放通道自身）：永不补发，避免递归。
        assert!(!needs_emergency_release(false, Unsettled, true, true, false));
        // 已证明不存在：不启动补发。
        assert!(!needs_emergency_release(true, ProvenAbsent, true, true, false));
        assert!(!needs_emergency_release(true, ProvenAbsent, true, false, false));
        // 已产生且已结清：不重复补发。
        assert!(!needs_emergency_release(true, Settled, true, true, false));
        // 已产生且未结清：被强杀 / 非零退出时补发一次。
        assert!(needs_emergency_release(true, Unsettled, true, true, false));
        assert!(needs_emergency_release(true, Unsettled, true, false, false));
        // 正常退出：`finally` 已经跑过，不补发。
        assert!(!needs_emergency_release(true, Unsettled, true, false, true));
        // 可能存在 / 证据冲突：先确认执行者静止，否则隔离（不补发）。
        assert!(needs_emergency_release(true, Possible, true, true, false));
        assert!(!needs_emergency_release(true, Possible, false, true, false));
        assert!(needs_emergency_release(true, EvidenceConflict, true, false, false));
        assert!(!needs_emergency_release(true, EvidenceConflict, false, false, false));
    }

    /// CU-F02：helper 收尾的等待链里**不得**再出现无界等待，且必须走共用的有界管道收尾。
    ///
    /// 机制事实（§B-32）：被强杀的 helper 会留下持有管道写端的孙进程（`Add-Type` 的编译器），
    /// 任何"等读取线程结束"的无界等待都会把四秒收尾拖成无界等待——
    /// 这正是 `out_reader.join()` / `err_reader.join()` 在旧实现里的失效方式。
    #[test]
    fn f02_helper_cleanup_has_no_unbounded_wait_and_uses_the_bounded_pipe_drain() {
        let source = include_str!("input_stroke.rs");
        // 边界取**列 0** 的 `#[cfg(test)]`（测试模块）：函数体里现在也有缩进的
        // 测试专用属性，按裸串切会把函数体截断。
        // 用空格拼接：下面的检查只看片段是否出现，不依赖换行。
        let helper: String = source
            .split("fn run_helper(")
            .nth(1)
            .expect("run_helper")
            .lines()
            .take_while(|line| !line.starts_with("#[cfg(test)]"))
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            !helper.contains(".join()"),
            "run_helper 不得对读取线程做无界 join：收尾只允许有界等待 + 核实完成"
        );
        assert!(
            !helper.contains("read_to_end"),
            "读取必须由原生监督器有界地进行（只看已经可读的字节）"
        );
        let drain_call = "super::helper_pipes::drain(";
        assert!(
            helper.contains(drain_call),
            "收尾必须走共用的有界管道收尾入口"
        );
        assert!(
            helper.contains("pipe: Some(pipe_facts)"),
            "管道收尾事实必须写进收尾报告（与协议事实分开）"
        );
        assert!(
            helper.contains("input_cleanup_policy()"),
            "管道收尾的等待额度仍然来自唯一的收尾策略定义点"
        );
    }

    /// S1.5：失联判定必须把"helper 报错退出"排除在外——否则会把正常的
    /// stale/取消/操作失败误报成执行者失联。
    #[test]
    fn helper_lost_excludes_reported_failures() {
        // 被强杀：失联（无论有没有 stderr）。
        assert!(is_helper_lost(true, false, false));
        assert!(is_helper_lost(true, false, true));
        // 非零退出且没有任何原因：失联。
        assert!(is_helper_lost(false, false, false));
        // 非零退出但给出了原因：这是"报告了失败"，不是失联。
        assert!(!is_helper_lost(false, false, true));
        // 正常退出：不是失联。
        assert!(!is_helper_lost(false, true, false));
    }
}
