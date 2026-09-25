//! 收尾策略：**业务期限与收尾期限分开**，收尾独立且有界。
//!
//! 两个期限的职责不同：
//!
//! - **业务 deadline**（由 [`crate::CuDeadline`] 承载）：规划、模型请求、新点击、新拖拽等
//!   任务操作。业务 deadline 到期后**不得启动新的业务操作**。
//! - **cleanup deadline**（本模块）：停止旧 helper、必要释放、有限等待与收尾对账。
//!   到期仍不确定则隔离，**不延长、不重试循环**。
//!
//! 允许**有限的安全收尾**超出业务期限（这就是两个期限分开的原因），但**不允许**借收尾
//! 继续规划、重画或补做任务。
//!
//! ## 数值口径
//!
//! [`CleanupPolicy`] 里的三个数值是**本轮裁决给出的待验默认值，不是实测时延**：
//! 协作退出等待 ≤ 2 秒、从第一次进入取消/异常收尾起自动收尾总窗口 ≤ 4 秒、
//! 独立释放最多一次且等待 ≤ `min(2 秒, 剩余收尾时间)`。
//! 三者集中定义在这里，**不得在多处各自调大**。
//!
//! ## 不承诺"释放一定成功"
//!
//! 必须保证的是"执行有界的收尾协议"：不能确认释放与静止时，明确记录未知
//! （[`CleanupReleaseStatus::Unconfirmed`]）并阻止后续输入。四秒结束**也不证明**
//! 原生阻塞操作已立即停止。

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// 协作退出等待上限（**待验默认值，非实测时延**）。
pub const DEFAULT_COOPERATIVE_EXIT_GRACE_MS: u64 = 2_000;
/// 自动收尾总窗口上限，自第一次进入取消/异常收尾起算（**待验默认值，非实测时延**）。
pub const DEFAULT_CLEANUP_WINDOW_MS: u64 = 4_000;
/// 独立（补发）释放的等待上限（**待验默认值，非实测时延**）。
pub const DEFAULT_INDEPENDENT_RELEASE_WAIT_CAP_MS: u64 = 2_000;

/// 管道收尾的诊断等待片长（**不是收尾策略数值**，不参与"四秒窗口"的定义）。
///
/// 它只回答一个问题："把**已经收到**的回执取回来最多等多久"。
/// 读取线程由原生监督器持有（`windows-process-guard` 的有界管道读取器）：
/// 它只看"已经可读"的字节、不发出可能无界阻塞的读取，因此这里等的线程
/// **已经具备退出条件**——孙进程是否仍持有管道写端都不影响它退出。
///
/// 等待时长仍受收尾窗口约束（取 `min(片长, 剩余收尾时间)`），
/// 所以它既不会延长四秒窗口，也不会挤掉独立释放的额度。
pub const PIPE_DRAIN_DIAGNOSTIC_WAIT_MS: u64 = 250;

/// 收尾策略：三个数值的唯一集中定义点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupPolicy {
    cooperative_exit_grace_ms: u64,
    cleanup_window_ms: u64,
    independent_release_wait_cap_ms: u64,
}

impl CleanupPolicy {
    #[must_use]
    pub const fn new(
        cooperative_exit_grace_ms: u64,
        cleanup_window_ms: u64,
        independent_release_wait_cap_ms: u64,
    ) -> Self {
        Self {
            cooperative_exit_grace_ms,
            cleanup_window_ms,
            independent_release_wait_cap_ms,
        }
    }

    /// 协作退出等待：≤ 2 秒（待验默认值）。
    #[must_use]
    pub const fn cooperative_exit_grace(self) -> Duration {
        Duration::from_millis(self.cooperative_exit_grace_ms)
    }

    /// 自动收尾总窗口：≤ 4 秒（待验默认值）。
    #[must_use]
    pub const fn cleanup_window(self) -> Duration {
        Duration::from_millis(self.cleanup_window_ms)
    }

    /// 独立释放等待上限：≤ 2 秒（待验默认值）。
    #[must_use]
    pub const fn independent_release_wait_cap(self) -> Duration {
        Duration::from_millis(self.independent_release_wait_cap_ms)
    }
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        Self::new(
            DEFAULT_COOPERATIVE_EXIT_GRACE_MS,
            DEFAULT_CLEANUP_WINDOW_MS,
            DEFAULT_INDEPENDENT_RELEASE_WAIT_CAP_MS,
        )
    }
}

/// 收尾截止时间：**从第一次进入取消/异常收尾起固定**，重复信号不得刷新。
///
/// 终止 helper、等待其静止、独立释放、同步对账**共同消耗**这一个窗口；
/// 它们不能各自再领取一个完整窗口。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupDeadline {
    started_at: Instant,
    started_at_unix_ms: u64,
    deadline_at: Instant,
    policy: CleanupPolicy,
}

impl CleanupDeadline {
    /// 建立收尾截止时间。只在"第一次进入取消/异常收尾"时调用一次。
    #[must_use]
    pub fn establish(policy: CleanupPolicy, started_at_unix_ms: u64) -> Self {
        Self::establish_at(policy, started_at_unix_ms, Instant::now())
    }

    /// 可注入起点的建立（测试用）。
    #[must_use]
    pub fn establish_at(
        policy: CleanupPolicy,
        started_at_unix_ms: u64,
        started_at: Instant,
    ) -> Self {
        Self {
            started_at,
            started_at_unix_ms,
            deadline_at: started_at + policy.cleanup_window(),
            policy,
        }
    }

    /// 取"已经固定的收尾截止时间"，没有时才建立。
    ///
    /// 这是"重复信号不得刷新"的实现点：后续的取消/到期/异常信号拿到的是**同一个**
    /// `deadline_at`，不可能靠再来一次信号把 4 秒窗口续上。
    #[must_use]
    pub fn fixed(
        slot: &mut Option<Self>,
        policy: CleanupPolicy,
        started_at_unix_ms: u64,
    ) -> Self {
        *slot.get_or_insert_with(|| Self::establish(policy, started_at_unix_ms))
    }

    #[must_use]
    pub const fn policy(self) -> CleanupPolicy {
        self.policy
    }

    #[must_use]
    pub const fn started_at(self) -> Instant {
        self.started_at
    }

    #[must_use]
    pub const fn started_at_unix_ms(self) -> u64 {
        self.started_at_unix_ms
    }

    #[must_use]
    pub const fn deadline_at(self) -> Instant {
        self.deadline_at
    }

    #[must_use]
    pub fn remaining_at(self, now: Instant) -> Duration {
        self.deadline_at.saturating_duration_since(now)
    }

    #[must_use]
    pub fn is_expired_at(self, now: Instant) -> bool {
        now >= self.deadline_at
    }

    /// 协作退出等待：`min(2 秒, 剩余收尾时间)`。
    #[must_use]
    pub fn cooperative_exit_grace_at(self, now: Instant) -> Duration {
        self.remaining_at(now).min(self.policy.cooperative_exit_grace())
    }

    /// 独立释放等待上限：`min(2 秒, 剩余收尾时间)`。
    #[must_use]
    pub fn independent_release_wait_at(self, now: Instant) -> Duration {
        self.remaining_at(now)
            .min(self.policy.independent_release_wait_cap())
    }
}

/// 释放状态：只报告能确认的事实，确认不了就明确写未确认。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupReleaseStatus {
    /// 本次收尾没有释放义务（例如根本没有按下过按键）。
    NotNeeded,
    /// 已确认释放。
    Confirmed,
    /// 无法确认释放。
    Unconfirmed,
}

impl CleanupReleaseStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotNeeded => "not_needed",
            Self::Confirmed => "confirmed",
            Self::Unconfirmed => "unconfirmed",
        }
    }

    /// 未知即隔离：未确认释放必须按 [未解除的释放义务] 处理。
    #[must_use]
    pub const fn quarantines(self) -> bool {
        matches!(self, Self::Unconfirmed)
    }
}

/// helper 输出管道（stdout/stderr）的收尾事实。
///
/// 与"协议事实"分开记录：协议事实来自 helper 自己写出的进度文件／协议记录，
/// 这里记的是**宿主侧管道收尾**发生了什么——读取线程是否**核实**结束、
/// 有没有输出被限额截断、未核实的读取资源现在由谁持有。
///
/// 「未核实」不等于「读取仍在运行」，更不等于「输入执行者仍在运行」：
/// 它只说明宿主**没有拿到**"读取线程已终止"的证据（[`Self::is_supervision_fault`]）。
/// 因此它**不**改写释放义务（不伪造"释放未知"），只作为监督/I/O 故障记录在案。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeSupervisionFacts {
    /// stdout 读取线程是否**核实**结束（`WaitForSingleObject` 返回已终止）。
    pub stdout_confirmed: bool,
    /// stderr 读取线程是否**核实**结束。
    pub stderr_confirmed: bool,
    /// 未核实结束、**仍由监督器持有**的读取数量（有上限的残留登记）。
    ///
    /// 口径（RD4-09 §B-8 / 第七轮 §4.2）：这个数反映**真实受持有数量**——
    /// 一个读取器恰好贡献 1，无论它是因为"收尾未核实"还是"契约被破坏"而留在登记里；
    /// 被**拒绝接纳**的读取器（根本没有创建）贡献 0。也就是说它**不是**
    /// "把所有错误状态都计为 1"。
    ///
    /// 这些读取器仍在监督器手里（线程、句柄、缓冲区都能继续核实与回收），
    /// 因此这个数**不**是内存字节数、也**不**是精确的运行线程数。
    pub readers_retained: u32,
    /// 因输出限额而未保留的字节数：**已经收到的部分照常保留**，缺口如实计数。
    pub dropped_bytes: u64,
    /// 是否发生过截断。
    pub truncated: bool,
    /// 管道收尾实际等待的毫秒数（与协作退出、终止等待、独立释放共同消耗同一个窗口）。
    pub waited_ms: u64,
    /// 是否两个流都观察到"全部写端已关闭"（EOF）。
    pub eof_seen: bool,
    /// **超出正常容量不变量、但仍被监督器持有**的读取数量：明确的故障状态。
    ///
    /// 与旧口径的关键区别（第七轮 §4.1/§4.2）：契约被破坏时**不丢任何所有权**——
    /// 读取线程、线程句柄、管道与缓冲区都留在登记项里，仍可核实与回收；这些读取器
    /// 同时计入 `readers_retained`（它们是**真实受持有**的），本字段单独给出**超出量**。
    /// 进程级的累计故障次数与"接纳是否仍被锁住"见
    /// `windows_process_guard::pipe_supervision_fault()`。
    pub capacity_fault_units: u64,
}

impl PipeSupervisionFacts {
    /// 两个读取线程都**核实**结束。
    #[must_use]
    pub const fn readers_confirmed(self) -> bool {
        self.stdout_confirmed && self.stderr_confirmed
    }

    /// 是否存在"证据缺口"：截断或读取未核实结束。
    #[must_use]
    pub const fn has_evidence_gap(self) -> bool {
        self.truncated || !self.readers_confirmed()
    }

    /// 是否属于**监督／I/O 故障**：还有读取没有核实结束，或出现了超出容量不变量的持有。
    ///
    /// 注意：这**不是**"输入执行者未知"。只有日志读取线程残留、而输入安全另有独立证据时，
    /// 事实就该落在这里——不去伪造一个"释放未知"。
    #[must_use]
    pub const fn is_supervision_fault(self) -> bool {
        !self.readers_confirmed() || self.capacity_fault_units > 0
    }
}

/// helper 层（`input_stroke`）的收尾事实。
///
/// 它与业务期限无关，因此不写 `business_deadline_ms`：业务截止时刻属于运行级报告
/// （[`CleanupReport`]），由知道 CU 截止时间的控制器填写。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelperCleanupFacts {
    /// 停止接纳新业务输入的时刻（第一次进入取消/异常收尾）。
    pub stopped_new_input_at_ms: u64,
    /// 收尾开始时刻。
    pub cleanup_started_at_ms: u64,
    /// 收尾结束时刻。
    pub cleanup_finished_at_ms: u64,
    /// 协作退出阶段实际等待的毫秒数。
    pub cooperative_exit_waited_ms: u64,
    /// 是否请求终止并强杀 helper。
    pub forced_kill: bool,
    /// 是否补发过独立释放（**最多一次**）。
    pub independent_release_issued: bool,
    /// 独立释放实际等待的毫秒数（≤ `min(2s, 剩余收尾时间)`）。
    pub independent_release_wait_ms: u64,
    /// 独立释放是否被确认。
    pub independent_release_confirmed: bool,
    /// 是否因为收尾窗口已经到期而放弃补发（不延长、不重试循环）。
    pub independent_release_skipped_window_expired: bool,
    pub release: CleanupReleaseStatus,
    /// 管道收尾事实；`None` = 本次没有管道参与（例如请求根本没送到 helper）。
    ///
    /// 协议事实与普通日志分别保留：这里不承载 helper 的执行回执，
    /// 只承载"管道收尾做到了哪一步"以及"缺了哪一块证据"。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pipe: Option<PipeSupervisionFacts>,
}

/// 运行级收尾报告：五项时刻/状态**分开给出**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupReport {
    /// 业务截止时刻（CU 截止时间）。
    pub business_deadline_ms: u64,
    /// 停止接纳新业务输入的时刻。
    pub new_business_input_stopped_at_ms: u64,
    /// 收尾开始时刻。
    pub cleanup_started_at_ms: u64,
    /// 收尾结束时刻。
    pub cleanup_finished_at_ms: u64,
    pub release: CleanupReleaseStatus,
    /// 是否进入隔离（释放未确认 → 未知即隔离）。
    pub quarantined: bool,
    /// 收尾是否超出了业务期限（允许有限安全收尾超出业务期限）。
    pub exceeded_business_deadline: bool,
    /// helper 层的收尾细节；没有 helper 参与时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helper: Option<HelperCleanupFacts>,
}

impl CleanupReport {
    /// 由运行级时刻与 helper 事实组装。`release` 取 helper 事实（有则为准）或调用方判定。
    #[must_use]
    pub fn assemble(
        business_deadline_ms: u64,
        new_business_input_stopped_at_ms: u64,
        cleanup_started_at_ms: u64,
        cleanup_finished_at_ms: u64,
        release: CleanupReleaseStatus,
        helper: Option<HelperCleanupFacts>,
    ) -> Self {
        Self {
            business_deadline_ms,
            new_business_input_stopped_at_ms,
            cleanup_started_at_ms,
            cleanup_finished_at_ms,
            release,
            quarantined: release.quarantines(),
            exceeded_business_deadline: new_business_input_stopped_at_ms >= business_deadline_ms
                || cleanup_finished_at_ms > business_deadline_ms,
            helper,
        }
    }
}

/// 当前 unix 毫秒；只在记录事实时使用。
#[must_use]
pub fn unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_defaults_are_the_pending_verification_values() {
        let policy = CleanupPolicy::default();
        assert_eq!(policy.cooperative_exit_grace(), Duration::from_secs(2));
        assert_eq!(policy.cleanup_window(), Duration::from_secs(4));
        assert_eq!(policy.independent_release_wait_cap(), Duration::from_secs(2));
    }

    #[test]
    fn the_whole_cleanup_window_is_shared_and_never_reissued_per_stage() {
        let policy = CleanupPolicy::default();
        let start = Instant::now();
        let deadline = CleanupDeadline::establish_at(policy, 1_000, start);

        // 协作退出最多 2 秒。
        assert_eq!(deadline.cooperative_exit_grace_at(start), Duration::from_secs(2));
        // 协作退出已经等满 2 秒：剩下的窗口只有 2 秒，独立释放不能再拿一个完整的 2 秒。
        let after_grace = start + Duration::from_secs(2);
        assert_eq!(
            deadline.independent_release_wait_at(after_grace),
            Duration::from_secs(2)
        );
        assert_eq!(deadline.remaining_at(after_grace), Duration::from_secs(2));
        // 再等 1 秒：独立释放的等待上限跟着剩余窗口一起缩小。
        let later = start + Duration::from_secs(3);
        assert_eq!(
            deadline.independent_release_wait_at(later),
            Duration::from_secs(1)
        );
        // 窗口到期：不再有任何等待额度。
        let expired = start + Duration::from_secs(4);
        assert!(deadline.is_expired_at(expired));
        assert_eq!(deadline.independent_release_wait_at(expired), Duration::ZERO);
        assert_eq!(deadline.cooperative_exit_grace_at(expired), Duration::ZERO);
    }

    #[test]
    fn repeated_signals_never_refresh_a_fixed_cleanup_deadline() {
        let policy = CleanupPolicy::default();
        let mut slot = None;

        let first = CleanupDeadline::fixed(&mut slot, policy, 1_000);
        let first_deadline_at = first.deadline_at();
        let first_started_at = first.started_at();
        assert_eq!(slot, Some(first));

        // 第二次、第三次信号（取消 / 到期 / 需收尾错误）都不得刷新窗口。
        let second = CleanupDeadline::fixed(&mut slot, policy, 3_000);
        let third = CleanupDeadline::fixed(&mut slot, policy, 9_000);
        assert_eq!(second.deadline_at(), first_deadline_at);
        assert_eq!(third.deadline_at(), first_deadline_at);
        assert_eq!(second.started_at(), first_started_at);
        assert_eq!(third.started_at(), first_started_at);
        assert_eq!(third.started_at_unix_ms(), 1_000, "首个信号的事实必须保留");
        assert_eq!(slot, Some(first));
    }

    #[test]
    fn finer_policy_makes_the_window_shared_between_all_cleanup_steps() {
        // 4 秒窗口 + 2 秒协作退出 + 2 秒独立释放：三者相加正好用满，不会超。
        let policy = CleanupPolicy::new(2_000, 4_000, 2_000);
        let start = Instant::now();
        let deadline = CleanupDeadline::establish_at(policy, 0, start);
        let grace = deadline.cooperative_exit_grace_at(start);
        let release = deadline.independent_release_wait_at(start + grace);
        assert!(grace + release <= policy.cleanup_window());
    }

    #[test]
    fn unconfirmed_release_is_reported_as_unknown_and_quarantined() {
        let report = CleanupReport::assemble(
            10_000,
            10_500,
            10_500,
            14_400,
            CleanupReleaseStatus::Unconfirmed,
            None,
        );
        assert_eq!(report.release.as_str(), "unconfirmed");
        assert!(report.quarantined, "未确认释放必须按未知隔离");
        assert!(report.exceeded_business_deadline);

        let clean = CleanupReport::assemble(
            10_000,
            9_000,
            9_000,
            10_200,
            CleanupReleaseStatus::Confirmed,
            None,
        );
        assert!(!clean.quarantined);
        assert!(clean.exceeded_business_deadline, "收尾允许有限超出业务期限");

        let not_needed = CleanupReport::assemble(
            10_000,
            9_000,
            9_000,
            9_100,
            CleanupReleaseStatus::NotNeeded,
            None,
        );
        assert!(!not_needed.quarantined);
        assert!(!not_needed.exceeded_business_deadline);
    }

    #[test]
    fn helper_facts_and_run_report_are_serializable_and_separate() {
        let helper = HelperCleanupFacts {
            stopped_new_input_at_ms: 100,
            cleanup_started_at_ms: 100,
            cleanup_finished_at_ms: 2_100,
            cooperative_exit_waited_ms: 2_000,
            forced_kill: true,
            independent_release_issued: true,
            independent_release_wait_ms: 2_000,
            independent_release_confirmed: true,
            independent_release_skipped_window_expired: false,
            release: CleanupReleaseStatus::Confirmed,
            pipe: Some(PipeSupervisionFacts {
                stdout_confirmed: true,
                stderr_confirmed: true,
                readers_retained: 0,
                dropped_bytes: 0,
                truncated: false,
                waited_ms: 12,
                eof_seen: true,
                capacity_fault_units: 0,
            }),
        };
        let report = CleanupReport::assemble(50_000, 100, 100, 2_100, helper.release, Some(helper));
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"business_deadline_ms\":50000"), "{json}");
        assert!(json.contains("\"release\":\"confirmed\""), "{json}");
        let decoded: CleanupReport = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, report);
        assert_eq!(decoded.helper.unwrap().cooperative_exit_waited_ms, 2_000);
    }

    /// CU-F02：管道收尾事实与协议事实分开，且"未核实结束"不被说成"已结束"。
    #[test]
    fn pipe_supervision_facts_report_evidence_gaps_without_faking_completion() {
        let confirmed = PipeSupervisionFacts {
            stdout_confirmed: true,
            stderr_confirmed: true,
            readers_retained: 0,
            dropped_bytes: 0,
            truncated: false,
            waited_ms: 7,
            eof_seen: true,
            capacity_fault_units: 0,
        };
        assert!(confirmed.readers_confirmed());
        assert!(!confirmed.has_evidence_gap());
        assert!(!confirmed.is_supervision_fault());

        // 未核实结束：记监督/I/O 故障，但**不**因此改写释放义务。
        let retained = PipeSupervisionFacts {
            stdout_confirmed: false,
            stderr_confirmed: true,
            readers_retained: 1,
            eof_seen: false,
            ..confirmed
        };
        assert!(!retained.readers_confirmed());
        assert!(retained.has_evidence_gap());
        assert!(retained.is_supervision_fault());

        // 截断：已收到的部分仍保留，缺口以字节数明示。
        let truncated = PipeSupervisionFacts {
            dropped_bytes: 4_096,
            truncated: true,
            ..confirmed
        };
        assert!(truncated.has_evidence_gap());
        assert!(!truncated.is_supervision_fault(), "截断本身不是监督故障");
        assert_eq!(truncated.dropped_bytes, 4_096);
    }

    /// 管道收尾的诊断片长**不是**收尾策略数值：四秒窗口的三个数不变。
    #[test]
    fn pipe_drain_slice_does_not_change_the_cleanup_policy_numbers() {
        assert_eq!(PIPE_DRAIN_DIAGNOSTIC_WAIT_MS, 250);
        let policy = CleanupPolicy::default();
        assert_eq!(policy.cooperative_exit_grace(), Duration::from_secs(2));
        assert_eq!(policy.cleanup_window(), Duration::from_secs(4));
        assert_eq!(policy.independent_release_wait_cap(), Duration::from_secs(2));
        assert!(
            Duration::from_millis(PIPE_DRAIN_DIAGNOSTIC_WAIT_MS) < policy.cooperative_exit_grace(),
            "诊断片长必须远小于协作退出额度，否则会挤掉独立释放"
        );
    }
}
