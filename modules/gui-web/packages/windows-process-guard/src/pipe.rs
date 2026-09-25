//! 有界进程管道收尾：读取线程、线程句柄与缓冲区的**唯一**所有权点。
//!
//! ## 背景（CU-F01 §B-32 / CU-F02）
//!
//! 受控输入 helper 是 `powershell.exe`，它的 stdout/stderr 是匿名管道。helper 自己
//! 通常会经 `Add-Type` 起一个编译器子进程（`csc.exe`）；Rust std 为子进程创建的管道
//! 写端是**可继承**的，于是孙进程会一起持有写端。被强杀的 helper 退出之后，写端仍然
//! 被孙进程持有，读取线程的 `ReadFile` **拿不到 EOF**——"等读取线程结束"的任何无界
//! 等待都会把四秒收尾拖成无界等待。这就是"四秒收尾在机制上失效"的真实原因。
//!
//! 本模块给出的机制：
//!
//! 1. **读取绝不无界阻塞**（[`PipeReadStrategy::PollAvailable`]，生产默认）：
//!    先用 `PeekNamedPipe` 看有多少字节**已经**可读，只对"已经可读"的字节发 `ReadFile`。
//!    因此读取线程一定会到达退出条件，**与孙进程是否退出无关**。
//! 2. **不依赖 EOF 取回执**：[`PipeReaderSupervisor::snapshot`] 在任何时刻都能取到
//!    "已收到"的字节；[`PipeReaderSupervisor::drain`] 返回的是"**已核实**线程结束 +
//!    已收到字节"，而不是"读到了 EOF"。
//! 3. **取消不等于结束**：[`PipeReaderSupervisor::cancel_synchronous_io`] 只表达
//!    "取消请求已发出"；完成状态只能由 `WaitForSingleObject` 核实（
//!    [`PipeReaderSupervisor::verified_completion`]）。禁止"发出取消 ⇒ 认为读取已结束"。
//! 4. **未回收资源不丢**：[`PipeReaderSupervisor::drain`] 无法核实结束时，读取线程句柄
//!    与缓冲区一起进入有上限的残留登记（[`MAX_RETAINED_PIPE_READERS`]），
//!    回收是**非阻塞**清扫，不会因为一个永不结束的任务阻塞其他所有任务；
//!    登记满时是**明确的故障状态**（[`PipeSupervisorFault`]），不是静默丢弃。
//! 5. **容量义务在创建之前预留，并一直持有到实际回收**（RD4-09 §B-8 / 第七轮 §4.1）：
//!    创建任何可能留下残留的读取器**之前**先占用 1 个容量单位
//!    （[`reserve_pipe_reader_capacity`]）；预留**从创建前一直持有到实际回收**——
//!    转入残留登记（[`PipeRetentionOutcome::Retained`]，`Active → Retained`）只是状态转换，
//!    令牌跟着读取器进入登记项，**不在收尾时重新竞争另一个容量池**。
//!    正常路径的不变量：
//!    `预留未创建 + 活动未回收 + 已保留未回收 ≤` [`MAX_RETAINED_PIPE_READERS`]。
//!    容量不足时在**创建读取器**这一步就明确拒绝（[`PipeCapacityDenial`]），
//!    **不创建任何新的失管读取器、不产生任何输入**；也**不允许**靠"驱逐旧未知读取器、
//!    丢掉它的句柄、再复用账本槽位"来恢复容量——只有**已核实结束**的残留才会被回收腾位。
//! 6. **契约被破坏时：仍然持有、锁住接纳、如实记账、不自动恢复**
//!    （[`PipeRetentionOutcome::CapacityFault`]）。这条分支**不丢任何所有权**：
//!    读取线程、线程句柄、管道句柄与缓冲区都留在登记项里（仍可核实与回收），
//!    同时该监督器的新 helper 接纳被**锁住**（[`PipeCapacityDenial::reason`] 给出
//!    [`PIPE_CAPACITY_FAULT`]），直到故障对象回收并**显式核查**
//!    （[`review_and_clear_pipe_capacity_fault`]）。
//!    正常接纳路径已经前置预留，因此这条分支的存在意义是**故障注入可验证**，
//!    而**不是**"文档宣称不可达"。
//! 7. **不碰别人的进程**：本模块只读自己拿到的管道句柄、只等自己创建的线程；
//!    不终止任何进程（终止仍由 [`crate::terminate_owned_process`] 按身份做）。
//!
//! ## 计量单位与"上限到底限了什么"（RD4-09 §B-8 核对结论，§C-18 细化）
//!
//! * 容量单位是**实际 reader**，不是 helper：一个 helper 同时占 stdout／stderr 时是
//!   **两个**单位（[`PIPE_READERS_PER_HELPER`]）；需要独立清理运行（独立释放）的执行者
//!   还要为它**一起**预留一份（见 [`HelperPipeReadersAdmission::acquire_with_cleanup_reserve`]）。
//! * 上限约束的是"**仍被监督器持有、可核实的读取资源**"的数量
//!   （[`PipeReaderCapacity`] 把"预留未创建／活动未回收／已保留未回收"三个桶分开给出）。
//! * 这些计数**不是**内存字节数，也**不是**精确的运行线程数；按 **reader 唯一身份**聚合，
//!   因此"错误回传的值"与"监督器汇总的值"不会对同一个读取器重复计数
//!   （每个读取器恰好持有一个容量单位，转交与登记都不改变这一点）。
//! * 旧形态里那条"丢登记对象 + 关线程句柄 + 分离线程、而线程仍存活"的分支
//!   （`unmanaged_dropped`）已经**不存在**：契约被破坏时对象仍受管理，超出量记在
//!   [`PipeReaderCapacity::capacity_fault_units`]；`unowned_unreclaimed` 因此是历史
//!   计数位，正常与故障路径都应为 0。
//!
//! ## 明确不做的事
//!
//! * **不**用"Drop 掉 `JoinHandle`"当作停止线程的手段（那只是分离线程）；
//!   连"复制线程句柄失败"这种局部失败也**不**丢 `JoinHandle`——对象转入登记继续受管理，
//!   核实与取消降级到 `JoinHandle`（同一内核证据）；
//! * **不**在 `drop` 无法核实时假装资源已释放；
//! * **不**定义第二套收尾数值：等待时长一律由调用方按既有
//!   `cleanup.rs` 的收尾窗口传入。

use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::process::{ChildStderr, ChildStdout};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_BROKEN_PIPE, ERROR_NOT_FOUND, HANDLE,
    WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::IO::CancelSynchronousIo;
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, WaitForSingleObject};

use crate::KernelHandle;

/// 单个管道最多保留的字节数。
///
/// 超出的字节**不静默丢弃**：它们被计入 [`PipeSnapshot::dropped_bytes`]，
/// 并把 `truncated` 置为真——"已经收到的执行回执"仍在缓冲区里，缺口作为事实上报。
pub const MAX_PIPE_BUFFER_BYTES: usize = 256 * 1024;

/// 轮询"有多少字节可读"的间隔。
const PIPE_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// 停止位置起之后，仍允许把"已经到达管道"的字节读掉的**最大轮数**（有界收尾）。
///
/// 停止位不是"立刻停"，而是"不再有新的等待"：已经写进管道的回执要收下，
/// 但不允许靠"还有数据"无限续期。
const POST_STOP_MAX_ROUNDS: u32 = 8;

/// 连续读错误达到这个次数就放弃读取（错误码被记录，不无限重试同一错误）。
const MAX_CONSECUTIVE_ERRORS: u32 = 32;

/// 一次 `ReadFile` 最多请求的字节数。
const READ_CHUNK_BYTES: usize = 64 * 1024;

/// 未核实结束的读取最多寄存多少份（**数量上限**）。
///
/// 达到上限是明确的故障状态（[`PipeSupervisorFault`]），不是"丢掉句柄然后宣称已结束"。
///
/// 这个数值是**设计上限**（RD4-09 §B-8），**不是**已证明最优的容量；本轮不调大它。
/// 它约束的是"已残留登记 + 已预留"之和，见模块文档的计量单位一节。
pub const MAX_RETAINED_PIPE_READERS: usize = 8;

/// 一个 helper 的收尾需要几个读取器容量单位：stdout + stderr = **两个**。
///
/// 容量计量单位必须是**实际 reader**：不能把"一个 helper 的两条流"算成一个。
pub const PIPE_READERS_PER_HELPER: usize = 2;

/// 容量不足时给出的**明确拒绝码**：调用方可以据此把"拒绝接纳"与其它 I/O 错误分开。
pub const PIPE_CAPACITY_EXHAUSTED: &str = "pipe_reader_capacity_exhausted";

/// **容量／所有权契约已被破坏**时的拒绝码：**严重内部故障**，不是"容量暂时不够"。
///
/// 与 [`PIPE_CAPACITY_EXHAUSTED`] 分开的两个理由：① 容量不足是**正常**的拒绝（没有创建
/// 读取器、没有产生输入，重试有意义）；② 这个码表示实际状态已经违反正常容量或所有权
/// 契约（例如出现了**没有预留**的读取器要求登记），此时监督器**锁住**该进程的 helper
/// 接纳，直到故障对象回收并完成显式核查（[`review_and_clear_pipe_capacity_fault`]）。
pub const PIPE_CAPACITY_FAULT: &str = "pipe_reader_capacity_fault";

/// **在创建读取器之前已经占用**的容量单位总数
/// （= `预留未创建 + 活动未回收 + 已保留未回收`）。
///
/// 预留槽位**从读者创建前一直持有到实际回收**：转入残留登记（`Active → Retained`）只是
/// 状态转换，令牌随读取器一起进入登记项，**不在收尾时重新竞争另一个容量池**。
///
/// 计数器之间的每一次转换都是**单独一步原子操作**（`reserve` 的两步在登记表锁内完成，
/// 读侧也全部在同一个锁内），因此"三个桶的和 ≤ 上限"这个不变量在锁内永远可核实。
static RESERVED_READER_CAPACITY: AtomicUsize = AtomicUsize::new(0);

/// 上面的总数里**尚未交给任何读取器**的部分（`预留未创建`）。
///
/// 其余部分由具体读取器持有（活动的或已登记保留的），因此
/// `活动 + 已保留 = 总数 − 本值`。
static UNCREATED_READER_CAPACITY: AtomicUsize = AtomicUsize::new(0);

/// 读取器容量与所有权的当前账目：**唯一**的容量事实来源。
///
/// ## 这些数是什么、不是什么
///
/// 它们是"**读取器容量单位**"的计数，按 **reader 唯一身份**聚合（每个读取器恰好一个
/// 单位）。它们**不是**内存字节数，也**不是**精确的运行线程数：名称不得与
/// 字节/线程数混用（读取缓冲区字节数另见 [`retained_pipe_reader_bytes`]，
/// 运行线程数只能由各读取器的完成核实给出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeReaderCapacity {
    /// 上限（[`MAX_RETAINED_PIPE_READERS`]）：设计上限，不是实测最优值。
    pub limit: usize,
    /// `reserved_not_created`：已取得预留、**尚未创建**读取器的单位。
    pub reserved_not_created: usize,
    /// `active`（活动未回收）：已创建、仍由监督器持有、尚未回收的读取器。
    pub active_unreclaimed: usize,
    /// `retained`（已保留未回收）：未核实结束、转入残留登记且**仍由监督器持有**的读取器。
    ///
    /// 故障态（契约被破坏）下**超出正常不变量**却仍被持有的读取器也在这里面，
    /// 其超出量另见 [`Self::capacity_fault_units`]。
    pub retained_unreclaimed: usize,
    /// 累计：因容量不足被**拒绝接纳**的次数。
    ///
    /// **拒绝本身不代表创建了 reader**：拒绝发生在创建读取线程之前（因此也不产生任何输入）。
    pub admission_rejected: u64,
    /// 累计：进入"容量／所有权契约被破坏"故障状态的读取器数量。
    pub capacity_faults: u64,
    /// 当前**超出正常不变量、但仍被持有**的读取器数量（故障态的实际数量）。
    pub capacity_fault_units: usize,
    /// 累计：已知未回收、但当前**已失去**管理所有权的读取器数量。
    ///
    /// 这是旧形态（`Saturated` 丢登记对象 + 关线程句柄 + 分离线程）的计数位。修复后
    /// 故障对象**仍受监督器管理**，因此正常路径与故障路径都应为 0；保留这个位就是为了
    /// 让"分离但存活"（未回收且**未受持有**）与"故障但仍持有"在诊断上**不可能被混为一谈**。
    pub unowned_unreclaimed: u64,
    /// 新 helper 的接纳是否**被锁住**：故障未完成核查前不自动恢复服务能力。
    pub admission_locked: bool,
}

impl PipeReaderCapacity {
    /// `unreclaimed`：已创建但尚未完成回收的读取器（活动 + 已保留）。
    ///
    /// **不代表**它们一定还在执行：其中可能有已经自行结束、只是还没被核实回收的。
    #[must_use]
    pub const fn unreclaimed(self) -> usize {
        self.active_unreclaimed
            .saturating_add(self.retained_unreclaimed)
    }

    /// `owned_unreclaimed`：未回收，且产品**仍持有可管理**的对象与句柄（读取线程、
    /// 线程句柄、管道句柄与缓冲区）——故障态超限持有的读取器同样计入（它们仍受管理）。
    ///
    /// 在本实现里所有"已创建未回收"的读取器都由监督器持有，因此它等于
    /// [`Self::unreclaimed`]；一旦两者出现差异，就说明有读取器已经失去管理所有权，
    /// 这正是必须把 `unreclaimed` 与 `owned_unreclaimed` 分开报告的原因。
    #[must_use]
    pub const fn owned_unreclaimed(self) -> usize {
        self.unreclaimed()
    }

    /// 已创建未回收 + 预留未创建 = **正常不变量的义务**。
    ///
    /// 正常路径上 `预留未创建 + 活动未回收 + 已保留未回收 ≤ limit`（见模块文档）。
    /// 故障态下"仍被持有"的数量可能超过上限：超出量记在 [`Self::capacity_fault_units`]，
    /// **不**被当成可用容量。
    #[must_use]
    pub const fn obligations(self) -> usize {
        self.reserved_not_created
            .saturating_add(self.active_unreclaimed)
            .saturating_add(self.retained_unreclaimed)
    }

    /// 还能接纳多少个 reader（故障锁生效时恒为 0）。
    #[must_use]
    pub const fn available(self) -> usize {
        if self.admission_locked {
            return 0;
        }
        self.limit.saturating_sub(self.obligations())
    }

    /// 现在能否接纳 `units` 个 reader（`units == 0` 恒为假；故障锁生效时恒为假）。
    #[must_use]
    pub const fn can_admit(self, units: usize) -> bool {
        !self.admission_locked && units > 0 && units <= self.available()
    }
}

impl fmt::Display for PipeReaderCapacity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "capacity[limit={} reserved_not_created={} active={} retained={} obligations={} \
             available={} unreclaimed={} owned_unreclaimed={} admission_rejected={} \
             capacity_faults={} capacity_fault_units={} unowned_unreclaimed={} admission_locked={}]",
            self.limit,
            self.reserved_not_created,
            self.active_unreclaimed,
            self.retained_unreclaimed,
            self.obligations(),
            self.available(),
            self.unreclaimed(),
            self.owned_unreclaimed(),
            self.admission_rejected,
            self.capacity_faults,
            self.capacity_fault_units,
            self.unowned_unreclaimed,
            self.admission_locked
        )
    }
}

/// 容量不足或故障锁导致的**明确拒绝**：发生在这里就意味着"**没有创建**读取器"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeCapacityDenial {
    pub requested: usize,
    pub available: usize,
    pub limit: usize,
    /// 拒绝时的实际持有量（`活动未回收 + 已保留未回收`）。
    pub held: usize,
    /// 拒绝时的"预留未创建"数量。
    pub reserved_not_created: usize,
    /// 这次拒绝是来自**故障锁**（[`PIPE_CAPACITY_FAULT`]）还是**容量不足**
    /// （[`PIPE_CAPACITY_EXHAUSTED`]）。
    pub locked_by_fault: bool,
}

impl PipeCapacityDenial {
    /// 拒绝码：`PIPE_CAPACITY_FAULT`（严重内部故障）或 `PIPE_CAPACITY_EXHAUSTED`（容量不足）。
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        if self.locked_by_fault {
            PIPE_CAPACITY_FAULT
        } else {
            PIPE_CAPACITY_EXHAUSTED
        }
    }
}

impl fmt::Display for PipeCapacityDenial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}: 拒绝接纳读取器（请求 {}，可用 {}，上限 {}，已持有 {}，预留未创建 {}{}）——未创建任何读取器",
            self.reason(),
            self.requested,
            self.available,
            self.limit,
            self.held,
            self.reserved_not_created,
            if self.locked_by_fault {
                "；接纳锁生效中，需显式核查故障后才能恢复"
            } else {
                ""
            }
        )
    }
}

impl std::error::Error for PipeCapacityDenial {}

/// 容量预留令牌：**在创建读取器之前**取得，创建时交给读取器（或被显式归还）。
///
/// 令牌存活期间，这部分容量**不会**被第二个申请者拿走；令牌被 Drop 或 [`Self::release`]
/// 时归还。读取器被**核实结束**（句柄随线程终止释放）时，令牌随之归还——因此
/// "容量按**实际完成**释放"，而不是靠驱逐未知读取器。
#[derive(Debug)]
#[must_use = "预留令牌必须交给读取器（with_options_and_reservation）或显式归还"]
pub struct PipeReaderCapacityReservation {
    units: usize,
    /// 是否在账上；`false` 的令牌不影响账目（已归还，或故障注入用的"不计账"令牌）。
    counted: bool,
    /// 这份单位是否仍记在"**预留未创建**"桶里（交给读取器之后就不再是）。
    ///
    /// 归还时必须按它决定要不要同时退出"预留未创建"：否则会留下"账目上凭空少一份预留"
    /// 的幽灵（那正是 `available()` 会算错的地方）。
    uncreated: bool,
}

impl PipeReaderCapacityReservation {
    #[must_use]
    pub const fn units(&self) -> usize {
        self.units
    }

    /// 是否在账上（`false` = 已归还或本来就不计账）。
    #[must_use]
    pub const fn is_counted(&self) -> bool {
        self.counted
    }

    /// 显式归还（幂等）。
    pub fn release(mut self) {
        self.release_in_place();
    }

    /// 计算出可切出的单位数（忽略 `units == 0` 与不计账令牌）。
    fn split_units(&mut self, units: usize) -> Option<usize> {
        if !self.counted || units == 0 {
            return None;
        }
        let taken = units.min(self.units);
        self.units -= taken;
        Some(taken)
    }

    /// 从这份预留里切出 `units` 个单位（切出来的仍是"预留未创建"，
    /// 真正的归属转换发生在它被交给读取器时——见 [`Self::handed_to_reader`]）。
    ///
    /// 已经归还（不计账）的令牌切出的是**不计账**占位令牌：**它不是合法的生产路径**
    /// （那等于"没有预留就创建读取器"），只用于故障注入。
    fn split_for_reader(&mut self, units: usize) -> Self {
        let Some(taken) = self.split_units(units) else {
            return Self::uncounted(units.min(self.units));
        };
        Self {
            units: taken,
            counted: true,
            uncreated: true,
        }
    }

    /// 把这份预留言**交给一个读取器**：离开"预留未创建"桶（总数不变）。
    ///
    /// 这是唯一的归属转换点，因此无论调用方是先切分再创建、还是直接把整份预留交给
    /// 读取器（含故障注入路径），"预留未创建"都不会留下幽灵额度。
    fn handed_to_reader(mut self) -> Self {
        if self.counted && self.uncreated {
            self.uncreated = false;
            let units = self.units;
            let _ = UNCREATED_READER_CAPACITY.fetch_update(
                Ordering::AcqRel,
                Ordering::Acquire,
                |current: usize| Some(current.saturating_sub(units)),
            );
        }
        self
    }

    /// 从这份预留里切出 `units` 个单位交给**另一个预留**（仍在"预留未创建"桶里）。
    ///
    /// 用于把"可能的一次清理"额度从普通动作的凭证里转交出去：它**还没**交给任何读取器，
    /// 因此必须继续算在"预留未创建"里；否则"没被用到就被丢弃"时会留下幽灵额度。
    fn split_for_reservation(&mut self, units: usize) -> Option<Self> {
        let taken = self.split_units(units)?;
        Some(Self {
            units: taken,
            counted: true,
            uncreated: true,
        })
    }

    fn release_in_place(&mut self) {
        if !self.counted {
            return;
        }
        self.counted = false;
        let units = self.units;
        // 单步原子转换：这份容量只在**读取器真的走到回收点**（Drop）时归还。
        // 饱和减法保证重复/迟到归还不会把账目减成负数（也就不会"多释放"槽位）。
        let _ = RESERVED_READER_CAPACITY.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |current: usize| Some(current.saturating_sub(units)),
        );
        if self.uncreated {
            // 还没交给读取器的部分同时退出"预留未创建"桶（否则会留下幽灵额度）。
            let _ = UNCREATED_READER_CAPACITY.fetch_update(
                Ordering::AcqRel,
                Ordering::Acquire,
                |current: usize| Some(current.saturating_sub(units)),
            );
        }
    }

    /// **故障注入/内部占位**：不计账的令牌——只用于"契约被破坏"的确切语义核对
    /// （"登记表不超过 8 本身不证明资源有界"、部分创建的额度归属）。
    /// **生产接纳路径不得使用它**：生产创建读取器时手里一定有一份在账上的预留。
    const fn uncounted(units: usize) -> Self {
        Self {
            units,
            counted: false,
            uncreated: false,
        }
    }
}

impl Drop for PipeReaderCapacityReservation {
    fn drop(&mut self) {
        self.release_in_place();
    }
}

/// 当前容量账目（诊断/测试/调用方决策用；**不**等待任何线程）。
///
/// 组合值是"同一时刻的账目快照"：每一项各自精确，且**所有读取都在登记表锁内**，
/// 因此不可能读到"半个申请"或"半次归还"。
#[must_use]
pub fn pipe_reader_capacity() -> PipeReaderCapacity {
    // 先做一次非阻塞清扫：已核实结束的残留先腾位，账目给出的是"现在真正能用的容量"。
    let _ = reclaim_finished_pipe_readers();
    let registry = lock_registry();
    capacity_locked(&registry)
}

/// 在持锁状态下读出账目：`活动 = 总数 − 预留未创建 − 已保留`。
///
/// 这三个量分别由不同的原子/表项承载，但每一次转换都是**单步原子操作**，
/// 因此锁内读出的组合永远是某个真实时刻的账目。
fn capacity_locked(registry: &PipeResidueRegistry) -> PipeReaderCapacity {
    let total = RESERVED_READER_CAPACITY.load(Ordering::Acquire);
    let reserved_not_created = UNCREATED_READER_CAPACITY.load(Ordering::Acquire);
    let retained_unreclaimed = registry.entries.len();
    let active_unreclaimed = total
        .saturating_sub(reserved_not_created)
        .saturating_sub(retained_unreclaimed);
    PipeReaderCapacity {
        limit: registry.limit,
        reserved_not_created,
        active_unreclaimed,
        retained_unreclaimed,
        admission_rejected: registry.admission_rejected,
        capacity_faults: registry.capacity_faults,
        capacity_fault_units: registry.capacity_fault_units,
        unowned_unreclaimed: registry.unowned_unreclaimed,
        admission_locked: registry.admission_locked,
    }
}

/// **在创建可能留下残留的读取器之前**预留其最坏情况回收容量。
///
/// 约定：一个 helper 要预留 [`PIPE_READERS_PER_HELPER`] 个单位（两条流各一个 reader）；
/// 需要独立清理运行（独立释放）的执行者还要为它多预留一份（见
/// [`HelperPipeReadersAdmission::acquire_with_cleanup_reserve`]）。
/// 不足时返回 [`PipeCapacityDenial`]：调用方应当在**创建执行者与输入之前**据此拒绝。
/// 本函数**绝不**驱逐、丢弃或重置任何未知读取器来"恢复容量"。
pub fn reserve_pipe_reader_capacity(
    units: usize,
) -> Result<PipeReaderCapacityReservation, PipeCapacityDenial> {
    // 申请与判定都在登记表锁内：并发申请因此不可能同时拿走同一份容量。
    let mut registry = lock_registry();
    let capacity = capacity_locked(&registry);
    if !capacity.can_admit(units) {
        registry.admission_rejected = registry.admission_rejected.saturating_add(1);
        return Err(PipeCapacityDenial {
            requested: units,
            available: capacity.available(),
            limit: capacity.limit,
            held: capacity.unreclaimed(),
            reserved_not_created: capacity.reserved_not_created,
            locked_by_fault: capacity.admission_locked,
        });
    }
    // 先记"预留未创建"，再记总数：任何时刻都不会出现"预留未创建 > 总数"的可观察状态，
    // 而且这一步在锁内完成，读数侧拿到的永远是某个真实时刻的账目。
    UNCREATED_READER_CAPACITY.fetch_add(units, Ordering::AcqRel);
    RESERVED_READER_CAPACITY.fetch_add(units, Ordering::AcqRel);
    Ok(PipeReaderCapacityReservation {
        units,
        counted: true,
        uncreated: true,
    })
}

/// 一个 helper 两条输出流（stdout + stderr）的**接纳凭证**。
///
/// 用法（"在创建执行者与输入之前拒绝"）：
///
/// 1. 在 `spawn` helper **之前** [`HelperPipeReadersAdmission::acquire`]；
///    容量不足就在这里返回 [`PipeCapacityDenial`]，**不创建 helper、不创建读取器、不产生输入**；
/// 2. 取得凭证后再 `spawn`，并用 [`Self::stdout`] / [`Self::stderr`] 建立两条流；
/// 3. 每个读取器从凭证里带走 1 个单位；凭证剩余的单位在 Drop 时归还，
///    因此"中间失败"也不会漏掉容量。
///
/// 计量单位是**实际 reader**：一个 helper 占 2 个单位（[`PIPE_READERS_PER_HELPER`]）。
#[derive(Debug)]
pub struct HelperPipeReadersAdmission {
    reservation: Option<PipeReaderCapacityReservation>,
}

impl HelperPipeReadersAdmission {
    /// 预留一个 helper 所需的全部读取器容量。
    pub fn acquire() -> Result<Self, PipeCapacityDenial> {
        reserve_pipe_reader_capacity(PIPE_READERS_PER_HELPER).map(|reservation| Self {
            reservation: Some(reservation),
        })
    }

    /// 预留"**本 helper 两条流 + 可能的一次清理（独立释放）**"所需的全部容量。
    ///
    /// 顺序固定为：校验请求与执行资格 → **计算最大需求** → **一次性预留** →
    /// 创建并监督 helper → 创建读取器 → 最终输入前检查 → 允许业务输入。
    ///
    /// 为什么要把清理需求**一起**预留：独立释放会再起一个 helper，它用的是**同一套**
    /// 管道读取机制。如果普通 helper 独占全部容量，需要独立释放时就没有读取器配额
    /// （收尾死角）。因此清理配额在**普通动作接纳时**一并预留（仍计入上限
    /// [`MAX_RETAINED_PIPE_READERS`]），清理时**转用**它（
    /// [`HelperPipeReadersAdmission::take_cleanup_reservation`] →
    /// [`HelperPipeCleanupReservation::into_admission`]）：既不重新竞争普通容量，
    /// 也**不递归**为清理再创建下一层清理额度。
    pub fn acquire_with_cleanup_reserve() -> Result<Self, PipeCapacityDenial> {
        reserve_pipe_reader_capacity(PIPE_READERS_PER_HELPER.saturating_mul(2)).map(|reservation| {
            Self {
                reservation: Some(reservation),
            }
        })
    }

    /// **转交**清理预留：从本凭证里切出 [`PIPE_READERS_PER_HELPER`] 个单位，
    /// 交给将来可能的那一次清理运行。没有剩余单位时返回 `None`。
    ///
    /// 切出去的令牌**仍然在账上**（转交不等于归还）：从切出到清理运行实际回收之间，
    /// 这部分容量一直由这份预留承担义务。
    pub fn take_cleanup_reservation(&mut self) -> Option<HelperPipeCleanupReservation> {
        let units = self.remaining_units();
        if units < PIPE_READERS_PER_HELPER {
            return None;
        }
        let reservation = self
            .reservation
            .as_mut()?
            .split_for_reservation(PIPE_READERS_PER_HELPER)?;
        Some(HelperPipeCleanupReservation { reservation })
    }

    /// 用**上层已经预留**的容量建立接纳凭证（清理运行**转用**它，不重新竞争普通容量）。
    pub fn from_reservation(reservation: PipeReaderCapacityReservation) -> Self {
        Self {
            reservation: Some(reservation),
        }
    }

    /// 尚未被两条流带走的容量单位。
    #[must_use]
    pub fn remaining_units(&self) -> usize {
        self.reservation.as_ref().map_or(0, |reservation| {
            if reservation.is_counted() {
                reservation.units()
            } else {
                0
            }
        })
    }

    /// 用凭证里的 1 个单位建立 stdout 读取器。
    pub fn stdout(
        &mut self,
        label: impl Into<String>,
        pipe: ChildStdout,
    ) -> io::Result<PipeReaderSupervisor> {
        let unit = self.take_one();
        PipeReaderSupervisor::with_options_and_reservation(
            label,
            pipe,
            PipeReadStrategy::PollAvailable,
            MAX_PIPE_BUFFER_BYTES,
            unit,
        )
    }

    /// 用凭证里的 1 个单位建立 stderr 读取器。
    pub fn stderr(
        &mut self,
        label: impl Into<String>,
        pipe: ChildStderr,
    ) -> io::Result<PipeReaderSupervisor> {
        let unit = self.take_one();
        PipeReaderSupervisor::with_options_and_reservation(
            label,
            pipe,
            PipeReadStrategy::PollAvailable,
            MAX_PIPE_BUFFER_BYTES,
            unit,
        )
    }

    fn take_one(&mut self) -> PipeReaderCapacityReservation {
        self.reservation.as_mut().map_or_else(
            || PipeReaderCapacityReservation::uncounted(0),
            |reservation| reservation.split_for_reader(1),
        )
    }
}

/// 为"**可能的一次清理**（独立释放）"预留的读取器容量。
///
/// 它在**普通动作接纳时**就与普通 helper 的容量一起预留（仍计入上限），
/// 清理时转用；没有被用到的部分在 Drop 时归还。**不得**再用它去预留下一层清理额度
/// （见 [`HelperPipeReadersAdmission::acquire_with_cleanup_reserve`]）。
#[derive(Debug)]
#[must_use = "清理预留必须转交给清理运行（into_admission）或显式归还"]
pub struct HelperPipeCleanupReservation {
    reservation: PipeReaderCapacityReservation,
}

impl HelperPipeCleanupReservation {
    /// 这份清理预留的单位数（正常情况下是 [`PIPE_READERS_PER_HELPER`]）。
    #[must_use]
    pub const fn units(&self) -> usize {
        self.reservation.units()
    }

    /// 转交给清理运行：清理 helper 的两条流从这份预留里各取 1 个单位。
    #[must_use]
    pub fn into_admission(self) -> HelperPipeReadersAdmission {
        HelperPipeReadersAdmission::from_reservation(self.reservation)
    }
}

/// 读取策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeReadStrategy {
    /// 轮询"已经可读"的字节（生产默认）：不发出可能无界阻塞的读取。
    PollAvailable,
    /// 阻塞读取（**测试接缝**）：保留"确实阻塞在 `ReadFile`"的形态，
    /// 用于验证"取消同步 I/O"这条核销路径本身是否真的能结束读取。
    ///
    /// 生产实现不使用它；它存在的意义是让取消路径有真实目标可测。
    Blocking,
}

impl PipeReadStrategy {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PollAvailable => "poll_available",
            Self::Blocking => "blocking",
        }
    }
}

/// 读取线程的完成状态：**只报告能核实的事实**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeReaderCompletion {
    /// 已核实：线程句柄的等待返回 `WAIT_OBJECT_0`（线程已终止）。
    Confirmed,
    /// 未核实：**不得**据此宣称读取已结束。
    Unconfirmed,
}

impl PipeReaderCompletion {
    #[must_use]
    pub const fn is_confirmed(self) -> bool {
        matches!(self, Self::Confirmed)
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Unconfirmed => "unconfirmed",
        }
    }
}

/// 无法核实读取线程结束的原因。
///
/// `Unconfirmed` **不等于**"仍在运行"：`StillRunning` 才是"当时仍在运行"，
/// `WaitFailed` 只说明"连是否仍在运行都拿不到"。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeReaderUnconfirmed {
    /// 非阻塞/有界等待超时：线程当时仍在运行。
    StillRunning,
    /// 等待调用本身失败：状态未知。
    WaitFailed,
}

impl PipeReaderUnconfirmed {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StillRunning => "still_running",
            Self::WaitFailed => "wait_failed",
        }
    }
}

/// 取消同步 I/O 的结果。
///
/// **语义边界**（平台事实）：`CancelSynchronousIo` 的返回**不表示**被取消的 I/O
/// 已经返回，也不表示读取线程已经结束。这里如实区分三种情况。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncIoCancelOutcome {
    /// 内核接受了取消请求。被取消的 I/O 何时返回、是否返回，仍要另行核实。
    Requested,
    /// 目标线程当时没有待取消的同步 I/O（`ERROR_NOT_FOUND`）。
    ///
    /// 这同样**不是**"读取已结束"的证据：I/O 可能尚未发出，也可能正好刚刚完成。
    NothingPending,
    /// 调用本身失败。
    Failed { code: u32 },
}

impl SyncIoCancelOutcome {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Requested => "requested",
            Self::NothingPending => "nothing_pending",
            Self::Failed { .. } => "failed",
        }
    }
}

/// 某一时刻的管道读取快照（**与 EOF 无关**）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeSnapshot {
    /// 已经收到的字节（受 [`MAX_PIPE_BUFFER_BYTES`] 限制）。
    pub bytes: Vec<u8>,
    /// 因为超出上限而没有保留的字节数（明确的证据缺口）。
    pub dropped_bytes: u64,
    /// 是否发生过截断。
    pub truncated: bool,
    /// 是否观察到"全部写端已关闭"（`ERROR_BROKEN_PIPE`）。
    pub eof_seen: bool,
    /// 最近一次读取错误的 Win32 码（诊断用）。
    pub last_error_code: Option<u32>,
    /// 读取线程的**已核实**完成状态。
    pub completion: PipeReaderCompletion,
    /// 未核实时给出原因。
    pub unconfirmed_reason: Option<PipeReaderUnconfirmed>,
    /// 累计收到的取消请求次数。
    pub cancel_requests: u32,
    /// 第一次取消请求的 unix 毫秒（**只固定一次**，重复取消不刷新）。
    pub first_cancel_unix_ms: Option<u64>,
}

impl PipeSnapshot {
    /// 已收到字节的文本视图（UTF-8 宽松解码）。
    #[must_use]
    pub fn text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).to_string()
    }

    /// 已保留的字节数。
    #[must_use]
    pub fn byte_count(&self) -> u64 {
        self.bytes.len() as u64
    }
}

/// 未核实结束的读取去哪了。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeRetentionOutcome {
    /// 已核实结束：读取资源随线程终止释放，没有残留。
    Confirmed,
    /// 未核实结束的读取**仍由监督器持有**（寄存数量 = `retained`）。
    ///
    /// 这是一次纯粹的 `Active → Retained` **状态转换**：读取器在创建之前取得的容量
    /// 预留随它一起留在登记项上，**不在收尾时重新竞争另一个容量池**。
    Retained { retained: usize },
    /// **容量／所有权契约被破坏**：本次未核实的读取**仍被持有**，但它超出了正常容量
    /// 不变量（登记时的实际数量见字段）——明确的故障状态，**不表示**它已结束。
    ///
    /// 与旧形态的关键区别：**没有丢任何所有权**。读取线程、线程句柄、管道句柄与缓冲区
    /// 都在登记项里，仍可继续核实与回收；同时该监督器的新 helper 接纳被**锁住**，
    /// 直到故障对象回收并完成显式核查（[`review_and_clear_pipe_capacity_fault`]）。
    CapacityFault {
        limit: usize,
        /// 登记时的**实际**持有数量（含已超出正常不变量的部分）。
        held: usize,
        /// 当前超出正常不变量的读取器数量。
        fault_units: usize,
        /// 契约是怎么被破坏的（诊断用）。
        reason: CapacityFaultReason,
    },
}

impl PipeRetentionOutcome {
    /// 是否还有由监督器持有的未核实读取。
    ///
    /// **`CapacityFault` 返回 `true` 是正确的**：修复后故障对象确实由监督器持有
    /// （这正是"返回 true 才是正确事实"的情形）；旧形态里"被丢掉的分离线程"不会
    /// 出现在这个判断里，因为那种情形不再存在。
    #[must_use]
    pub const fn holds_unreclaimed_reader(self) -> bool {
        matches!(self, Self::Retained { .. } | Self::CapacityFault { .. })
    }

    /// 当前**超出正常不变量**、但仍被持有的读取器数量（只有 [`Self::CapacityFault`] 会 > 0）。
    ///
    /// 它**不是**"被丢掉的数量"：这些读取器仍在登记表里受管理，
    /// [`reclaim_finished_pipe_readers`] 会在核实结束后回收它们。
    #[must_use]
    pub const fn capacity_fault_units(self) -> usize {
        match self {
            Self::Confirmed | Self::Retained { .. } => 0,
            Self::CapacityFault { fault_units, .. } => fault_units,
        }
    }

    /// 是否属于容量／所有权契约被破坏的故障态。
    #[must_use]
    pub const fn is_capacity_fault(self) -> bool {
        matches!(self, Self::CapacityFault { .. })
    }

    /// 当前登记的未核实读取数量（[`Self::Retained`] 时给出）。
    #[must_use]
    pub const fn retained_count(self) -> Option<usize> {
        match self {
            Self::Confirmed | Self::CapacityFault { .. } => None,
            Self::Retained { retained } => Some(retained),
        }
    }
}

/// 容量／所有权契约被破坏的方式（诊断用，不参与判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityFaultReason {
    /// 读取器**没有携带任何计账的预留**就要求登记：接纳路径被绕过。
    ReservationAbsent,
    /// 账目说预留仍在手，但登记表已经到达上限：正常容量不变量被破坏。
    ///
    /// 这一支是**防御性**的：按设计它不可达（在手的预留本身就是容量义务的一部分），
    /// 因此它只可能由"账目与登记表不一致"造成——如实记为故障，绝不放行。
    InvariantViolated,
}

impl CapacityFaultReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReservationAbsent => "reservation_absent",
            Self::InvariantViolated => "capacity_invariant_violated",
        }
    }
}

/// 残留登记与容量账目的故障状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PipeSupervisorFault {
    /// 当前仍在登记里的未核实读取数量（**仍由监督器持有**）。
    pub retained: usize,
    /// 数量上限。
    pub limit: usize,
    /// 当前**超出正常不变量**、但仍被持有的读取器数量。
    pub capacity_fault_units: usize,
    /// 累计进入容量／所有权契约故障状态的读取器数量。
    pub capacity_faults: u64,
    /// 累计：已知未回收、但已失去管理所有权的读取器数量（修复后应为 0）。
    pub unowned_unreclaimed: u64,
    /// 接纳是否仍被锁住（需显式核查才能恢复）。
    pub admission_locked: bool,
}

impl fmt::Display for PipeSupervisorFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "supervisor-fault[retained={} limit={} capacity_fault_units={} capacity_faults={} \
             unowned_unreclaimed={} admission_locked={}]",
            self.retained,
            self.limit,
            self.capacity_fault_units,
            self.capacity_faults,
            self.unowned_unreclaimed,
            self.admission_locked
        )
    }
}

/// 管道收尾的结果：**已核实到什么**、**拿到了什么**、**未回收的在哪**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PipeDrainOutcome {
    pub label: String,
    /// 已收到的字节（未核实时也照常给出：回执不因收尾未完而丢失）。
    pub bytes: Vec<u8>,
    pub dropped_bytes: u64,
    pub truncated: bool,
    pub eof_seen: bool,
    pub last_error_code: Option<u32>,
    /// 读取线程的**已核实**完成状态。
    pub completion: PipeReaderCompletion,
    pub unconfirmed_reason: Option<PipeReaderUnconfirmed>,
    /// 本次收尾实际等待的毫秒数。
    pub waited_ms: u64,
    /// 策略（诊断用）。
    pub strategy: &'static str,
    /// 读取线程名（诊断：残留登记里能看出是谁没结束）。
    pub reader_thread_name: Option<String>,
    pub cancel_requests: u32,
    pub first_cancel_unix_ms: Option<u64>,
    /// 未核实结束的读取资源去了哪里。
    pub retention: PipeRetentionOutcome,
}

impl PipeDrainOutcome {
    /// 失败路径上的空结果：**没有**管道参与时使用（不是"读取已结束"）。
    #[must_use]
    fn unavailable(label: &str) -> Self {
        Self {
            label: label.to_string(),
            bytes: Vec::new(),
            dropped_bytes: 0,
            truncated: false,
            eof_seen: false,
            last_error_code: None,
            completion: PipeReaderCompletion::Unconfirmed,
            unconfirmed_reason: Some(PipeReaderUnconfirmed::WaitFailed),
            waited_ms: 0,
            strategy: "unavailable",
            reader_thread_name: None,
            cancel_requests: 0,
            first_cancel_unix_ms: None,
            retention: PipeRetentionOutcome::Confirmed,
        }
    }

    /// 文本视图（UTF-8 宽松解码）。
    #[must_use]
    pub fn text_lossy(&self) -> String {
        String::from_utf8_lossy(&self.bytes).to_string()
    }

    /// 是否存在"证据缺口"：截断、读取错误或未能核实结束。
    #[must_use]
    pub fn has_evidence_gap(&self) -> bool {
        self.truncated || self.last_error_code.is_some() || !self.completion.is_confirmed()
    }
}

impl fmt::Display for PipeDrainOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}[{}]: completion={} bytes={} dropped={} truncated={} eof={} waited={}ms retention={:?}",
            self.label,
            self.strategy,
            self.completion.as_str(),
            self.bytes.len(),
            self.dropped_bytes,
            self.truncated,
            self.eof_seen,
            self.waited_ms,
            self.retention
        )
    }
}

// ---------------------------------------------------------------------------
// 读取线程的内部状态
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct PipeBufferState {
    bytes: Vec<u8>,
    dropped_bytes: u64,
    truncated: bool,
    eof_seen: bool,
    last_error_code: Option<u32>,
    rounds: u64,
}

#[derive(Debug)]
struct ReaderShared {
    /// "不再发出新的等待"的协作停止位。**不是**"线程已结束"的证据。
    stop: AtomicBool,
    buffer: Mutex<PipeBufferState>,
    cancel_requests: AtomicU32,
    /// `u64::MAX` = 从未发出过取消请求。
    first_cancel_unix_ms: AtomicU64,
    last_cancel_outcome: AtomicU8,
    last_cancel_code: AtomicU32,
    /// 读取循环的轮数（诊断计数）：用来核对"受控等待"而不是忙等，
    /// 以及核对"被分离的读取线程是否仍在运行"。不参与任何策略数值。
    loops: AtomicU64,
}

impl Default for ReaderShared {
    fn default() -> Self {
        Self {
            stop: AtomicBool::new(false),
            buffer: Mutex::new(PipeBufferState::default()),
            cancel_requests: AtomicU32::new(0),
            first_cancel_unix_ms: AtomicU64::new(u64::MAX),
            last_cancel_outcome: AtomicU8::new(CANCEL_NONE),
            last_cancel_code: AtomicU32::new(0),
            loops: AtomicU64::new(0),
        }
    }
}

const CANCEL_NONE: u8 = 0;
const CANCEL_REQUESTED: u8 = 1;
const CANCEL_NOTHING_PENDING: u8 = 2;
const CANCEL_FAILED: u8 = 3;

/// 读取线程 + 它的线程句柄 + 缓冲区。**所有权的唯一持有者**。
#[derive(Debug)]
struct PipeReaderInner {
    strategy: PipeReadStrategy,
    shared: Arc<ReaderShared>,
    /// 读取线程本体：保留它才能让 `thread_handle` 一直指向**这个**线程实例。
    ///
    /// 它也承担"句柄副本不可用时"的降级核实与取消（见 [`Self::wait_thread`]）：
    /// 因此**任何情况下都不会**出现"丢掉 `JoinHandle` 让线程失管"的做法。
    thread: Option<JoinHandle<()>>,
    /// 线程句柄副本：非阻塞检查、有界等待、取消同步 I/O 都只通过它进行。
    ///
    /// `None` = 复制线程句柄失败（故障注入或真实的资源耗尽）。此时对象**仍受管理**，
    /// 核实与取消降级走 `JoinHandle`（同一内核证据，见 [`Self::wait_thread`]），
    /// 容量预留**不归还**——直到读取器真的走到回收点。
    thread_handle: Option<KernelHandle>,
    /// 这个读取器**在创建之前**就占好的容量预留：**从创建前一直持有到实际回收**。
    ///
    /// 它的三次归属：活动（含本次创建）→ 转入残留登记（登记项继续持有，只是状态转换）
    /// → 核实结束被回收时随读取器一起归还。**任何一步都不重新竞争容量池。**
    capacity: PipeReaderCapacityReservation,
}

/// 线程句柄的有界等待结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThreadWait {
    Signaled,
    TimedOut,
    Failed,
}

impl ThreadWait {
    const fn completion(self) -> PipeReaderCompletion {
        match self {
            Self::Signaled => PipeReaderCompletion::Confirmed,
            Self::TimedOut | Self::Failed => PipeReaderCompletion::Unconfirmed,
        }
    }

    const fn reason(self) -> Option<PipeReaderUnconfirmed> {
        match self {
            Self::Signaled => None,
            Self::TimedOut => Some(PipeReaderUnconfirmed::StillRunning),
            Self::Failed => Some(PipeReaderUnconfirmed::WaitFailed),
        }
    }
}

impl PipeReaderInner {
    /// **非阻塞**核实线程是否已终止。四秒终止证明的来源就是它，不是 `is_finished` 风格的猜测。
    fn verified_completion(&self) -> (PipeReaderCompletion, Option<PipeReaderUnconfirmed>) {
        let wait = self.wait_thread(Duration::ZERO);
        (wait.completion(), wait.reason())
    }

    fn wait_thread(&self, wait: Duration) -> ThreadWait {
        let Some(handle) = self.thread_handle.as_ref() else {
            // 没有句柄副本（复制失败）时的**降级核实**：对象仍由本结构持有，
            // 只是等待与取消改用 `JoinHandle`（它的 `is_finished` 在内核里就是
            // 同一线程句柄上的 `WaitForSingleObject(handle, 0)`，即同一份证据）。
            // 有界等待用有界轮询实现，**不引入第二套收尾数值**。
            return self.wait_thread_via_join_handle(wait);
        };
        let millis = wait.as_millis();
        // `INFINITE` 明确不使用：这里的每一次等待都必须有界。
        let millis = if millis > u128::from(u32::MAX - 1) {
            u32::MAX - 1
        } else {
            millis as u32
        };
        // SAFETY: thread_handle 是本结构独占的线程句柄副本，等待它不修改任何状态。
        match unsafe { WaitForSingleObject(handle.raw(), millis) } {
            WAIT_OBJECT_0 => ThreadWait::Signaled,
            WAIT_TIMEOUT => ThreadWait::TimedOut,
            WAIT_FAILED => ThreadWait::Failed,
            _ => ThreadWait::Failed,
        }
    }

    /// 句柄副本不可用时的降级路径：用 `JoinHandle::is_finished()`（同一内核证据）核实，
    /// 用有界轮询代替单次等待。
    fn wait_thread_via_join_handle(&self, wait: Duration) -> ThreadWait {
        let Some(thread) = self.thread.as_ref() else {
            return ThreadWait::Failed;
        };
        if thread.is_finished() {
            return ThreadWait::Signaled;
        }
        let started = Instant::now();
        let step = PIPE_POLL_INTERVAL.max(Duration::from_millis(1));
        while started.elapsed() < wait {
            let remaining = wait.saturating_sub(started.elapsed());
            thread::sleep(remaining.min(step));
            if thread.is_finished() {
                return ThreadWait::Signaled;
            }
        }
        if thread.is_finished() {
            ThreadWait::Signaled
        } else {
            ThreadWait::TimedOut
        }
    }

    /// 可用来做等待/取消的线程句柄：优先用句柄副本，没有就借 `JoinHandle` 的句柄值。
    fn raw_thread_handle(&self) -> Option<HANDLE> {
        if let Some(handle) = self.thread_handle.as_ref() {
            return Some(handle.raw());
        }
        self.thread
            .as_ref()
            .map(|thread| thread.as_raw_handle() as HANDLE)
    }

    fn buffer_snapshot(&self) -> PipeSnapshot {
        let (completion, unconfirmed_reason) = self.verified_completion();
        let buffer = self.lock_buffer();
        PipeSnapshot {
            bytes: buffer.bytes.clone(),
            dropped_bytes: buffer.dropped_bytes,
            truncated: buffer.truncated,
            eof_seen: buffer.eof_seen,
            last_error_code: buffer.last_error_code,
            completion,
            unconfirmed_reason,
            cancel_requests: self.shared.cancel_requests.load(Ordering::Acquire),
            first_cancel_unix_ms: self.first_cancel_unix_ms(),
        }
    }

    fn first_cancel_unix_ms(&self) -> Option<u64> {
        match self.shared.first_cancel_unix_ms.load(Ordering::Acquire) {
            u64::MAX => None,
            value => Some(value),
        }
    }

    fn lock_buffer(&self) -> MutexGuard<'_, PipeBufferState> {
        // 读取线程只在极短的临界区里持锁写入；这里沿用"毒化锁仍可用"的既有策略。
        self.shared
            .buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 发出协作停止信号：读取线程最多再读掉"已经到达管道"的字节就退出。
    ///
    /// 这一步**不构成**"读取已结束"的证据；结束只能由 [`Self::wait_thread`] 核实。
    fn request_stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
    }

    /// 取消目标线程上待处理的同步 I/O。
    ///
    /// 返回 [`SyncIoCancelOutcome::Requested`] 只说明"请求已发出"。
    fn cancel_synchronous_io(&self) -> SyncIoCancelOutcome {
        self.shared.cancel_requests.fetch_add(1, Ordering::AcqRel);
        // 第一次取消的时刻**只固定一次**：重复取消不得刷新任何窗口。
        let _ = self.shared.first_cancel_unix_ms.compare_exchange(
            u64::MAX,
            unix_ms_now(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        // SAFETY: 句柄指向本结构自己创建、自己持有的那个读取线程；取消是跨线程请求。
        // 句柄副本缺失（复制失败）时借用 `JoinHandle` 的同一线程句柄，取消能力不降级。
        let ok = match self.raw_thread_handle() {
            Some(handle) => unsafe { CancelSynchronousIo(handle) },
            None => {
                // 连线程本体都不在了：这只能发生在已被收尾的读取器上。
                self.shared
                    .last_cancel_outcome
                    .store(CANCEL_FAILED, Ordering::Release);
                self.shared.last_cancel_code.store(0, Ordering::Release);
                return SyncIoCancelOutcome::Failed { code: 0 };
            }
        };
        let outcome = if ok != 0 {
            SyncIoCancelOutcome::Requested
        } else {
            let code = io::Error::last_os_error().raw_os_error().unwrap_or_default();
            if code as u32 == ERROR_NOT_FOUND {
                SyncIoCancelOutcome::NothingPending
            } else {
                SyncIoCancelOutcome::Failed { code: code as u32 }
            }
        };
        self.shared
            .last_cancel_outcome
            .store(cancel_code_of(outcome), Ordering::Release);
        if let SyncIoCancelOutcome::Failed { code } = outcome {
            self.shared.last_cancel_code.store(code, Ordering::Release);
        }
        outcome
    }
}

fn cancel_code_of(outcome: SyncIoCancelOutcome) -> u8 {
    match outcome {
        SyncIoCancelOutcome::Requested => CANCEL_REQUESTED,
        SyncIoCancelOutcome::NothingPending => CANCEL_NOTHING_PENDING,
        SyncIoCancelOutcome::Failed { .. } => CANCEL_FAILED,
    }
}

/// 受监督的管道读取器：**读取线程、线程句柄与缓冲区的唯一所有权点**。
///
/// 用 [`PipeReaderSupervisor::stdout`] / [`PipeReaderSupervisor::stderr`] 从子进程管道构造，
/// 用 [`PipeReaderSupervisor::snapshot`] 在任意时刻取回执，
/// 用 [`PipeReaderSupervisor::drain`] 做**有界**收尾。
pub struct PipeReaderSupervisor {
    label: String,
    inner: Option<PipeReaderInner>,
}

impl fmt::Debug for PipeReaderSupervisor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PipeReaderSupervisor")
            .field("label", &self.label)
            .field("live", &self.inner.is_some())
            .finish()
    }
}

impl PipeReaderSupervisor {
    /// 监督子进程 stdout（生产策略：轮询可用字节）。
    pub fn stdout(label: impl Into<String>, pipe: ChildStdout) -> io::Result<Self> {
        Self::with_options(
            label,
            pipe,
            PipeReadStrategy::PollAvailable,
            MAX_PIPE_BUFFER_BYTES,
        )
    }

    /// 监督子进程 stderr（生产策略：轮询可用字节）。
    pub fn stderr(label: impl Into<String>, pipe: ChildStderr) -> io::Result<Self> {
        Self::with_options(
            label,
            pipe,
            PipeReadStrategy::PollAvailable,
            MAX_PIPE_BUFFER_BYTES,
        )
    }

    /// 带策略与输出上限的构造（策略是测试接缝；上限用于验证截断的缺口记录）。
    ///
    /// **接纳点**：这里在**创建读取线程之前**先占 1 个容量单位
    /// （[`reserve_pipe_reader_capacity`]）；容量不足就返回错误——**不创建线程、
    /// 不产生新的失管读取器**（交进来的管道句柄由 [`RawPipeHandle`] 关闭）。
    pub(crate) fn with_options(
        label: impl Into<String>,
        pipe: impl IntoRawHandleExt,
        strategy: PipeReadStrategy,
        output_limit: usize,
    ) -> io::Result<Self> {
        let reservation = reserve_pipe_reader_capacity(1).map_err(|denial| {
            io::Error::new(io::ErrorKind::Other, denial.to_string())
        })?;
        Self::with_options_and_reservation(label, pipe, strategy, output_limit, reservation)
    }

    /// 用**调用方事先预留**的容量建立读取器。
    ///
    /// 这是"在创建执行者与输入之前预留容量"的接缝：调用方先
    /// [`reserve_pipe_reader_capacity`]（一个 helper 预留
    /// [`PIPE_READERS_PER_HELPER`] 个单位），不足就**在创建 helper 与输入之前**拒绝；
    /// 预留成功后再把手里的令牌交给这里，令牌与读取器同生共死。
    pub(crate) fn with_options_and_reservation(
        label: impl Into<String>,
        pipe: impl IntoRawHandleExt,
        strategy: PipeReadStrategy,
        output_limit: usize,
        reservation: PipeReaderCapacityReservation,
    ) -> io::Result<Self> {
        Self::with_options_and_reservation_impl(
            label.into(),
            pipe,
            strategy,
            output_limit,
            reservation,
            HandleDuplication::Duplicate,
        )
    }

    fn with_options_and_reservation_impl(
        label: String,
        pipe: impl IntoRawHandleExt,
        strategy: PipeReadStrategy,
        output_limit: usize,
        reservation: PipeReaderCapacityReservation,
        duplication: HandleDuplication,
    ) -> io::Result<Self> {
        // 归属转换：这份预留从此由**这个**读取器持有（离开"预留未创建"桶）。
        let reservation = reservation.handed_to_reader();
        let raw = pipe.into_raw_pipe_handle();
        let thread_name = format!("coolzhu-pipe-reader-{}", sanitize_thread_name(&label));
        let shared = Arc::new(ReaderShared::default());
        let shared_for_thread = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name(thread_name)
            .spawn(move || read_pipe(raw, &shared_for_thread, strategy, output_limit))?;
        // 线程句柄副本：线程本体（JoinHandle）保留在本结构里，句柄副本才允许跨线程等待/取消。
        let thread_handle = match duplication {
            HandleDuplication::Duplicate => Some(duplicate_handle(thread.as_raw_handle() as HANDLE)?),
            // **故障注入**：跳过句柄复制，模拟"复制线程句柄失败"的真实故障形态。
            #[cfg(test)]
            HandleDuplication::Skip => None,
        };
        let Some(thread_handle) = thread_handle else {
            // **部分创建成功**：线程已经存在。此处绝不"丢 `JoinHandle` 再归还容量"
            // （那既让线程失管，又把仍在被占用的容量放回池子）。做法是：
            // ① 请求协作停止；② 连同预留令牌一起转入残留登记（**受管理**，仍可核实与回收）；
            // ③ 把这次降级如实上报给调用方——失败结果**不携带也不丢弃**资源。
            let inner = PipeReaderInner {
                strategy,
                shared,
                thread: Some(thread),
                thread_handle: None,
                capacity: reservation,
            };
            inner.request_stop();
            let retention = park_retained(label.clone(), inner);
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "读取线程句柄副本建立失败：读取器仍受监督器管理（retention={retention:?}），\
                     容量按实际回收释放"
                ),
            ));
        };
        Ok(Self {
            label,
            inner: Some(PipeReaderInner {
                strategy,
                shared,
                thread: Some(thread),
                thread_handle: Some(thread_handle),
                capacity: reservation,
            }),
        })
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// 任意时刻取"已经收到"的回执（**不依赖 EOF**）。
    ///
    /// 监督器已被收尾消费时返回 `None`：那时事实由 [`PipeDrainOutcome`] 承载。
    #[must_use]
    pub fn snapshot(&self) -> Option<PipeSnapshot> {
        self.inner.as_ref().map(PipeReaderInner::buffer_snapshot)
    }

    /// **非阻塞**核实读取线程是否已结束。
    #[must_use]
    pub fn verified_completion(&self) -> PipeReaderCompletion {
        self.inner.as_ref().map_or(
            PipeReaderCompletion::Unconfirmed,
            |inner| inner.verified_completion().0,
        )
    }

    /// 已核实结束（`true`）**只**由线程句柄的等待结果给出。
    #[must_use]
    pub fn is_confirmed_finished(&self) -> bool {
        self.verified_completion().is_confirmed()
    }

    /// 发出取消同步 I/O 的请求。返回值**不表示**读取已结束。
    pub fn cancel_synchronous_io(&self) -> SyncIoCancelOutcome {
        self.inner.as_ref().map_or(
            SyncIoCancelOutcome::Failed { code: 0 },
            PipeReaderInner::cancel_synchronous_io,
        )
    }

    /// 累计取消请求次数。
    #[must_use]
    pub fn cancel_requests(&self) -> u32 {
        self.inner
            .as_ref()
            .map_or(0, |inner| inner.shared.cancel_requests.load(Ordering::Acquire))
    }

    /// 第一次取消请求的时刻（只固定一次：重复取消不刷新窗口）。
    #[must_use]
    pub fn first_cancel_unix_ms(&self) -> Option<u64> {
        self.inner.as_ref().and_then(PipeReaderInner::first_cancel_unix_ms)
    }

    /// 有界收尾：发出停止信号，**有界等待并核实**读取线程结束。
    ///
    /// * `wait` 由调用方按既有收尾窗口给出（本模块不定义任何收尾数值）；
    /// * 已核实时：读取线程资源随线程终止释放，结果里记 [`PipeRetentionOutcome::Confirmed`]；
    /// * 未核实时：读取线程、句柄与缓冲区一起转入有上限的残留登记
    ///   （[`PipeRetentionOutcome::Retained`]），**不会**被丢弃后宣称已终止。
    pub fn drain(mut self, wait: Duration) -> PipeDrainOutcome {
        let label = self.label.clone();
        let Some(inner) = self.inner.take() else {
            return PipeDrainOutcome::unavailable(&label);
        };
        drain_inner(label, inner, wait)
    }
}

impl Drop for PipeReaderSupervisor {
    fn drop(&mut self) {
        let Some(inner) = self.inner.take() else {
            return;
        };
        // 没有走过 `drain` 就被丢弃：**绝不静默失管**。
        // 未核实结束的读取连同句柄与缓冲区转入残留登记（有上限、有故障状态）。
        if inner.verified_completion().0.is_confirmed() {
            return;
        }
        inner.request_stop();
        let _ = park_retained(self.label.clone(), inner);
    }
}

/// 读取线程句柄副本的取得方式（生产恒为 [`Self::Duplicate`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HandleDuplication {
    /// 生产路径：复制线程句柄。
    Duplicate,
    /// **故障注入**（只在测试构建里存在）：跳过复制，模拟"复制线程句柄失败"的真实故障形态。
    #[cfg(test)]
    Skip,
}

impl PipeReaderSupervisor {
    /// **故障注入**：不经过接纳预留直接建立读取器（**故意**破坏"创建前预留"的账目）。
    ///
    /// 只用于在隔离子进程里核对"契约被破坏 ⇒ 故障态"分支的确切语义：
    /// ① 返回明确的严重内部故障；② **仍保留**读取器、线程句柄、管道与缓冲区所有权；
    /// ③ 锁住新 helper 接纳；④ 记录实际数量；⑤ 不自动恢复。生产与常规测试路径不得使用。
    #[cfg(test)]
    fn fault_injection_reader_without_reservation(
        label: impl Into<String>,
        pipe: impl IntoRawHandleExt,
        strategy: PipeReadStrategy,
        output_limit: usize,
    ) -> io::Result<Self> {
        Self::with_options_and_reservation(
            label,
            pipe,
            strategy,
            output_limit,
            PipeReaderCapacityReservation::uncounted(1),
        )
    }

    /// **故障注入**：建立读取器时让"复制线程句柄"这一步失败，用于核对
    /// "部分创建成功"路径：线程必须**仍受管理**，容量**只能按实际回收释放**。
    #[cfg(test)]
    fn fault_injection_reader_without_verification_handle(
        label: impl Into<String>,
        pipe: impl IntoRawHandleExt,
        strategy: PipeReadStrategy,
        output_limit: usize,
        reservation: PipeReaderCapacityReservation,
    ) -> io::Result<Self> {
        Self::with_options_and_reservation_impl(
            label.into(),
            pipe,
            strategy,
            output_limit,
            reservation,
            HandleDuplication::Skip,
        )
    }

    /// **测试接缝**：把内部状态取出来（供测试按生产登记接口 `park_retained` 直接登记，
    /// 以及观察被分离的读取线程是否仍在运行）。
    #[cfg(test)]
    fn test_take_inner(&mut self) -> Option<PipeReaderInner> {
        self.inner.take()
    }

    /// **测试接缝**：读取线程的内部共享状态（只读观察用）。
    #[cfg(test)]
    fn test_shared(&self) -> Option<Arc<ReaderShared>> {
        self.inner.as_ref().map(|inner| Arc::clone(&inner.shared))
    }
}

impl ReaderShared {
    /// **测试接缝**：读取循环已经跑了多少轮（诊断计数）。
    #[cfg(test)]
    fn test_loops(&self) -> u64 {
        self.loops.load(Ordering::Relaxed)
    }
}

fn drain_inner(label: String, inner: PipeReaderInner, wait: Duration) -> PipeDrainOutcome {
    let started = Instant::now();
    // 先停止"新的等待"：读取线程会在一次轮询之内收下已经到达的字节并退出。
    inner.request_stop();
    let wait_outcome = inner.wait_thread(wait);
    let completion = wait_outcome.completion();
    let unconfirmed_reason = wait_outcome.reason();
    let snapshot = inner.buffer_snapshot();
    let waited_ms = millis_of(started.elapsed());
    let strategy = inner.strategy.as_str();
    // 读取线程名：句柄副本必须与"这个线程实例"一一对应，所以线程本体由监督器持有。
    let reader_thread_name = inner
        .thread
        .as_ref()
        .and_then(|thread| thread.thread().name())
        .map(str::to_string);
    let cancel_requests = inner.shared.cancel_requests.load(Ordering::Acquire);
    let first_cancel_unix_ms = inner.first_cancel_unix_ms();
    let retention = if completion.is_confirmed() {
        // 已核实终止：句柄与 JoinHandle 在本函数返回时释放。
        // 注意这里是"已核实的终止"，不是"丢句柄"。
        drop(inner);
        PipeRetentionOutcome::Confirmed
    } else {
        park_retained(label.clone(), inner)
    };
    PipeDrainOutcome {
        label,
        bytes: snapshot.bytes,
        dropped_bytes: snapshot.dropped_bytes,
        truncated: snapshot.truncated,
        eof_seen: snapshot.eof_seen,
        last_error_code: snapshot.last_error_code,
        completion,
        unconfirmed_reason,
        waited_ms,
        strategy,
        reader_thread_name,
        cancel_requests,
        first_cancel_unix_ms,
        retention,
    }
}

// ---------------------------------------------------------------------------
// 读取线程本体
// ---------------------------------------------------------------------------

/// 把"交出所有权的管道读句柄"读成一个有界读取器。
///
/// 句柄所有权在这里被接管：`File` 是唯一拥有者，线程结束时关闭它。
fn read_pipe(raw: RawPipeHandle, shared: &ReaderShared, strategy: PipeReadStrategy, limit: usize) {
    let handle = raw.handle;
    // 所有权转交给 `File`：放弃 `RawPipeHandle` 的 Drop，避免同一句柄被关闭两次。
    std::mem::forget(raw);
    // SAFETY: `handle` 由调用方以 `IntoRawHandle` 交出所有权，本次是本进程唯一接管点。
    let mut file = unsafe { File::from_raw_handle(handle) };
    match strategy {
        PipeReadStrategy::PollAvailable => poll_available(&mut file, shared, limit),
        PipeReadStrategy::Blocking => blocking_read(&mut file, shared, limit),
    }
}

/// 生产读取：只看"已经可读"的字节，**不发出可能无界阻塞的读取**。
///
/// 退出条件只有三种，且都与"孙进程是否还在"无关：观察到写端全关、协作停止位、
/// 或连续错误达到上限（错误状态被记录，不无限重试同一个错误——那会变成忙等）。
fn poll_available(file: &mut File, shared: &ReaderShared, limit: usize) {
    let mut consecutive_errors = 0u32;
    let mut post_stop_rounds = 0u32;
    let mut chunk = vec![0u8; READ_CHUNK_BYTES];
    loop {
        // 诊断计数：每轮一次；"受控等待"的证据就是"轮数 ≈ 时间/轮询间隔"，而不是忙等。
        shared.loops.fetch_add(1, Ordering::Relaxed);
        let stopped = shared.stop.load(Ordering::Acquire);
        match peek_available(file) {
            Peek::Available(available) => {
                let want = usize::try_from(available)
                    .unwrap_or(usize::MAX)
                    .min(READ_CHUNK_BYTES)
                    .max(1);
                match file.read(&mut chunk[..want]) {
                    Ok(0) => {
                        // 读回 0 字节：写端已全部关闭。
                        mark_eof(shared);
                        break;
                    }
                    Ok(read) => {
                        consecutive_errors = 0;
                        append_bytes(shared, &chunk[..read], limit);
                    }
                    Err(error) => {
                        // 读到断裂/错误都记事实；错误不无限重试（不做忙等）。
                        let broken = record_read_error(shared, &error);
                        consecutive_errors += 1;
                        if broken || stopped || consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            break;
                        }
                        thread::sleep(PIPE_POLL_INTERVAL);
                        continue;
                    }
                }
                if stopped {
                    // 停止之后仍把"已经到达管道"的字节收下，但有界（不允许靠数据续期）。
                    post_stop_rounds += 1;
                    if post_stop_rounds >= POST_STOP_MAX_ROUNDS {
                        break;
                    }
                }
            }
            Peek::Empty => {
                if stopped {
                    break;
                }
                thread::sleep(PIPE_POLL_INTERVAL);
            }
            Peek::BrokenPipe => {
                // 全部写端已关闭：这就是"读到结尾"的可观察事实（含孙进程终于退出）。
                mark_eof(shared);
                break;
            }
            Peek::Failed(code) => {
                record_error_code(shared, code);
                consecutive_errors += 1;
                if stopped || consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    break;
                }
                thread::sleep(PIPE_POLL_INTERVAL);
            }
        }
    }
}

/// 阻塞读取（**测试接缝**）：保留"确实阻塞在 `ReadFile`"的形态。
///
/// 只能靠"管道断裂"或"取消同步 I/O 真的生效"结束；一次读取返回之后也会检查停止位。
fn blocking_read(file: &mut File, shared: &ReaderShared, limit: usize) {
    let mut chunk = vec![0u8; READ_CHUNK_BYTES];
    loop {
        shared.loops.fetch_add(1, Ordering::Relaxed);
        match file.read(&mut chunk) {
            Ok(0) => {
                mark_eof(shared);
                break;
            }
            Ok(read) => {
                append_bytes(shared, &chunk[..read], limit);
                if shared.stop.load(Ordering::Acquire) {
                    break;
                }
            }
            Err(error) => {
                // 非"管道已断"的错误不会无限重试：记录之后直接结束读取线程。
                let _ = record_read_error(shared, &error);
                break;
            }
        }
    }
}

enum Peek {
    Available(u32),
    Empty,
    BrokenPipe,
    Failed(u32),
}

fn peek_available(file: &File) -> Peek {
    let mut available = 0u32;
    // SAFETY: 只查询可读字节数，不提供缓冲、也不消费数据。
    let ok = unsafe {
        PeekNamedPipe(
            file.as_raw_handle() as HANDLE,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if ok != 0 {
        return if available == 0 {
            Peek::Empty
        } else {
            Peek::Available(available)
        };
    }
    let code = io::Error::last_os_error().raw_os_error().unwrap_or_default();
    if code as u32 == ERROR_BROKEN_PIPE {
        // 全部写端已关闭：这就是"读到结尾"的可观察事实（含孙进程终于退出）。
        Peek::BrokenPipe
    } else {
        Peek::Failed(code as u32)
    }
}

fn record_error_code(shared: &ReaderShared, code: u32) {
    let mut buffer = shared
        .buffer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    buffer.last_error_code = Some(code);
}

fn mark_eof(shared: &ReaderShared) {
    let mut buffer = shared
        .buffer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    buffer.eof_seen = true;
}

fn append_bytes(shared: &ReaderShared, bytes: &[u8], limit: usize) {
    let mut buffer = shared
        .buffer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    buffer.rounds = buffer.rounds.saturating_add(1);
    let room = limit.saturating_sub(buffer.bytes.len());
    let keep = room.min(bytes.len());
    buffer.bytes.extend_from_slice(&bytes[..keep]);
    let dropped = bytes.len().saturating_sub(keep);
    if dropped > 0 {
        buffer.dropped_bytes = buffer.dropped_bytes.saturating_add(dropped as u64);
        buffer.truncated = true;
    }
}

/// 记录读取错误；返回 `true` 表示"管道已断"（属于正常的结尾观察）。
fn record_read_error(shared: &ReaderShared, error: &io::Error) -> bool {
    let code = error.raw_os_error().unwrap_or_default() as u32;
    let broken = code == ERROR_BROKEN_PIPE;
    let mut buffer = shared
        .buffer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if broken {
        buffer.eof_seen = true;
    } else {
        buffer.last_error_code = Some(code);
    }
    broken
}

// ---------------------------------------------------------------------------
// 残留登记：未核实结束的读取资源由监督器继续持有（有上限、有故障状态）
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct RetainedReader {
    label: String,
    inner: PipeReaderInner,
    /// 这次登记是否属于**契约被破坏**的故障态；`Some(..)` 时它的超出量计入
    /// `capacity_fault_units`，直到它被核实回收。
    fault: Option<CapacityFaultReason>,
}

#[derive(Debug, Default)]
struct PipeResidueRegistry {
    entries: Vec<RetainedReader>,
    reclaimed: u64,
    /// 进入容量／所有权契约故障状态的**累计**读取器数量。
    capacity_faults: u64,
    /// 当前仍被持有、但**超出正常不变量**的读取器数量。
    capacity_fault_units: usize,
    /// 因容量不足／故障锁被**拒绝接纳**的累计次数。
    admission_rejected: u64,
    /// 已知未回收但已失去管理所有权的读取器数量（修复后为 0）。
    unowned_unreclaimed: u64,
    limit: usize,
    /// 故障锁：**不自动恢复**，需 [`review_and_clear_pipe_capacity_fault`] 显式核查。
    admission_locked: bool,
}

fn residue_registry() -> &'static Mutex<PipeResidueRegistry> {
    static REGISTRY: OnceLock<Mutex<PipeResidueRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        Mutex::new(PipeResidueRegistry {
            entries: Vec::new(),
            reclaimed: 0,
            capacity_faults: 0,
            capacity_fault_units: 0,
            admission_rejected: 0,
            unowned_unreclaimed: 0,
            limit: MAX_RETAINED_PIPE_READERS,
            admission_locked: false,
        })
    })
}

fn lock_registry() -> MutexGuard<'static, PipeResidueRegistry> {
    residue_registry()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// 登记决策（**纯函数**）：这是"契约是否被破坏"的唯一判定点。
///
/// * 读取器**在创建之前**取得的预留仍在手（`reserved == true`）且登记表未满 ⇒
///   登记只是 `Active → Retained` 的状态转换（不重新竞争另一个容量池）；
/// * 没有计账预留（接纳被绕过）⇒ [`CapacityFaultReason::ReservationAbsent`]；
/// * 有预留在手、登记表却已到上限 ⇒ 账目与登记表不一致
///   ⇒ [`CapacityFaultReason::InvariantViolated`]（防御性判定，见该变体文档）。
fn decide_retention(reserved: bool, held: usize, limit: usize) -> RetentionDecision {
    if !reserved {
        RetentionDecision::CapacityFault(CapacityFaultReason::ReservationAbsent)
    } else if held >= limit {
        RetentionDecision::CapacityFault(CapacityFaultReason::InvariantViolated)
    } else {
        RetentionDecision::Retain
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetentionDecision {
    Retain,
    CapacityFault(CapacityFaultReason),
}

/// 登记一个未核实结束的读取器，返回这次登记的**确切去向**。
///
/// ## 为什么这里不再有"容量归属转换"
///
/// 读取器的容量预留**从创建前一直持有到实际回收**：转入登记只是状态转换，令牌跟着
/// 读取器一起进入登记项（登记项本身就是"受持有的未回收读取器"的账目载体）。
/// 因此登记**不再**去比较"另一个容量池"（旧的 `entries.len() < limit`），
/// 也就不存在"登记时重新竞争容量"的死角。
///
/// 若契约已被破坏（无预留 / 账目与登记表不一致），本函数**仍然保留全部所有权**：
/// 读取线程、线程句柄、管道句柄与缓冲区都留在登记项里，只把它记为故障态、
/// 锁住新 helper 的接纳，并**不自动恢复**服务能力。
fn park_retained(label: String, inner: PipeReaderInner) -> PipeRetentionOutcome {
    let mut registry = lock_registry();
    // 先做一次**非阻塞**清扫：已经核实的残留腾出位置。
    // 清扫不等待任何线程，因此不会因为一个永不结束的读取阻塞其他所有任务。
    // 注意：这里**只回收已核实结束的**残留——绝不驱逐、绝不丢弃未知读取器。
    reclaim_locked(&mut registry);
    let reserved = inner.capacity.is_counted();
    match decide_retention(reserved, registry.entries.len(), registry.limit) {
        RetentionDecision::Retain => {
            registry.entries.push(RetainedReader {
                label,
                inner,
                fault: None,
            });
            PipeRetentionOutcome::Retained {
                retained: registry.entries.len(),
            }
        }
        RetentionDecision::CapacityFault(reason) => {
            // 契约破坏：**保留**所有权并记为故障态（**不**丢句柄、**不**分离线程）。
            registry.entries.push(RetainedReader {
                label,
                inner,
                fault: Some(reason),
            });
            registry.capacity_faults = registry.capacity_faults.saturating_add(1);
            registry.capacity_fault_units = registry.capacity_fault_units.saturating_add(1);
            // 锁住该监督器的新 helper 接纳：账目已经不可信，不再接纳新对象。
            registry.admission_locked = true;
            PipeRetentionOutcome::CapacityFault {
                limit: registry.limit,
                held: registry.entries.len(),
                fault_units: registry.capacity_fault_units,
                reason,
            }
        }
    }
}

/// 非阻塞清扫（持锁版本）：回收已经**核实**结束的残留读取，返回回收数量。
///
/// 绝不等待任何线程，因此不会被"永不结束的任务"阻塞。
fn reclaim_locked(registry: &mut PipeResidueRegistry) -> usize {
    let (finished, keep): (Vec<_>, Vec<_>) = registry
        .entries
        .drain(..)
        .partition(|entry| entry.inner.verified_completion().0.is_confirmed());
    registry.entries = keep;
    // 故障态对象被回收时，它的"超出量"随之归零（累计故障次数保留为历史）。
    let fault_finished = finished
        .iter()
        .filter(|entry| entry.fault.is_some())
        .count();
    registry.capacity_fault_units = registry.capacity_fault_units.saturating_sub(fault_finished);
    let reclaimed = finished.len();
    registry.reclaimed = registry.reclaimed.saturating_add(reclaimed as u64);
    // 丢在这里 = 读取器随线程终止释放（**已核实**结束），令牌随之归还。
    drop(finished);
    reclaimed
}

/// 非阻塞清扫：回收已经**核实**结束的残留读取，返回回收数量。
///
/// 绝不等待任何线程，因此不会被"永不结束的任务"阻塞。**只有已核实结束的残留**会被回收：
/// 从不驱逐、从不丢弃未知读取器。
pub fn reclaim_finished_pipe_readers() -> usize {
    let mut registry = lock_registry();
    reclaim_locked(&mut registry)
}

/// 显式核查并解除故障锁：**不自动恢复**服务能力。
///
/// 只有当①所有故障态对象都已经**被核实回收**（`capacity_fault_units == 0`）、
/// ②仍被持有的数量回到正常不变量之内（`≤ limit`）时，才解除接纳锁；
/// 否则返回当前的故障状态，接纳继续被锁住。
///
/// 累计故障次数（[`PipeSupervisorFault::capacity_faults`]）是**历史**，不因为解除锁而清零：
/// 核查通过只说明"现在可以重新服务"，不抹掉曾经发生过的事实。
pub fn review_and_clear_pipe_capacity_fault() -> Result<(), PipeSupervisorFault> {
    let mut registry = lock_registry();
    // 核查前先做一次非阻塞清扫：**已核实结束**的故障对象可以在这时回收。
    reclaim_locked(&mut registry);
    if registry.capacity_fault_units > 0 || registry.entries.len() > registry.limit {
        return Err(fault_snapshot(&registry));
    }
    registry.admission_locked = false;
    Ok(())
}

/// 当前仍被监督器持有的未核实读取数量。
pub fn retained_pipe_reader_count() -> usize {
    lock_registry().entries.len()
}

/// 指定标签的未核实读取数量（诊断/测试用）。
pub fn retained_pipe_reader_count_labeled(label: &str) -> usize {
    lock_registry()
        .entries
        .iter()
        .filter(|entry| entry.label == label)
        .count()
}

/// 指定标签的未核实读取**当前仍持有**的字节数（证明缓冲区没有被丢弃）。
///
/// 返回 `None` 表示该标签没有未核实的读取被登记。
pub fn retained_pipe_reader_bytes(label: &str) -> Option<u64> {
    let registry = lock_registry();
    registry
        .entries
        .iter()
        .find(|entry| entry.label == label)
        .map(|entry| entry.inner.lock_buffer().bytes.len() as u64)
}

/// 当前被持有未核实读取的标签。
pub fn retained_pipe_reader_labels() -> Vec<String> {
    lock_registry()
        .entries
        .iter()
        .map(|entry| entry.label.clone())
        .collect()
}

/// 残留登记与容量账目的故障状态：**没有故障时返回 `None`**。
///
/// "有未核实的读取仍在登记里"本身**不算**故障（那正是监督器在履行职责）。
/// 故障指：容量／所有权契约被破坏过（`capacity_faults > 0`）、有读取器失去过管理所有权
/// （`unowned_unreclaimed > 0`）、或接纳仍被锁住（`admission_locked`）。
#[must_use]
pub fn pipe_supervision_fault() -> Option<PipeSupervisorFault> {
    let registry = lock_registry();
    fault_of(&registry)
}

fn fault_snapshot(registry: &PipeResidueRegistry) -> PipeSupervisorFault {
    PipeSupervisorFault {
        retained: registry.entries.len(),
        limit: registry.limit,
        capacity_fault_units: registry.capacity_fault_units,
        capacity_faults: registry.capacity_faults,
        unowned_unreclaimed: registry.unowned_unreclaimed,
        admission_locked: registry.admission_locked,
    }
}

fn fault_of(registry: &PipeResidueRegistry) -> Option<PipeSupervisorFault> {
    if registry.capacity_faults == 0
        && registry.unowned_unreclaimed == 0
        && !registry.admission_locked
    {
        return None;
    }
    Some(fault_snapshot(registry))
}

/// 当前故障状态下**仍在登记表里受持有**的读取器标签（诊断用）。
///
/// 与 [`retained_pipe_reader_labels`] 的区别：这里只给出被记为故障态的那些。
#[must_use]
pub fn capacity_fault_reader_labels() -> Vec<String> {
    lock_registry()
        .entries
        .iter()
        .filter(|entry| entry.fault.is_some())
        .map(|entry| entry.label.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// 平台小工具
// ---------------------------------------------------------------------------

/// 交出所有权的管道读句柄（`ChildStdout` / `ChildStderr` 都实现）。
pub(crate) trait IntoRawHandleExt: Send + 'static {
    fn into_raw_pipe_handle(self) -> RawPipeHandle;
}

impl IntoRawHandleExt for ChildStdout {
    fn into_raw_pipe_handle(self) -> RawPipeHandle {
        RawPipeHandle {
            handle: std::os::windows::io::IntoRawHandle::into_raw_handle(self) as HANDLE,
        }
    }
}

impl IntoRawHandleExt for ChildStderr {
    fn into_raw_pipe_handle(self) -> RawPipeHandle {
        RawPipeHandle {
            handle: std::os::windows::io::IntoRawHandle::into_raw_handle(self) as HANDLE,
        }
    }
}

/// 已交出所有权的管道句柄。
///
/// 未交给读取线程就析构时关闭它（例如线程创建失败）：不会泄漏句柄。
pub(crate) struct RawPipeHandle {
    handle: HANDLE,
}

// SAFETY: Win32 句柄是进程级资源；所有权随本值转移，可以跨线程移动。
unsafe impl Send for RawPipeHandle {}

impl Drop for RawPipeHandle {
    fn drop(&mut self) {
        // SAFETY: 句柄由本值独占，只在这里关闭一次。
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

fn duplicate_handle(source: HANDLE) -> io::Result<KernelHandle> {
    let process = unsafe { GetCurrentProcess() };
    let mut target: HANDLE = std::ptr::null_mut();
    // SAFETY: 同进程内复制；target 是本栈上的可写缓冲。
    let ok = unsafe {
        DuplicateHandle(
            process,
            source,
            process,
            &mut target,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(KernelHandle::from_raw(target))
}

fn sanitize_thread_name(label: &str) -> String {
    label
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn millis_of(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{capture_process_identity, terminate_owned_process, ProcessIdentity};
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};
    use std::os::windows::process::CommandExt;
    use std::path::{Path as FsPath, PathBuf};
    use std::process::{Child, Command, ExitStatus, Stdio};
    use std::sync::mpsc;

    /// 无窗口地启动一个存活一段时间的子进程（测试用；不注入任何桌面输入）。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// 子进程打印的协议记录：**孙进程持管道时也必须能取到**（不依赖 EOF）。
    const RECORD_LINE: &str = "coolzhu-record:stdout:ready";

    /// 孙进程 `ping` 的报文数：约 19 秒的寿命，足够覆盖"收尾不得等它"的整个断言窗口。
    const GRANDCHILD_PING_COUNT: &str = "20";

    /// 真实"子进程退出、孙进程继续持有 stdout/stderr"的构造脚本。
    ///
    /// * 子进程 = `powershell.exe`：打印一行协议记录、把孙进程 PID 写进一个文件，随即退出；
    /// * 孙进程 = `Start-Process -NoNewWindow` 起的 `ping.exe -n 20`（单进程、无后代、约 19 秒）：
    ///   它**继承并继续持有**父进程的 stdout/stderr 管道写端——正是 `Add-Type` 的编译器
    ///   孙进程在真实事故里扮演的角色（Rust std 给子进程的管道写端是可继承的）。
    ///
    /// PID 落到文件而不是只走 stdout，是为了让测试的收尾**不依赖**管道行为：
    /// 即使读取线程恰好卡在 `ReadFile` 里，也能按身份终止自己起的那个孙进程。
    fn pipe_holder_script(pid_file: &FsPath) -> String {
        let path = pid_file.to_string_lossy().replace('\\', "/");
        let start_grandchild = format!(
            "Start-Process -FilePath ping.exe -ArgumentList '-n','{GRANDCHILD_PING_COUNT}','127.0.0.1' -NoNewWindow -PassThru | ForEach-Object {{ 'coolzhu-grandchild:' + $_.Id; [System.IO.File]::WriteAllText('{path}', [string]$_.Id) }}"
        );
        [
            "$ErrorActionPreference='Stop'".to_string(),
            "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)".to_string(),
            format!("'{RECORD_LINE}'"),
            start_grandchild,
            "'coolzhu-child:done'".to_string(),
            "exit 0".to_string(),
        ]
        .join("\n")
    }

    fn run_child(script: &str) -> Child {
        let mut command = Command::new("powershell.exe");
        command
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command.spawn().expect("必须能启动 powershell.exe 子进程")
    }

    /// 读取器容量是**进程级**的（这正是 RD4-09 §B-8 的要求：活动读取器的预留与已残留
    /// 读取器共同计入容量义务）。因此创建读取器的用例必须**在测试进程内串行**：
    /// 否则测试自己并行的读取器会互相把 8 个单位的容量抢光，得到的拒绝与生产语义无关。
    fn pipe_reader_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn wait_child(child: &mut Child, limit: Duration) -> ExitStatus {
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status,
                Ok(None) => {
                    assert!(started.elapsed() < limit, "子进程未在 {limit:?} 内退出");
                    thread::sleep(Duration::from_millis(20));
                }
                Err(error) => panic!("try_wait 失败: {error}"),
            }
        }
    }

    /// 轮询"**已经收到**"的字节（不依赖 EOF）：等到某个文本出现或到点。
    fn wait_for_text(reader: &PipeReaderSupervisor, needle: &str, limit: Duration) -> bool {
        let started = Instant::now();
        loop {
            if reader
                .snapshot()
                .is_some_and(|snapshot| snapshot.text_lossy().contains(needle))
            {
                return true;
            }
            if started.elapsed() >= limit {
                return false;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// 只终止**本测试自己观测到**的孙进程：身份在它还活着时捕获，终止时按身份核对。
    struct GrandchildGuard {
        pid: u32,
        identity: ProcessIdentity,
    }

    impl GrandchildGuard {
        fn capture(pid: u32) -> Option<Self> {
            capture_process_identity(pid)
                .ok()
                .map(|identity| Self { pid, identity })
        }

        fn is_alive(&self) -> bool {
            capture_process_identity(self.pid).is_ok()
        }
    }

    impl Drop for GrandchildGuard {
        fn drop(&mut self) {
            // 只按身份终止：PID 若已被复用，`terminate_owned_process` 会拒绝，不会误伤。
            let _ = terminate_owned_process(self.pid, &self.identity);
        }
    }

    /// 一次真实的"子进程退出、孙进程持管道"场景。
    struct Scenario {
        child: Child,
        stdout: Option<PipeReaderSupervisor>,
        stderr: Option<PipeReaderSupervisor>,
        grandchild: Option<GrandchildGuard>,
        pid_file: PathBuf,
    }

    impl Scenario {
        fn spawn(
            tag: &str,
            stdout_strategy: PipeReadStrategy,
            stderr_strategy: PipeReadStrategy,
        ) -> Self {
            let pid_file = std::env::temp_dir().join(format!(
                "coolzhu-pipe-supervision-{tag}-{}.pid",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&pid_file);
            let mut child = run_child(&pipe_holder_script(&pid_file));
            let stdout = PipeReaderSupervisor::with_options(
                format!("{tag}-stdout"),
                child.stdout.take().expect("子进程 stdout"),
                stdout_strategy,
                MAX_PIPE_BUFFER_BYTES,
            )
            .expect("stdout 监督器");
            let stderr = PipeReaderSupervisor::with_options(
                format!("{tag}-stderr"),
                child.stderr.take().expect("子进程 stderr"),
                stderr_strategy,
                MAX_PIPE_BUFFER_BYTES,
            )
            .expect("stderr 监督器");
            let status = wait_child(&mut child, Duration::from_secs(30));
            assert!(status.success(), "子进程本身必须正常退出: {status:?}");
            // 孙进程 PID 由子进程写进文件：收尾不依赖管道是否还能读。
            let mut grandchild = None;
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if let Ok(raw) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = raw.trim().parse::<u32>() {
                        grandchild = GrandchildGuard::capture(pid);
                        if grandchild.is_some() {
                            break;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(20));
            }
            assert!(
                grandchild.is_some(),
                "必须先观测到孙进程身份，才能保证只终止自己起的那个进程"
            );
            // 子进程已退出：给读取线程一点时间做"结尾观察"。
            thread::sleep(Duration::from_millis(400));
            Self {
                child,
                stdout: Some(stdout),
                stderr: Some(stderr),
                grandchild,
                pid_file,
            }
        }

        fn stdout(&self) -> &PipeReaderSupervisor {
            self.stdout.as_ref().expect("stdout 监督器未被消费")
        }

        fn stderr(&self) -> &PipeReaderSupervisor {
            self.stderr.as_ref().expect("stderr 监督器未被消费")
        }

        fn take_stdout(&mut self) -> PipeReaderSupervisor {
            self.stdout.take().expect("stdout 监督器只被消费一次")
        }

        fn take_stderr(&mut self) -> PipeReaderSupervisor {
            self.stderr.take().expect("stderr 监督器只被消费一次")
        }

        fn grandchild_alive(&self) -> bool {
            self.grandchild.as_ref().is_some_and(GrandchildGuard::is_alive)
        }

        /// 只终止本场景的孙进程（按身份核对），并给读取线程一点时间结束。
        fn terminate_grandchild(&mut self) {
            if let Some(guard) = self.grandchild.take() {
                drop(guard);
            }
            thread::sleep(Duration::from_millis(500));
        }
    }

    impl Drop for Scenario {
        fn drop(&mut self) {
            if let Some(guard) = self.grandchild.take() {
                drop(guard);
            }
            let _ = std::fs::remove_file(&self.pid_file);
            let _ = self.child.wait();
        }
    }

    // ------------------------------------------------------------------
    // 前提探针：机制本身（A/B 对照）
    // ------------------------------------------------------------------

    /// **孙进程持管道**才是"读取线程拿不到 EOF"的原因；这也正是旧实现
    /// （`join()` 等 EOF）会在四秒收尾里无界等待的地方。
    ///
    /// A/B 对照：
    /// * 有孙进程（持管道）⇒ 子进程退出后读取线程**看不到** EOF，且当时仍在运行；
    /// * 无孙进程（对照）⇒ 子进程一退出管道就断裂（`ERROR_BROKEN_PIPE`）。
    #[test]
    fn grandchild_holding_the_pipe_is_what_defers_eof() {
        // 进程级容量：与本进程其它创建读取器的用例串行。
        let _pipe_slot = pipe_reader_test_lock();
        let mut held = Scenario::spawn(
            "probe-hold",
            PipeReadStrategy::PollAvailable,
            PipeReadStrategy::PollAvailable,
        );
        assert!(
            wait_for_text(held.stdout(), RECORD_LINE, Duration::from_secs(10)),
            "协议记录必须能从**已经收到**的字节里读到（不依赖 EOF）"
        );
        let snapshot = held.stdout().snapshot().expect("监督器仍在");
        println!(
            "[前提 A] 孙进程持管道：eof_seen={} completion={} bytes={} reason={:?}",
            snapshot.eof_seen,
            snapshot.completion.as_str(),
            snapshot.byte_count(),
            snapshot.unconfirmed_reason
        );
        assert!(!snapshot.eof_seen, "孙进程仍持有写端时不得看到 EOF");
        assert!(
            !snapshot.completion.is_confirmed(),
            "孙进程持管道时读取线程仍在运行：不得报成已结束"
        );
        assert!(held.grandchild_alive(), "孙进程仍在运行（约 19 秒）");
        // 有界收尾：**不等待孙进程自然退出**。
        let started = Instant::now();
        let drained = held.take_stdout().drain(Duration::from_millis(250));
        let elapsed = started.elapsed();
        println!("[前提 A] 有界收尾 {drained} elapsed={elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(2),
            "收尾必须有界（孙进程还有约 19 秒才退）：{elapsed:?}"
        );
        assert!(
            drained.completion.is_confirmed(),
            "生产策略必须能核实读取线程结束：{drained}"
        );
        assert!(held.grandchild_alive(), "CU 不得等待孙进程自然退出");
        drop(held.take_stderr().drain(Duration::from_millis(250)));

        // B：同样的脚本但**不**起孙进程。
        let script = [
            "$ErrorActionPreference='Stop'",
            "[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)",
            &format!("'{RECORD_LINE}'"),
            "'coolzhu-child:done'",
            "exit 0",
        ]
        .join("\n");
        let mut child = run_child(&script);
        let stdout =
            PipeReaderSupervisor::stdout("probe-nodead-stdout", child.stdout.take().unwrap())
                .expect("监督器");
        let _stderr =
            PipeReaderSupervisor::stderr("probe-nodead-stderr", child.stderr.take().unwrap())
                .expect("监督器");
        let status = wait_child(&mut child, Duration::from_secs(30));
        assert!(status.success());
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut eof_seen = false;
        while Instant::now() < deadline {
            if stdout.snapshot().is_some_and(|s| s.eof_seen) {
                eof_seen = true;
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        println!("[前提 B] 无孙进程：eof_seen={eof_seen}");
        assert!(eof_seen, "对照组：没有孙进程时子进程退出即 EOF——这才是正常情形");
        let drained = stdout.drain(Duration::from_millis(250));
        assert!(drained.completion.is_confirmed());
        assert!(drained.eof_seen);
        let _ = wait_child(&mut child, Duration::from_secs(5));
    }

    // ------------------------------------------------------------------
    // T11：子进程退出、孙进程继续持有 stdout/stderr
    // ------------------------------------------------------------------

    /// **T11**：CU 不等待孙进程自然退出；收尾有界；未知状态明确；无失管 reader。
    ///
    /// 同一场景里用两条流分别覆盖两种策略：
    /// * stdout 用**生产策略**（轮询可用字节）⇒ 必须核实结束、有界、不丢回执、不留残留；
    /// * stderr 用**阻塞读取**（真实的"读取线程确实卡在 ReadFile"形态）⇒ 必须如实报
    ///   "未核实结束 + 当时仍在运行"，并把读取线程与缓冲区**留在监督器持有的残留登记**里，
    ///   而不是丢句柄后宣称已终止；孙进程退出后再由非阻塞清扫闭环。
    #[test]
    fn t11_bounded_cleanup_never_waits_for_a_grandchild_keeping_the_pipes() {
        // 进程级容量：与本进程其它创建读取器的用例串行。
        let _pipe_slot = pipe_reader_test_lock();
        let mut scenario = Scenario::spawn(
            "t11",
            PipeReadStrategy::PollAvailable,
            PipeReadStrategy::Blocking,
        );
        let out_label = scenario.stdout().label().to_string();
        let err_label = scenario.stderr().label().to_string();

        // 回执不依赖 EOF：子进程已退出，而孙进程仍在持管道。
        assert!(
            wait_for_text(scenario.stdout(), RECORD_LINE, Duration::from_secs(10)),
            "已经收到的协议记录必须在没有 EOF 的情况下可读"
        );
        let before = scenario.stdout().snapshot().expect("监督器仍在");
        assert!(!before.eof_seen, "孙进程仍持有写端：不得看到 EOF");
        assert!(
            !before.completion.is_confirmed(),
            "读取线程当时仍在运行：不得报成已结束"
        );

        // 有界收尾：先 stdout。
        let started = Instant::now();
        let out = scenario.take_stdout().drain(Duration::from_millis(250));
        let out_elapsed = started.elapsed();
        println!("[T11] stdout {out} elapsed={out_elapsed:?}");
        assert!(
            out_elapsed < Duration::from_secs(2),
            "收尾必须有界（孙进程还有约 19 秒才退）：{out_elapsed:?}"
        );
        assert!(
            out.completion.is_confirmed() && out.retention == PipeRetentionOutcome::Confirmed,
            "生产策略必须有界地核实读取线程结束：{out}"
        );
        assert!(
            out.text_lossy().contains(RECORD_LINE),
            "已经收到的执行回执不得因为收尾而丢失：{out}"
        );
        assert!(
            scenario.grandchild_alive(),
            "CU 不得等待孙进程自然退出：收尾结束时它必须还活着"
        );
        // 无失管 reader：没有"未核实结束"的读取被留在登记里（已核实的随线程终止释放）。
        assert_eq!(retained_pipe_reader_count_labeled(&out_label), 0);

        // 未知状态明确：阻塞读取无法核实结束 ⇒ 事实必须写"未核实 + 当时仍在运行"，
        // 且读取线程、句柄与缓冲区**仍由监督器持有**（含缓冲区字节数）。
        let err = scenario.take_stderr().drain(Duration::from_millis(250));
        println!("[T11] stderr {err}");
        assert_eq!(
            err.completion,
            PipeReaderCompletion::Unconfirmed,
            "无法核实的读取不得被报成已结束：{err}"
        );
        assert_eq!(
            err.unconfirmed_reason,
            Some(PipeReaderUnconfirmed::StillRunning),
            "未核实必须给出原因（仍在运行），不能含糊：{err}"
        );
        assert!(
            err.retention.holds_unreclaimed_reader(),
            "未核实结束的读取资源必须仍由监督器持有：{err}"
        );
        assert_eq!(
            retained_pipe_reader_count_labeled(&err_label),
            1,
            "残留登记必须能按标签找到那一份未核实的读取"
        );
        assert_eq!(
            retained_pipe_reader_bytes(&err_label),
            Some(err.bytes.len() as u64),
            "被持有的不只是句柄：缓冲区字节数也要能对上"
        );
        assert!(scenario.grandchild_alive(), "第二次收尾同样不得等待孙进程");

        // 读取**尚未**结束时清扫：不得删除它，也不得因此阻塞其他任务。
        let started = Instant::now();
        let swept_while_running = reclaim_finished_pipe_readers();
        let sweep_elapsed = started.elapsed();
        println!("[T11] 未结束时的清扫：{swept_while_running} 份（{sweep_elapsed:?}）");
        assert!(
            sweep_elapsed < Duration::from_millis(200),
            "清扫必须是非阻塞的：{sweep_elapsed:?}"
        );
        assert_eq!(
            retained_pipe_reader_count_labeled(&err_label),
            1,
            "未核实的读取不得被清扫掉（它只能由核实结束来闭环）"
        );
        assert!(
            pipe_supervision_fault().is_none(),
            "还没有任何读取因为上限而无法寄存：不得出现故障状态"
        );

        // 孙进程退出后：有界闭环。
        // 注意登记是**进程级**的：并行运行的其它用例也可能顺手清扫同一登记，
        // 因此这里断言的是**终态**（该标签的残留归零）与**有界性**，而不是"由我这次调用回收了几份"。
        scenario.terminate_grandchild();
        let started = Instant::now();
        while retained_pipe_reader_count_labeled(&err_label) != 0
            && started.elapsed() < Duration::from_secs(3)
        {
            let _ = reclaim_finished_pipe_readers();
            thread::sleep(Duration::from_millis(50));
        }
        println!("[T11] 闭环用时 {elapsed:?}", elapsed = started.elapsed());
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "孙进程退出后必须能及时回收：{elapsed:?}",
            elapsed = started.elapsed()
        );
        assert_eq!(
            retained_pipe_reader_count_labeled(&err_label),
            0,
            "孙进程退出后阻塞的读取必须结束并被回收"
        );
        assert_eq!(retained_pipe_reader_bytes(&err_label), None);
        assert_eq!(retained_pipe_reader_count_labeled(&out_label), 0);
    }

    // ------------------------------------------------------------------
    // T12：取消同步 I/O 与读取完成竞争
    // ------------------------------------------------------------------

    /// **T12**：取消 I/O 与读取完成竞争 ⇒ 不提前释放资源、不误报线程结束；
    /// 重复取消不刷新窗口。
    ///
    /// 取消请求的返回值**只表示请求已发出**：本测试先核实"未结束"就绝不报"已结束"，
    /// 再核实取消之后的缓冲区与线程句柄依然由监督器持有（没有提前释放）。
    #[test]
    fn t12_cancel_is_not_completion_and_repeated_cancels_never_refresh_the_window() {
        // 进程级容量：与本进程其它创建读取器的用例串行。
        let _pipe_slot = pipe_reader_test_lock();
        let mut scenario = Scenario::spawn(
            "t12",
            PipeReadStrategy::Blocking,
            PipeReadStrategy::PollAvailable,
        );
        let label = scenario.stdout().label().to_string();
        // 阻塞读取的第一次读会取走"已经可读"的字节；等它到达（不依赖 EOF）。
        let saw_record = wait_for_text(scenario.stdout(), RECORD_LINE, Duration::from_secs(5));
        let before = scenario.stdout().snapshot().expect("监督器仍在");
        println!(
            "[T12] 取消前：bytes={} completion={} saw_record={saw_record}",
            before.byte_count(),
            before.completion.as_str()
        );
        assert!(
            !before.completion.is_confirmed(),
            "孙进程持管道：读取不可能已结束"
        );

        // 重复取消：窗口只固定一次。
        let mut outcomes = Vec::new();
        for _ in 0..3 {
            outcomes.push(scenario.stdout().cancel_synchronous_io());
        }
        let first_cancel = scenario.stdout().first_cancel_unix_ms();
        println!(
            "[T12] 取消结果 {outcomes:?} requests={} first_cancel={first_cancel:?}",
            scenario.stdout().cancel_requests()
        );
        assert_eq!(
            scenario.stdout().cancel_requests(),
            3,
            "三次取消都要被计数（请求确实发出去了）"
        );
        assert!(first_cancel.is_some(), "第一次取消必须固定时刻");
        let _ = scenario.stdout().cancel_synchronous_io();
        assert_eq!(
            scenario.stdout().first_cancel_unix_ms(),
            first_cancel,
            "重复取消不得刷新窗口起点"
        );
        assert_eq!(scenario.stdout().cancel_requests(), 4);

        // 不误报线程结束：完成状态只能由线程句柄核实。
        let after = scenario.stdout().snapshot().expect("监督器仍在");
        assert_eq!(
            after.completion.is_confirmed(),
            scenario.stdout().is_confirmed_finished(),
            "报出的完成状态必须与核实结果一致"
        );
        if after.completion.is_confirmed() {
            println!("[T12] 证据：取消请求确实结束了阻塞读取（已由线程句柄核实，非假设）");
            assert_eq!(after.unconfirmed_reason, None);
        } else {
            assert_eq!(
                after.unconfirmed_reason,
                Some(PipeReaderUnconfirmed::StillRunning),
                "未核实时必须给出原因，且不得声称已结束"
            );
            println!(
                "[T12] 证据：取消请求发出后读取**未**结束 ⇒ 如实报未核实（禁止把发出取消等同结束）"
            );
        }
        // 不提前释放资源：取消之后缓冲区与线程句柄仍然可用、内容不缩水。
        assert!(
            after.byte_count() >= before.byte_count(),
            "取消请求不得清空/缩短已经收到的字节"
        );
        assert_eq!(after.bytes, before.bytes, "已经收到的字节不得因取消而改变");
        assert!(scenario.grandchild_alive(), "取消不针对孙进程：它必须还在跑");

        // 有界收尾：读数不丢，缺口如实上报。
        let drained = scenario.take_stdout().drain(Duration::from_millis(1_500));
        println!(
            "[T12] 收尾 {drained} unconfirmed_reason={:?} 有缺口={}",
            drained.unconfirmed_reason,
            drained.has_evidence_gap()
        );
        assert!(
            drained.waited_ms <= 1_500,
            "收尾必须是有界等待，不得无界阻塞：{drained}"
        );
        if !drained.completion.is_confirmed() {
            assert!(drained.has_evidence_gap(), "未核实结束必须记成证据缺口");
            assert!(
                drained.retention.holds_unreclaimed_reader(),
                "未核实的读取资源必须仍被监督器持有：{drained}"
            );
            assert_eq!(retained_pipe_reader_count_labeled(&label), 1);
            assert_eq!(
                retained_pipe_reader_bytes(&label),
                Some(drained.bytes.len() as u64)
            );
        }
        if saw_record {
            assert!(
                drained.text_lossy().contains(RECORD_LINE),
                "已经收到的回执不得因为取消/收尾而丢失：{drained}"
            );
        }

        // 闭环：孙进程退出后非阻塞回收。
        scenario.terminate_grandchild();
        let reclaimed = reclaim_finished_pipe_readers();
        println!("[T12] 回收 {reclaimed} 份");
        assert_eq!(retained_pipe_reader_count_labeled(&label), 0);
    }

    // ------------------------------------------------------------------
    // 资源所有权与数量上限
    // ------------------------------------------------------------------

    /// **不能**靠"Drop 掉 `JoinHandle`"了事：未核实的读取连同句柄与缓冲区被寄存，
    /// 而不是被丢弃后宣称已终止。
    #[test]
    fn dropping_a_supervisor_parks_an_unverified_reader_instead_of_losing_it() {
        // 进程级容量：与本进程其它创建读取器的用例串行。
        let _pipe_slot = pipe_reader_test_lock();
        let mut scenario = Scenario::spawn(
            "drop-park",
            PipeReadStrategy::Blocking,
            PipeReadStrategy::PollAvailable,
        );
        let label = scenario.stdout().label().to_string();
        assert!(
            !scenario.stdout().is_confirmed_finished(),
            "阻塞读取尚未结束"
        );
        let held_bytes = scenario.stdout().snapshot().expect("监督器仍在").byte_count();

        // 直接丢弃监督器（没有走过 drain）：资源必须转入残留登记，不能失管。
        drop(scenario.take_stdout());
        assert_eq!(
            retained_pipe_reader_count_labeled(&label),
            1,
            "未核实结束的读取必须被监督器继续持有（Drop 不得失管）"
        );
        assert_eq!(
            retained_pipe_reader_bytes(&label),
            Some(held_bytes as u64),
            "被寄存的读取必须连缓冲区一起保留"
        );

        scenario.terminate_grandchild();
        let started = Instant::now();
        while retained_pipe_reader_count_labeled(&label) != 0
            && started.elapsed() < Duration::from_secs(3)
        {
            let _ = reclaim_finished_pipe_readers();
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(
            retained_pipe_reader_count_labeled(&label),
            0,
            "孙进程退出后必须能回收"
        );
    }

    /// 登记决策是**纯函数**，且只有"契约被破坏"才进入故障态（不是"看到上限就丢"）。
    #[test]
    fn retention_decision_is_a_pure_state_transition_and_faults_only_on_contract_breach() {
        assert!(MAX_RETAINED_PIPE_READERS >= 1);
        // 在手的预留 + 登记表未满 ⇒ 只是 `Active → Retained` 的状态转换。
        assert_eq!(decide_retention(true, 0, 8), RetentionDecision::Retain);
        assert_eq!(decide_retention(true, 7, 8), RetentionDecision::Retain);
        // 没有计账预留 ⇒ 接纳被绕过：故障（并**保留**所有权）。
        assert_eq!(
            decide_retention(false, 0, 8),
            RetentionDecision::CapacityFault(CapacityFaultReason::ReservationAbsent)
        );
        assert_eq!(
            decide_retention(false, 8, 8),
            RetentionDecision::CapacityFault(CapacityFaultReason::ReservationAbsent)
        );
        // 有预留在手、登记表却已满 ⇒ 账目与登记表不一致：防御性故障判定。
        assert_eq!(
            decide_retention(true, 8, 8),
            RetentionDecision::CapacityFault(CapacityFaultReason::InvariantViolated)
        );
        assert_eq!(
            decide_retention(true, 9, 8),
            RetentionDecision::CapacityFault(CapacityFaultReason::InvariantViolated)
        );
        assert_eq!(
            decide_retention(true, 0, 0),
            RetentionDecision::CapacityFault(CapacityFaultReason::InvariantViolated)
        );
        assert_eq!(
            CapacityFaultReason::ReservationAbsent.as_str(),
            "reservation_absent"
        );
        assert_eq!(
            CapacityFaultReason::InvariantViolated.as_str(),
            "capacity_invariant_violated"
        );
    }

    /// 故障态的事实表达：**仍被持有**（不是"已结束"，也不是"被丢掉"）。
    #[test]
    fn capacity_fault_outcome_reports_a_held_reader_and_a_real_excess_count() {
        let fault = PipeRetentionOutcome::CapacityFault {
            limit: 8,
            held: 9,
            fault_units: 1,
            reason: CapacityFaultReason::ReservationAbsent,
        };
        // §C-18：修复后故障资源**确实由监督器持有**，因此这里是 true 才是正确事实。
        assert!(fault.holds_unreclaimed_reader());
        assert!(fault.is_capacity_fault());
        assert_eq!(fault.capacity_fault_units(), 1);
        assert_eq!(fault.retained_count(), None);
        // 正常登记：持有但**不**故障。
        let retained = PipeRetentionOutcome::Retained { retained: 2 };
        assert!(retained.holds_unreclaimed_reader());
        assert!(!retained.is_capacity_fault());
        assert_eq!(retained.capacity_fault_units(), 0);
        assert_eq!(retained.retained_count(), Some(2));
        // 已核实结束：没有残留、也没有故障。
        assert!(!PipeRetentionOutcome::Confirmed.holds_unreclaimed_reader());
        assert_eq!(PipeRetentionOutcome::Confirmed.capacity_fault_units(), 0);
        assert_eq!(PipeRetentionOutcome::Confirmed.retained_count(), None);
    }

    /// 输出限额**不能静默丢掉已经收到的内容**：保留下来的字节仍在，
    /// 缺口以 `dropped_bytes`/`truncated` 明示。
    #[test]
    fn truncated_output_keeps_what_arrived_and_reports_the_gap() {
        // 进程级容量：与本进程其它创建读取器的用例串行。
        let _pipe_slot = pipe_reader_test_lock();
        let mut child = Command::new("cmd.exe")
            .args(["/c", "for /L %i in (1,1,200) do @echo 0123456789abcdef"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("必须能启动 cmd.exe");
        let limit = 128;
        let stdout = PipeReaderSupervisor::with_options(
            "truncation-stdout",
            child.stdout.take().unwrap(),
            PipeReadStrategy::PollAvailable,
            limit,
        )
        .expect("监督器");
        let _stderr =
            PipeReaderSupervisor::stderr("truncation-stderr", child.stderr.take().unwrap())
                .expect("监督器");
        let status = wait_child(&mut child, Duration::from_secs(30));
        assert!(status.success());
        let drained = stdout.drain(Duration::from_millis(500));
        println!("[截断] {drained}");
        assert!(drained.completion.is_confirmed());
        assert_eq!(drained.bytes.len(), limit, "限额内的字节必须完整保留");
        assert!(drained.truncated, "超限必须置截断标记");
        assert!(
            drained.dropped_bytes > 2_000,
            "缺口必须以字节数明示：{}",
            drained.dropped_bytes
        );
        assert!(
            drained.text_lossy().contains("0123456789abcdef"),
            "已经收到的内容不得被静默丢弃"
        );
        assert!(drained.has_evidence_gap(), "截断属于证据缺口");
        let _ = child.wait();
    }

    // ------------------------------------------------------------------
    // RD4-09 §B-8：固定容量政策（创建前预留 + 按实际完成释放）
    // ------------------------------------------------------------------

    /// 容量账目是**纯算术**：义务 = 预留未创建 + 活动 + 已保留；可用 = 上限 − 义务。
    ///
    /// §C-18 要求的分类在这里逐个核对：`unreclaimed` / `owned_unreclaimed` /
    /// `unowned_unreclaimed` / `admission_rejected` / `capacity_fault` 是**不同的量**，
    /// 不是"把所有错误状态都计为 1"。
    #[test]
    fn capacity_arithmetic_keeps_unreclaimed_owned_and_unowned_counts_separate() {
        let capacity = PipeReaderCapacity {
            limit: 8,
            reserved_not_created: 2,
            active_unreclaimed: 3,
            retained_unreclaimed: 3,
            admission_rejected: 11,
            capacity_faults: 1,
            capacity_fault_units: 1,
            unowned_unreclaimed: 0,
            admission_locked: false,
        };
        assert_eq!(capacity.obligations(), 8, "三个桶的和就是容量义务");
        assert_eq!(capacity.unreclaimed(), 6);
        assert_eq!(capacity.owned_unreclaimed(), 6, "未回收的读取器仍受管理");
        assert_eq!(capacity.unowned_unreclaimed, 0, "没有读取器失去管理所有权");
        assert_eq!(capacity.admission_rejected, 11, "拒绝是累计量，不是持有量");
        assert_eq!(capacity.capacity_fault_units, 1, "超出不变量的实际数量");
        assert_eq!(capacity.available(), 0);
        assert!(!capacity.can_admit(1));
        assert!(!capacity.can_admit(0), "0 个单位不是一次合法接纳请求");

        // 部分可用：判据只看义务。
        let partial = PipeReaderCapacity {
            reserved_not_created: 1,
            active_unreclaimed: 0,
            retained_unreclaimed: 6,
            capacity_fault_units: 0,
            admission_rejected: 0,
            capacity_faults: 0,
            unowned_unreclaimed: 0,
            admission_locked: false,
            limit: 8,
        };
        assert_eq!(partial.obligations(), 7);
        assert_eq!(partial.available(), 1);
        assert!(partial.can_admit(1));
        assert!(!partial.can_admit(2), "超过可用容量必须拒绝");

        // 故障锁：账目再空也**不**接纳（不自动恢复服务能力）。
        let locked = PipeReaderCapacity {
            reserved_not_created: 0,
            active_unreclaimed: 0,
            retained_unreclaimed: 0,
            capacity_fault_units: 0,
            admission_rejected: 1,
            capacity_faults: 1,
            unowned_unreclaimed: 0,
            admission_locked: true,
            limit: 8,
        };
        assert_eq!(locked.obligations(), 0);
        assert_eq!(locked.available(), 0, "故障锁生效时可用额度为 0");
        assert!(!locked.can_admit(8), "故障锁生效时任何请求都不被接纳");

        // 一个 helper 的容量义务是**两个** reader（stdout + stderr）；
        // 需要独立清理运行的执行者还要为它一起预留一份。
        assert_eq!(PIPE_READERS_PER_HELPER, 2);
        assert_eq!(PIPE_CAPACITY_EXHAUSTED, "pipe_reader_capacity_exhausted");
        assert_eq!(PIPE_CAPACITY_FAULT, "pipe_reader_capacity_fault");
    }

    /// **中途失败的额度归属**：没有被用到的预留必须**一分不少、也不重复**地归还。
    ///
    /// 覆盖三种真实形态（都在生产接纳顺序里出现）：
    /// ① 普通动作预留 `2 + 2` 之后 `spawn` 失败 ⇒ 全部未用额度归还；
    /// ② 只建了一条流（第二条流失败）⇒ 已交给读取器的额度**继续由读取器承担**，
    ///    其余未用额度归还；
    /// ③ 清理预留没有被用掉（不需要独立释放）⇒ 它单独归还，且不触碰普通 helper 的额度。
    ///
    /// 这里只核对**账目**（不构造真实失败）：`spawn` 失败那条分支在代码里就是
    /// "凭证随作用域 Drop"，与 ① 的形态一致。
    #[test]
    fn abandoned_reservations_return_every_unit_without_leaking_or_double_counting() {
        let _pipe_slot = pipe_reader_test_lock();
        let start = pipe_reader_capacity();

        // ① 普通动作：一次性预留 4 个单位，随后整个凭证被放弃（= spawn 失败）。
        {
            let admission = HelperPipeReadersAdmission::acquire_with_cleanup_reserve()
                .expect("空账目下必须接纳 4 个单位");
            let mid = pipe_reader_capacity();
            assert_eq!(
                (mid.reserved_not_created, mid.obligations()),
                (4, start.obligations() + 4),
                "4 个单位必须同时记在'预留未创建'与义务里"
            );
            drop(admission);
        }
        let after_spawn_failure = pipe_reader_capacity();
        assert_eq!(
            after_spawn_failure.obligations(),
            start.obligations(),
            "全部未用额度必须归还（不泄漏）"
        );
        assert_eq!(after_spawn_failure.reserved_not_created, 0, "不留幽灵额度");
        assert_eq!(after_spawn_failure.unowned_unreclaimed, start.unowned_unreclaimed);

        // ② 只切出清理预留、另一部分留在凭证里 ⇒ 两部分都要各自归还，且只归还一次。
        {
            let mut admission = HelperPipeReadersAdmission::acquire_with_cleanup_reserve()
                .expect("空账目下必须接纳 4 个单位");
            let cleanup = admission
                .take_cleanup_reservation()
                .expect("应当切得出一份清理预留");
            assert_eq!(cleanup.units(), PIPE_READERS_PER_HELPER);
            assert_eq!(admission.remaining_units(), PIPE_READERS_PER_HELPER);
            drop(cleanup);
            let after_cleanup = pipe_reader_capacity();
            assert_eq!(
                after_cleanup.reserved_not_created, PIPE_READERS_PER_HELPER,
                "清理预留言归还后只应剩下普通 helper 的 2 个单位"
            );
            drop(admission);
        }
        let after_partial = pipe_reader_capacity();
        assert_eq!(after_partial.obligations(), start.obligations());
        assert_eq!(after_partial.reserved_not_created, 0);
        assert_eq!(
            after_partial.admission_rejected, start.admission_rejected,
            "合法归还不得被记成拒绝"
        );
    }


    /// **饱和下已经预留的清理容量仍能转成读取器**（§C-21 第 8 条"收尾死角"的直接证据）。
    ///
    /// 构造：两个普通动作各一次性预留 4 个单位（2 条流 + 2 清理）= 上限 8，账目随即饱和；
    /// 此时新 helper 的接纳必须被拒，但**接纳时就已经在手的清理预留**照常可用——
    /// 它只是从"预留未创建"转到"活动未回收"，不重新竞争普通容量，也不递归预留下一层。
    #[test]
    fn a_pre_reserved_cleanup_quota_still_serves_cleanup_when_the_ledger_is_saturated() {
        let _pipe_slot = pipe_reader_test_lock();
        let before = pipe_reader_capacity();
        let mut ordinary = Vec::new();
        for _ in 0..2 {
            let mut admission = HelperPipeReadersAdmission::acquire_with_cleanup_reserve()
                .unwrap_or_else(|denial| panic!("账目应当放得下两个 4 单位接纳：{denial}"));
            let cleanup = admission
                .take_cleanup_reservation()
                .expect("普通动作必须连清理额度一起预留");
            assert_eq!(cleanup.units(), PIPE_READERS_PER_HELPER);
            ordinary.push((admission, cleanup));
        }
        let saturated = pipe_reader_capacity();
        println!("[清理预留·饱和] 两个普通动作接纳后 {saturated}");
        assert_eq!(
            saturated.available(),
            0,
            "两个普通动作的 4+4 个单位必须占满上限（含各自的清理额度）"
        );
        assert!(
            HelperPipeReadersAdmission::acquire().is_err(),
            "饱和下新 helper 的接纳必须被拒绝"
        );

        // 转用清理预留：它**已经**在账上，因此不受"新接纳被拒"影响。
        let (unused_admission, cleanup) = ordinary.pop().expect("第二个普通动作");
        drop(unused_admission);
        let mut holder = spawn_pipe_holder();
        let mut cleanup_admission = cleanup.into_admission();
        let out = cleanup_admission
            .stdout("cleanup-after-saturation-stdout", holder.stdout.take().expect("stdout"))
            .expect("已预留的清理容量必须能建出读取器（不重新竞争普通容量）");
        let err = cleanup_admission
            .stderr("cleanup-after-saturation-stderr", holder.stderr.take().expect("stderr"))
            .expect("已预留的清理容量必须能建出第二个读取器");
        let during_cleanup = pipe_reader_capacity();
        println!("[清理预留·饱和] 清理转用后 {during_cleanup}");
        assert_eq!(
            during_cleanup.active_unreclaimed,
            before.active_unreclaimed + 2,
            "清理运行的两条流必须真的在跑（它们来自预留的 2 个单位）"
        );
        assert_eq!(
            during_cleanup.obligations(),
            before.obligations() + 6,
            "另一个普通动作的 4 个单位仍在账上 + 清理转用后的 2 个活动读取器             （转用只是换桶，不改变义务总量）"
        );

        // 有界收尾 + 回收：清理读取器随写端关闭结束，额度按实际回收归还。
        let _ = holder.kill();
        let _ = holder.wait();
        let out_drain = out.drain(Duration::from_millis(1_000));
        let err_drain = err.drain(Duration::from_millis(1_000));
        assert!(
            out_drain.completion.is_confirmed() && err_drain.completion.is_confirmed(),
            "清理运行的两条流必须能核实结束：{out_drain} / {err_drain}"
        );
        drop(ordinary);
        let started = Instant::now();
        while pipe_reader_capacity().unreclaimed() > before.unreclaimed()
            && started.elapsed() < Duration::from_secs(5)
        {
            thread::sleep(Duration::from_millis(50));
        }
        let after = pipe_reader_capacity();
        println!("[清理预留·饱和] 归还后 {after}");
        assert!(
            after.obligations() <= before.obligations(),
            "本用例占用的额度必须全部归还（允许顺手回收别的残留，但不许多占）"
        );
        assert_eq!(after.capacity_fault_units, 0);
        assert!(!after.admission_locked, "本用例不得触发容量契约故障");
    }


    /// 接纳点必须在**创建读取线程之前**预留；容量只能靠"已核实结束"的回收恢复；
    /// **登记不再重新竞争另一个容量池**，故障态**不丢任何所有权**。
    #[test]
    fn capacity_is_reserved_before_any_reader_thread_exists_and_only_verified_readers_are_reclaimed() {
        let source = include_str!("pipe.rs");
        let admission = source
            .split("pub(crate) fn with_options(")
            .nth(1)
            .expect("with_options")
            .split("pub(crate) fn with_options_and_reservation(")
            .next()
            .expect("with_options 函数体");
        let reserve_at = admission
            .find("reserve_pipe_reader_capacity(1)")
            .expect("接纳必须预留容量");
        let delegate_at = admission
            .find("with_options_and_reservation(")
            .expect("接纳必须把令牌交给读取器");
        assert!(
            reserve_at < delegate_at,
            "预留必须发生在创建读取线程**之前**（否则会先产生失管读取器再谈容量）"
        );
        // 唯一会创建读取线程的构造器必须**收下**一份容量预留作为参数：
        // 因此不存在"没有预留的读取线程"。
        let spawn_owner = source
            .split("pub(crate) fn with_options_and_reservation(")
            .nth(1)
            .expect("with_options_and_reservation")
            .split("fn drain_inner(")
            .next()
            .expect("with_options_and_reservation 函数体");
        assert!(
            spawn_owner.contains("reservation: PipeReaderCapacityReservation")
                && spawn_owner.contains(".spawn(")
                && spawn_owner.contains("capacity: reservation"),
            "读取线程只能由收下容量预留的构造器创建"
        );
        assert!(
            !admission.contains("mem::forget"),
            "拒绝接纳时交进来的句柄必须被关闭，不得遗忘"
        );
        // 清扫/回收只认"已核实结束"：不得驱逐未知读取器来腾位。
        let park = source
            .split("fn park_retained(")
            .nth(1)
            .expect("park_retained")
            .split("pub fn reclaim_finished_pipe_readers(")
            .next()
            .expect("park_retained 函数体");
        assert!(
            park.contains("reclaim_locked(&mut registry)"),
            "登记前的清扫只能回收已核实结束的残留（且与回收走同一个实现）"
        );
        // 登记只是 `Active → Retained` 的状态转换：令牌**跟着读取器进入登记项**，
        // 既不归还、也不与"另一个容量池"（旧的 `entries.len() < limit`）竞争。
        assert!(
            !park.contains("mem::replace") && !park.contains("decide_retention(registry.limit"),
            "登记不得重新竞争另一个容量池（预留从创建前一直持有到实际回收）"
        );
        assert!(
            park.contains("inner.capacity.is_counted()")
                && park.contains("decide_retention(reserved, registry.entries.len(), registry.limit)"),
            "登记决策只问'手里有没有计账的预留'"
        );
        // 故障态**保留**所有权：登记项照常入表（不是丢句柄、不是分离线程）。
        assert!(
            park.contains("registry.entries.push(RetainedReader {")
                && park.contains("fault: Some(reason)"),
            "契约被破坏时读取器仍须入表受管理"
        );
        assert!(
            park.contains("registry.admission_locked = true"),
            "契约被破坏时必须锁住该监督器的新 helper 接纳"
        );
        let reclaim = source
            .split("fn reclaim_locked(")
            .nth(1)
            .expect("reclaim_locked")
            .split("pub fn reclaim_finished_pipe_readers(")
            .next()
            .expect("回收函数体");
        assert!(
            reclaim.contains("verified_completion().0.is_confirmed()"),
            "回收必须核实结束之后才释放容量"
        );
        assert!(
            source.contains("let mut registry = lock_registry();\n    reclaim_locked(&mut registry)"),
            "唯一的非阻塞清扫入口必须复用同一个实现（登记前的清扫与回收不得各写一份）"
        );
        // 故障锁只能由**显式核查**解除：没有任何一处代码在回收时顺手把锁打开。
        let review = source
            .split("pub fn review_and_clear_pipe_capacity_fault(")
            .nth(1)
            .expect("review_and_clear_pipe_capacity_fault")
            .split("pub fn retained_pipe_reader_count(")
            .next()
            .expect("核查函数体");
        assert!(
            review.contains("registry.admission_locked = false")
                && review.contains("capacity_fault_units > 0"),
            "故障锁必须在显式核查里、且只在故障对象回收之后才解除"
        );
        let production = source
            .split("#[cfg(test)]\nmod tests {")
            .next()
            .expect("生产代码部分");
        assert_eq!(
            production.matches("admission_locked = false").count(),
            1,
            "解除接纳锁只允许一处（否则会出现'自动恢复'的第二个入口）"
        );
        // 复制线程句柄失败时**不得**丢 `JoinHandle`：对象转入登记继续受管理，
        // 核实与取消降级到 `JoinHandle`（同一内核证据）。
        let duplicate_failure = source
            .split("let Some(thread_handle) = thread_handle else {")
            .nth(1)
            .expect("句柄复制失败分支")
            .split("return Err(io::Error::new(")
            .next()
            .expect("失败分支函数体");
        assert!(
            duplicate_failure.contains("thread: Some(thread)")
                && duplicate_failure.contains("capacity: reservation")
                && duplicate_failure.contains("park_retained("),
            "部分创建成功时：线程与容量预留都必须保持受管理（不得丢句柄、不得归还容量）"
        );
        assert!(source.contains("fn raw_thread_handle(&self) -> Option<HANDLE>"));
    }

    // ------------------------------------------------------------------
    // RD4-09 §B-7(4)：PeekNamedPipe 的句柄模式与调用点（条件性核查）
    // ------------------------------------------------------------------

    /// `PeekNamedPipe` 的阻塞前提**实测**：句柄是不是同步句柄、并发阻塞读会不会拖住它。
    ///
    /// 官方文档（PeekNamedPipe, Remarks）：*"The **PeekNamedPipe** function can block thread
    /// execution the same way any I/O function can when called on a synchronous handle in a
    /// multi-threaded application. To avoid this condition, use a pipe handle created for
    /// asynchronous I/O."* 所以**不能**由"peek／轮询"这个名字证明它绝不阻塞。
    ///
    /// 本测试用真实子进程 + 真实匿名管道实测两件事，并如实打印数值：
    /// 1. **句柄模式**：管道里没有数据、写端仍由存活子进程持有时，`lpOverlapped = NULL` 的
    ///    一次读**不会立即返回**。若句柄是 `FILE_FLAG_OVERLAPPED`（异步句柄），按 ReadFile
    ///    文档，NULL `lpOverlapped` 下"the function can incorrectly report that the read
    ///    operation is complete"，即会立刻返回——所以"读被阻塞"就是"**同步句柄**"的证据。
    /// 2. **文档所述的多线程情形**：同一句柄上另有一个线程正阻塞在读里时，`PeekNamedPipe`
    ///    是否被拖住——实测值如实记录，不作推断。
    #[test]
    fn peek_named_pipe_handle_mode_and_concurrent_blocking_read_are_measured_not_assumed() {
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 3"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("必须能启动 powershell.exe");
        // 所有权交给探针线程（读完就关），本线程只保留句柄值做 PeekNamedPipe。
        let raw = child.stdout.take().expect("stdout 管道").into_raw_handle();
        let raw_value = raw as usize;
        let (read_done_tx, read_done_rx) = mpsc::channel::<(String, Duration)>();
        let started = Instant::now();
        let reader = thread::spawn(move || {
            let handle = raw_value as HANDLE;
            // SAFETY: 句柄所有权在本线程独占接管；读结束即随 `File` 关闭。
            let mut file = unsafe { std::fs::File::from_raw_handle(handle) };
            let mut buffer = [0u8; 1];
            let outcome = match file.read(&mut buffer) {
                Ok(read) => format!("ok {read}"),
                Err(error) => format!("err {error}"),
            };
            let _ = read_done_tx.send((outcome, started.elapsed()));
        });
        // 等"读确实阻塞着"成立：1 秒内没有返回，而管道里没有任何数据。
        thread::sleep(Duration::from_millis(800));
        assert!(
            read_done_rx.try_recv().is_err(),
            "空管道 + 存活写端上的 NULL-overlapped 读必须阻塞：若立即返回，句柄就是异步句柄"
        );
        assert!(!reader.is_finished(), "读取线程当时仍在阻塞读里");
        println!(
            "[B-7 句柄模式] 无数据且写端存活时，NULL-overlapped 读在 800ms 内没有返回 ⇒ 同步句柄"
        );

        // 文档所述的多线程情形：同一句柄上另一个线程正阻塞在读里。
        let (peek_tx, peek_rx) = mpsc::channel::<(i32, u32, Duration)>();
        let peek_thread = thread::spawn(move || {
            let handle = raw_value as HANDLE;
            let mut available = 0u32;
            let started = Instant::now();
            // SAFETY: 只查询可读字节数，不提供缓冲、也不消费数据。
            let ok = unsafe {
                PeekNamedPipe(
                    handle,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            };
            let code = if ok != 0 {
                0
            } else {
                io::Error::last_os_error().raw_os_error().unwrap_or_default() as u32
            };
            let _ = peek_tx.send((ok, code, started.elapsed()));
        });
        // 有界观察：不在"PeekNamedPipe 是否会被永久拖住"上做无界等待。
        let peek_observation = peek_rx.recv_timeout(Duration::from_secs(6));
        let (peek_ok, peek_code, peek_elapsed) = match peek_observation {
            Ok(value) => value,
            Err(_) => {
                println!(
                    "[B-7 并发阻塞读] 实测：PeekNamedPipe 在同一句柄有并发阻塞读时**未在 6s 内返回**\
                     （文档所述条件成立；该探针线程随后随写端关闭自行结束）"
                );
                (0, 0, Duration::from_secs(6))
            }
        };
        if peek_elapsed < Duration::from_millis(200) {
            println!(
                "[B-7 并发阻塞读] 实测：PeekNamedPipe 在并发阻塞读下仍然及时返回（ok={peek_ok} code={peek_code} {peek_elapsed:?}）\
                 ⇒ 本机/本配置下文档所述条件没有发生"
            );
        } else {
            println!(
                "[B-7 并发阻塞读] 实测：PeekNamedPipe 被拖住 {peek_elapsed:?}（ok={peek_ok} code={peek_code}）\
                 ⇒ 文档所述条件在本配置下**成立**：它只在'同一句柄上有并发操作'时才会阻塞"
            );
        }
        // 读在写端关闭（子进程退出）后必须结束：探针不留下无界等待。
        let (read_outcome, read_elapsed) = read_done_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("写端关闭后阻塞的读必须结束");
        println!("[B-7 句柄模式] 读结束于 {read_elapsed:?}（{read_outcome}）");
        assert!(read_elapsed < Duration::from_secs(15));
        let _ = reader.join();
        let _ = peek_thread.join();
        let _ = child.wait();
    }

    /// **B-7 验收锚点**：`PeekNamedPipe` 的调用点与"有界/受控"的实现事实。
    ///
    /// 生产调用点只有一处：`peek_available`（它只在**持有该管道句柄的那个读取线程**里被调用）；
    /// 宿主线程只对**线程**句柄做有界等待与同步 I/O 取消，不碰管道句柄。因此
    /// "同一句柄上有并发操作"这个 `PeekNamedPipe` 阻塞前提在生产路径上不成立。
    #[test]
    fn b07_peek_call_site_and_bounded_wait_anchors_are_pinned() {
        let source = include_str!("pipe.rs");
        // 只看生产代码（测试模块里的直接调用不算生产调用点）。
        let production = source
            .split("#[cfg(test)]\nmod tests {")
            .next()
            .expect("生产代码部分");
        // 运行时拼出探针串，避免"本测试自身"被算进调用点计数。
        let peek_needle = format!("PeekNamedPipe{}", "(");
        let helper_needle = format!("peek_available{}", "(");
        assert_eq!(
            production.matches(&peek_needle).count(),
            1,
            "PeekNamedPipe 在生产代码里只能有一处调用"
        );
        assert_eq!(
            production.matches(&helper_needle).count(),
            2,
            "peek_available 只应有定义与唯一调用点"
        );
        let poll = source
            .split("fn poll_available(")
            .nth(1)
            .expect("poll_available")
            .split("fn blocking_read(")
            .next()
            .expect("poll_available 函数体");
        assert!(
            poll.contains("peek_available(file)") && poll.contains("file.read("),
            "peek 与 read 必须在同一个读取线程里串行发生（同一个 File 所有者）"
        );
        assert!(
            poll.contains("thread::sleep(PIPE_POLL_INTERVAL)"),
            "轮询必须有受控等待，不能忙等"
        );
        assert!(PIPE_POLL_INTERVAL >= Duration::from_millis(1));
        // 有界等待：毫秒数被夹在 u32::MAX-1 以内，且实现里显式排除 INFINITE。
        assert!(
            source.contains("// `INFINITE` 明确不使用"),
            "无界等待必须被显式排除并留下理由"
        );
        assert!(
            source.contains("millis > u128::from(u32::MAX - 1)"),
            "等待时长必须被夹到有界范围"
        );
        // 宿主侧只等**线程**句柄：完成状态由 WaitForSingleObject 核实。
        assert!(source.contains("WaitForSingleObject(handle.raw(), millis)"));
        // 句柄副本缺失时的降级核实用的是**同一份内核证据**（`JoinHandle::is_finished`
        // 在内核里就是同一线程句柄上的 `WaitForSingleObject(handle, 0)`），
        // 且有界轮询实现——不得退化成无界等待。
        assert!(
            source.contains("thread.is_finished()")
                && source.contains("fn wait_thread_via_join_handle(&self, wait: Duration) -> ThreadWait"),
            "复制线程句柄失败时必须保留可核实的完成状态（同一内核证据）"
        );
    }

    /// **B-7(5) 受控等待的实测**：空管道上读取线程的轮数 ≈ 时间 / 轮询间隔，而不是忙等。
    #[test]
    fn reader_polling_is_a_controlled_wait_not_a_busy_loop() {
        // 进程级容量：与本进程其它创建读取器的用例串行。
        let _pipe_slot = pipe_reader_test_lock();
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 3"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("必须能启动 powershell.exe");
        let stdout = PipeReaderSupervisor::stdout("poll-wait-probe", child.stdout.take().unwrap())
            .expect("监督器");
        let shared = stdout.test_shared().expect("读取线程的内部状态");
        let window = Duration::from_millis(600);
        let before = shared.test_loops();
        thread::sleep(window);
        let after = shared.test_loops();
        let rounds = after.saturating_sub(before);
        let expected = window.as_millis() as u64 / PIPE_POLL_INTERVAL.as_millis() as u64;
        println!(
            "[B-7 受控等待] {window:?} 内轮数={rounds}（期望≈{expected}）⇒ 每轮都 sleep，不是忙等"
        );
        assert!(rounds >= 1, "读取循环必须真的在轮询");
        assert!(
            rounds <= expected + 8,
            "轮数必须与轮询间隔一致（{rounds} vs ≈{expected}）：不能是忙等"
        );
        let drained = stdout.drain(Duration::from_millis(200));
        assert!(drained.completion.is_confirmed());
        let _ = child.wait();
    }

    // ------------------------------------------------------------------
    // RD4-09 §B-8(4)：隔离子进程的真实接纳/容量测试
    // ------------------------------------------------------------------

    /// 父测试**驱动的专用测试子进程**的环境变量名（子进程体只在它存在时运行）。
    const CAPACITY_CHILD_ENV: &str = "COOLZHU_PIPE_CAPACITY_CHILD";
    /// 子进程体的测试名（父进程用 `--exact` 只跑它）。
    const CAPACITY_CHILD_TEST: &str = "pipe::tests::capacity_child_process_body";

    /// 子进程体：**只有**被父测试以专用环境变量启动时才执行。
    ///
    /// 为什么必须独立进程：残留登记是**进程级**的，同进程内的并行用例会互相看到对方
    /// 占用的容量；容量饱和与接纳拒绝必须在"没有别的用例在跑"的干净进程里核对。
    ///
    /// 构件说明（如实标注）：reader 用**真实生产接纳/登记接口**建立；让它们"尚未完成"
    /// 的方式是"**真实资源 + 受控生命周期注入**"——每个 reader 的管道写端由一个**存活的
    /// 真实子进程**（`powershell Start-Sleep`）持有，测试自己控制它的存活时间并在有界窗口内
    /// 结束它。本测试**不**声称复现了某种永不返回的系统调用，也**不**向真实桌面注入输入。
    #[test]
    fn capacity_child_process_body() {
        if std::env::var_os(CAPACITY_CHILD_ENV).is_none() {
            return;
        }
        let initial = pipe_reader_capacity();
        assert_eq!(initial.limit, MAX_RETAINED_PIPE_READERS);
        assert_eq!(
            initial.obligations(),
            0,
            "子进程必须是干净起点（残留 + 预留都为 0）"
        );
        println!("[capacity-child] 起点 {initial}");

        // 4 个存活子进程 × (stdout + stderr) = 8 个真实 reader = 上限。
        let mut holders = Vec::new();
        let mut labels = Vec::new();
        for index in 0..4 {
            let mut child = spawn_pipe_holder();
            let stdout_label = format!("capacity-child-{index}-stdout");
            let stderr_label = format!("capacity-child-{index}-stderr");
            // 真实生产接纳接口：一个 helper 预留两个单位，再逐条建流。
            let mut admission = HelperPipeReadersAdmission::acquire()
                .unwrap_or_else(|denial| panic!("空账目下必须接纳：{denial}"));
            assert_eq!(admission.remaining_units(), PIPE_READERS_PER_HELPER);
            let mut stdout = admission
                .stdout(stdout_label.clone(), child.stdout.take().expect("stdout"))
                .expect("stdout 读取器");
            let mut stderr = admission
                .stderr(stderr_label.clone(), child.stderr.take().expect("stderr"))
                .expect("stderr 读取器");
            assert_eq!(admission.remaining_units(), 0, "两条流必须各带走一个单位");
            // 尚未完成：登记接口（**不**走 drain，因此没有发出停止信号）。
            for (label, reader) in [
                (stdout_label.clone(), &mut stdout),
                (stderr_label.clone(), &mut stderr),
            ] {
                let inner = reader.test_take_inner().expect("监督器内部状态");
                let outcome = park_retained(label.clone(), inner);
                assert!(
                    matches!(outcome, PipeRetentionOutcome::Retained { .. }),
                    "容量足够时未核实的读取必须被寄存：{outcome:?}"
                );
                labels.push(label);
            }
            holders.push(child);
        }
        let full = pipe_reader_capacity();
        println!("[capacity-child] 8 个 reader 全部登记后 {full}");
        assert_eq!(full.retained_unreclaimed, 8);
        assert_eq!(full.active_unreclaimed, 0);
        assert_eq!(
            full.reserved_not_created, 0,
            "两条流各自带走了 1 个单位（凭证剩余为 0）"
        );
        assert_eq!(
            full.obligations(),
            8,
            "预留从创建前一直持有到实际回收：登记（Active→Retained）不改变容量义务"
        );
        assert_eq!(full.unreclaimed(), 8);
        assert_eq!(full.owned_unreclaimed(), 8, "8 个残留仍由监督器持有");
        assert_eq!(full.unowned_unreclaimed, 0, "没有任何读取器失去管理所有权");
        assert_eq!(full.capacity_fault_units, 0);
        assert!(!full.admission_locked);
        assert_eq!(full.available(), 0);
        assert!(
            pipe_supervision_fault().is_none(),
            "还没有读取器失去限额/所有权：不得出现故障状态"
        );

        // 9/10 次真实接纳请求：必须**明确拒绝**，且没有产生任何新的失管 reader。
        let helper_denial = HelperPipeReadersAdmission::acquire()
            .expect_err("容量已满：新 helper 的接纳必须被拒绝");
        println!("[capacity-child] 新 helper 接纳被拒：{helper_denial}");
        assert_eq!(helper_denial.reason(), PIPE_CAPACITY_EXHAUSTED);
        assert_eq!(helper_denial.requested, PIPE_READERS_PER_HELPER);
        assert_eq!(helper_denial.available, 0);
        assert_eq!(helper_denial.limit, MAX_RETAINED_PIPE_READERS);
        assert_eq!(helper_denial.held, 8);
        assert_eq!(helper_denial.reserved_not_created, 0);
        assert!(
            !helper_denial.locked_by_fault,
            "这是**容量不足**，不是故障锁：两个拒绝码不能混用"
        );
        let single_denial = reserve_pipe_reader_capacity(1).expect_err("单个 reader 也必须被拒绝");
        println!("[capacity-child] 单 reader 预留被拒：{single_denial}");
        assert!(reserve_pipe_reader_capacity(0).is_err(), "0 个单位不合法");
        let after_denials = pipe_reader_capacity();
        println!("[capacity-child] 拒绝之后 {after_denials}");
        assert_eq!(after_denials.retained_unreclaimed, 8, "拒绝不得改变已登记数量");
        assert_eq!(after_denials.reserved_not_created, 0, "拒绝不得留下预留");
        assert_eq!(
            after_denials.admission_rejected, 3,
            "三次拒绝（helper 2 单位 / 单 reader 1 单位 / 0 单位）必须被如实计数"
        );
        assert_eq!(
            after_denials.unowned_unreclaimed, 0,
            "拒绝发生在创建之前 ⇒ 不得丢下任何失管 reader"
        );
        assert!(
            pipe_supervision_fault().is_none(),
            "拒绝不是故障：不得出现故障状态"
        );
        assert_eq!(
            retained_pipe_reader_labels().len(),
            8,
            "拒绝不得创建第 9 个 reader"
        );

        // ------------------------------------------------------------------
        // **故障注入**（§C-17）：契约被破坏——一个**没有计账预留**的读取器要求登记。
        // 正常接纳路径已经在创建之前预留，因此这条分支只能用注入构造；它既然存在于
        // 代码里，就必须有真实的行为证据，而不是"文档宣称不可达"。
        // ------------------------------------------------------------------
        let mut ninth_holder = spawn_pipe_holder();
        let ninth_label = "capacity-child-9-fault-injection".to_string();
        let mut ninth = PipeReaderSupervisor::fault_injection_reader_without_reservation(
            ninth_label.clone(),
            ninth_holder.stdout.take().expect("stdout"),
            PipeReadStrategy::PollAvailable,
            MAX_PIPE_BUFFER_BYTES,
        )
        .expect("故障注入：不经接纳预留建立第 9 个 reader");
        let ninth_shared = ninth.test_shared().expect("第 9 个 reader 的内部状态");
        let ninth_inner = ninth.test_take_inner().expect("第 9 个 reader 的内部状态");
        let fault = park_retained(ninth_label.clone(), ninth_inner);
        println!(
            "[capacity-child] 契约破坏后的登记结果：{fault:?}（对照：上限 {MAX_RETAINED_PIPE_READERS}）"
        );
        // ① 明确的严重内部故障。
        assert_eq!(
            fault,
            PipeRetentionOutcome::CapacityFault {
                limit: MAX_RETAINED_PIPE_READERS,
                held: 9,
                fault_units: 1,
                reason: CapacityFaultReason::ReservationAbsent,
            },
            "契约被破坏必须如实记为故障（含实际数量），不得宣称读取已结束"
        );
        // ② 保留所有权：读取器仍在登记表里，缓冲区与线程都还受管理。
        assert!(
            fault.holds_unreclaimed_reader(),
            "故障资源**仍被持有**（这正是 holds_unreclaimed_reader 返回 true 正确的场景）"
        );
        assert_eq!(fault.capacity_fault_units(), 1);
        assert_eq!(
            capacity_fault_reader_labels(),
            vec![ninth_label.clone()],
            "故障读取器必须仍在监督器手里"
        );
        assert_eq!(retained_pipe_reader_count(), 9);
        assert!(
            retained_pipe_reader_bytes(&ninth_label).is_some(),
            "故障读取器的缓冲区没有被丢弃"
        );
        let loops_before = ninth_shared.test_loops();
        thread::sleep(Duration::from_millis(300));
        let loops_after = ninth_shared.test_loops();
        println!("[capacity-child] 故障登记后被持有的读取线程轮数 {loops_before} → {loops_after}");
        assert!(loops_after > loops_before, "它还在跑（未核实结束）");
        // ③ 锁住新 helper 接纳：账目已经不可信，不再接纳新对象。
        let fault_state = pipe_supervision_fault().expect("契约破坏必须留下故障状态");
        println!("[capacity-child] 故障状态 {fault_state}");
        assert_eq!(fault_state.capacity_fault_units, 1);
        assert_eq!(fault_state.capacity_faults, 1);
        assert!(fault_state.admission_locked);
        assert_eq!(
            fault_state.unowned_unreclaimed, 0,
            "与旧的 Saturated 形态的关键区别：**没有任何所有权丢失**"
        );
        let locked_denial = HelperPipeReadersAdmission::acquire()
            .expect_err("故障锁未核查前必须继续拒绝接纳");
        println!("[capacity-child] 故障锁下的接纳：{locked_denial}");
        assert_eq!(locked_denial.reason(), PIPE_CAPACITY_FAULT);
        assert!(locked_denial.locked_by_fault);
        // ⑤ 诊断给出**实际数量**（含已超出正常不变量的部分）。
        let diagnosis = pipe_reader_capacity();
        println!("[capacity-child] 故障态账目 {diagnosis}");
        assert_eq!(diagnosis.retained_unreclaimed, 9, "实际持有 9 个");
        assert_eq!(diagnosis.unreclaimed(), 9);
        assert_eq!(diagnosis.owned_unreclaimed(), 9, "9 个都仍受管理");
        assert_eq!(diagnosis.capacity_fault_units, 1, "超出不变量 1 个");
        assert_eq!(
            diagnosis.obligations(),
            9,
            "义务（9）超过上限（8）正是这条分支必须被记录为故障的原因"
        );
        assert_eq!(diagnosis.available(), 0);
        // ⑥ 不自动恢复：故障对象**尚未回收**时核查必须失败。
        let premature = review_and_clear_pipe_capacity_fault()
            .expect_err("故障对象还没有被回收：核查不得通过、接纳必须继续被锁住");
        println!("[capacity-child] 过早核查被拒 {premature}");
        assert!(premature.admission_locked);
        assert_eq!(premature.capacity_fault_units, 1);
        // ④ 有界取消/回收：写端关闭 ⇒ 线程结束 ⇒ **可按核实结果回收**（旧形态做不到这件事，
        //    因为它已经丢掉了登记对象与线程句柄）。
        let _ = ninth_holder.kill();
        let _ = ninth_holder.wait();
        let started = Instant::now();
        let mut settled = false;
        while started.elapsed() < Duration::from_secs(5) {
            if reclaim_finished_pipe_readers() > 0 {
                settled = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        println!(
            "[capacity-child] 故障读取器在写端关闭后 {} 轮（可核实 = 所有权没有丢）",
            if settled { "被核实并回收" } else { "**未被**回收" }
        );
        assert!(
            settled,
            "故障对象仍受监督器管理 ⇒ 它必须能被核实结束并回收（旧的分离线程没有这条路径）"
        );
        let after_fault_reclaim = pipe_reader_capacity();
        println!("[capacity-child] 故障对象回收后 {after_fault_reclaim}");
        assert_eq!(after_fault_reclaim.retained_unreclaimed, 8);
        assert_eq!(after_fault_reclaim.capacity_fault_units, 0);
        assert!(
            after_fault_reclaim.admission_locked,
            "⑥ 回收**不**自动恢复服务能力：必须先完成核查"
        );
        assert!(pipe_supervision_fault().is_some(), "历史故障仍然被记录");
        // 显式核查（对象已回收 + 账目回到不变量内）⇒ 解除接纳锁。
        review_and_clear_pipe_capacity_fault().expect("对象已回收且账目回到不变量内：核查必须通过");
        let after_review = pipe_reader_capacity();
        println!("[capacity-child] 显式核查通过后 {after_review}");
        assert!(!after_review.admission_locked);
        assert_eq!(after_review.capacity_fault_units, 0);
        assert_eq!(
            pipe_supervision_fault().expect("历史故障次数是历史，不清零").capacity_faults,
            1
        );
        assert_eq!(after_review.obligations(), 8);

        // 让部分 reader **真正完成**：结束一个持有者 ⇒ 它的两条流读到 EOF ⇒ 线程结束。
        let mut released = holders.remove(0);
        let _ = released.kill();
        let _ = released.wait();
        let started = Instant::now();
        while pipe_reader_capacity().retained_unreclaimed > 6
            && started.elapsed() < Duration::from_secs(5)
        {
            thread::sleep(Duration::from_millis(50));
        }
        let after_release = pipe_reader_capacity();
        println!("[capacity-child] 结束一个持有者后 {after_release}");
        assert_eq!(
            after_release.retained_unreclaimed, 6,
            "容量必须按**实际完成**释放：两条流各腾出一个单位"
        );
        assert_eq!(after_release.reserved_not_created, 0);
        assert_eq!(after_release.unowned_unreclaimed, 0);
        assert_eq!(after_release.available(), 2);

        // 重复收尾/迟到完成不得重复释放槽位。
        assert_eq!(
            reclaim_finished_pipe_readers(),
            0,
            "已经没有可回收的残留：重复收尾必须返回 0"
        );
        let repeated = pipe_reader_capacity();
        assert_eq!(repeated.retained_unreclaimed, 6, "重复收尾不得多放出容量");
        assert_eq!(repeated.reserved_not_created, 0);
        assert_eq!(repeated.available(), 2);

        // 并发申请：可用 2 个单位，4 个线程各要 1 个 ⇒ 恰好 2 个成功、2 个被拒，且账目回到原点。
        let mut handles = Vec::new();
        for _ in 0..4 {
            handles.push(thread::spawn(|| reserve_pipe_reader_capacity(1)));
        }
        let mut granted = 0;
        let mut denied = 0;
        let mut tokens = Vec::new();
        for handle in handles {
            match handle.join().expect("申请线程") {
                Ok(token) => {
                    granted += 1;
                    tokens.push(token);
                }
                Err(denial) => {
                    denied += 1;
                    println!("[capacity-child] 并发申请被拒：{denial}");
                }
            }
        }
        println!("[capacity-child] 并发申请（可用 2）：成功 {granted} / 被拒 {denied}");
        assert_eq!((granted, denied), (2, 2), "并发申请不得超接纳、也不得多释放");
        drop(tokens);
        let after_concurrent = pipe_reader_capacity();
        println!("[capacity-child] 并发申请归还后 {after_concurrent}");
        assert_eq!(after_concurrent.reserved_not_created, 0);

        // ------------------------------------------------------------------
        // 容量判据的**两个方向**在"部分可用"这一侧也必须有用例：
        // 若拒绝只发生在 available == 0，就无法把"按义务判定"与"看到残留就拒绝"区分开；
        // 若接纳只在空账目下发生，就无法证明"可用 == 请求"这个边界是接纳而不是拒绝。
        // ------------------------------------------------------------------
        {
            // 占用 1 个单位：已保留 6、预留未创建 1、可用 1。
            let held = reserve_pipe_reader_capacity(1).expect("部分可用时单 reader 必须被接纳");
            let partial = pipe_reader_capacity();
            println!("[capacity-child] 判据·部分可用（占用 1 后）{partial}");
            assert_eq!(
                (
                    partial.retained_unreclaimed,
                    partial.reserved_not_created,
                    partial.available()
                ),
                (6, 1, 1)
            );
            // **必须触发**：请求 2 > 可用 1。容量明明还剩 1 个单位也必须拒绝——
            // 判据是"义务 + 请求 > 上限"，不是"看到残留就拒绝"，也不是"还有空闲就放行"。
            let labels_before = retained_pipe_reader_labels().len();
            let partial_denial = HelperPipeReadersAdmission::acquire()
                .expect_err("可用 1 < 请求 2：helper 接纳必须被拒绝");
            println!("[capacity-child] 判据·部分可用（必须触发）{partial_denial}");
            assert_eq!(partial_denial.reason(), PIPE_CAPACITY_EXHAUSTED);
            assert_eq!(partial_denial.requested, PIPE_READERS_PER_HELPER);
            assert_eq!(
                (
                    partial_denial.available,
                    partial_denial.held,
                    partial_denial.reserved_not_created
                ),
                (1, 6, 1)
            );
            // 这次拒绝同样是"没有创建读取器"：登记数不变、失管数不变
            // （此时已结束 1 个持有者 ⇒ 只剩 3 个持有者的 6 条流仍在登记表里）。
            assert_eq!(
                (labels_before, retained_pipe_reader_labels().len()),
                (6, 6),
                "拒绝不得新增登记项"
            );
            assert_eq!(
                pipe_reader_capacity().unowned_unreclaimed,
                0,
                "没有任何读取器失去管理所有权（拒绝也不得增加它）"
            );
            assert_eq!(pipe_reader_capacity().capacity_fault_units, 0);
            held.release();
        }
        // **必须不触发**：请求恰好等于可用（已保留 6 仍在账上，但义务允许）⇒ 必须接纳。
        {
            let admission = HelperPipeReadersAdmission::acquire()
                .expect("可用 2 == 请求 2：必须接纳（判据只看义务，不因为账上有残留就拒绝）");
            assert_eq!(admission.remaining_units(), PIPE_READERS_PER_HELPER);
            let boundary = pipe_reader_capacity();
            println!("[capacity-child] 判据·边界（可用 == 请求，必须接纳）{boundary}");
            assert_eq!(
                (
                    boundary.retained_unreclaimed,
                    boundary.reserved_not_created,
                    boundary.available()
                ),
                (6, 2, 0)
            );
            // 恰好用满之后再要 1 个：必须被拒绝（上界就是上限本身，不是近似值）。
            assert!(reserve_pipe_reader_capacity(1).is_err(), "可用 0 时必须拒绝");
            drop(admission);
            let restored = pipe_reader_capacity();
            println!("[capacity-child] 判据·边界用例归还后 {restored}");
            assert_eq!(
                (
                    restored.retained_unreclaimed,
                    restored.reserved_not_created,
                    restored.available()
                ),
                (6, 0, 2)
            );
        }

        // ------------------------------------------------------------------
        // **部分创建成功**（§C-21 第 9 条）：读取线程已经建好、线程句柄副本却复制失败。
        // 注入方式：跳过句柄复制。断言的两件事：
        // ① 已创建对象的额度**只能在真正回收时**释放（不得随失败一起归还）；
        // ② 线程不丢（`JoinHandle` 留在登记项里，仍可核实结束）。
        // ------------------------------------------------------------------
        {
            let mut holder = spawn_pipe_holder();
            let label = "capacity-child-partial-creation".to_string();
            let reservation = reserve_pipe_reader_capacity(1).expect("部分创建用例需要 1 个单位");
            let before = pipe_reader_capacity();
            println!("[capacity-child] 部分创建前 {before}");
            let attempted = PipeReaderSupervisor::fault_injection_reader_without_verification_handle(
                label.clone(),
                holder.stdout.take().expect("stdout"),
                PipeReadStrategy::PollAvailable,
                MAX_PIPE_BUFFER_BYTES,
                reservation,
            );
            let error = attempted
                .err()
                .expect("句柄副本失败必须**返回 Err**（不抛、不静默）");
            println!("[capacity-child] 部分创建失败如实上报：{error}");
            assert!(error.to_string().contains("仍受监督器管理"));
            let after_failure = pipe_reader_capacity();
            println!("[capacity-child] 部分创建失败后 {after_failure}");
            assert_eq!(
                after_failure.reserved_not_created, 0,
                "那 1 个单位已经交给读取器，不得退回'预留未创建'"
            );
            assert_eq!(
                after_failure.retained_unreclaimed,
                before.retained_unreclaimed + 1,
                "已创建的对象转入登记（仍受管理），额度**没有**被归还"
            );
            assert_eq!(
                after_failure.obligations(),
                before.obligations(),
                "义务总量不变：失败不得凭空多出或释放容量"
            );
            assert!(retained_pipe_reader_labels().contains(&label));
            // 真正回收时才释放：写端关闭 ⇒ 线程结束 ⇒ 可核实、可回收。
            let _ = holder.kill();
            let _ = holder.wait();
            let started = Instant::now();
            let mut reclaimed = false;
            while started.elapsed() < Duration::from_secs(5) {
                if reclaim_finished_pipe_readers() > 0 {
                    reclaimed = true;
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
            assert!(reclaimed, "降级核实的读取器也必须能被回收");
            let after_reclaim = pipe_reader_capacity();
            println!("[capacity-child] 部分创建对象回收后 {after_reclaim}");
            assert_eq!(after_reclaim.obligations(), before.obligations() - 1);
            assert_eq!(after_reclaim.unowned_unreclaimed, 0);
        }

        // 有界清理：结束全部持有者，等到容量完全归位。
        for mut holder in holders {
            let _ = holder.kill();
            let _ = holder.wait();
        }
        let started = Instant::now();
        while pipe_reader_capacity().retained_unreclaimed > 0
            && started.elapsed() < Duration::from_secs(10)
        {
            thread::sleep(Duration::from_millis(50));
        }
        let final_capacity = pipe_reader_capacity();
        println!("[capacity-child] 清理完成 {final_capacity}");
        assert_eq!(final_capacity.retained_unreclaimed, 0);
        assert_eq!(final_capacity.reserved_not_created, 0);
        assert_eq!(final_capacity.unreclaimed(), 0);
        assert_eq!(final_capacity.unowned_unreclaimed, 0);
        assert_eq!(
            final_capacity.available(),
            MAX_RETAINED_PIPE_READERS,
            "故障核查通过后服务能力完全恢复"
        );
        println!("[capacity-child] done labels={}", labels.len());
    }

    /// 启动一个**存活一段时间**的真实子进程（stdout/stderr 都是管道；不注入桌面输入）。
    fn spawn_pipe_holder() -> Child {
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 45",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("必须能启动 powershell.exe 持有管道写端")
    }

    /// **父级**：启动专用测试子进程，并**自己**承担独立看门限时与回收责任。
    ///
    /// 父级只做三件事：起子进程、在**有界**窗口内等它、核对它的输出与退出状态；
    /// 超时/未退出就按身份终止（不无界等待）。
    #[test]
    fn capacity_isolation_parent_runs_a_dedicated_child_process_with_a_bounded_watchdog() {
        let exe = std::env::current_exe().expect("当前测试可执行文件");
        let child = Command::new(&exe)
            .args(["--exact", "--nocapture", CAPACITY_CHILD_TEST])
            .env(CAPACITY_CHILD_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("必须能启动专用测试子进程");
        let pid = child.id();
        let identity = capture_process_identity(pid).ok();
        let mut child = child;
        let (tx, rx) = mpsc::channel();
        let watchdog = Duration::from_secs(120);
        let watcher = thread::spawn(move || {
            // 子进程的输出必须被读取，否则管道写满会把它卡住。
            let output = {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    let _ = pipe.read_to_string(&mut stdout);
                }
                if let Some(mut pipe) = child.stderr.take() {
                    let _ = pipe.read_to_string(&mut stderr);
                }
                (stdout, stderr, child.wait())
            };
            let _ = tx.send(output);
        });
        let output = match rx.recv_timeout(watchdog) {
            Ok(output) => output,
            Err(_) => {
                // 有界回收：按身份终止子进程（PID 若被复用会被拒绝），不无界等待。
                if let Some(identity) = identity.as_ref() {
                    let _ = terminate_owned_process(pid, identity);
                }
                panic!("专用测试子进程未在 {watchdog:?} 内结束：已按身份终止");
            }
        };
        let _ = watcher.join();
        let (stdout, stderr, status) = output;
        println!("[capacity-parent] 子进程退出状态 {status:?}");
        for line in stdout.lines().filter(|line| line.starts_with("[capacity-child]")) {
            println!("[capacity-parent 转发] {line}");
        }
        assert!(
            status.is_ok_and(|status| status.success()),
            "专用测试子进程必须成功退出：stdout={stdout}\nstderr={stderr}"
        );
        for anchor in [
            "[capacity-child] 起点",
            "[capacity-child] 8 个 reader 全部登记后",
            "[capacity-child] 新 helper 接纳被拒",
            "[capacity-child] 契约破坏后的登记结果",
            "[capacity-child] 故障状态",
            "[capacity-child] 故障锁下的接纳",
            "[capacity-child] 故障态账目",
            "[capacity-child] 过早核查被拒",
            "[capacity-child] 故障读取器在写端关闭后 被核实并回收",
            "[capacity-child] 显式核查通过后",
            "[capacity-child] 部分创建失败如实上报",
            "[capacity-child] 部分创建对象回收后",
            "[capacity-child] 结束一个持有者后",
            "[capacity-child] 并发申请（可用 2）：成功 2 / 被拒 2",
            "[capacity-child] 判据·部分可用（必须触发）",
            "[capacity-child] 判据·边界（可用 == 请求，必须接纳）",
            "[capacity-child] 清理完成",
        ] {
            assert!(
                stdout.contains(anchor),
                "子进程输出缺少证据锚点 {anchor:?}：\n{stdout}"
            );
        }
        // 隔离证据：子进程用过的标签**不会**出现在父进程的登记表里（登记是进程级的）。
        for label in retained_pipe_reader_labels() {
            assert!(
                !label.starts_with("capacity-child-"),
                "子进程的残留登记不得出现在父进程：{label}"
            );
        }
        println!("[capacity-parent] 隔离核对通过：父进程登记表里没有子进程的标签");
    }
}
