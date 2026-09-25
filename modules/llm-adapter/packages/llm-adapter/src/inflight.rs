//! 在途请求登记、四态语义与**受控排空**（工单 RPR-11a 补强·排空）。
//!
//! ## 术语（严格，不得放宽）
//!
//! * [`EndpointDrainReport::client_drained`] 只说明**已知客户端已排空**：本端认证地不再持有在途登记。
//!   它**不**表述"服务端所有生成均已停止"——Drop 只能结束本地资源持有，**不能**证明远端已停止计算。
//! * 断流、超时、取消、提前丢弃流一律进入 [`AttemptPhase::RemoteResultUnknown`]（远端结果未知），
//!   **不能直接归零**；只有迟到的可信结束事实才能把它对账结清。
//! * [`EndpointIdentity::unsupported`]（身份未知 / 该连接不支持身份查询）在排空判定里一律
//!   拒绝自动切换，**不得**被解释成"没有在途请求"。
//!
//! ## 在途四态
//!
//! | 状态 | 含义 | 排空判断 |
//! |---|---|---|
//! | [`AttemptPhase::NotDispatched`] | guard 已登记，请求尚未发出 | 失败/取消即结清登记 |
//! | [`AttemptPhase::Dispatched`] | 已派发（含已收响应头、正文仍在生成） | **仍是在途** |
//! | [`AttemptPhase::Settled`] | 已获得可信结束事实 | 可结清远端请求状态 |
//! | [`AttemptPhase::RemoteResultUnknown`] | 断流/超时/取消/提前丢弃 | 远端结果未知，不能归零 |
//!
//! ## 与裁决约束的对应
//!
//! * 约束 1：本模块即"受控排空"的实现（[`local_endpoint_drain`] / [`local_endpoint_drain_all`]）。
//! * 约束 2：不提供"握手结束即归零"，也不把流 Drop 当作服务端空闲。
//! * 约束 3：未知身份 → [`DrainVerdict::IdentityUnknown`]，[`EndpointDrainReport::permits_automatic_switch`] 恒为 `false`。
//! * 约束 4：身份来自[真实已解析配置](EndpointIdentitySource::ResolvedConfig)或[托管实例记录](EndpointIdentitySource::ManagedInstance)，
//!   **不含密钥**，也**不由端口号推导进程所有权**。
//! * 约束 6：四态 + 规则表逐条落在 [`InFlightGuard::settle`] / [`InFlightGuard::mark_remote_result_unknown`] /
//!   [`SharedAttempt`] 的 `Drop` 上。
//! * 约束 7：guard 在**实际发出请求之前**登记（[`InFlightGuard::register`] 早于 `reqwest::send`），
//!   流式请求把同一个 guard 从握手阶段转移到返回的流对象，非流式请求覆盖完整请求生命周期。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant};

use crate::error::ApiError;

/// 未知身份登记的桶名（`local_endpoint_drain_all` 会把它单独列出）。
const UNIDENTIFIED_BUCKET: &str = "<unidentified>";
/// 每个端点最多保留的已结清明细（防止长时间运行无限增长）。
const SETTLED_LEDGER_CAP: usize = 32;
/// 有界排空的轮询粒度（`tokio::time` 已启用，不引入 `sync` feature）。
const DRAIN_POLL_INTERVAL: Duration = Duration::from_millis(10);

// ============================================================================
// 一、连接 / 服务身份（约束 1、3、4）
// ============================================================================

/// 服务身份的来源。优先级：托管实例记录 > 真实已解析配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointIdentitySource {
    /// 来自 `EndpointResolver` 解析出的真实端点（真实已解析配置）。
    ResolvedConfig,
    /// 来自托管实例记录（进程内显式登记，不由端口号推导）。
    ManagedInstance,
    /// 身份未知 / 该连接不支持身份查询。
    Unsupported,
}

impl EndpointIdentitySource {
    /// 便于报告与诊断的稳定标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::ResolvedConfig => "resolved-config",
            Self::ManagedInstance => "managed-instance",
            Self::Unsupported => "unsupported",
        }
    }
}

/// 连接 / 服务身份。
///
/// * **不含密钥**：构造时剥掉 URL userinfo、query、fragment。
/// * **不由端口号推导进程所有权**：身份键包含完整已解析端点（scheme/host/port/path）；
///   同一个端口上的不同端点得到不同身份，端口号本身不产生任何身份。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EndpointIdentity {
    key: Option<String>,
    provider_tag: Option<String>,
    resolved_endpoint: Option<String>,
    managed_instance: Option<String>,
    source: EndpointIdentitySource,
}

impl Default for EndpointIdentitySource {
    fn default() -> Self {
        Self::Unsupported
    }
}

impl EndpointIdentity {
    /// "身份未知 / 不支持"：排空判定据此**拒绝**自动切换。
    #[must_use]
    pub fn unsupported() -> Self {
        Self {
            key: None,
            provider_tag: None,
            resolved_endpoint: None,
            managed_instance: None,
            source: EndpointIdentitySource::Unsupported,
        }
    }

    /// 由真实已解析配置构造身份（`resolved_endpoint` 应是 resolver 解析出的完整端点）。
    ///
    /// 若该端点已有托管实例记录（见 [`record_managed_instance`]），来源升级为
    /// [`EndpointIdentitySource::ManagedInstance`]。端点被剥空时退化为"身份未知"。
    #[must_use]
    pub fn from_resolved_config(provider_tag: &str, resolved_endpoint: &str) -> Self {
        let endpoint = sanitize_endpoint(resolved_endpoint);
        if endpoint.is_empty() {
            return Self::unsupported();
        }
        let provider_tag = provider_tag.trim();
        let provider_tag = if provider_tag.is_empty() {
            "unknown".to_string()
        } else {
            provider_tag.to_string()
        };
        let managed_instance = managed_instance_for(&endpoint);
        let mut key = format!("{provider_tag}@{endpoint}");
        if let Some(instance) = managed_instance.as_deref() {
            key.push('#');
            key.push_str(instance);
        }
        Self {
            key: Some(key),
            provider_tag: Some(provider_tag),
            resolved_endpoint: Some(endpoint),
            source: if managed_instance.is_some() {
                EndpointIdentitySource::ManagedInstance
            } else {
                EndpointIdentitySource::ResolvedConfig
            },
            managed_instance,
        }
    }

    /// 身份是否已知（未知即不可自动切换）。
    #[must_use]
    pub const fn is_known(&self) -> bool {
        self.key.is_some()
    }

    /// 稳定身份键：`<provider>@<已解析端点>[#<托管实例>]`，不含密钥。
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// 身份来源。
    #[must_use]
    pub const fn source(&self) -> EndpointIdentitySource {
        self.source
    }

    /// provider 标签（canonical provider id / provider kind 名）。
    #[must_use]
    pub fn provider_tag(&self) -> Option<&str> {
        self.provider_tag.as_deref()
    }

    /// 真实已解析端点。
    #[must_use]
    pub fn resolved_endpoint(&self) -> Option<&str> {
        self.resolved_endpoint.as_deref()
    }

    /// 托管实例标识（若存在）。
    #[must_use]
    pub fn managed_instance(&self) -> Option<&str> {
        self.managed_instance.as_deref()
    }
}

/// 登记一条托管实例记录（`resolved_endpoint` 用**已解析端点**，不是端口号）。
///
/// 返回被替换的旧值。记录只是身份旁证，**不会**用来从端口推导进程所有权。
pub fn record_managed_instance(resolved_endpoint: &str, instance_id: &str) -> Option<String> {
    let endpoint = sanitize_endpoint(resolved_endpoint);
    let instance_id = instance_id.trim();
    if endpoint.is_empty() || instance_id.is_empty() {
        return None;
    }
    let mut instances = lock_managed_instances();
    instances.insert(endpoint, instance_id.to_string())
}

/// 撤销一条托管实例记录，返回被移除的值。
pub fn clear_managed_instance(resolved_endpoint: &str) -> Option<String> {
    let endpoint = sanitize_endpoint(resolved_endpoint);
    if endpoint.is_empty() {
        return None;
    }
    lock_managed_instances().remove(&endpoint)
}

fn lock_managed_instances() -> MutexGuard<'static, HashMap<String, String>> {
    static INSTANCES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    INSTANCES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn managed_instance_for(endpoint: &str) -> Option<String> {
    lock_managed_instances().get(endpoint).cloned()
}

/// 剥掉 userinfo（可能含密钥）、query、fragment（可能含会话令牌），并去掉尾斜杠。
fn sanitize_endpoint(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let without_userinfo = trimmed
        .split_once("://")
        .and_then(|(scheme, rest)| {
            rest.split_once('@')
                .filter(|(_, host)| !host.is_empty())
                .map(|(_, host)| format!("{scheme}://{host}"))
        })
        .unwrap_or_else(|| trimmed.to_string());
    let cut_at = without_userinfo.find(|c| c == '?' || c == '#');
    let cut = cut_at.map_or(without_userinfo.as_str(), |index| {
        &without_userinfo[..index]
    });
    cut.trim_end_matches('/').to_string()
}

// ============================================================================
// 二、四态与终止事实（约束 6）
// ============================================================================

/// 登记的请求形态：流式（含长生成）与非流式都覆盖完整请求生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMode {
    /// 非流式：登记覆盖到响应体读取完成。
    NonStreaming,
    /// 流式：登记从握手前开始，随流对象转移到流生命周期结束。
    Streaming,
}

impl RequestMode {
    /// 稳定标签，便于报告与测试断言。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NonStreaming => "non-streaming",
            Self::Streaming => "streaming",
        }
    }
}

/// 在途四态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttemptPhase {
    /// 已登记但请求尚未发出。
    NotDispatched,
    /// 已派发（含已收到响应头、正文仍在生成）。
    Dispatched,
    /// 已获得可信结束事实（协议完整结束 / 从未发出）。
    Settled,
    /// 远端结果未知：断流、超时、取消或提前丢弃流，**不能直接归零**。
    RemoteResultUnknown,
}

impl AttemptPhase {
    /// 是否仍在途（影响排空判定）。
    #[must_use]
    pub const fn is_in_flight(self) -> bool {
        matches!(self, Self::NotDispatched | Self::Dispatched)
    }

    /// 稳定标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::NotDispatched => "not-dispatched",
            Self::Dispatched => "dispatched",
            Self::Settled => "settled",
            Self::RemoteResultUnknown => "remote-result-unknown",
        }
    }
}

/// 终止事实：区分"可信结束"与"只是本端不再持有"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationFact {
    /// 请求尚未发出即失败或取消 → 可以结清该请求登记。
    AbortedBeforeDispatch,
    /// 依据对应协议获得完整结束事实 → 可以结清远端请求状态。
    ProtocolCompletion,
    /// 断流 / 超时 / 取消后丢弃流 → 远端结果未知。
    RemoteResultUnknown,
}

impl TerminationFact {
    /// 稳定标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AbortedBeforeDispatch => "aborted-before-dispatch",
            Self::ProtocolCompletion => "protocol-completion",
            Self::RemoteResultUnknown => "remote-result-unknown",
        }
    }
}

/// 结算结果：区分首次结算、迟到事实对账、重复终止回调。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOutcome {
    /// 本次调用完成结算。
    Settled,
    /// 迟到事实对账：此前已登记"远端结果未知"，本次可信结束事实把它结清（**不重新执行任务**）。
    ReconciledLateFact,
    /// 重复终止回调：已结算过，本次不改变任何状态。
    AlreadySettled,
    /// 事实与"已派发"矛盾（例如已派发却声称"尚未发出"），按远端结果未知结清。
    DowngradedToRemoteResultUnknown,
}

/// 单次在途登记的只读快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptStatus {
    /// 进程内单调递增的登记号。
    pub attempt_id: u64,
    /// 请求形态。
    pub mode: RequestMode,
    /// 当前状态。
    pub phase: AttemptPhase,
    /// 是否有过至少一次实际派发。
    pub dispatched: bool,
    /// 终止事实（未终止为 `None`）。
    pub terminal_fact: Option<TerminationFact>,
    /// 是否为迟到事实对账结清（超时/未知之后才拿到可信结束事实）。
    pub reconciled_late: bool,
    /// 结算次数：重复终止回调只会让这里停在 `1`。
    pub settlements: u32,
}

impl AttemptStatus {
    /// 是否仍在途。
    #[must_use]
    pub const fn is_in_flight(&self) -> bool {
        self.phase.is_in_flight()
    }
}

// ============================================================================
// 三、登记状态与 guard（约束 6、7、8）
// ============================================================================

#[derive(Debug)]
struct AttemptState {
    mode: RequestMode,
    phase: AttemptPhase,
    dispatched: bool,
    terminal_fact: Option<TerminationFact>,
    reconciled_late: bool,
    settlements: u32,
}

impl AttemptState {
    fn new(mode: RequestMode) -> Self {
        Self {
            mode,
            phase: AttemptPhase::NotDispatched,
            dispatched: false,
            terminal_fact: None,
            reconciled_late: false,
            settlements: 0,
        }
    }

    fn snapshot(&self, attempt_id: u64) -> AttemptStatus {
        AttemptStatus {
            attempt_id,
            mode: self.mode,
            phase: self.phase,
            dispatched: self.dispatched,
            terminal_fact: self.terminal_fact,
            reconciled_late: self.reconciled_late,
            settlements: self.settlements,
        }
    }
}

/// 在途登记本体。生命周期 = **所有持有者**（握手阶段的 guard、provider 流对象、
/// `api::MessageStream` 包装）里最后一个的存活期；`Arc` 计数归零时按 Drop 规则收尾。
#[derive(Debug)]
struct SharedAttempt {
    attempt_id: u64,
    identity: EndpointIdentity,
    bucket_key: String,
    state: Mutex<AttemptState>,
}

impl SharedAttempt {
    fn lock_state(&self) -> MutexGuard<'_, AttemptState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn status(&self) -> AttemptStatus {
        self.lock_state().snapshot(self.attempt_id)
    }

    /// 请求**即将发出**时调用（约束 7：登记早于派发，派发早于等待响应）。
    fn mark_dispatched(&self) {
        let mut state = self.lock_state();
        if state.dispatched {
            return;
        }
        state.dispatched = true;
        if state.phase == AttemptPhase::NotDispatched {
            state.phase = AttemptPhase::Dispatched;
        }
    }

    /// 终止回调：重复调用只结算一次；迟到事实只对账、不重放任务（约束 6）。
    fn settle(&self, fact: TerminationFact) -> SettleOutcome {
        let mut registry = lock_registry();
        let mut state = self.lock_state();

        // 规则：请求尚未发出即失败或取消 → 只结清登记，不产生远端风险。
        let effective = if state.dispatched {
            fact
        } else {
            TerminationFact::AbortedBeforeDispatch
        };

        // 规则：已派发的请求不允许用"尚未发出"结清，落成远端结果未知。
        let (fact, downgraded) =
            if state.dispatched && fact == TerminationFact::AbortedBeforeDispatch {
                (TerminationFact::RemoteResultUnknown, true)
            } else {
                (effective, false)
            };

        if !state.phase.is_in_flight() {
            // 已终止：可信结束事实可以给"远端结果未知"做迟到对账，避免长期挂着未知。
            if state.phase == AttemptPhase::RemoteResultUnknown
                && fact == TerminationFact::ProtocolCompletion
            {
                state.phase = AttemptPhase::Settled;
                state.terminal_fact = Some(TerminationFact::ProtocolCompletion);
                state.reconciled_late = true;
                if let Some(bucket) = registry.buckets.get_mut(&self.bucket_key) {
                    bucket.remote_result_unknown_recorded =
                        bucket.remote_result_unknown_recorded.saturating_sub(1);
                    bucket.settled_recorded += 1;
                }
                return SettleOutcome::ReconciledLateFact;
            }
            return SettleOutcome::AlreadySettled;
        }

        state.phase = match fact {
            TerminationFact::ProtocolCompletion | TerminationFact::AbortedBeforeDispatch => {
                AttemptPhase::Settled
            }
            TerminationFact::RemoteResultUnknown => AttemptPhase::RemoteResultUnknown,
        };
        state.terminal_fact = Some(fact);
        state.settlements = 1;

        if let Some(bucket) = registry.buckets.get_mut(&self.bucket_key) {
            if fact == TerminationFact::RemoteResultUnknown {
                bucket.remote_result_unknown_recorded += 1;
            } else {
                bucket.settled_recorded += 1;
            }
        }

        if downgraded {
            SettleOutcome::DowngradedToRemoteResultUnknown
        } else {
            SettleOutcome::Settled
        }
    }

    /// 消费者放弃等待（超时/取消）但仍持有流：记为远端结果未知，**不是归零**。
    fn mark_remote_result_unknown(&self) -> SettleOutcome {
        self.settle(TerminationFact::RemoteResultUnknown)
    }

    fn settle_from_error(&self, error: &ApiError) -> SettleOutcome {
        match error {
            // 拿到了完整响应（哪怕是错误响应/不合法正文）＝ 协议级完整结束事实。
            ApiError::Api { .. } | ApiError::Json(_) | ApiError::InvalidSseFrame(_) => {
                self.settle(TerminationFact::ProtocolCompletion)
            }
            ApiError::RetriesExhausted { last_error, .. } => self.settle_from_error(last_error),
            // 传输层失败（超时/连接/请求构造前失败）：远端结果未知，或从未发出时只结清登记。
            _ => self.settle(TerminationFact::RemoteResultUnknown),
        }
    }
}

impl Drop for SharedAttempt {
    fn drop(&mut self) {
        let mut registry = lock_registry();
        let (status, entered_remote_unknown, entered_settled) = {
            let mut state = self.lock_state();
            let mut entered_remote_unknown = false;
            let mut entered_settled = false;
            if state.phase.is_in_flight() {
                // Drop 只结束本地资源持有：从未派发 → 结清登记；已派发 → 远端结果未知。
                let fact = if state.dispatched {
                    TerminationFact::RemoteResultUnknown
                } else {
                    TerminationFact::AbortedBeforeDispatch
                };
                state.phase = match fact {
                    TerminationFact::RemoteResultUnknown => AttemptPhase::RemoteResultUnknown,
                    TerminationFact::AbortedBeforeDispatch | TerminationFact::ProtocolCompletion => {
                        AttemptPhase::Settled
                    }
                };
                state.terminal_fact = Some(fact);
                state.settlements = 1;
                entered_remote_unknown = fact == TerminationFact::RemoteResultUnknown;
                entered_settled = !entered_remote_unknown;
            }
            (
                state.snapshot(self.attempt_id),
                entered_remote_unknown,
                entered_settled,
            )
        };
        if let Some(bucket) = registry.buckets.get_mut(&self.bucket_key) {
            bucket.registered.remove(&self.attempt_id);
            if entered_remote_unknown {
                bucket.remote_result_unknown_recorded += 1;
            }
            if entered_settled {
                bucket.settled_recorded += 1;
            }
            bucket.record_settled(status);
        }
    }
}

/// 在途 guard：请求发出前登记、随请求/流生命周期持有。
///
/// 可 `Clone`（多个持有者共享同一条登记）；**最后一个**持有者析构时才按 Drop 规则收尾。
#[derive(Debug, Clone)]
pub struct InFlightGuard {
    attempt: Arc<SharedAttempt>,
}

impl InFlightGuard {
    /// 在**实际发出请求之前**登记（约束 7）。
    #[must_use]
    pub fn register(identity: &EndpointIdentity, mode: RequestMode) -> Self {
        let bucket_key = identity
            .key()
            .unwrap_or(UNIDENTIFIED_BUCKET)
            .to_string();
        let mut registry = lock_registry();
        let attempt_id = registry.next_attempt_id;
        registry.next_attempt_id = registry.next_attempt_id.saturating_add(1);
        let bucket = registry
            .buckets
            .entry(bucket_key.clone())
            .or_insert_with(|| Bucket::new(identity.clone()));
        let attempt = Arc::new(SharedAttempt {
            attempt_id,
            identity: identity.clone(),
            bucket_key: bucket_key.clone(),
            state: Mutex::new(AttemptState::new(mode)),
        });
        bucket
            .registered
            .insert(attempt_id, Arc::downgrade(&attempt));
        drop(registry);
        Self { attempt }
    }

    /// 请求即将发出（`reqwest::send` 之前）时标记派发。
    pub fn mark_dispatched(&self) {
        self.attempt.mark_dispatched();
    }

    /// 按终止事实结清（幂等；重复终止回调只结算一次）。
    pub fn settle(&self, fact: TerminationFact) -> SettleOutcome {
        self.attempt.settle(fact)
    }

    /// 记为远端结果未知（断流/超时/取消后仍持有流）。
    pub fn mark_remote_result_unknown(&self) -> SettleOutcome {
        self.attempt.mark_remote_result_unknown()
    }

    /// 按错误性质结清：完整响应（含错误响应）→ 结清；传输层失败 → 远端结果未知。
    pub fn settle_from_error(&self, error: &ApiError) -> SettleOutcome {
        self.attempt.settle_from_error(error)
    }

    /// 只读快照。
    #[must_use]
    pub fn status(&self) -> AttemptStatus {
        self.attempt.status()
    }

    /// 进程内登记号。
    #[must_use]
    pub fn attempt_id(&self) -> u64 {
        self.attempt.attempt_id
    }

    /// 登记所归属的服务身份。
    #[must_use]
    pub fn identity(&self) -> &EndpointIdentity {
        &self.attempt.identity
    }
}

// ============================================================================
// 四、注册表 / 快照 / 排空（约束 1、3）
// ============================================================================

#[derive(Debug)]
struct Bucket {
    identity: EndpointIdentity,
    /// 已登记但尚未析构的登记（`Weak`：只有真实持有者会让它存活）。
    registered: HashMap<u64, Weak<SharedAttempt>>,
    /// 已结清明细（有上限）。
    ledger: VecDeque<AttemptStatus>,
    /// 累计结清数。
    settled_recorded: u64,
    /// 当前仍记着"远端结果未知"的数量：只有迟到事实对账才会减少，不会因 Drop 归零。
    remote_result_unknown_recorded: u64,
}

impl Bucket {
    fn new(identity: EndpointIdentity) -> Self {
        Self {
            identity,
            registered: HashMap::new(),
            ledger: VecDeque::new(),
            settled_recorded: 0,
            remote_result_unknown_recorded: 0,
        }
    }

    fn record_settled(&mut self, status: AttemptStatus) {
        self.ledger.push_back(status);
        while self.ledger.len() > SETTLED_LEDGER_CAP {
            self.ledger.pop_front();
        }
    }

    /// 扫描时顺带清理已析构的弱引用。
    fn live_attempts(&mut self) -> Vec<Arc<SharedAttempt>> {
        self.registered
            .retain(|_, weak| weak.strong_count() > 0);
        self.registered.values().filter_map(Weak::upgrade).collect()
    }
}

#[derive(Debug, Default)]
struct Registry {
    next_attempt_id: u64,
    buckets: HashMap<String, Bucket>,
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

fn lock_registry() -> MutexGuard<'static, Registry> {
    registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn snapshot_bucket(identity: &EndpointIdentity) -> EndpointInFlightStatus {
    let key = identity.key().unwrap_or(UNIDENTIFIED_BUCKET);
    let mut registry = lock_registry();
    let Some(bucket) = registry.buckets.get_mut(key) else {
        return EndpointInFlightStatus {
            identity: identity.clone(),
            in_flight: 0,
            remote_result_unknown: 0,
            settled: 0,
            in_flight_attempts: Vec::new(),
            recent_settled: Vec::new(),
        };
    };
    let live = bucket.live_attempts();
    let mut in_flight_attempts: Vec<AttemptStatus> = Vec::new();
    for attempt in &live {
        let status = attempt.status();
        if status.is_in_flight() {
            in_flight_attempts.push(status);
        }
    }
    in_flight_attempts.sort_by_key(|status| status.attempt_id);
    EndpointInFlightStatus {
        identity: bucket.identity.clone(),
        in_flight: in_flight_attempts.len() as u64,
        remote_result_unknown: bucket.remote_result_unknown_recorded,
        settled: bucket.settled_recorded,
        in_flight_attempts,
        recent_settled: bucket.ledger.iter().cloned().collect(),
    }
}

/// 按端点身份查询在途状态（不等待）。
#[must_use]
pub fn local_endpoint_inflight(identity: &EndpointIdentity) -> EndpointInFlightStatus {
    snapshot_bucket(identity)
}

/// 按端点身份做**有界**排空等待。
///
/// 等待本端在途登记全部取得终止事实（结清或转入远端结果未知）；`bound` 用尽即返回。
/// 返回的 [`EndpointDrainReport`] 必须能区分"已知客户端已排空"与"存在远端结果未知"。
pub async fn local_endpoint_drain(
    identity: &EndpointIdentity,
    bound: Duration,
) -> EndpointDrainReport {
    let started = Instant::now();
    loop {
        let status = snapshot_bucket(identity);
        if status.in_flight == 0 || started.elapsed() >= bound {
            return EndpointDrainReport {
                verdict: drain_verdict_for(identity, &status),
                status,
                waited: started.elapsed(),
            };
        }
        let remaining = bound.saturating_sub(started.elapsed());
        tokio::time::sleep(remaining.min(DRAIN_POLL_INTERVAL)).await;
    }
}

/// 排空进程内**全部**已知端点，并单独列出未识别身份的登记。
///
/// 切换流程建议用它做整机闸门；只要任何一个报告不允许自动切换，就不得自动切换。
pub async fn local_endpoint_drain_all(bound: Duration) -> Vec<EndpointDrainReport> {
    let identities: Vec<EndpointIdentity> = {
        let mut registry = lock_registry();
        registry.buckets.retain(|_, bucket| {
            bucket.live_attempts();
            // 只清理"彻底空且无任何记账"的桶；带远端未知记账的桶必须保留证据。
            !(bucket.registered.is_empty()
                && bucket.ledger.is_empty()
                && bucket.settled_recorded == 0
                && bucket.remote_result_unknown_recorded == 0)
        });
        registry
            .buckets
            .values()
            .map(|bucket| bucket.identity.clone())
            .collect()
    };
    let mut reports = Vec::with_capacity(identities.len());
    for identity in identities {
        reports.push(local_endpoint_drain(&identity, bound).await);
    }
    if reports.is_empty() {
        reports.push(EndpointDrainReport {
            status: EndpointInFlightStatus {
                identity: EndpointIdentity::unsupported(),
                in_flight: 0,
                remote_result_unknown: 0,
                settled: 0,
                in_flight_attempts: Vec::new(),
                recent_settled: Vec::new(),
            },
            verdict: DrainVerdict::ClientSettled,
            waited: Duration::ZERO,
        });
    }
    reports
}

/// 整机闸门：全部报告都允许自动切换时才为 `true`。
#[must_use]
pub fn drain_reports_permit_automatic_switch(reports: &[EndpointDrainReport]) -> bool {
    reports.iter().all(EndpointDrainReport::permits_automatic_switch)
}

/// 由身份 + 快照算出排空判定（不等待；[`local_endpoint_drain`] 内部同样用它）。
#[must_use]
pub fn drain_verdict_for(
    identity: &EndpointIdentity,
    status: &EndpointInFlightStatus,
) -> DrainVerdict {
    if !identity.is_known() {
        // 约束 3：身份未知/不支持 → 不能自动切换，且**不得**解释成"没有在途请求"。
        return DrainVerdict::IdentityUnknown;
    }
    if status.in_flight > 0 {
        return DrainVerdict::InFlightPending;
    }
    if status.remote_result_unknown > 0 {
        return DrainVerdict::ClientSettledWithRemoteResultUnknown;
    }
    DrainVerdict::ClientSettled
}

/// 端点排空判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainVerdict {
    /// 身份未知 / 该连接不支持身份查询 → 拒绝自动切换（不得当作空闲）。
    IdentityUnknown,
    /// 已知客户端已排空，且不存在远端结果未知。**只描述本端**：不表示服务端已停止生成。
    ClientSettled,
    /// 已知客户端已排空，但仍存在远端结果未知的请求（不能归零）。
    ClientSettledWithRemoteResultUnknown,
    /// 本端仍持有在途请求（快照时仍存在，或有界等待已用尽）。
    InFlightPending,
}

impl DrainVerdict {
    /// 是否允许自动切换：只有"已知客户端已排空且无远端未知"才允许。
    #[must_use]
    pub const fn permits_automatic_switch(self) -> bool {
        matches!(self, Self::ClientSettled)
    }

    /// 稳定标签。
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::IdentityUnknown => "identity-unknown",
            Self::ClientSettled => "client-settled",
            Self::ClientSettledWithRemoteResultUnknown => "client-settled-remote-unknown",
            Self::InFlightPending => "in-flight-pending",
        }
    }
}

/// 端点侧在途快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointInFlightStatus {
    /// 端点身份。
    pub identity: EndpointIdentity,
    /// 本端仍持有的在途登记数。
    pub in_flight: u64,
    /// 仍记为"远端结果未知"的数量（Drop 不会让它归零）。
    pub remote_result_unknown: u64,
    /// 累计结清数。
    pub settled: u64,
    /// 在途登记明细。
    pub in_flight_attempts: Vec<AttemptStatus>,
    /// 最近结清明细（有上限）。
    pub recent_settled: Vec<AttemptStatus>,
}

impl EndpointInFlightStatus {
    /// 是否仍有在途请求。
    #[must_use]
    pub const fn has_in_flight(&self) -> bool {
        self.in_flight > 0
    }

    /// 是否存在远端结果未知。
    #[must_use]
    pub const fn has_remote_result_unknown(&self) -> bool {
        self.remote_result_unknown > 0
    }
}

/// 受控排空报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointDrainReport {
    /// 端点侧快照。
    pub status: EndpointInFlightStatus,
    /// 排空判定。
    pub verdict: DrainVerdict,
    /// 实际等待时长。
    pub waited: Duration,
}

impl EndpointDrainReport {
    /// 服务身份是否已知。
    #[must_use]
    pub fn identity_known(&self) -> bool {
        self.status.identity.is_known()
    }

    /// **已知客户端已排空**（本端不再持有在途登记且身份已知）。
    ///
    /// 术语严格：这不是"服务端所有生成均已停止"，也不是"远端结果已确定"。
    #[must_use]
    pub fn client_drained(&self) -> bool {
        self.identity_known() && self.status.in_flight == 0
    }

    /// 远端结果是否仍未知（不能归零的那部分）。
    #[must_use]
    pub const fn remote_result_unknown(&self) -> u64 {
        self.status.remote_result_unknown
    }

    /// 是否允许自动切换：身份未知、仍有在途、或存在远端结果未知，都不允许。
    #[must_use]
    pub const fn permits_automatic_switch(&self) -> bool {
        self.verdict.permits_automatic_switch()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(tag: &str, endpoint: &str) -> EndpointIdentity {
        EndpointIdentity::from_resolved_config(tag, endpoint)
    }

    /// 约束 6：请求尚未发出即失败或取消 → 可以结清该请求登记（不产生远端未知）。
    #[test]
    fn abort_before_dispatch_settles_registration() {
        let identity = identity("custom", "http://127.0.0.1:1/abort-before-dispatch");
        let guard = InFlightGuard::register(&identity, RequestMode::NonStreaming);
        assert_eq!(guard.status().phase, AttemptPhase::NotDispatched);
        assert!(guard.status().is_in_flight());

        assert_eq!(
            guard.settle(TerminationFact::RemoteResultUnknown),
            SettleOutcome::Settled
        );
        let status = guard.status();
        assert_eq!(status.phase, AttemptPhase::Settled);
        assert_eq!(
            status.terminal_fact,
            Some(TerminationFact::AbortedBeforeDispatch)
        );
        assert_eq!(status.settlements, 1);

        let status = local_endpoint_inflight(&identity);
        assert_eq!(status.in_flight, 0);
        assert_eq!(status.remote_result_unknown, 0);
    }

    /// 约束 6：收到响应头但正文仍在生成 → 仍是在途（不算排空）；
    /// 只有拿到完整结束事实的那一条才结清。
    #[test]
    fn dispatched_attempt_stays_in_flight_until_termination_fact() {
        let identity = identity("custom", "http://127.0.0.1:1/dispatched-stays-in-flight");
        let streaming = InFlightGuard::register(&identity, RequestMode::Streaming);
        streaming.mark_dispatched();
        let sibling = InFlightGuard::register(&identity, RequestMode::Streaming);
        sibling.mark_dispatched();

        let status = local_endpoint_inflight(&identity);
        assert_eq!(status.in_flight, 2, "已派发·流处理中仍是在途");
        assert_eq!(
            status.in_flight_attempts[0].phase,
            AttemptPhase::Dispatched
        );

        streaming.settle(TerminationFact::ProtocolCompletion);
        let status = local_endpoint_inflight(&identity);
        assert_eq!(status.in_flight, 1, "只有取得完整结束事实的那条才结清");
        assert!(status.has_in_flight());
        drop(sibling);
    }

    /// 约束 6/8：已派发的流被提前 Drop → 远端结果未知，而不是归零。
    #[test]
    fn dropped_dispatched_stream_becomes_remote_unknown_not_zero() {
        let identity = identity("custom", "http://127.0.0.1:1/drop-unknown");
        {
            let guard = InFlightGuard::register(&identity, RequestMode::Streaming);
            guard.mark_dispatched();
            let status = local_endpoint_inflight(&identity);
            assert_eq!(status.in_flight, 1);
        }
        let status = local_endpoint_inflight(&identity);
        assert_eq!(status.in_flight, 0, "本端已不再持有");
        assert_eq!(
            status.remote_result_unknown, 1,
            "提前 Drop 只能结束本地持有，不能归零"
        );
        let report = super::drain_verdict_for(&identity, &status);
        assert_eq!(report, DrainVerdict::ClientSettledWithRemoteResultUnknown);
        assert!(!report.permits_automatic_switch());
    }

    /// 约束 6/8：尚未派发就被 Drop → 只结清登记，不留远端未知。
    #[test]
    fn dropped_never_dispatched_attempt_settles_cleanly() {
        let identity = identity("custom", "http://127.0.0.1:1/drop-clean");
        {
            let _guard = InFlightGuard::register(&identity, RequestMode::NonStreaming);
        }
        let status = local_endpoint_inflight(&identity);
        assert_eq!(status.in_flight, 0);
        assert_eq!(status.remote_result_unknown, 0);
        assert_eq!(status.settled, 1);
    }

    /// 约束 6：超时/取消后丢弃流 → 远端结果未知；迟到可信结束事实 → 对账结清且不重放任务。
    #[test]
    fn late_trusted_fact_reconciles_unknown_without_replay() {
        let identity = identity("custom", "http://127.0.0.1:1/late-fact");
        let guard = InFlightGuard::register(&identity, RequestMode::Streaming);
        guard.mark_dispatched();

        assert_eq!(
            guard.mark_remote_result_unknown(),
            SettleOutcome::Settled,
            "本端超时/放弃等待：进入远端结果未知"
        );
        assert_eq!(local_endpoint_inflight(&identity).remote_result_unknown, 1);

        assert_eq!(
            guard.settle(TerminationFact::ProtocolCompletion),
            SettleOutcome::ReconciledLateFact
        );
        let status = guard.status();
        assert_eq!(status.phase, AttemptPhase::Settled);
        assert!(status.reconciled_late);
        assert_eq!(status.settlements, 1, "对账不算第二次结算");
        let bucket = local_endpoint_inflight(&identity);
        assert_eq!(bucket.remote_result_unknown, 0, "迟到事实把未知对账掉了");
        assert_eq!(bucket.settled, 1);
        assert_eq!(
            super::drain_verdict_for(&identity, &bucket),
            DrainVerdict::ClientSettled
        );
    }

    /// 约束 6：收到重复终止回调 → 只能结算一次。
    #[test]
    fn repeated_terminal_callbacks_settle_once() {
        let identity = identity("custom", "http://127.0.0.1:1/repeat");
        let guard = InFlightGuard::register(&identity, RequestMode::Streaming);
        guard.mark_dispatched();

        assert_eq!(
            guard.settle(TerminationFact::ProtocolCompletion),
            SettleOutcome::Settled
        );
        assert_eq!(
            guard.settle(TerminationFact::ProtocolCompletion),
            SettleOutcome::AlreadySettled
        );
        assert_eq!(
            guard.mark_remote_result_unknown(),
            SettleOutcome::AlreadySettled
        );
        let status = guard.status();
        assert_eq!(status.settlements, 1);
        assert_eq!(status.terminal_fact, Some(TerminationFact::ProtocolCompletion));
        drop(guard);
        let bucket = local_endpoint_inflight(&identity);
        assert_eq!(bucket.settled, 1);
        assert_eq!(bucket.remote_result_unknown, 0);
    }

    /// 约束 6：已派发却声称"尚未发出" → 降级为远端结果未知，不能凭此结清。
    #[test]
    fn dispatched_attempt_cannot_claim_abort_before_dispatch() {
        let identity = identity("custom", "http://127.0.0.1:1/downgrade");
        let guard = InFlightGuard::register(&identity, RequestMode::NonStreaming);
        guard.mark_dispatched();
        assert_eq!(
            guard.settle(TerminationFact::AbortedBeforeDispatch),
            SettleOutcome::DowngradedToRemoteResultUnknown
        );
        assert_eq!(
            guard.status().terminal_fact,
            Some(TerminationFact::RemoteResultUnknown)
        );
    }

    /// 约束 3：身份未知/不支持 → 拒绝自动切换，且不得被当作"没有在途请求"。
    ///
    /// 用**确定性的空快照**表达"零在途 + 身份未知"这一最容易误判的情形
    /// （不依赖进程级注册表里其它并行用例的登记）。
    #[test]
    fn unknown_identity_never_permits_automatic_switch() {
        let unknown = EndpointIdentity::unsupported();
        assert!(!unknown.is_known());
        assert_eq!(unknown.source(), EndpointIdentitySource::Unsupported);

        let empty = EndpointInFlightStatus {
            identity: unknown.clone(),
            in_flight: 0,
            remote_result_unknown: 0,
            settled: 0,
            in_flight_attempts: Vec::new(),
            recent_settled: Vec::new(),
        };
        assert!(!empty.has_in_flight());

        let verdict = super::drain_verdict_for(&unknown, &empty);
        assert_eq!(verdict, DrainVerdict::IdentityUnknown);
        assert!(!verdict.permits_automatic_switch());
        assert_ne!(verdict, DrainVerdict::ClientSettled);

        let report = EndpointDrainReport {
            status: empty,
            verdict,
            waited: Duration::ZERO,
        };
        assert!(!report.client_drained(), "身份未知不得声称已排空");
        assert!(!report.identity_known());
    }

    /// 约束 4：身份不含密钥，且不由端口号推导进程所有权。
    #[test]
    fn identity_excludes_secrets_and_port_derived_ownership() {
        let first = identity("custom", "http://user:sk-secret@127.0.0.1:8765/alpha/v1?api_key=sk-secret");
        let second = identity("custom", "http://127.0.0.1:8765/beta/v1");
        let third = identity("custom", "http://127.0.0.1:8766/beta/v1");

        assert_eq!(
            first.resolved_endpoint(),
            Some("http://127.0.0.1:8765/alpha/v1")
        );
        let key = first.key().expect("identity key");
        assert!(!key.contains("sk-secret"), "身份键不得含密钥: {key}");
        assert!(!key.contains("user:"), "身份键不得含 userinfo: {key}");
        assert!(!key.contains("api_key"), "身份键不得含 query: {key}");
        assert_eq!(first.source(), EndpointIdentitySource::ResolvedConfig);

        // 同端口不同路径 → 不同身份；不同端口自然不同身份：端口本身不产生身份。
        assert_ne!(first.key(), second.key());
        assert_ne!(second.key(), third.key());
    }

    /// 约束 4：托管实例记录是身份旁证，且不会外溢到同端口的其它端点。
    #[test]
    fn managed_instance_record_refines_identity_only_for_its_endpoint() {
        let endpoint = "http://127.0.0.1:8790/managed/v1";
        let sibling = "http://127.0.0.1:8790/sibling/v1";
        let before = identity("custom", endpoint);
        assert_eq!(before.source(), EndpointIdentitySource::ResolvedConfig);

        let previous = record_managed_instance(endpoint, "instance-rpr11a");
        assert!(previous.is_none());
        let after = identity("custom", endpoint);
        assert_eq!(after.source(), EndpointIdentitySource::ManagedInstance);
        assert_eq!(after.managed_instance(), Some("instance-rpr11a"));
        assert_ne!(before.key(), after.key());
        assert!(
            after.key().is_some_and(|key| key.contains("instance-rpr11a")),
            "托管实例记录应进入身份键"
        );
        let sibling_identity = identity("custom", sibling);
        assert_eq!(sibling_identity.managed_instance(), None);
        assert_eq!(
            sibling_identity.source(),
            EndpointIdentitySource::ResolvedConfig
        );

        assert_eq!(
            clear_managed_instance(endpoint),
            Some("instance-rpr11a".to_string())
        );
        assert_eq!(
            identity("custom", endpoint).source(),
            EndpointIdentitySource::ResolvedConfig
        );
    }

    /// 约束 1：有界排空在"流仍持有"时超时返回，且不谎报排空。
    #[tokio::test]
    async fn bounded_drain_reports_in_flight_while_stream_is_held() {
        let identity = identity("custom", "http://127.0.0.1:1/bounded-drain");
        let guard = InFlightGuard::register(&identity, RequestMode::Streaming);
        guard.mark_dispatched();

        let report = local_endpoint_drain(&identity, Duration::from_millis(30)).await;
        assert_eq!(report.verdict, DrainVerdict::InFlightPending);
        assert!(!report.client_drained());
        assert!(!report.permits_automatic_switch());
        assert!(report.waited >= Duration::from_millis(20), "应有界等待");

        // 消费者不再轮询但流仍持有 → 依旧不算排空。
        let report = local_endpoint_inflight(&identity);
        assert!(report.has_in_flight());

        guard.settle(TerminationFact::ProtocolCompletion);
        let report = local_endpoint_drain(&identity, Duration::from_millis(30)).await;
        assert_eq!(report.verdict, DrainVerdict::ClientSettled);
        assert!(report.client_drained());
        assert!(report.permits_automatic_switch());
    }

    /// 约束 1/2：排空等待能等到在途结清（跨任务并发结清）。
    #[tokio::test]
    async fn bounded_drain_waits_until_settlement() {
        let identity = identity("custom", "http://127.0.0.1:1/drain-waits");
        let guard = InFlightGuard::register(&identity, RequestMode::Streaming);
        guard.mark_dispatched();
        let settle_handle = guard.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            settle_handle.settle(TerminationFact::ProtocolCompletion);
        });

        let report = local_endpoint_drain(&identity, Duration::from_millis(500)).await;
        assert_eq!(report.verdict, DrainVerdict::ClientSettled);
        assert!(report.client_drained());
        assert!(report.waited >= Duration::from_millis(30));
    }

    /// 约束 3：未识别身份的登记不被丢掉，且整机闸门会因此拒绝自动切换。
    ///
    /// 注意：注册表是进程级的，本用例只断言"与自身相关的桶"，不假设全局只有一条登记
    /// （`cargo test` 默认并行跑，其它用例也会登记）。
    #[tokio::test]
    async fn unidentified_attempts_are_reported_and_block_switch() {
        let unknown = EndpointIdentity::unsupported();
        let guard = InFlightGuard::register(&unknown, RequestMode::Streaming);
        guard.mark_dispatched();

        let status = local_endpoint_inflight(&unknown);
        assert!(status.has_in_flight());
        assert_eq!(
            super::drain_verdict_for(&unknown, &status),
            DrainVerdict::IdentityUnknown
        );

        let reports = local_endpoint_drain_all(Duration::from_millis(20)).await;
        let unidentified_report = reports
            .iter()
            .find(|report| report.status.identity == unknown)
            .expect("未识别身份的登记必须出现在整机排空报告里");
        assert_eq!(unidentified_report.verdict, DrainVerdict::IdentityUnknown);
        assert!(unidentified_report.status.in_flight >= 1);
        assert!(!unidentified_report.client_drained());
        assert!(!drain_reports_permit_automatic_switch(&reports));

        guard.settle(TerminationFact::ProtocolCompletion);
        let report = local_endpoint_drain(&unknown, Duration::from_millis(20)).await;
        assert_eq!(report.verdict, DrainVerdict::IdentityUnknown);
        assert!(
            !report.permits_automatic_switch(),
            "身份未知即便零在途也不得自动切换"
        );
        assert!(!report.client_drained());
    }

    /// 非流式请求也覆盖完整生命周期：登记 → 派发 → 读到完整响应才结清。
    #[test]
    fn non_streaming_lifecycle_is_fully_covered() {
        let identity = identity("custom", "http://127.0.0.1:1/non-streaming");
        let guard = InFlightGuard::register(&identity, RequestMode::NonStreaming);
        assert_eq!(guard.status().phase, AttemptPhase::NotDispatched);
        assert_eq!(guard.status().mode, RequestMode::NonStreaming);
        guard.mark_dispatched();
        assert_eq!(guard.status().phase, AttemptPhase::Dispatched);
        guard.settle(TerminationFact::ProtocolCompletion);
        let status = guard.status();
        assert_eq!(status.phase, AttemptPhase::Settled);
        assert!(status.dispatched);
        assert_eq!(status.settlements, 1);
    }

    /// 传输层失败（超时/连接）→ 远端结果未知；完整错误响应 → 结清。
    #[test]
    fn error_classification_distinguishes_transport_from_response() {
        let transport_identity = identity("custom", "http://127.0.0.1:1/transport-error");
        let guard = InFlightGuard::register(&transport_identity, RequestMode::NonStreaming);
        guard.mark_dispatched();
        let error = ApiError::RetriesExhausted {
            attempts: 2,
            last_error: Box::new(ApiError::BackoffOverflow {
                attempt: 3,
                base_delay: Duration::from_millis(1),
            }),
        };
        assert_eq!(guard.settle_from_error(&error), SettleOutcome::Settled);
        assert_eq!(
            guard.status().terminal_fact,
            Some(TerminationFact::RemoteResultUnknown)
        );
    }
}
