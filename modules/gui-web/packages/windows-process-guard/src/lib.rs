//! Small safe boundary around the Windows handle APIs used by Web Console.
//!
//! The rest of the workspace forbids unsafe code. Keeping the Win32 calls in
//! this crate prevents listener handles from leaking to child processes and
//! binds local-model children to the Web Console lifetime.

#![cfg(windows)]

use std::cell::RefCell;
use std::io;
use std::os::windows::io::{AsRawHandle, RawSocket};
use std::process::Child;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc, Mutex, OnceLock,
};
use std::thread;
use std::time::Duration;
use std::{collections::BTreeMap, fmt};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetHandleInformation, SetHandleInformation, FILETIME, HANDLE, HANDLE_FLAG_INHERIT,
    STILL_ACTIVE, WAIT_ABANDONED, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcessId, GetCurrentThreadId, GetExitCodeProcess, GetProcessTimes,
    OpenProcess, QueryFullProcessImageNameW, ReleaseMutex, TerminateProcess, WaitForSingleObject,
    PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

mod pipe;
mod local_control;
mod async_process;
mod conpty;
mod secret_protection;
pub use secret_protection::{protect_user_secret, unprotect_user_secret};
pub use conpty::{ConPtyOutput, ManagedConPty};
pub use local_control::{LocalProcessPeer, LocalRecoveryPipeServer, process_peer_identity,
    recovery_pipe_name, local_recovery_pipe_request};

pub use pipe::{
    reclaim_finished_pipe_readers, retained_pipe_reader_count, retained_pipe_reader_count_labeled,
    retained_pipe_reader_bytes, retained_pipe_reader_labels, capacity_fault_reader_labels,
    pipe_supervision_fault, review_and_clear_pipe_capacity_fault,
    pipe_reader_capacity, reserve_pipe_reader_capacity,
    CapacityFaultReason, HelperPipeCleanupReservation, HelperPipeReadersAdmission,
    PipeCapacityDenial, PipeReaderCapacity, PipeReaderCapacityReservation,
    PipeDrainOutcome, PipeReadStrategy,
    PipeReaderCompletion, PipeReaderSupervisor, PipeReaderUnconfirmed,
    PipeRetentionOutcome, PipeSnapshot, PipeSupervisorFault, SyncIoCancelOutcome,
    MAX_PIPE_BUFFER_BYTES, MAX_RETAINED_PIPE_READERS, PIPE_CAPACITY_EXHAUSTED,
    PIPE_CAPACITY_FAULT, PIPE_READERS_PER_HELPER,
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn ProcessIdToSessionId(process_id: u32, session_id: *mut u32) -> i32;
}

/// 当前 Windows 登录会话的输入仲裁范围。
///
/// 这不是任意桌面或人手输入的全局锁；它只约束接入本 broker 的正式产品输入通道。
pub fn current_interactive_session_scope() -> io::Result<String> {
    let mut session_id = 0;
    let ok = unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session_id) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(format!("windows-session-{session_id}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputLeaseBusy {
    pub scope: String,
    pub owner_id: String,
    pub owner_epoch: u64,
    /// 该 `Busy` 是否由**本线程正在执行的派发临界区**（重入）造成，而不是别的 owner 真正持有。
    ///
    /// `Some(dispatching_scope)` = 本次取用被拒是因为本线程已经在这一 broker 的派发临界区里
    /// （`dispatching_scope` 即在途派发的 scope）；此时上面的 `owner_id` / `owner_epoch` 描述的是
    /// **在途那个 section 的 owner**——只有当请求的 scope 与在途 scope 相同时，它才直接描述该
    /// scope 的持有者。`None` = 一次真实的 owner 冲突（字段语义与过去完全一致）。
    ///
    /// 精确的重入原因请用 `dispatch_if_current` 的 [`InputDispatchRefusal::Reentrant`]。
    pub reentrant_dispatching_scope: Option<String>,
}

impl fmt::Display for InputLeaseBusy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(dispatching_scope) = &self.reentrant_dispatching_scope {
            return write!(
                formatter,
                "input scope {} is unavailable: reentrant broker call on this thread while dispatching scope {}",
                self.scope, dispatching_scope
            );
        }
        write!(
            formatter,
            "input scope {} is owned by {} at epoch {}",
            self.scope, self.owner_id, self.owner_epoch
        )
    }
}

impl std::error::Error for InputLeaseBusy {}

#[derive(Debug, Clone)]
struct InputLeaseRecord {
    owner_id: String,
    owner_epoch: u64,
}

/// 同一 Windows 登录会话内，正式输入执行者的 fencing lease。
#[derive(Debug)]
pub struct InteractiveInputLeaseBroker {
    records: Mutex<BTreeMap<String, InputLeaseRecord>>,
    next_epoch: AtomicU64,
}

impl Default for InteractiveInputLeaseBroker {
    fn default() -> Self {
        Self {
            records: Mutex::new(BTreeMap::new()),
            next_epoch: AtomicU64::new(0),
        }
    }
}

impl InteractiveInputLeaseBroker {
    /// 取 `records` 锁，**容忍中毒**。
    ///
    /// `dispatch_if_current` 会在持锁期间执行调用方代码（守卫与 `dispatch`），调用方 panic 会让
    /// 互斥体中毒。`records` 的每一次改动都是单条 `insert` / `remove`，不存在"改到一半"的中间态，
    /// 因此中毒后继续使用是安全的；相比让整个 broker 永久 panic（scope 再也无法被取得），
    /// 继续可用更符合"输入所有权必须能重新取得"。
    fn lock_records(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, InputLeaseRecord>> {
        self.records
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn acquire(
        &self,
        scope: impl Into<String>,
        owner_id: impl Into<String>,
    ) -> Result<InteractiveInputLease<'_>, InputLeaseBusy> {
        let scope = scope.into();
        let owner_id = owner_id.into();
        // 重入：本线程已经在这一 broker 的派发临界区内，`records` 锁就在我们手里；直接取锁会自锁。
        // 必须在取锁**之前**返回（见本文件 "RPR-02a" 段）。
        if let Some((dispatching_scope, in_flight_owner, in_flight_epoch)) = dispatching_section(self) {
            return Err(InputLeaseBusy {
                scope,
                owner_id: in_flight_owner,
                owner_epoch: in_flight_epoch,
                reentrant_dispatching_scope: Some(dispatching_scope),
            });
        }
        let mut records = self.lock_records();
        if let Some(record) = records.get(&scope) {
            return Err(InputLeaseBusy {
                scope,
                owner_id: record.owner_id.clone(),
                owner_epoch: record.owner_epoch,
                reentrant_dispatching_scope: None,
            });
        }
        let owner_epoch = self.next_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        records.insert(
            scope.clone(),
            InputLeaseRecord {
                owner_id: owner_id.clone(),
                owner_epoch,
            },
        );
        Ok(InteractiveInputLease {
            broker: self,
            scope,
            owner_id,
            owner_epoch,
            released: false,
        })
    }

    fn release(&self, scope: &str, owner_id: &str, owner_epoch: u64) {
        // 重入（本线程正在派发这一 broker 的临界区）：锁在我们自己手里，取锁会自锁，也没有别的
        // 线程能替我们取。改为记入延迟队列，由该临界区在返回之前（仍持锁时）补做——于是
        // "派发中被请求的撤销"仍然表现为"派发返回后生效"，既不死锁也不吞掉撤销。
        if dispatching_section(self).is_some() {
            defer_removal(
                self,
                scope,
                DeferredRemovalKind::ExactOwner {
                    owner_id: owner_id.to_string(),
                    owner_epoch,
                },
            );
            return;
        }
        let mut records = self.lock_records();
        if records
            .get(scope)
            .is_some_and(|record| record.owner_id == owner_id && record.owner_epoch == owner_epoch)
        {
            records.remove(scope);
        }
    }

    fn is_current(&self, scope: &str, owner_id: &str, owner_epoch: u64) -> bool {
        // 重入：临界区内无法在不自锁的前提下回答"是否仍当前"。按 fail-closed 返回 false
        //（绝不放行输入）。副作用：守卫与 dispatch 回调里调用 `is_current()` 一律得到 false，
        // 这正是本文档建议的"不确定就不要发输入"。
        if dispatching_section(self).is_some() {
            return false;
        }
        self.lock_records()
            .get(scope)
            .is_some_and(|record| record.owner_id == owner_id && record.owner_epoch == owner_epoch)
    }

    /// 仅测试构建可见的"业主 token 失效"接缝。
    ///
    /// 生产代码里 lease 由执行器自身持有（`release` 消费 `self`），外部无法让某个 owner
    /// 在运行中失效，因此"输入前发现 lease 已失效"这条防御分支无法被回归测试覆盖。
    /// 该方法只补这条路，语义被刻意收窄：
    ///
    /// * 只作用于 `scope` 指定的那一个输入范围，**不清空** broker 的其它状态；
    /// * 只在当前记录 epoch 与 `expected_epoch` 相等时生效，否则返回 `false` 且不改动任何记录；
    /// * 失效后旧 token 不可能重新有效（epoch 全局单调递增，旧 epoch 不会被重新分配）；
    /// * 旧 lease 随后析构时按 `(scope, owner_id, owner_epoch)` 全字段匹配删除，因此
    ///   不会清除之后取得的新 owner。
    ///
    /// 该入口由 `#[cfg(any(test, feature = "test-support"))]` 门控，默认关闭；发布依赖图
    /// 不得启用 `test-support`（见 web-console 只在 `[dev-dependencies]` 打开它）。
    ///
    /// RPR-02a 补充：若在**本线程派发临界区内**调用（重入），直接取锁会自锁，因此该请求被记入
    /// 延迟队列并返回 `true`（= 已接受），由派发临界区在返回之前（仍持锁时）生效。
    #[cfg(any(test, feature = "test-support"))]
    pub fn revoke_owner_for_test(&self, scope: &str, expected_epoch: u64) -> bool {
        if dispatching_section(self).is_some() {
            defer_removal(self, scope, DeferredRemovalKind::Epoch { expected_epoch });
            return true;
        }
        let mut records = self.lock_records();
        match records.get(scope) {
            Some(record) if record.owner_epoch == expected_epoch => {
                records.remove(scope);
                true
            }
            _ => false,
        }
    }
}

/// 持有期间的不可复用输入 owner epoch；释放后旧 token 不再有效。
#[derive(Debug)]
pub struct InteractiveInputLease<'a> {
    broker: &'a InteractiveInputLeaseBroker,
    scope: String,
    owner_id: String,
    owner_epoch: u64,
    released: bool,
}

impl InteractiveInputLease<'_> {
    pub fn owner_epoch(&self) -> u64 {
        self.owner_epoch
    }

    pub fn is_current(&self) -> bool {
        !self.released
            && self
                .broker
                .is_current(&self.scope, &self.owner_id, self.owner_epoch)
    }

    pub fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if !self.released {
            self.broker
                .release(&self.scope, &self.owner_id, self.owner_epoch);
            self.released = true;
        }
    }
}

impl Drop for InteractiveInputLease<'_> {
    fn drop(&mut self) {
        self.release_inner();
    }
}

/// Web Console 正式桌面输入通道的进程内 broker。
pub fn interactive_input_lease_broker() -> &'static InteractiveInputLeaseBroker {
    static BROKER: OnceLock<InteractiveInputLeaseBroker> = OnceLock::new();
    BROKER.get_or_init(InteractiveInputLeaseBroker::default)
}

// ---------------------------------------------------------------------------
// RPR-02a：输入前复核与派发的**同一序列化决策边界**
//
// 要关闭的交错（旧写法把两者分开：`if lease.is_current() { ...任意等待... 落输入 }`）：
//
//     is_current() == true →（任意长时间等待：落库 / 日志 / 调度）→ 已被撤销 → **仍调用移动**
//
// 做法：把"复核（owner/epoch + 调用方守卫）"与"派发"放进**同一个临界区**——复核通过之后到
// `dispatch()` 被调用之间不存在任何锁释放点；而撤销路径（`revoke_owner_for_test` / `release` /
// `Drop`）与复核共用同一把 `records` 互斥锁。于是只剩两种可能，且都可测：
//
//   * 撤销在复核**之前**被接受 → 复核必然失败、`dispatch()` 调用次数为**零**
//     （调用方据此不得回退到无 lease 的底层输入原语）；
//   * 派发已被正式接纳并开始 → 撤销被阻塞到本次派发返回之后才生效，**不**把已经发生的输入
//     改记为 `not_sent`（按真实回执或未知效果收尾）。
//
// 重入：`Mutex` 不可重入，`dispatch()` 或守卫回调若再进 broker 就会自锁。用**线程局部**标记
// （本线程正在为哪个 broker 执行派发临界区）在**取锁之前**检测并处理：
//
//   * `dispatch_if_current` → [`InputDispatchRefusal::Reentrant`]（明确错误，不死锁）；
//   * `acquire` → [`InputLeaseBusy`]，其 `reentrant_dispatching_scope` 为 `Some(..)`；
//   * `is_current` → `false`（fail-closed：临界区内无法回答"是否仍当前"，一律按失效处理）；
//   * `release` / `Drop` / `revoke_owner_for_test` → 记入**延迟移除队列**，由该临界区在返回
//     之前（仍持锁时）补做，因此"派发中被请求的撤销"依旧表现为"派发返回后生效"：
//     不死锁，也不吞掉撤销。
//
// 重入检测是**按 broker** 的：只有"同一把锁"才会自锁。同一线程嵌套另一个 broker 的边界
// 不会自锁（另有一把锁），但本模块不校验跨 broker 的加锁顺序，调用方不应在同一线程里嵌套
// 两个 broker 的派发。
// ---------------------------------------------------------------------------

/// 本线程正在执行的派发临界区条目（用于重入检测与"在途 owner"的汇报）。
struct DispatchSection {
    broker_id: usize,
    scope: String,
    owner_id: String,
    owner_epoch: u64,
}

/// 延迟移除的种类，与 `release` / `revoke_owner_for_test` 的匹配语义一一对应。
enum DeferredRemovalKind {
    /// `release` / `Drop`：按 `(scope, owner_id, owner_epoch)` 全字段匹配删除。
    ExactOwner { owner_id: String, owner_epoch: u64 },
    /// `revoke_owner_for_test`：按 `(scope, owner_epoch)` 匹配删除。
    /// 只由测试接缝构造，因此与它同门控：发布构建里不存在这条分支。
    #[cfg(any(test, feature = "test-support"))]
    Epoch { expected_epoch: u64 },
}

/// 因重入而无法立即执行的移除请求。只可能由**正在派发的那个线程**产生，并由该线程自己的
/// 派发临界区补做，因此不需要任何跨线程可见的存储（不引入第二把锁）。
struct DeferredRemoval {
    broker_id: usize,
    scope: String,
    kind: DeferredRemovalKind,
}

#[derive(Default)]
struct DispatchThreadState {
    /// 本线程的派发临界区栈（严格 LIFO）。
    sections: Vec<DispatchSection>,
    /// 本线程因重入被延迟的移除请求。
    deferred: Vec<DeferredRemoval>,
}

thread_local! {
    static DISPATCH_THREAD_STATE: RefCell<DispatchThreadState> =
        RefCell::new(DispatchThreadState::default());
}

/// broker 的线程内身份。broker 以 `&self` 使用，地址在生命周期内稳定，足以区分不同实例。
fn broker_identity(broker: &InteractiveInputLeaseBroker) -> usize {
    std::ptr::from_ref(broker) as usize
}

/// 本线程是否正为 `broker` 执行派发临界区；是则返回在途的 `(scope, owner_id, owner_epoch)`。
fn dispatching_section(broker: &InteractiveInputLeaseBroker) -> Option<(String, String, u64)> {
    let broker_id = broker_identity(broker);
    DISPATCH_THREAD_STATE.with(|cell| {
        cell.borrow()
            .sections
            .iter()
            .find(|section| section.broker_id == broker_id)
            .map(|section| {
                (
                    section.scope.clone(),
                    section.owner_id.clone(),
                    section.owner_epoch,
                )
            })
    })
}

fn defer_removal(broker: &InteractiveInputLeaseBroker, scope: &str, kind: DeferredRemovalKind) {
    let entry = DeferredRemoval {
        broker_id: broker_identity(broker),
        scope: scope.to_string(),
        kind,
    };
    DISPATCH_THREAD_STATE.with(|cell| cell.borrow_mut().deferred.push(entry));
}

fn apply_deferred_removal(records: &mut BTreeMap<String, InputLeaseRecord>, entry: DeferredRemoval) {
    match entry.kind {
        DeferredRemovalKind::ExactOwner {
            owner_id,
            owner_epoch,
        } => {
            if records.get(&entry.scope).is_some_and(|record| {
                record.owner_id == owner_id && record.owner_epoch == owner_epoch
            }) {
                records.remove(&entry.scope);
            }
        }
        #[cfg(any(test, feature = "test-support"))]
        DeferredRemovalKind::Epoch { expected_epoch } => {
            if records
                .get(&entry.scope)
                .is_some_and(|record| record.owner_epoch == expected_epoch)
            {
                records.remove(&entry.scope);
            }
        }
    }
}

/// 派发临界区的 RAII 标记：进入时压栈（重入检测的事实来源），退出时出栈。
///
/// 正常路径由 `dispatch_if_current` 在**仍持有** `records` 锁时显式调用 `drain_deferred`；
/// 若 `dispatch()` 或守卫 panic 导致提前退栈，`Drop` 会补做一次（重新取锁），以免留下
/// "谁都不再持有、却仍然占用 scope"的记录。
struct DispatchSectionGuard<'a> {
    broker: &'a InteractiveInputLeaseBroker,
    drained: bool,
}

impl<'a> DispatchSectionGuard<'a> {
    fn enter(
        broker: &'a InteractiveInputLeaseBroker,
        scope: &str,
        owner_id: &str,
        owner_epoch: u64,
    ) -> Self {
        let section = DispatchSection {
            broker_id: broker_identity(broker),
            scope: scope.to_string(),
            owner_id: owner_id.to_string(),
            owner_epoch,
        };
        DISPATCH_THREAD_STATE.with(|cell| cell.borrow_mut().sections.push(section));
        Self {
            broker,
            drained: false,
        }
    }

    /// 在**仍持有** `records` 临界区锁时补做本线程的延迟移除。
    fn drain_deferred(&mut self, records: &mut BTreeMap<String, InputLeaseRecord>) {
        self.drained = true;
        let broker_id = broker_identity(self.broker);
        let pending: Vec<DeferredRemoval> = DISPATCH_THREAD_STATE.with(|cell| {
            let mut state = cell.borrow_mut();
            let mut mine = Vec::new();
            let mut other = Vec::new();
            for entry in state.deferred.drain(..) {
                if entry.broker_id == broker_id {
                    mine.push(entry);
                } else {
                    other.push(entry);
                }
            }
            state.deferred = other;
            mine
        });
        for entry in pending {
            apply_deferred_removal(records, entry);
        }
    }
}

impl Drop for DispatchSectionGuard<'_> {
    fn drop(&mut self) {
        let popped = DISPATCH_THREAD_STATE.with(|cell| cell.borrow_mut().sections.pop());
        debug_assert_eq!(
            popped.map(|section| section.broker_id),
            Some(broker_identity(self.broker)),
            "派发临界区栈必须严格 LIFO"
        );
        if !self.drained {
            let mut records = self.broker.lock_records();
            self.drain_deferred(&mut records);
        }
    }
}

/// 调用方提供的**临界区内**守卫（取消、Quarantined / 恢复中、剩余执行预算……）。
///
/// 本 crate 不知道这些状态的真实来源（例如 Quarantined 在别的模块里），因此**不硬编码任何一条**，
/// 只提供同一临界区内的求值点：守卫在"owner/epoch 复核通过之后、`dispatch()` 调用之前"被逐一
/// 求值，因此守卫之外发生的撤销无法插进"守卫通过"与"派发"之间。
///
/// `check` 返回 `true` = 通过（允许派发）；返回 `false` = 拒绝，`dispatch()` 一次都不会被调用。
/// 求值发生在 broker 的临界区内，所以 `check` **不得**回调本 broker（会被判为重入）。
pub struct InputDispatchGuard<'a> {
    label: &'a str,
    check: &'a dyn Fn() -> bool,
}

impl<'a> InputDispatchGuard<'a> {
    /// `label` 会原样出现在 [`InputDispatchRefusal::GuardRefused`] 里，用于区分是哪一条守卫拒绝。
    pub fn new(label: &'a str, check: &'a dyn Fn() -> bool) -> Self {
        Self { label, check }
    }

    pub fn label(&self) -> &'a str {
        self.label
    }

    /// 求值：`false` 表示拒绝派发（调用方不得把它当作"输入已发送"）。
    pub fn allows(&self) -> bool {
        (self.check)()
    }
}

impl fmt::Debug for InputDispatchGuard<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InputDispatchGuard")
            .field("label", &self.label)
            .finish_non_exhaustive()
    }
}

/// 拒绝"复核 + 派发"的原因；四类失效原因可区分（非当前 owner / epoch 过期 / scope 已被撤销 /
/// 重入），外加调用方守卫的拒绝。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputDispatchRefusal {
    /// 本线程已经在为同一 broker 执行派发临界区（重入）。**不是**"输入已发送"。
    Reentrant {
        /// 本次被拒绝的 scope。
        scope: String,
        /// 本线程当前正在派发的 scope。
        dispatching_scope: String,
    },
    /// 该 scope 已不再由任何 owner 持有：被撤销、被释放，或从未取得（旧 token 不得复活）。
    ScopeRevoked {
        scope: String,
        expected_epoch: u64,
    },
    /// 记录存在，但属于另一个 owner（本 token 已不是当前 owner）。
    NotCurrentOwner {
        scope: String,
        owner_id: String,
        expected_epoch: u64,
        current_owner_id: String,
        current_owner_epoch: u64,
    },
    /// 记录存在且 owner 相同，但 epoch 已经过期（fencing：旧 epoch 不再有效）。
    StaleEpoch {
        scope: String,
        owner_id: String,
        expected_epoch: u64,
        current_owner_epoch: u64,
    },
    /// 调用方提供的守卫未通过（取消 / Quarantined / 恢复中 / 执行预算耗尽 —— 由调用方判定）。
    GuardRefused { scope: String, label: String },
}

impl InputDispatchRefusal {
    pub fn scope(&self) -> &str {
        match self {
            Self::Reentrant { scope, .. }
            | Self::ScopeRevoked { scope, .. }
            | Self::NotCurrentOwner { scope, .. }
            | Self::StaleEpoch { scope, .. }
            | Self::GuardRefused { scope, .. } => scope,
        }
    }

    /// 是否因为重入（而不是所有权失效）被拒。
    pub fn is_reentrant(&self) -> bool {
        matches!(self, Self::Reentrant { .. })
    }

    /// 是否属于"这个输入 owner 已经失效"：非当前 owner / epoch 过期 / scope 已被撤销。
    pub fn is_lease_lost(&self) -> bool {
        matches!(
            self,
            Self::ScopeRevoked { .. } | Self::NotCurrentOwner { .. } | Self::StaleEpoch { .. }
        )
    }

    /// 被拒绝的调用方守卫标签（仅 `GuardRefused`）。
    pub fn guard_label(&self) -> Option<&str> {
        match self {
            Self::GuardRefused { label, .. } => Some(label),
            _ => None,
        }
    }
}

impl fmt::Display for InputDispatchRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reentrant {
                scope,
                dispatching_scope,
            } => write!(
                formatter,
                "input dispatch for scope {scope} refused: reentrant broker call on this thread while dispatching scope {dispatching_scope}"
            ),
            Self::ScopeRevoked {
                scope,
                expected_epoch,
            } => write!(
                formatter,
                "input dispatch for scope {scope} refused: no owner holds it any more (expected epoch {expected_epoch})"
            ),
            Self::NotCurrentOwner {
                scope,
                owner_id,
                expected_epoch,
                current_owner_id,
                current_owner_epoch,
            } => write!(
                formatter,
                "input dispatch for scope {scope} refused: {owner_id} at epoch {expected_epoch} is not the current owner ({current_owner_id} at epoch {current_owner_epoch} holds it)"
            ),
            Self::StaleEpoch {
                scope,
                owner_id,
                expected_epoch,
                current_owner_epoch,
            } => write!(
                formatter,
                "input dispatch for scope {scope} refused: {owner_id} epoch {expected_epoch} is stale (current epoch {current_owner_epoch})"
            ),
            Self::GuardRefused { scope, label } => write!(
                formatter,
                "input dispatch for scope {scope} refused: caller guard {label} did not pass"
            ),
        }
    }
}

impl std::error::Error for InputDispatchRefusal {}

impl InteractiveInputLeaseBroker {
    /// **复核 + 派发**的同一序列化决策边界（RPR-02a）。
    ///
    /// 在**同一临界区**内完成"当前 owner/epoch 校验 + 调用方守卫求值 + `dispatch()` 调用"：
    /// 复核通过之后到 `dispatch()` 被调用之间没有任何锁释放点，因此撤销无法插进两者之间。
    /// 被拒绝时 `dispatch()` **一次都不会被调用**——调用方因此不得回退到无 lease 的底层输入原语。
    ///
    /// * `scope` / `owner_id` / `expected_epoch`：调用方持有的输入所有权凭据（例如
    ///   `lease.owner_epoch()`）。
    /// * `guards`：调用方提供的**临界区内**守卫，见 [`InputDispatchGuard`]。传空切片即"只做
    ///   owner/epoch 复核"。
    /// * `dispatch`：真正落输入的动作。它（以及守卫）**不得**回调本 broker；否则以
    ///   `InputDispatchRefusal::Reentrant` 明确拒绝，而不是自锁。
    pub fn dispatch_if_current<R>(
        &self,
        scope: &str,
        owner_id: &str,
        expected_epoch: u64,
        guards: &[InputDispatchGuard<'_>],
        dispatch: impl FnOnce() -> R,
    ) -> Result<R, InputDispatchRefusal> {
        // 重入必须在**取锁之前**检测：`records` 锁此刻就在本线程手里，取锁会自锁。
        if let Some((dispatching_scope, _, _)) = dispatching_section(self) {
            return Err(InputDispatchRefusal::Reentrant {
                scope: scope.to_string(),
                dispatching_scope,
            });
        }
        let mut section = DispatchSectionGuard::enter(self, scope, owner_id, expected_epoch);
        let mut records = self.lock_records();
        // 复核一：所有权围栏（先围栏、后守卫：已失效的 token 一律报失效，不被守卫原因掩盖）。
        let refusal = match records.get(scope) {
            None => Some(InputDispatchRefusal::ScopeRevoked {
                scope: scope.to_string(),
                expected_epoch,
            }),
            Some(record) if record.owner_id != owner_id => {
                Some(InputDispatchRefusal::NotCurrentOwner {
                    scope: scope.to_string(),
                    owner_id: owner_id.to_string(),
                    expected_epoch,
                    current_owner_id: record.owner_id.clone(),
                    current_owner_epoch: record.owner_epoch,
                })
            }
            Some(record) if record.owner_epoch != expected_epoch => {
                Some(InputDispatchRefusal::StaleEpoch {
                    scope: scope.to_string(),
                    owner_id: owner_id.to_string(),
                    expected_epoch,
                    current_owner_epoch: record.owner_epoch,
                })
            }
            Some(_) => None,
        }
        // 复核二：调用方守卫（同一临界区，仍在 `records` 锁内）。
        .or_else(|| {
            guards
                .iter()
                .find(|guard| !guard.allows())
                .map(|guard| InputDispatchRefusal::GuardRefused {
                    scope: scope.to_string(),
                    label: guard.label().to_string(),
                })
        });
        // 复核通过后立刻派发：这一句与上面的校验之间不存在任何锁释放点。
        let outcome = match refusal {
            Some(refusal) => Err(refusal),
            None => Ok(dispatch()),
        };
        // 临界区内被请求、因重入而延迟的撤销在这里生效：仍在同一临界区内，先于解锁。
        section.drain_deferred(&mut records);
        drop(records);
        outcome
    }
}

impl InteractiveInputLease<'_> {
    /// 用本 lease 的 `(scope, owner_id, owner_epoch)` 走
    /// [`InteractiveInputLeaseBroker::dispatch_if_current`]（集成推荐入口：epoch 直接取自 lease，
    /// 不会与记录错配）。
    ///
    /// 已 `release`（或已失效）的 lease 返回 `ScopeRevoked`，不会派发输入。
    pub fn dispatch_if_current<R>(
        &self,
        guards: &[InputDispatchGuard<'_>],
        dispatch: impl FnOnce() -> R,
    ) -> Result<R, InputDispatchRefusal> {
        if self.released {
            return Err(InputDispatchRefusal::ScopeRevoked {
                scope: self.scope.clone(),
                expected_epoch: self.owner_epoch,
            });
        }
        self.broker.dispatch_if_current(
            &self.scope,
            &self.owner_id,
            self.owner_epoch,
            guards,
            dispatch,
        )
    }
}

fn socket_as_handle(socket: RawSocket) -> HANDLE {
    socket as usize as HANDLE
}

/// Clear `HANDLE_FLAG_INHERIT` on a socket.
pub fn make_socket_non_inheritable(socket: RawSocket) -> io::Result<()> {
    let result = unsafe { SetHandleInformation(socket_as_handle(socket), HANDLE_FLAG_INHERIT, 0) };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

/// Inspect whether a socket currently has `HANDLE_FLAG_INHERIT`.
pub fn socket_is_inheritable(socket: RawSocket) -> io::Result<bool> {
    let mut flags = 0;
    let result = unsafe { GetHandleInformation(socket_as_handle(socket), &mut flags) };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(flags & HANDLE_FLAG_INHERIT != 0)
    }
}

/// Job Object configured to terminate all assigned children when its handle closes.
#[derive(Debug)]
pub struct ChildProcessJob {
    handle: usize,
}

impl ChildProcessJob {
    /// 先以挂起状态创建，再纳入 Job，最后恢复初始线程；业务代码没有逃出 Job 的窗口。
    /// 本入口专供无窗口的受管命令，不能用于用户主动请求保持运行的后台程序。
    pub fn spawn_managed(command: &mut std::process::Command) -> io::Result<(Child, Self)> {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
        let job = Self::new_kill_on_close()?;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
        let mut child = command.spawn()?;
        if let Err(error) = job.assign(&child).and_then(|()| resume_initial_thread(child.id())) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        Ok((child, job))
    }

    /// Create a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
    pub fn new_kill_on_close() -> io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let result = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                std::ptr::addr_of!(info).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if result == 0 {
            let error = io::Error::last_os_error();
            unsafe {
                CloseHandle(handle);
            }
            return Err(error);
        }

        Ok(Self {
            handle: handle as usize,
        })
    }

    /// Assign a spawned child process to this job.
    pub fn assign(&self, child: &Child) -> io::Result<()> {
        self.assign_handle(child.as_raw_handle() as HANDLE)
    }

    /// ConPTY 直接使用 Win32 CreateProcessW 时仍须沿用同一先挂 Job 边界。
    pub(crate) fn assign_handle(&self, process_handle: HANDLE) -> io::Result<()> {
        let result = unsafe { AssignProcessToJobObject(self.handle as HANDLE, process_handle) };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

fn resume_initial_thread(process_id: u32) -> io::Result<()> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Thread32First, Thread32Next, THREADENTRY32, TH32CS_SNAPTHREAD,
    };
    use windows_sys::Win32::System::Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME};
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE { return Err(io::Error::last_os_error()); }
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    let mut found = unsafe { Thread32First(snapshot, &mut entry) };
    let mut result = Err(io::Error::new(io::ErrorKind::NotFound, "受管进程初始线程不存在"));
    while found != 0 {
        if entry.th32OwnerProcessID == process_id {
            // 初始线程尚未运行，因此此进程只有这一条业务线程；句柄只用于解除初始挂起。
            let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
            if thread.is_null() {
                result = Err(io::Error::last_os_error());
            } else {
                let count = unsafe { ResumeThread(thread) };
                result = if count == u32::MAX { Err(io::Error::last_os_error()) } else { Ok(()) };
                unsafe { CloseHandle(thread); }
            }
            break;
        }
        found = unsafe { Thread32Next(snapshot, &mut entry) };
    }
    unsafe { CloseHandle(snapshot); }
    result
}

impl Drop for ChildProcessJob {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle as HANDLE);
        }
    }
}

// ---------------------------------------------------------------------------
// 任务一：跨进程输入 scope 排他（命名内核互斥体）
//
// 与上面的进程内 broker 的分工：
//   * broker（`records` + `next_epoch`）负责**进程内**的"同时只有一个 owner"以及
//     **epoch fencing**（旧 token 失效）；
//   * 命名内核互斥体负责**跨进程**的"同时只有一个 owner"，并由内核保证
//     "持有者崩溃 → 所有权被放弃（abandoned）"，不会出现需要人工清理的死锁。
// 两者都不覆盖对方的职责，组合方式见 `ScopedInputOwnership`。
// ---------------------------------------------------------------------------

/// 命名内核互斥体所属的命名空间前缀。
///
/// 选 `Local\`（每个登录会话一个命名空间）而不是 `Global\`（全机命名空间）：
///
/// * 输入仲裁只在**同一个 Windows 登录会话**内有意义——`current_interactive_session_scope()`
///   本身就把 session id 编进了 scope；用 `Global\` 会让会话 A 的持有者挡住会话 B 的合法输入；
/// * `Global\` 是全机命名空间，跨会话/跨用户可见，命名与权限的耦合面更大；
/// * `Local\` 的隔离由会话管理器保证，正好落在"一个登录会话一个输入所有者"这条语义上。
const INPUT_SCOPE_NAMESPACE: &str = "Local";

/// scope 派生的内核对象名前缀（与 `INPUT_SCOPE_NAMESPACE` 一起构成完整名字）。
const INPUT_SCOPE_NAME_PREFIX: &str = "coolzhu-input-scope-";

/// scope 十六进制编码的长度上限；超过则退化为"截断 + 64 位摘要"，保持名字有界。
const INPUT_SCOPE_HEX_LIMIT: usize = 160;

/// keeper 线程的名字前缀（诊断用）。
const INPUT_SCOPE_KEEPER_THREAD_PREFIX: &str = "coolzhu-input-scope-keeper";

/// 取得内核所有权的等待上限：明确**不使用** `INFINITE`，
/// 以免"别人正持有"这条正常路径把调用方无限挂住。
const INPUT_SCOPE_MAX_WAIT_MILLIS: u64 = 300_000;

/// 等待 keeper 线程回报结果时，在其自身等待超时之外额外给的余量。
const INPUT_SCOPE_HANDSHAKE_SLACK: Duration = Duration::from_secs(5);

/// 由 scope 派生出内核对象名：`Local\coolzhu-input-scope-<scope 的十六进制编码>`。
///
/// 十六进制编码保证 scope → 名字是单射（反斜杠、大小写、空白都不会造成歧义或越出命名空间），
/// 因此同一 scope 在不同进程里必然得到同一个名字——这是跨进程互斥的前提。
/// 超长 scope 退化为"前 160 个十六进制字符 + FNV-1a 64 位摘要"，仍是单射的实用近似。
pub fn input_scope_kernel_object_name(scope: &str) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(scope.len() * 2);
    for byte in scope.as_bytes() {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    if encoded.len() > INPUT_SCOPE_HEX_LIMIT {
        let digest = fnv1a64(scope.as_bytes());
        encoded.truncate(INPUT_SCOPE_HEX_LIMIT);
        return format!("{INPUT_SCOPE_NAMESPACE}\\{INPUT_SCOPE_NAME_PREFIX}{encoded}-{digest:016x}");
    }
    format!("{INPUT_SCOPE_NAMESPACE}\\{INPUT_SCOPE_NAME_PREFIX}{encoded}")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn wide_null_terminated(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn timeout_to_millis(timeout: Duration) -> u32 {
    let millis = timeout.as_millis().min(u128::from(INPUT_SCOPE_MAX_WAIT_MILLIS));
    millis as u32
}

/// 把 `io::Error` 的原始错误码（Windows 上是 `GetLastError()` 的值）与 Win32 码比较。
fn is_win32_code(error: &io::Error, code: u32) -> bool {
    error.raw_os_error() == Some(code as i32)
}

/// 取得跨进程输入 scope 失败的原因。
#[derive(Debug)]
pub enum CrossProcessScopeError {
    /// 该 scope 已被其它所有者持有（可能是本进程的另一线程，也可能是另一个进程），
    /// 且在给定超时内没有让出。对应 `WaitForSingleObject` 的 `WAIT_TIMEOUT`。
    Busy { scope: String, waited: Duration },
    /// 等待 keeper 线程回报结果超时（异常路径，正常情况下不会发生）。
    HandshakeTimeout { scope: String, waited: Duration },
    /// 内核对象创建/线程创建/等待调用失败。
    Kernel { scope: String, source: io::Error },
}

impl CrossProcessScopeError {
    /// 是否属于"别人正持有"（而不是内核调用失败）。
    pub fn is_busy(&self) -> bool {
        matches!(self, Self::Busy { .. })
    }

    pub fn scope(&self) -> &str {
        match self {
            Self::Busy { scope, .. }
            | Self::HandshakeTimeout { scope, .. }
            | Self::Kernel { scope, .. } => scope,
        }
    }
}

impl fmt::Display for CrossProcessScopeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy { scope, waited } => write!(
                formatter,
                "input scope {scope} 已被其它所有者持有（等待 {:?} 未让出）",
                waited
            ),
            Self::HandshakeTimeout { scope, waited } => write!(
                formatter,
                "input scope {scope} 等待内核取得结果握手超时（等待 {:?}）",
                waited
            ),
            Self::Kernel { scope, source } => {
                write!(formatter, "input scope {scope} 内核对象操作失败: {source}")
            }
        }
    }
}

impl std::error::Error for CrossProcessScopeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Kernel { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// 内核对象句柄的所有权包装。
///
/// `HANDLE` 是**进程级**资源，可以在本进程任意线程上使用；因此该包装是 `Send + Sync`。
/// 关闭句柄只发生在 `Drop`：本 crate 之外既不复制也不关闭句柄。
#[derive(Debug)]
pub(crate) struct KernelHandle(HANDLE);

// SAFETY: 句柄的所有权由本包装独占，且 Win32 句柄是进程级资源（可跨线程使用）。
// 把它标记为 Send/Sync 不会引入数据竞争：所有使用点都在本 crate 内，要么是 `&self`
// 只读传递，要么在 `Drop` 里关闭；关闭之后不再有任何使用点。
unsafe impl Send for KernelHandle {}
unsafe impl Sync for KernelHandle {}

impl KernelHandle {
    /// 接管一个已拥有的句柄（唯一所有权转移点）。
    pub(crate) fn from_raw(handle: HANDLE) -> Self {
        Self(handle)
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }

    fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

impl Drop for KernelHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// scope 对应的命名内核互斥体句柄——**不代表持有所有权**。
///
/// 它的作用是固定内核对象的**生存期与身份**：只要还有进程持有该对象的句柄，对象就不会被销毁，
/// 于是"前一个持有者崩溃"会以 `WAIT_ABANDONED` 的形式被后来的取得者观察到。
/// 想区分"前任崩溃"和"这是一个全新对象"，就必须在等待之前先 `open` 并保活。
#[derive(Debug)]
pub struct InputScopeKernelObject {
    handle: KernelHandle,
    scope: String,
    kernel_name: String,
}

impl InputScopeKernelObject {
    /// 打开（必要时创建）scope 对应的命名互斥体，但不尝试取得所有权。
    ///
    /// 对应 `CreateMutexW(.., bInitialOwner = FALSE, ..)`：创建者并不自动成为所有者。
    pub fn open(scope: &str) -> io::Result<Self> {
        let kernel_name = input_scope_kernel_object_name(scope);
        let name = wide_null_terminated(&kernel_name);
        // SAFETY: name 是有效的 NUL 结尾 UTF-16 串；安全属性取默认、初始所有权为 FALSE。
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            handle: KernelHandle(handle),
            scope: scope.to_string(),
            kernel_name,
        })
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn kernel_object_name(&self) -> &str {
        &self.kernel_name
    }

    /// 在 `timeout` 内尝试成为该 scope 的跨进程所有者。
    ///
    /// * `Duration::ZERO` 即"非阻塞探测"（`WaitForSingleObject(handle, 0)`）；
    /// * 内部等待上限被夹在 `INPUT_SCOPE_MAX_WAIT_MILLIS`，绝不使用 `INFINITE`；
    /// * 失败时返回的 `self` 已析构：命名对象句柄被关闭（若本进程是最后一个持有者，
    ///   对象随之销毁——这与"scope 空闲"并不冲突）。
    pub fn try_acquire(
        self,
        timeout: Duration,
    ) -> Result<CrossProcessInputScopeGuard, CrossProcessScopeError> {
        let Self {
            handle,
            scope,
            kernel_name,
        } = self;
        let timeout_ms = timeout_to_millis(timeout);
        let (report_tx, report_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let raw_handle = handle.as_usize();
        let thread_name = format!("{INPUT_SCOPE_KEEPER_THREAD_PREFIX}-{}", fnv1a64(scope.as_bytes()));
        let keeper = match thread::Builder::new()
            .name(thread_name)
            .spawn(move || run_input_scope_keeper(raw_handle, timeout_ms, report_tx, release_rx))
        {
            Ok(keeper) => keeper,
            Err(source) => return Err(CrossProcessScopeError::Kernel { scope, source }),
        };

        match report_rx.recv_timeout(timeout + INPUT_SCOPE_HANDSHAKE_SLACK) {
            Ok(KeeperReport::Owned {
                abandoned,
                owner_thread_id,
            }) => Ok(CrossProcessInputScopeGuard {
                object: Self {
                    handle,
                    scope,
                    kernel_name,
                },
                recovered_from_abandoned: abandoned,
                owner_pid: unsafe { GetCurrentProcessId() },
                owner_thread_id,
                release: Mutex::new(Some(release_tx)),
                keeper: Some(keeper),
                released: false,
            }),
            Ok(KeeperReport::Busy) => Err(CrossProcessScopeError::Busy { scope, waited: timeout }),
            Ok(KeeperReport::Failed(source)) => {
                Err(CrossProcessScopeError::Kernel { scope, source })
            }
            // keeper 没能按时回报：请它释放（它一旦取得就必须释放）并等它结束，
            // 避免留下一个"谁都看不见的持有者"。
            Err(_) => {
                let _ = release_tx.send(());
                let _ = keeper.join();
                Err(CrossProcessScopeError::HandshakeTimeout {
                    scope,
                    waited: timeout + INPUT_SCOPE_HANDSHAKE_SLACK,
                })
            }
        }
    }
}

/// keeper 线程的回报内容。
enum KeeperReport {
    Owned {
        /// 是否观察到 `WAIT_ABANDONED`：前一个所有者**崩溃**（或被强杀）而未释放。
        abandoned: bool,
        owner_thread_id: u32,
    },
    Busy,
    Failed(io::Error),
}

/// 内核所有权的 keeper 线程。
///
/// 为什么要单独一个线程：内核互斥体的所有权是**线程所属**的，只有所有者线程（或它自己）
/// 才能 `ReleaseMutex`。如果把所有权挂在调用者线程上，调用者一旦在别的线程释放就会
/// `ERROR_NOT_OWNER` 失败，留下"进程内以为释放了、内核对象仍被持有"的缺口。
/// 把所有权固定在这条 keeper 线程上，`Drop`/`release` 就可以从任意线程发起。
///
/// 崩溃可恢复性来自同一条线索：keeper 线程一旦终止（无论正常退出、panic 还是进程崩溃），
/// 内核就把互斥体标记为 abandoned，而不是永久锁死。
fn run_input_scope_keeper(
    handle: usize,
    timeout_ms: u32,
    report_tx: mpsc::SyncSender<KeeperReport>,
    release_rx: mpsc::Receiver<()>,
) {
    let handle = handle as HANDLE;
    // SAFETY: handle 由调用方（`InputScopeKernelObject`）保活到本线程结束；
    // timeout_ms 已被夹到有界值，不会是 INFINITE。
    let wait = unsafe { WaitForSingleObject(handle, timeout_ms) };
    let report = match wait {
        WAIT_OBJECT_0 => KeeperReport::Owned {
            abandoned: false,
            owner_thread_id: unsafe { GetCurrentThreadId() },
        },
        WAIT_ABANDONED => KeeperReport::Owned {
            abandoned: true,
            owner_thread_id: unsafe { GetCurrentThreadId() },
        },
        WAIT_TIMEOUT => KeeperReport::Busy,
        WAIT_FAILED => KeeperReport::Failed(io::Error::last_os_error()),
        other => KeeperReport::Failed(io::Error::other(format!(
            "WaitForSingleObject 返回未预期状态码 {other}"
        ))),
    };
    let owned = matches!(report, KeeperReport::Owned { .. });
    // 回报失败（调用方已离开）不影响下面的释放逻辑。
    let _ = report_tx.send(report);
    if owned {
        // 收到释放信号 → 正常释放；通道断开（调用方已析构或句柄泄漏） → 同样必须释放。
        let _ = release_rx.recv();
        // SAFETY: 本线程是唯一的取得者，因此是唯一的合法所有者；
        // 句柄仍由调用方的 `InputScopeKernelObject` 保活（Drop 顺序已保证先释放再关句柄）。
        unsafe {
            ReleaseMutex(handle);
        }
    }
}

/// 跨进程输入 scope 的所有权凭据（命名内核互斥体的持有者）。
///
/// 语义要点：
///
/// * 所有权由内部 keeper 线程持有，因此 `Drop` / `release(self)` 可以从**任意线程**执行，
///   不会出现 `ReleaseMutex` 因跨线程而 `ERROR_NOT_OWNER` 失败、导致"进程内以为已释放、
///   内核对象仍被持有"的窗口；`self` 是 `Send + Sync` 的，可以放在 async 任务里跨越 `.await`。
/// * `Drop` 与显式 `release(self)` **完全等价**：请 keeper 释放内核所有权 → 等 keeper 退出 →
///   关闭命名对象句柄。两者都不是"只改本地标记"。
/// * scope 不会被永久锁死：即使 `Drop` 被跳过（`mem::forget`）、keeper 线程 panic、
///   或整个进程崩溃，内核所有权也会随 keeper 线程终止而被**放弃**（abandoned），
///   下一个取得者拿到 `WAIT_ABANDONED` 并成为新所有者。
/// * 取得 `WAIT_ABANDONED` 之后我们**就是**所有者，`Drop`/`release` 与正常取得完全相同
///   （`ReleaseMutex`），不会把 scope 留在 abandoned 状态。
#[derive(Debug)]
pub struct CrossProcessInputScopeGuard {
    /// 保活命名对象，并持有其句柄（字段顺序即析构顺序，见 `release_inner`）。
    object: InputScopeKernelObject,
    recovered_from_abandoned: bool,
    owner_pid: u32,
    owner_thread_id: u32,
    release: Mutex<Option<mpsc::Sender<()>>>,
    keeper: Option<thread::JoinHandle<()>>,
    released: bool,
}

impl CrossProcessInputScopeGuard {
    pub fn scope(&self) -> &str {
        self.object.scope()
    }

    pub fn kernel_object_name(&self) -> &str {
        self.object.kernel_object_name()
    }

    /// 取得时是否观察到 `WAIT_ABANDONED`（前一个持有者崩溃/被强杀未释放）。
    pub fn recovered_from_abandoned(&self) -> bool {
        self.recovered_from_abandoned
    }

    /// 当前内核所有权所属的进程（取得者进程）。
    pub fn owner_pid(&self) -> u32 {
        self.owner_pid
    }

    /// 当前内核所有权真正的所属线程（内部 keeper 线程）。
    pub fn owner_thread_id(&self) -> u32 {
        self.owner_thread_id
    }

    /// 是否仍持有跨进程所有权（释放后为 `false`）。
    pub fn is_held(&self) -> bool {
        !self.released
    }

    /// 显式释放：与 `Drop` 等价（释放内核所有权 + 关闭句柄）。
    pub fn release(self) {
        // 走 `Drop`，保证两条路径只有一份实现。
    }

    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        // 顺序不能反：先请 keeper 释放内核所有权并退出，再（由 `self.object` 字段析构）
        // 关闭命名对象句柄。反过来会关掉 keeper 正在等待的句柄。
        let sender = self
            .release
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(sender) = sender {
            let _ = sender.send(());
        }
        if let Some(keeper) = self.keeper.take() {
            // keeper 收到信号后只做 ReleaseMutex + 退出，因此这里的 join 不会长时间阻塞；
            // 唯一可能等一会儿的是"取得时报文握手已超时"的路径，那条路径在
            // `try_acquire` 内部已经用同一个有界超时完成过 join。
            let _ = keeper.join();
        }
    }
}

impl Drop for CrossProcessInputScopeGuard {
    fn drop(&mut self) {
        self.release_inner();
    }
}

/// 取得 scope 的跨进程排他所有权（一步到位：创建/打开命名对象 + 等待所有权）。
///
/// `timeout` 为等待上限；`Duration::ZERO` 表示只做非阻塞探测。
/// 正常情况下"别人正持有"会在 `timeout` 内以 `CrossProcessScopeError::Busy` 返回，
/// 不会无限阻塞。
pub fn acquire_input_scope_across_processes(
    scope: &str,
    timeout: Duration,
) -> Result<CrossProcessInputScopeGuard, CrossProcessScopeError> {
    InputScopeKernelObject::open(scope)
        .map_err(|source| CrossProcessScopeError::Kernel {
            scope: scope.to_string(),
            source,
        })?
        .try_acquire(timeout)
}

/// 组合取得失败的原因。
#[derive(Debug)]
pub enum ScopedInputOwnershipError {
    /// 同一进程内已有 owner（broker 记录命中）：未触碰内核对象。
    InProcessBusy(InputLeaseBusy),
    /// 跨进程范围内已被其它所有者持有，或内核对象操作失败。
    CrossProcess(CrossProcessScopeError),
}

impl ScopedInputOwnershipError {
    /// 是否属于"别人正持有"（进程内或跨进程）。
    pub fn is_busy(&self) -> bool {
        match self {
            Self::InProcessBusy(_) => true,
            Self::CrossProcess(error) => error.is_busy(),
        }
    }
}

impl fmt::Display for ScopedInputOwnershipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InProcessBusy(busy) => write!(formatter, "进程内输入 owner 冲突: {busy}"),
            Self::CrossProcess(error) => write!(formatter, "跨进程输入 scope 冲突: {error}"),
        }
    }
}

impl std::error::Error for ScopedInputOwnershipError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InProcessBusy(busy) => Some(busy),
            Self::CrossProcess(error) => Some(error),
        }
    }
}

/// 进程内 epoch fencing + 跨进程内核对象排他的**组合**凭据。
///
/// 两层各自负责一件事，缺一不可：
///
/// | 层 | 解决什么 | 由谁负责 |
/// |---|---|---|
/// | 进程内 broker 记录 + epoch | 同进程内"同时只有一个 owner"；旧 token 失效 | `InteractiveInputLeaseBroker` |
/// | 命名内核互斥体 | 跨进程"同时只有一个 owner"；持有者崩溃后自动释放 | `CrossProcessInputScopeGuard` |
///
/// 取得顺序：**先**取进程内 lease（同进程竞争在此快速失败，同时拿到唯一 epoch），
/// **再**取内核所有权。跨进程层失败时必须立刻释放刚拿到的进程内记录，
/// 否则会留下反向错配——"进程内以为持有、内核并没有所有权"。
///
/// 释放顺序（由字段声明顺序保证）= 取得顺序的逆序：先交还内核所有权，再撤掉进程内记录。
/// 反过来会在仍持有内核所有权时先放开进程内记录，让同进程竞争者拿到 epoch 后
/// 卡在内核等待上（噪音，不是安全漏洞，但没有理由制造它）。
///
/// epoch 与内核对象的关系：内核对象不携带任何 epoch，它只回答"现在谁持有"；
/// 旧 epoch 的 token **不会**因为重新取得内核所有权而复活——epoch 在 broker 里全局
/// 单调递增，旧 epoch 不可能被重新分配，而 `is_current()` 只认记录的 `(owner_id, epoch)`。
#[derive(Debug)]
pub struct ScopedInputOwnership<'a> {
    /// 字段顺序 = 析构顺序：必须先释放内核所有权，再撤进程内记录。**不要调换**。
    scope_guard: CrossProcessInputScopeGuard,
    lease: InteractiveInputLease<'a>,
}

impl<'a> ScopedInputOwnership<'a> {
    /// 依次取得进程内 lease 与跨进程所有权。
    pub fn acquire(
        broker: &'a InteractiveInputLeaseBroker,
        scope: &str,
        owner_id: &str,
        timeout: Duration,
    ) -> Result<Self, ScopedInputOwnershipError> {
        let lease = broker
            .acquire(scope.to_string(), owner_id.to_string())
            .map_err(ScopedInputOwnershipError::InProcessBusy)?;
        match acquire_input_scope_across_processes(scope, timeout) {
            Ok(scope_guard) => Ok(Self { scope_guard, lease }),
            Err(error) => {
                // 显式释放进程内记录：不允许留下"进程内已锁、内核没有所有权"的错配。
                drop(lease);
                Err(ScopedInputOwnershipError::CrossProcess(error))
            }
        }
    }

    pub fn scope(&self) -> &str {
        self.scope_guard.scope()
    }

    pub fn owner_epoch(&self) -> u64 {
        self.lease.owner_epoch()
    }

    /// 返回实际取得 lease 的 owner，宿主不得以另一个上下文字符串冒充它。
    pub fn owner_id(&self) -> &str {
        &self.lease.owner_id
    }

    /// 旧 token 失效语义由 broker 的 epoch 记录决定（与进程内语义完全一致）。
    pub fn is_current(&self) -> bool {
        self.lease.is_current()
    }

    /// 与 [`InteractiveInputLeaseBroker::dispatch_if_current`] 同一序列化决策边界，
    /// 直接使用本组合所有权（进程内 epoch 围栏 + 跨进程内核保活）的 `(scope, owner_id, epoch)`。
    ///
    /// 桌面输入执行器推荐走这条入口：复核（含调用方守卫）与真正落输入在同一个临界区内完成，
    /// 被拒绝时输入调用次数为零。
    pub fn dispatch_if_current<R>(
        &self,
        guards: &[InputDispatchGuard<'_>],
        dispatch: impl FnOnce() -> R,
    ) -> Result<R, InputDispatchRefusal> {
        self.lease.dispatch_if_current(guards, dispatch)
    }

    pub fn scope_guard(&self) -> &CrossProcessInputScopeGuard {
        &self.scope_guard
    }
}

// ---------------------------------------------------------------------------
// 任务二：托管实例身份核对（关闭 PID 复用窗口）
// ---------------------------------------------------------------------------

/// Agent 终止托管实例时使用的退出码（便于事后从退出码辨认终止来源，仅作诊断）。
const AGENT_TERMINATE_EXIT_CODE: u32 = 0x435A_0001;

/// 进程实例身份标记：把"Agent 登记过的那个实例"与"后来复用同一 PID 的别的进程"区分开。
///
/// 判定只依据 `creation_time`（`GetProcessTimes` 返回的创建时间）：
///
/// * 同一 PID 被复用成新进程时，创建时间必然不同（内核在进程对象创建时打时间戳），
///   而同一实例的创建时间恒定——这正是"不可复用的身份标记"；
/// * `image_path` 只用于诊断/记录，**不参与**判定：路径比较会因大小写、8.3 短路径、
///   WOW64 重定向等产生假阴性，把假阴性当"身份不符"会拒绝一次合法终止；
///   反之，创建时间相等时路径不可能不同（进程主映像在生存期内不变）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pid: u32,
    creation_time: u64,
    image_path: Option<String>,
}

impl ProcessIdentity {
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// `GetProcessTimes` 的创建时间（FILETIME，100ns 自 1601-01-01）。
    pub fn creation_time_filetime(&self) -> u64 {
        self.creation_time
    }

    /// `QueryFullProcessImageNameW` 的结果；查询失败时为 `None`（不影响身份判定）。
    pub fn image_path(&self) -> Option<&str> {
        self.image_path.as_deref()
    }

    /// 是否同一个实例：PID 相同**且**创建时间相同。
    pub fn is_same_instance(&self, other: &Self) -> bool {
        self.pid == other.pid && self.creation_time == other.creation_time
    }
}

/// 仅测试构建可见的身份构造接缝。
///
/// 用于在不真正复用 PID 的前提下验证"同一 PID、不同实例身份"被拒绝终止
/// （真实 PID 复用无法在测试里按需制造）。与 `revoke_owner_for_test` 相同，
/// 由 `#[cfg(any(test, feature = "test-support"))]` 门控，发布依赖图不得启用 `test-support`。
#[cfg(any(test, feature = "test-support"))]
pub fn process_identity_for_test(pid: u32, creation_time_filetime: u64) -> ProcessIdentity {
    ProcessIdentity {
        pid,
        creation_time: creation_time_filetime,
        image_path: None,
    }
}

/// 取身份失败的原因。
#[derive(Debug)]
pub enum ProcessIdentityError {
    /// 该 PID 不存在（进程已完全消失）：`OpenProcess` 报 `ERROR_INVALID_PARAMETER`。
    NotFound { pid: u32 },
    /// 取到的身份与登记身份不是同一实例（PID 已被复用成别的进程）。
    IdentityMismatch {
        pid: u32,
        expected: ProcessIdentity,
        actual: ProcessIdentity,
    },
    /// 进程存在但拒绝访问（受保护进程、更高权限的进程等）。
    AccessDenied { pid: u32, source: io::Error },
    /// 其它 OS 层失败。
    Os { pid: u32, source: io::Error },
}

impl fmt::Display for ProcessIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { pid } => write!(formatter, "进程 {pid} 不存在"),
            Self::IdentityMismatch { pid, .. } => {
                write!(formatter, "进程 {pid} 的身份与登记标记不符（PID 已被复用？）")
            }
            Self::AccessDenied { pid, source } => {
                write!(formatter, "进程 {pid} 拒绝身份查询: {source}")
            }
            Self::Os { pid, source } => write!(formatter, "进程 {pid} 身份查询失败: {source}"),
        }
    }
}

impl std::error::Error for ProcessIdentityError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AccessDenied { source, .. } | Self::Os { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// 按身份终止托管实例失败的原因。
///
/// `NotFound` 与 `IdentityMismatch` 必须可区分：前者是"**已经不在**"（无需再动），
/// 后者是"**PID 被复用成了别的进程**"（我们与该进程毫无关系，绝不能终止）。
#[derive(Debug)]
pub enum TerminateOwnedProcessError {
    /// 该 PID 不存在（进程已完全消失）。
    NotFound { pid: u32 },
    /// 进程已经结束，但进程对象仍被句柄保留着（PID 尚未释放）。
    /// 事实含义与 `NotFound` 同属"已经不在"，但来源不同，故单独区分。
    AlreadyExited {
        pid: u32,
        identity: ProcessIdentity,
    },
    /// 登记的身份标记不属于被终止的 PID（调用方把 A 的标记用在 B 上）：拒绝终止。
    RegisteredIdentityPidMismatch {
        pid: u32,
        registered: ProcessIdentity,
    },
    /// 目标 PID 存在，但身份与登记标记不符（PID 被复用成别的进程）：**拒绝终止**。
    IdentityMismatch {
        pid: u32,
        expected: ProcessIdentity,
        actual: ProcessIdentity,
    },
    /// 进程存在但拒绝访问：未发出终止。
    AccessDenied { pid: u32, source: io::Error },
    /// 身份核对通过，但 `TerminateProcess` 本身失败。
    TerminateFailed {
        pid: u32,
        identity: ProcessIdentity,
        source: io::Error,
    },
    /// 其它 OS 层失败（打开进程、读取创建时间等）。
    Os { pid: u32, source: io::Error },
}

impl TerminateOwnedProcessError {
    /// 是否属于"目标已经不在"一类（`NotFound` / `AlreadyExited`）。
    ///
    /// 上层的处置是"无需终止、清理台账即可"，与 `IdentityMismatch`
    /// （"登记实例已消失、PID 被别人占用"）的事实含义完全不同。
    pub fn means_already_gone(&self) -> bool {
        matches!(self, Self::NotFound { .. } | Self::AlreadyExited { .. })
    }

    /// 是否是"身份核对不通过"（含标记与 PID 不匹配）。
    pub fn is_identity_mismatch(&self) -> bool {
        matches!(
            self,
            Self::IdentityMismatch { .. } | Self::RegisteredIdentityPidMismatch { .. }
        )
    }

    pub fn pid(&self) -> u32 {
        match self {
            Self::NotFound { pid }
            | Self::AlreadyExited { pid, .. }
            | Self::RegisteredIdentityPidMismatch { pid, .. }
            | Self::IdentityMismatch { pid, .. }
            | Self::AccessDenied { pid, .. }
            | Self::TerminateFailed { pid, .. }
            | Self::Os { pid, .. } => *pid,
        }
    }
}

impl fmt::Display for TerminateOwnedProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { pid } => write!(formatter, "进程 {pid} 不存在（已经不在）"),
            Self::AlreadyExited { pid, .. } => {
                write!(formatter, "进程 {pid} 已结束（句柄仍保留其对象）")
            }
            Self::RegisteredIdentityPidMismatch { pid, registered } => write!(
                formatter,
                "登记标记不属于 PID {pid}（标记的 PID 是 {}）",
                registered.pid()
            ),
            Self::IdentityMismatch { pid, .. } => write!(
                formatter,
                "进程 {pid} 的身份与登记标记不符：拒绝终止（PID 已被复用）"
            ),
            Self::AccessDenied { pid, source } => {
                write!(formatter, "进程 {pid} 拒绝访问（未发出终止）: {source}")
            }
            Self::TerminateFailed { pid, source, .. } => {
                write!(formatter, "进程 {pid} 终止请求失败: {source}")
            }
            Self::Os { pid, source } => write!(formatter, "进程 {pid} 身份核对失败: {source}"),
        }
    }
}

impl std::error::Error for TerminateOwnedProcessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AccessDenied { source, .. }
            | Self::TerminateFailed { source, .. }
            | Self::Os { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// 按身份终止托管实例的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminateOwnedProcessOutcome {
    /// 身份核对通过，`TerminateProcess` 已被内核接受。
    ///
    /// 注意：这只表示"终止请求已对该身份发出"，**不**表示进程已经消失；
    /// 需要"已消失"的事实请自行 `WaitForSingleObject` / `Child::try_wait`。
    Terminated {
        pid: u32,
        identity: ProcessIdentity,
    },
}

impl TerminateOwnedProcessOutcome {
    pub fn pid(&self) -> u32 {
        match self {
            Self::Terminated { pid, .. } => *pid,
        }
    }

    pub fn identity(&self) -> &ProcessIdentity {
        match self {
            Self::Terminated { identity, .. } => identity,
        }
    }
}

fn open_process_handle(
    pid: u32,
    access: PROCESS_ACCESS_RIGHTS,
) -> Result<KernelHandle, ProcessIdentityError> {
    // SAFETY: 无继承、按 PID 打开；失败返回空句柄。
    let handle = unsafe { OpenProcess(access, 0, pid) };
    if handle.is_null() {
        let source = io::Error::last_os_error();
        // ERROR_INVALID_PARAMETER = "该 PID 不存在"；ERROR_ACCESS_DENIED = 存在但无权访问。
        return Err(if is_win32_code(&source, windows_sys::Win32::Foundation::ERROR_INVALID_PARAMETER)
        {
            ProcessIdentityError::NotFound { pid }
        } else if is_win32_code(&source, windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED) {
            ProcessIdentityError::AccessDenied { pid, source }
        } else {
            ProcessIdentityError::Os { pid, source }
        });
    }
    Ok(KernelHandle(handle))
}

/// 从已打开的进程句柄读取身份标记。
fn identity_from_handle(handle: HANDLE, pid: u32) -> Result<ProcessIdentity, ProcessIdentityError> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    // SAFETY: 四个 FILETIME 都是本栈上的可写缓冲。
    let ok = unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user) };
    if ok == 0 {
        let source = io::Error::last_os_error();
        return Err(if is_win32_code(&source, windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED) {
            ProcessIdentityError::AccessDenied { pid, source }
        } else {
            ProcessIdentityError::Os { pid, source }
        });
    }
    Ok(ProcessIdentity {
        pid,
        creation_time: (u64::from(creation.dwHighDateTime) << 32)
            | u64::from(creation.dwLowDateTime),
        image_path: process_image_path(handle),
    })
}

/// 尽力读取映像路径；失败只影响诊断信息，不影响身份判定。
fn process_image_path(handle: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 32 * 1024];
    let mut size = u32::try_from(buffer.len()).ok()?;
    // SAFETY: buffer 是 size 个 u16 的可写缓冲，size 按 API 要求传"容量/实际长度"。
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size)
    };
    if ok == 0 {
        return None;
    }
    let len = usize::try_from(size).ok()?;
    if len == 0 || len > buffer.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..len]))
}

/// 取得某 PID 当前实例的身份标记。
///
/// 注意：如果调用方刚刚 spawn 了进程，"spawn 完成"与"按 PID 取身份"之间存在
/// 一小段窗口；要完全消除该窗口，请用 `capture_child_process_identity(&child)`
/// （直接用 spawn 得到的进程句柄，不存在 PID 复用问题）。
pub fn capture_process_identity(pid: u32) -> Result<ProcessIdentity, ProcessIdentityError> {
    let handle = open_process_handle(pid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    identity_from_handle(handle.raw(), pid)
}

/// 同一次句柄上确认存活并读取身份；供执行者登记使用，不代替通知前的 Child 复核。
pub fn capture_live_process_identity(pid: u32) -> Result<ProcessIdentity, ProcessIdentityError> {
    let handle = open_process_handle(pid, PROCESS_QUERY_LIMITED_INFORMATION | windows_sys::Win32::System::Threading::PROCESS_SYNCHRONIZE)?;
    let identity = identity_from_handle(handle.raw(), pid)?;
    // SAFETY: 当前句柄带 SYNCHRONIZE，只做零等待；不将退出码 259 误判为存活。
    match unsafe { WaitForSingleObject(handle.raw(), 0) } {
        WAIT_TIMEOUT => Ok(identity),
        WAIT_OBJECT_0 => Err(ProcessIdentityError::NotFound { pid }),
        _ => Err(ProcessIdentityError::Os { pid, source: io::Error::last_os_error() }),
    }
}

/// 直接从 `Child` 的进程句柄取身份标记（推荐路径：没有 PID 复用窗口）。
pub fn capture_child_process_identity(child: &Child) -> Result<ProcessIdentity, ProcessIdentityError> {
    // std 的 `Child` 在 spawn 之后即持有进程句柄，这里不复制也不关闭它。
    let handle = child.as_raw_handle() as HANDLE;
    identity_from_handle(handle, child.id())
}

/// 核对 PID 当前实例是否就是 `expected`（诊断/记录用途）。
///
/// 该方法**另开**一个句柄做核对，因此天然带有"核对完 → 再动手"的窗口，
/// 不要用它来给终止做前置检查——终止请用 `terminate_owned_process`，
/// 它在同一个句柄上完成核对与终止。
///
/// `expected` 的 PID 与 `pid` 不一致时同样以 `IdentityMismatch` 呈现
/// （`actual` 是 `pid` 的真实身份，两者放在一起可直接看出是"标记属别的进程"）。
pub fn verify_process_identity(
    pid: u32,
    expected: &ProcessIdentity,
) -> Result<ProcessIdentity, ProcessIdentityError> {
    let actual = capture_process_identity(pid)?;
    if expected.is_same_instance(&actual) {
        Ok(actual)
    } else {
        Err(ProcessIdentityError::IdentityMismatch {
            pid,
            expected: expected.clone(),
            actual,
        })
    }
}

/// 把取身份失败的原因映射到"按身份终止"的错误分类。
fn map_identity_error(error: ProcessIdentityError) -> TerminateOwnedProcessError {
    match error {
        ProcessIdentityError::NotFound { pid } => TerminateOwnedProcessError::NotFound { pid },
        ProcessIdentityError::AccessDenied { pid, source } => {
            TerminateOwnedProcessError::AccessDenied { pid, source }
        }
        ProcessIdentityError::IdentityMismatch {
            pid,
            expected,
            actual,
        } => TerminateOwnedProcessError::IdentityMismatch {
            pid,
            expected,
            actual,
        },
        ProcessIdentityError::Os { pid, source } => TerminateOwnedProcessError::Os { pid, source },
    }
}

fn process_has_exited(handle: HANDLE) -> bool {
    let mut exit_code = 0;
    // SAFETY: exit_code 是本栈上的可写缓冲。
    if unsafe { GetExitCodeProcess(handle, &mut exit_code) } == 0 {
        return false;
    }
    // STILL_ACTIVE 在 windows-sys 里是 NTSTATUS(259)；退出码按 u32 比较。
    exit_code != STILL_ACTIVE as u32
}

/// 按**身份**终止托管实例：先核对身份，不匹配就拒绝终止。
///
/// 语义（产品裁决要求）：
///
/// * 核对与终止在**同一个进程句柄**上完成，因此"核对通过之后 PID 被复用"这个二次窗口
///   不存在：句柄绑定的就是被核对的那个进程对象；
/// * 身份不匹配时**只**返回 `IdentityMismatch`，绝不退化成"按 PID 硬杀"；
/// * `NotFound`（PID 不存在）与 `IdentityMismatch`（PID 被复用成别的进程）可区分，
///   二者对上层的事实含义不同：前者"无需再动"，后者"我们与当前占用者毫无关系"；
/// * 身份无法核对时（无权限、读取失败）同样不终止，返回 `AccessDenied` / `Os`。
pub fn terminate_owned_process(
    pid: u32,
    expected: &ProcessIdentity,
) -> Result<TerminateOwnedProcessOutcome, TerminateOwnedProcessError> {
    if expected.pid() != pid {
        return Err(TerminateOwnedProcessError::RegisteredIdentityPidMismatch {
            pid,
            registered: expected.clone(),
        });
    }
    let handle = open_process_handle(pid, PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION)
        .map_err(map_identity_error)?;

    let actual = identity_from_handle(handle.raw(), pid).map_err(map_identity_error)?;

    if !expected.is_same_instance(&actual) {
        // 拒绝终止：此处不发出任何终止调用。
        return Err(TerminateOwnedProcessError::IdentityMismatch {
            pid,
            expected: expected.clone(),
            actual,
        });
    }

    // SAFETY: handle 是有效的进程句柄且带 PROCESS_TERMINATE；退出码只作诊断。
    if unsafe { TerminateProcess(handle.raw(), AGENT_TERMINATE_EXIT_CODE) } == 0 {
        let source = io::Error::last_os_error();
        // 已经结束的进程会以 ERROR_ACCESS_DENIED 拒绝终止；先分清"已经不在"。
        if process_has_exited(handle.raw()) {
            return Err(TerminateOwnedProcessError::AlreadyExited {
                pid,
                identity: actual,
            });
        }
        return Err(
            if is_win32_code(&source, windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED) {
                TerminateOwnedProcessError::AccessDenied { pid, source }
            } else {
                TerminateOwnedProcessError::TerminateFailed {
                    pid,
                    identity: actual,
                    source,
                }
            },
        );
    }

    Ok(TerminateOwnedProcessOutcome::Terminated {
        pid,
        identity: actual,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_lease_blocks_competing_owner_and_fences_released_epoch() {
        let broker = InteractiveInputLeaseBroker::default();
        let first = broker.acquire("session-a", "run-a").expect("first lease");
        assert!(first.is_current());
        let first_epoch = first.owner_epoch();

        let busy = broker
            .acquire("session-a", "run-b")
            .expect_err("competing owner must not inject input");
        assert_eq!(busy.owner_id, "run-a");
        assert_eq!(busy.owner_epoch, first_epoch);

        drop(first);
        let second = broker
            .acquire("session-a", "run-b")
            .expect("released scope");
        assert!(second.is_current());
        assert!(second.owner_epoch() > first_epoch);
    }

    /// 复刻执行器在真正落输入之前的守卫（`TracingAdapter::act`）：
    /// `lease.is_current()` 为假即拒绝输入。测试只断言这个判定，不引入新的生产入口。
    fn pre_input_check_rejects(lease: &InteractiveInputLease<'_>) -> bool {
        !lease.is_current()
    }

    /// 失效后旧 lease 立刻不再是当前 owner：输入前校验必须据此拒绝。
    #[test]
    fn revoked_owner_token_is_no_longer_current_and_pre_input_check_rejects() {
        let broker = InteractiveInputLeaseBroker::default();
        let lease = broker.acquire("session-a", "run-a").expect("first lease");
        let epoch = lease.owner_epoch();
        assert!(lease.is_current(), "刚取得的 lease 必须是当前 owner");
        assert!(!pre_input_check_rejects(&lease));

        assert!(
            broker.revoke_owner_for_test("session-a", epoch),
            "匹配当前 epoch 的失效必须生效"
        );

        assert!(
            !lease.is_current(),
            "失效后旧 token 不得再被认作当前 owner"
        );
        assert!(
            pre_input_check_rejects(&lease),
            "输入前校验必须因此拒绝该 owner 的输入"
        );
    }

    /// 失效后新 owner 可以取得同一 scope，且拿到**更高** epoch（旧 token 不可能复活）。
    #[test]
    fn revoked_scope_admits_new_owner_with_higher_epoch() {
        let broker = InteractiveInputLeaseBroker::default();
        let stale = broker.acquire("session-a", "run-a").expect("first lease");
        let stale_epoch = stale.owner_epoch();
        assert!(broker.revoke_owner_for_test("session-a", stale_epoch));

        let fresh = broker
            .acquire("session-a", "run-b")
            .expect("失效后同一 scope 必须可被重新取得");
        assert!(fresh.is_current());
        assert!(
            fresh.owner_epoch() > stale_epoch,
            "新 owner 必须拿到更高 epoch，旧 token 无从复活"
        );
        assert!(!stale.is_current(), "旧 token 依然无效");
    }

    /// 关键回归：旧 lease 在失效之后才析构，不得清除后来取得的新 owner。
    #[test]
    fn stale_lease_drop_does_not_clear_the_new_owner() {
        let broker = InteractiveInputLeaseBroker::default();
        let stale = broker.acquire("session-a", "run-a").expect("first lease");
        let stale_epoch = stale.owner_epoch();
        assert!(broker.revoke_owner_for_test("session-a", stale_epoch));

        let fresh = broker
            .acquire("session-a", "run-b")
            .expect("失效后同一 scope 必须可被重新取得");

        // 旧持有者现在才析构（真实场景：执行器在下一轮才 drop 掉已失效的 lease）。
        drop(stale);

        assert!(
            fresh.is_current(),
            "旧 lease 的 Drop 不得清除后来取得的新 owner"
        );
        assert!(!pre_input_check_rejects(&fresh));
        assert_eq!(
            broker
                .acquire("session-a", "run-c")
                .expect_err("新 owner 仍在持有，第三方必须让位")
                .owner_id,
            "run-b"
        );
    }

    /// 只作用于指定 epoch：epoch 不匹配时失效必须失败且不改动任何状态。
    #[test]
    fn revoke_with_mismatched_epoch_is_ignored() {
        let broker = InteractiveInputLeaseBroker::default();
        let lease = broker.acquire("session-a", "run-a").expect("first lease");
        let epoch = lease.owner_epoch();

        for wrong in [0, epoch.wrapping_sub(1), epoch + 1, u64::MAX] {
            if wrong == epoch {
                continue;
            }
            assert!(
                !broker.revoke_owner_for_test("session-a", wrong),
                "epoch {wrong} 与当前 owner epoch 不符，失效必须失败"
            );
        }
        assert!(
            !broker.revoke_owner_for_test("session-unknown", epoch),
            "不存在的 scope 不得被判为失效成功"
        );

        assert!(
            lease.is_current(),
            "不匹配的失效调用不得影响真实 owner"
        );
        assert_eq!(
            broker
                .acquire("session-a", "run-b")
                .expect_err("owner 仍然有效")
                .owner_epoch,
            epoch
        );
    }

    /// 只作用于指定 scope：并行测试使用的另一个 scope 不受影响，broker 其余状态也不被清空。
    #[test]
    fn revoke_touches_only_the_named_scope() {
        let broker = InteractiveInputLeaseBroker::default();
        let target = broker.acquire("session-a", "run-a").expect("target lease");
        let neighbour = broker
            .acquire("session-b", "run-b")
            .expect("parallel scope lease");
        let neighbour_epoch = neighbour.owner_epoch();

        assert!(broker.revoke_owner_for_test("session-a", target.owner_epoch()));

        assert!(!target.is_current(), "指定的 scope 必须失效");
        assert!(
            neighbour.is_current(),
            "另一个 scope 的 owner 不得被牵连失效"
        );
        assert_eq!(neighbour.owner_epoch(), neighbour_epoch);
        assert_eq!(
            broker
                .acquire("session-b", "run-c")
                .expect_err("并行 scope 的 owner 仍在持有")
                .owner_epoch,
            neighbour_epoch
        );
    }

    // ---- 任务一：跨进程排他 ------------------------------------------------

    /// 单一测试进程内的默认取得超时。
    const ACQUIRE_TIMEOUT: Duration = Duration::from_millis(300);

    /// 每个测试用独立 scope，避免并行测试互相干扰。
    fn probe_scope(tag: &str) -> String {
        format!("xproc-{tag}-{}", std::process::id())
    }

    #[test]
    fn kernel_object_name_is_session_local_and_injective() {
        let base = input_scope_kernel_object_name("windows-session-1");
        assert!(
            base.starts_with("Local\\coolzhu-input-scope-"),
            "必须落在会话本地命名空间（Local\\），实际 {base}"
        );
        assert_ne!(
            base,
            input_scope_kernel_object_name("windows-session-2"),
            "不同 scope 必须得到不同对象名"
        );
        assert_ne!(
            base,
            input_scope_kernel_object_name("windows-session-11")
        );
        assert_ne!(
            input_scope_kernel_object_name("a\\b"),
            input_scope_kernel_object_name("ab"),
            "十六进制编码必须消除分隔符歧义"
        );
        let long = input_scope_kernel_object_name(&"x".repeat(500));
        let long_other = input_scope_kernel_object_name(&format!("{}y", "x".repeat(499)));
        assert_ne!(long, long_other, "超长 scope 必须仍然单射（截断 + 摘要）");
        assert!(
            long.len() < 260,
            "内核对象名必须远低于 MAX_PATH，实际 {}",
            long.len()
        );
    }

    /// 同一 scope 的竞争者在取得之前必须让位；Drop 之后必须能重新取得。
    /// 注意：竞争者与持有者是**不同线程**（内部 keeper 线程），这也顺带证明
    /// "同一线程递归等待内核互斥体"的陷阱在本地实现里不存在。
    #[test]
    fn cross_process_guard_blocks_competitor_and_releases_on_drop() {
        let guard =
            acquire_input_scope_across_processes("xproc-inproc-drop", ACQUIRE_TIMEOUT).expect("空 scope 必须可取得");
        assert!(guard.is_held());
        assert!(!guard.recovered_from_abandoned());
        assert_eq!(guard.owner_pid(), std::process::id());
        assert_ne!(guard.owner_thread_id(), 0);

        let busy = acquire_input_scope_across_processes("xproc-inproc-drop", ACQUIRE_TIMEOUT)
            .expect_err("同 scope 的竞争者必须让位");
        assert!(busy.is_busy(), "竞争者必须得到 Busy，实际 {busy:?}");

        drop(guard);
        let second = acquire_input_scope_across_processes("xproc-inproc-drop", ACQUIRE_TIMEOUT)
            .expect("Drop 之后必须可重新取得");
        assert!(second.is_held());
    }

    /// Drop 可以从任意线程执行（这是 keeper 线程存在的理由）：
    /// 若所有权挂在取得线程上，跨线程 `ReleaseMutex` 会 `ERROR_NOT_OWNER` 失败，
    /// 于是"进程内以为释放了、内核对象还持有"。
    #[test]
    fn guard_release_works_from_another_thread() {
        let guard = acquire_input_scope_across_processes("xproc-cross-thread", ACQUIRE_TIMEOUT)
            .expect("取得 scope");
        // 移动到别的线程再析构（也顺带断言 guard 是 Send）。
        std::thread::spawn(move || drop(guard))
            .join()
            .expect("释放线程不得 panic");

        let again = acquire_input_scope_across_processes("xproc-cross-thread", ACQUIRE_TIMEOUT)
            .expect("跨线程释放之后必须可再次取得（内核所有权确实交还了）");
        drop(again);
    }

    /// 显式 release 与 Drop 等价：消费 `self` 之后内核所有权已经交还。
    #[test]
    fn explicit_release_has_the_same_effect_as_drop() {
        let guard = acquire_input_scope_across_processes("xproc-explicit-release", ACQUIRE_TIMEOUT)
            .expect("取得 scope");
        assert!(guard.is_held());
        guard.release();

        let again = acquire_input_scope_across_processes("xproc-explicit-release", ACQUIRE_TIMEOUT)
            .expect("release 之后必须可再次取得（不是'只改标记仍持有'）");
        assert!(again.is_held());
    }

    /// 非阻塞探测：别人持有时 `Duration::ZERO` 立即返回 Busy，不挂住调用方。
    #[test]
    fn zero_timeout_probe_returns_busy_without_blocking() {
        let scope = "xproc-zero-timeout";
        let guard = acquire_input_scope_across_processes(scope, ACQUIRE_TIMEOUT).expect("取得");
        let started = std::time::Instant::now();
        let busy = acquire_input_scope_across_processes(scope, Duration::ZERO)
            .expect_err("非阻塞探测必须立即得到 Busy");
        let elapsed = started.elapsed();
        assert!(busy.is_busy(), "实际 {busy:?}");
        assert!(
            elapsed < Duration::from_secs(1),
            "非阻塞探测不得等待，实际耗时 {elapsed:?}"
        );
        drop(guard);
    }

    // ---- 任务一的跨进程验证（真实第二个进程） ------------------------------

    const XPROC_MODE_ENV: &str = "COOLZHU_GUARD_XPROC_MODE";
    const XPROC_SCOPE_ENV: &str = "COOLZHU_GUARD_XPROC_SCOPE";
    /// `pin_then_acquire` 模式里"固定句柄"到"再取一次"之间的等待毫秒数。
    const XPROC_HOLD_MS_ENV: &str = "COOLZHU_GUARD_XPROC_HOLD_MS";
    /// 子进程探针里"别人持有 → 让位"的期望标记。
    const XPROC_CHILD_BUSY: &str = "XPROC_CHILD_BUSY";
    /// 子进程探针里"取得成功"的期望标记。
    const XPROC_CHILD_ACQUIRED: &str = "XPROC_CHILD_ACQUIRED";
    /// 子进程探针里"已固定命名对象句柄"的标记。
    const XPROC_CHILD_PINNED: &str = "XPROC_CHILD_PINNED";
    /// 子进程探针里"观察到 WAIT_ABANDONED"的标记。
    const XPROC_CHILD_ABANDONED: &str = "XPROC_CHILD_ABANDONED";
    /// 子进程探针里"未观察到 WAIT_ABANDONED"的标记（反面对照）。
    const XPROC_CHILD_NOT_ABANDONED: &str = "XPROC_CHILD_NOT_ABANDONED";

    /// 真实第二个进程的探针。
    ///
    /// `#[ignore]` 保证正常 `cargo test` 不会运行它；父测试用
    /// `当前测试二进制 --exact tests::xproc_child_probe --ignored --nocapture`
    /// 把它作为**真实子进程**拉起，并通过环境变量指定行为。
    #[test]
    #[ignore = "由跨进程排他测试作为真实第二进程拉起（需要 COOLZHU_GUARD_XPROC_MODE）"]
    fn xproc_child_probe() {
        let Ok(mode) = std::env::var(XPROC_MODE_ENV) else {
            println!("{XPROC_CHILD_BUSY} 未设置 {XPROC_MODE_ENV}，本进程不是探针（忽略）");
            return;
        };
        let scope = std::env::var(XPROC_SCOPE_ENV).expect("探针必须带 scope");
        match mode.as_str() {
            "expect_busy" => {
                let error =
                    acquire_input_scope_across_processes(&scope, Duration::from_millis(300))
                        .expect_err("父进程持有期间，第二个进程不得取得同名 scope");
                assert!(error.is_busy(), "第二进程必须得到 Busy，实际 {error:?}");
                println!("{XPROC_CHILD_BUSY} scope={scope}");
            }
            "hold_and_die" => {
                let guard = acquire_input_scope_across_processes(&scope, Duration::from_secs(2))
                    .expect("空 scope 必须可取得");
                assert!(guard.is_held());
                println!("{XPROC_CHILD_ACQUIRED} scope={scope} pid={}", std::process::id());
                // 关键：不 Drop、不释放，直接终止进程 → 内核所有权被"放弃"（abandoned）。
                std::mem::forget(guard);
                std::process::exit(0);
            }
            // 人工验证用：先固定命名对象句柄，等外部把**另一个实现**（如 PowerShell/.NET）
            // 的持有者强杀之后，再取一次并报告是否观察到 `WAIT_ABANDONED`。
            "pin_then_acquire" => {
                let hold_ms = std::env::var(XPROC_HOLD_MS_ENV)
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(3_000u64);
                let object = InputScopeKernelObject::open(&scope).expect("打开命名对象");
                println!("{XPROC_CHILD_PINNED} scope={scope} pid={}", std::process::id());
                std::thread::sleep(Duration::from_millis(hold_ms));
                let guard = object
                    .try_acquire(Duration::from_secs(2))
                    .expect("外部持有者被强杀后，scope 必须可取得");
                if guard.recovered_from_abandoned() {
                    println!("{XPROC_CHILD_ABANDONED} scope={scope}");
                } else {
                    println!("{XPROC_CHILD_NOT_ABANDONED} scope={scope}");
                }
                println!("{XPROC_CHILD_ACQUIRED} scope={scope} pid={}", std::process::id());
                drop(guard);
            }
            other => panic!("未知的探针模式 {other}"),
        }
    }

    fn spawn_xproc_probe(mode: &str, scope: &str) -> std::process::Output {
        let exe = std::env::current_exe().expect("测试进程自身的 exe 路径");
        std::process::Command::new(exe)
            .args([
                "--exact",
                "tests::xproc_child_probe",
                "--ignored",
                "--nocapture",
            ])
            .env(XPROC_MODE_ENV, mode)
            .env(XPROC_SCOPE_ENV, scope)
            .output()
            .expect("拉起第二个进程（同一测试二进制）")
    }

    fn probe_stdout(output: &std::process::Output) -> String {
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn assert_probe_reported(output: &std::process::Output, marker: &str, mode: &str) {
        let stdout = probe_stdout(output);
        // 原始证据：`cargo test -- --nocapture` 时可看到第二进程的输出与退出码。
        println!(
            "[xproc probe mode={mode}] status={:?} stdout={}",
            output.status,
            stdout.trim()
        );
        assert!(
            output.status.success(),
            "第二进程探针（{mode}）失败：status={:?}\nstdout={stdout}\nstderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains(marker),
            "第二进程探针（{mode}）缺少标记 {marker}，实际输出：{stdout}"
        );
    }

    /// 反向对照：没人持有 scope 时，"期望 Busy"的探针必须**失败**。
    /// 这条测试保证上面那两条跨进程断言不是空调用（否则探针恒退出 0，测试会假通过）。
    #[test]
    fn xproc_probe_fails_when_the_scope_is_actually_free() {
        let scope = probe_scope("negative-control");
        let output = spawn_xproc_probe("expect_busy", &scope);
        let stdout = probe_stdout(&output);
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!(
            "[xproc probe negative-control] status={:?} stdout={} stderr={}",
            output.status,
            stdout.trim(),
            stderr.trim()
        );
        assert!(
            !output.status.success(),
            "空 scope 上'期望 Busy'必须失败，说明探针真的在尝试取得"
        );
        assert!(
            !stdout.contains(XPROC_CHILD_BUSY),
            "空 scope 上不得出现 Busy 标记：{stdout}"
        );
    }

    /// 实测跨进程：本进程持有 scope 时，真实的第二个进程拿不到它；
    /// 本进程释放后，第二个进程立刻能拿到（证明 Drop 真的把内核所有权交还了）。
    #[test]
    fn second_process_cannot_acquire_scope_held_here() {
        let scope = probe_scope("busy");
        let held = acquire_input_scope_across_processes(&scope, ACQUIRE_TIMEOUT).expect("取得 scope");

        let output = spawn_xproc_probe("expect_busy", &scope);
        assert_probe_reported(&output, XPROC_CHILD_BUSY, "expect_busy");

        drop(held);
        let output = spawn_xproc_probe("hold_and_die", &scope);
        assert_probe_reported(&output, XPROC_CHILD_ACQUIRED, "hold_and_die");
    }

    /// 实测跨进程 + `WAIT_ABANDONED`：
    /// 1. 父进程先固定命名对象句柄（这样对象不会随子进程死亡而销毁，
    ///    "前任崩溃"才可被区分出来）；
    /// 2. 子进程取得所有权后不释放直接退出 → 内核把互斥体标记为 abandoned；
    /// 3. 父进程取得时必须观察到 abandoned，并成为新所有者；
    /// 4. 之后 scope 仍然可用（**没有**被永久锁死）。
    #[test]
    fn scope_recovers_after_holder_process_dies_without_releasing() {
        let scope = probe_scope("abandon");
        let object = InputScopeKernelObject::open(&scope).expect("打开命名对象");
        assert!(object.kernel_object_name().starts_with("Local\\"));

        let output = spawn_xproc_probe("hold_and_die", &scope);
        assert_probe_reported(&output, XPROC_CHILD_ACQUIRED, "hold_and_die");

        let guard = object
            .try_acquire(Duration::from_secs(2))
            .expect("前任持有者崩溃后，scope 必须可取得（而不是永久锁死）");
        assert!(
            guard.recovered_from_abandoned(),
            "必须观察到 WAIT_ABANDONED：前任持有者崩溃未释放"
        );
        assert!(guard.is_held());

        // 我们成为新所有者：第二个进程依然让位（abandoned 状态被我们正常接管）。
        let output = spawn_xproc_probe("expect_busy", &scope);
        assert_probe_reported(&output, XPROC_CHILD_BUSY, "expect_busy");

        drop(guard);
        let output = spawn_xproc_probe("hold_and_die", &scope);
        assert_probe_reported(&output, XPROC_CHILD_ACQUIRED, "hold_and_die");
    }

    // ---- 任务一的组合语义（epoch × 跨进程） --------------------------------

    #[test]
    fn composed_ownership_holds_both_layers_and_fences_old_epoch() {
        let broker = InteractiveInputLeaseBroker::default();
        let scope = "xproc-compose";
        let ownership =
            ScopedInputOwnership::acquire(&broker, scope, "run-a", ACQUIRE_TIMEOUT).expect("取得");
        let epoch = ownership.owner_epoch();
        assert!(ownership.is_current());
        assert!(ownership.scope_guard().is_held());

        // 同一 broker：进程内层先挡住。
        let same_broker =
            ScopedInputOwnership::acquire(&broker, scope, "run-b", ACQUIRE_TIMEOUT).expect_err("进程内竞争者让位");
        assert!(
            matches!(same_broker, ScopedInputOwnershipError::InProcessBusy(_)),
            "同 broker 的竞争者必须在内层失败，实际 {same_broker:?}"
        );

        // 另一个 broker（模拟同进程内不共享 broker 的调用方）：跨进程层挡住。
        let other_broker = InteractiveInputLeaseBroker::default();
        let other = ScopedInputOwnership::acquire(&other_broker, scope, "run-b", ACQUIRE_TIMEOUT)
            .expect_err("跨进程层必须挡住");
        assert!(
            matches!(&other, ScopedInputOwnershipError::CrossProcess(error) if error.is_busy()),
            "实际 {other:?}"
        );

        // 关键不变量：跨进程层失败时不得留下进程内记录，
        // 否则就是"进程内以为持有、内核并没有所有权"的反向错配。
        assert!(
            other_broker.acquire(scope, "run-c").is_ok(),
            "跨进程取得失败后，刚拿到的进程内记录必须已被释放"
        );

        // epoch fencing：旧 token 失效后不得复活，新 owner 必须拿到更高 epoch。
        assert!(broker.revoke_owner_for_test(scope, epoch));
        assert!(!ownership.is_current(), "失效后的旧 token 不得再被认作 owner");
        drop(ownership);

        let fresh =
            ScopedInputOwnership::acquire(&broker, scope, "run-d", ACQUIRE_TIMEOUT).expect("重新取得");
        assert!(
            fresh.owner_epoch() > epoch,
            "新 owner 必须拿到更高 epoch（旧 token 无从复活）：{} vs {epoch}",
            fresh.owner_epoch()
        );
        assert!(fresh.scope_guard().is_held());
        assert!(fresh.is_current());
    }

    // ---- 任务二：托管实例身份核对 ------------------------------------------

    /// 单进程 sleeper（不产生子进程），便于安全地验证终止路径。
    fn spawn_sleeper(ping_count: &str) -> Child {
        std::process::Command::new("ping")
            .args(["-n", ping_count, "127.0.0.1"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn ping sleeper")
    }

    #[test]
    fn child_identity_is_stable_and_reports_image_path() {
        let mut child = spawn_sleeper("20");
        let identity = capture_child_process_identity(&child).expect("从 Child 句柄取身份");
        assert_eq!(identity.pid(), child.id());
        assert!(identity.creation_time_filetime() > 0);
        let path = identity.image_path().expect("子进程映像路径");
        assert!(
            path.to_ascii_lowercase().ends_with("ping.exe"),
            "实际映像路径 {path}"
        );

        // 事后按 PID 复取：必须是同一实例（创建时间相同）。
        let again = capture_process_identity(child.id()).expect("按 PID 复取身份");
        assert!(
            identity.is_same_instance(&again),
            "{identity:?} 与 {again:?} 必须是同一实例"
        );

        // 收尾：只终止本测试自己创建的进程。
        let outcome = terminate_owned_process(child.id(), &identity).expect("身份相符必须终止");
        assert_eq!(outcome.pid(), child.id());
        assert_eq!(outcome.identity(), &identity);
        let _ = child.wait();
    }

    #[test]
    fn identity_mismatch_is_refused_and_never_falls_back_to_pid_kill() {
        let mut child = spawn_sleeper("20");
        let identity = capture_child_process_identity(&child).expect("取身份");
        // 构造"同一 PID、不同实例"的标记——正是 PID 被复用后的情形。
        let stale = process_identity_for_test(
            child.id(),
            identity.creation_time_filetime().wrapping_add(1),
        );

        let error = terminate_owned_process(child.id(), &stale).expect_err("身份不符必须拒绝终止");
        match &error {
            TerminateOwnedProcessError::IdentityMismatch {
                pid,
                expected,
                actual,
            } => {
                assert_eq!(*pid, child.id());
                assert_eq!(
                    expected.creation_time_filetime(),
                    stale.creation_time_filetime()
                );
                assert_eq!(
                    actual.creation_time_filetime(),
                    identity.creation_time_filetime()
                );
            }
            other => panic!("必须归类为 IdentityMismatch，实际 {other:?}"),
        }
        assert!(error.is_identity_mismatch());
        assert!(!error.means_already_gone());
        // 关键：拒绝终止不得退化成"按 PID 硬杀"——进程必须还活着。
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "身份不符的路径绝不能终止该进程"
        );

        // 登记身份正确时才允许终止。
        let outcome = terminate_owned_process(child.id(), &identity).expect("身份相符必须终止");
        assert_eq!(outcome.identity(), &identity);
        let _ = child.wait();
    }

    #[test]
    fn nonexistent_pid_is_reported_as_not_found() {
        // Windows 的 PID 是 4 的倍数且远小于该值，可以确定它不会被分配。
        const IMPOSSIBLE_PID: u32 = 0xFFFF_FFF0;
        let marker = process_identity_for_test(IMPOSSIBLE_PID, 1);
        let error = terminate_owned_process(IMPOSSIBLE_PID, &marker).expect_err("不存在的 PID 必须失败");
        println!("[not-found evidence] {error:?} / display={error}");
        assert!(
            matches!(&error, TerminateOwnedProcessError::NotFound { .. }),
            "必须归类为 NotFound，实际 {error:?}"
        );
        assert!(error.means_already_gone());
        assert!(!error.is_identity_mismatch());

        let capture = capture_process_identity(IMPOSSIBLE_PID).expect_err("取身份也要 NotFound");
        println!("[not-found capture evidence] {capture:?}");
        assert!(matches!(capture, ProcessIdentityError::NotFound { .. }));
    }

    #[test]
    fn verify_process_identity_agrees_with_capture_and_flags_reuse() {
        let mut child = spawn_sleeper("20");
        let identity = capture_child_process_identity(&child).expect("取身份");
        let verified = verify_process_identity(child.id(), &identity).expect("同一实例必须核对通过");
        assert!(identity.is_same_instance(&verified));

        let stale = process_identity_for_test(
            child.id(),
            identity.creation_time_filetime().wrapping_add(1),
        );
        let mismatch = verify_process_identity(child.id(), &stale).expect_err("不同实例必须核对失败");
        println!("[verify mismatch evidence] {mismatch:?}");
        assert!(matches!(
            mismatch,
            ProcessIdentityError::IdentityMismatch { .. }
        ));

        let outcome = terminate_owned_process(child.id(), &identity).expect("收尾终止");
        assert_eq!(outcome.pid(), child.id());
        let _ = child.wait();
    }

    /// 进程已结束、但仍有句柄保活其进程对象（PID 尚未回收）时的归类：
    /// 必须落到 `AlreadyExited`，而不是被误报成 `AccessDenied`（"拒绝访问"）。
    #[test]
    fn exited_process_kept_alive_by_a_handle_is_reported_as_already_exited() {
        let mut child = spawn_sleeper("2");
        let identity = capture_child_process_identity(&child).expect("取身份");
        // 另开一个句柄"钉住"进程对象，PID 因此不会马上被回收。
        let pinned =
            open_process_handle(child.id(), PROCESS_QUERY_LIMITED_INFORMATION).expect("钉住进程对象");
        let _ = child.wait();
        drop(child);

        let error = terminate_owned_process(identity.pid(), &identity)
            .expect_err("已经结束的进程必须失败");
        println!("[already-exited evidence] {error:?} / display={error}");
        drop(pinned);
        assert!(
            matches!(&error, TerminateOwnedProcessError::AlreadyExited { .. }),
            "实际 {error:?}"
        );
        assert!(error.means_already_gone());
        assert!(!error.is_identity_mismatch());
    }

    #[test]
    fn finished_child_is_reported_as_already_gone() {
        let mut child = spawn_sleeper("2");
        let identity = capture_child_process_identity(&child).expect("取身份");
        let _ = child.wait();
        drop(child);

        let error = terminate_owned_process(identity.pid(), &identity)
            .expect_err("已经结束的进程必须失败，且不得误报成身份不符");
        println!("[finished-child evidence] {error:?} / display={error}");
        assert!(
            error.means_already_gone(),
            "必须属于'已经不在'一类（NotFound / AlreadyExited），实际 {error:?}"
        );
        assert!(!error.is_identity_mismatch());
        assert!(!matches!(
            &error,
            TerminateOwnedProcessError::AccessDenied { .. }
                | TerminateOwnedProcessError::TerminateFailed { .. }
        ));
    }

    #[test]
    fn registered_identity_for_another_pid_is_refused() {
        let mut child = spawn_sleeper("20");
        let identity = capture_child_process_identity(&child).expect("取身份");
        let error = terminate_owned_process(child.id().wrapping_add(4), &identity)
            .expect_err("标记与目标 PID 不一致必须拒绝");
        assert!(
            matches!(
                &error,
                TerminateOwnedProcessError::RegisteredIdentityPidMismatch { .. }
            ),
            "实际 {error:?}"
        );
        assert!(error.is_identity_mismatch());
        assert!(
            child.try_wait().expect("try_wait").is_none(),
            "拒绝路径绝不能终止任何进程"
        );
        let outcome = terminate_owned_process(child.id(), &identity).expect("收尾终止");
        assert_eq!(outcome.pid(), child.id());
        let _ = child.wait();
    }

    #[test]
    fn own_identity_capture_reports_this_process() {
        let identity = capture_process_identity(std::process::id()).expect("自身身份");
        assert_eq!(identity.pid(), std::process::id());
        let exe = std::env::current_exe().expect("current_exe");
        let path = identity.image_path().expect("自身映像路径");
        assert_eq!(
            std::path::Path::new(path).file_name(),
            exe.file_name(),
            "映像路径 {path} 与 current_exe {} 的文件名必须一致",
            exe.display()
        );
    }

    // ---- RPR-02a：复核 + 派发在同一个序列化决策边界内 ----------------------

    mod dispatch_boundary {
        use super::*;
        use std::sync::atomic::{AtomicBool, AtomicUsize};
        use std::sync::{Arc, Barrier};

        /// 带超时保护的执行：把 `body` 放到独立线程，超时未返回即判定为自锁/死锁，
        /// 而不是让整个测试套件挂住（重入检测失效时会走到这里）。
        fn run_with_timeout<T: Send + 'static>(
            label: &str,
            timeout: Duration,
            body: impl FnOnce() -> T + Send + 'static,
        ) -> T {
            let (tx, rx) = mpsc::channel();
            let handle = thread::spawn(move || {
                let _ = tx.send(body());
            });
            match rx.recv_timeout(timeout) {
                Ok(value) => {
                    handle.join().expect("被测线程不得 panic");
                    value
                }
                // 被测线程已经退出（发送端析构）：把它的真实 panic 原因原样抛出。
                Err(mpsc::RecvTimeoutError::Disconnected) => match handle.join() {
                    Ok(()) => panic!("{label}: 线程已退出但没有返回结果"),
                    Err(payload) => std::panic::resume_unwind(payload),
                },
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if handle.is_finished() {
                        if let Err(payload) = handle.join() {
                            std::panic::resume_unwind(payload);
                        }
                    }
                    panic!("{label}: {timeout:?} 内没有返回（重入检测失效 → 自锁/死锁）");
                }
            }
        }

        #[test]
        fn boundary_dispatches_once_while_the_owner_epoch_is_current() {
            let broker = InteractiveInputLeaseBroker::default();
            let scope = "boundary-happy-path";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();
            let calls = AtomicUsize::new(0);

            let first = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                calls.fetch_add(1, Ordering::SeqCst);
                epoch
            });
            assert_eq!(first, Ok(epoch), "当前的 owner/epoch 必须被接纳");
            assert_eq!(calls.load(Ordering::SeqCst), 1);

            // lease 级包装走同一条边界，epoch 直接取自 lease。
            let second = lease.dispatch_if_current(&[], || calls.fetch_add(1, Ordering::SeqCst));
            assert_eq!(second, Ok(1));
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert!(lease.is_current(), "派发本身不得改变输入所有权");
        }

        /// 判别性正面对照（"改前会交错"）：把**旧**写法（先 `is_current()`，中间可以任意等待，
        /// 再调用输入原语）跑在同一时序上——撤销会被接受，而输入**照样**被发出。
        /// 这条测试证明这套时序确实能暴露该窗口（不是空测试）；下一条测试证明新边界关掉了它。
        #[test]
        fn legacy_check_then_commit_window_admits_input_after_an_accepted_revocation() {
            let broker = InteractiveInputLeaseBroker::default();
            let scope = "legacy-window";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();

            // 第 1 步：检查（此刻为真）。
            let checked_current = lease.is_current();
            assert!(checked_current, "刚取得的 lease 必须是当前 owner");
            // 第 2 步：任意长时间等待（真实场景：落库 / 日志 / 调度）。撤销在这一窗口内被接受。
            assert!(broker.revoke_owner_for_test(scope, epoch), "撤销必须被接受");
            assert!(!lease.is_current(), "撤销确实已生效");
            // 第 3 步：旧写法只认第 1 步的结果，仍然提交输入。
            let mut legacy_input_calls = 0usize;
            if checked_current {
                legacy_input_calls += 1;
            }
            assert_eq!(
                legacy_input_calls, 1,
                "旧写法在'已撤销'之后仍然发出了输入——这正是本项要关闭的交错"
            );

            // 同一时序下，新边界一次都不会调用 dispatch。
            let mut boundary_input_calls = 0usize;
            let refused = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                boundary_input_calls += 1;
            });
            assert!(
                matches!(refused, Err(InputDispatchRefusal::ScopeRevoked { .. })),
                "实际 {refused:?}"
            );
            assert_eq!(boundary_input_calls, 0, "新边界下输入调用次数必须为零");
        }

        /// acquire 之后、派发之前注入撤销：`dispatch` 未被调用，返回撤销类拒绝；
        /// 三条撤销/释放路径（revoke / release / Drop）都必须如此。
        #[test]
        fn revocation_before_the_boundary_yields_zero_input_on_every_path() {
            let broker = InteractiveInputLeaseBroker::default();
            let calls = AtomicUsize::new(0);

            // (1) revoke_owner_for_test
            let scope = "revoked-before-boundary";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();
            assert!(broker.revoke_owner_for_test(scope, epoch));
            let refusal = broker
                .dispatch_if_current(scope, "run-a", epoch, &[], || {
                    calls.fetch_add(1, Ordering::SeqCst)
                })
                .expect_err("撤销之后必须拒绝派发");
            assert_eq!(
                refusal,
                InputDispatchRefusal::ScopeRevoked {
                    scope: scope.into(),
                    expected_epoch: epoch,
                }
            );
            assert!(refusal.is_lease_lost(), "必须被归类为'owner 已失效'");
            // lease 级包装同样拒绝。
            let refused = lease.dispatch_if_current(&[], || calls.fetch_add(1, Ordering::SeqCst));
            assert!(matches!(refused, Err(InputDispatchRefusal::ScopeRevoked { .. })));
            drop(lease);

            // (2) 显式 release
            let scope = "released-before-boundary";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();
            lease.release();
            let refused = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                calls.fetch_add(1, Ordering::SeqCst)
            });
            assert!(matches!(refused, Err(InputDispatchRefusal::ScopeRevoked { .. })));

            // (3) Drop
            let scope = "dropped-before-boundary";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();
            drop(lease);
            let refused = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                calls.fetch_add(1, Ordering::SeqCst)
            });
            assert!(matches!(refused, Err(InputDispatchRefusal::ScopeRevoked { .. })));

            assert_eq!(
                calls.load(Ordering::SeqCst),
                0,
                "失效之后输入调用次数必须为零（不得回退到无 lease 的底层原语）"
            );
        }

        /// 四类拒绝原因可区分：scope 已被撤销 / 非当前 owner / epoch 过期 / 重入（重入见下一条）。
        #[test]
        fn refusal_reasons_distinguish_owner_epoch_and_revocation() {
            let broker = InteractiveInputLeaseBroker::default();
            let scope = "refusal-reasons";
            let calls = AtomicUsize::new(0);
            let mut dispatch = || calls.fetch_add(1, Ordering::SeqCst);

            // scope 已被撤销（记录不存在）。
            let first = broker.acquire(scope, "run-a").expect("first lease");
            let first_epoch = first.owner_epoch();
            assert!(broker.revoke_owner_for_test(scope, first_epoch));
            let revoked = broker
                .dispatch_if_current(scope, "run-a", first_epoch, &[], &mut dispatch)
                .expect_err("scope 已被撤销必须被拒绝");
            assert_eq!(
                revoked,
                InputDispatchRefusal::ScopeRevoked {
                    scope: scope.into(),
                    expected_epoch: first_epoch,
                }
            );

            // 非当前 owner（记录存在，但 owner 不同）。
            let second = broker.acquire(scope, "run-b").expect("second lease");
            let second_epoch = second.owner_epoch();
            let wrong_owner = broker
                .dispatch_if_current(scope, "run-a", second_epoch, &[], &mut dispatch)
                .expect_err("非当前 owner 必须被拒绝");
            assert_eq!(
                wrong_owner,
                InputDispatchRefusal::NotCurrentOwner {
                    scope: scope.into(),
                    owner_id: "run-a".into(),
                    expected_epoch: second_epoch,
                    current_owner_id: "run-b".into(),
                    current_owner_epoch: second_epoch,
                }
            );
            assert!(wrong_owner.is_lease_lost());

            // epoch 过期（owner 相同，但 epoch 已被新取得者推进）。
            let stale_epoch = broker
                .dispatch_if_current(scope, "run-b", first_epoch, &[], &mut dispatch)
                .expect_err("过期 epoch 必须被拒绝");
            assert_eq!(
                stale_epoch,
                InputDispatchRefusal::StaleEpoch {
                    scope: scope.into(),
                    owner_id: "run-b".into(),
                    expected_epoch: first_epoch,
                    current_owner_epoch: second_epoch,
                }
            );
            assert!(stale_epoch.is_lease_lost());

            assert_eq!(calls.load(Ordering::SeqCst), 0, "全部拒绝路径都必须是零输入");
            assert!(
                !revoked.is_reentrant() && !wrong_owner.is_reentrant(),
                "这三条都不是重入"
            );
            assert_eq!(revoked.guard_label(), None);
            assert!(!revoked.to_string().is_empty(), "拒绝原因必须可读");
        }

        /// 调用方守卫：在同一临界区内求值，拒绝时 `dispatch` 一次都不调用，label 可区分。
        #[test]
        fn caller_guards_are_evaluated_inside_the_boundary() {
            let broker = InteractiveInputLeaseBroker::default();
            let scope = "guard-scope";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();
            let calls = AtomicUsize::new(0);
            let cancelled = Arc::new(AtomicBool::new(false));
            let cancelled_flag = Arc::clone(&cancelled);
            // 集成方要传进来的正是这种闭包：取消 / Quarantined / 恢复中 / 剩余预算。
            let cancel_check = move || !cancelled_flag.load(Ordering::SeqCst);

            let allow = InputDispatchGuard::new("cancelled", &cancel_check);
            assert_eq!(
                broker.dispatch_if_current(scope, "run-a", epoch, &[allow], || calls
                    .fetch_add(1, Ordering::SeqCst)
                    + 1),
                Ok(1),
                "守卫通过时必须派发"
            );

            cancelled.store(true, Ordering::SeqCst);
            let refuse = InputDispatchGuard::new("cancelled", &cancel_check);
            let refused = broker
                .dispatch_if_current(scope, "run-a", epoch, &[refuse], || {
                    calls.fetch_add(1, Ordering::SeqCst)
                })
                .expect_err("守卫不通过必须拒绝派发");
            assert_eq!(
                refused,
                InputDispatchRefusal::GuardRefused {
                    scope: scope.into(),
                    label: "cancelled".into(),
                }
            );
            assert_eq!(refused.guard_label(), Some("cancelled"));
            assert!(!refused.is_lease_lost(), "守卫拒绝不等于 lease 失效");
            assert!(!refused.is_reentrant(), "守卫拒绝不是重入");
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "守卫拒绝时 dispatch 调用次数必须为零（新增）"
            );

            // 顺序：所有权围栏优先于守卫（已失效的 token 一律报失效，不被守卫原因掩盖）。
            let allow = InputDispatchGuard::new("cancelled", &|| true);
            assert!(broker.revoke_owner_for_test(scope, epoch));
            let refused = broker.dispatch_if_current(scope, "run-a", epoch, &[allow], || {
                calls.fetch_add(1, Ordering::SeqCst)
            });
            assert!(matches!(refused, Err(InputDispatchRefusal::ScopeRevoked { .. })));
            assert_eq!(calls.load(Ordering::SeqCst), 1);
        }

        /// 守卫与派发同处一个临界区：临界区**内部**发起的撤销无法插进"守卫通过"与"派发"之间
        /// （它被延迟到本次派发返回之后才生效）。因此"立刻别发这次输入"的正确入口是守卫返回
        /// `false`，而不是撤销。
        #[test]
        fn revocation_requested_inside_the_boundary_cannot_slip_between_guard_and_dispatch() {
            let broker = InteractiveInputLeaseBroker::default();
            let scope = "guard-revokes-inside";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();
            let calls = AtomicUsize::new(0);

            let revoke_inside = || {
                assert!(
                    broker.revoke_owner_for_test(scope, epoch),
                    "临界区内的撤销请求必须被接受（延迟生效）"
                );
                true
            };
            let guard = InputDispatchGuard::new("revoke-from-guard", &revoke_inside);
            let outcome =
                broker.dispatch_if_current(scope, "run-a", epoch, &[guard], || {
                    calls.fetch_add(1, Ordering::SeqCst) + 1
                });
            assert_eq!(
                outcome,
                Ok(1),
                "撤销是在临界区内发起的，无法阻断已被接纳的本次派发（分支：输入已开始，撤销只能阻止后续输入）"
            );
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert!(
                !lease.is_current(),
                "延迟的撤销必须在派发返回之前由同一临界区补做（不得丢撤销）"
            );
            let post = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                calls.fetch_add(1, Ordering::SeqCst)
            });
            assert!(matches!(post, Err(InputDispatchRefusal::ScopeRevoked { .. })));
            assert_eq!(calls.load(Ordering::SeqCst), 1, "撤销之后不得再派发新输入");
        }

        /// 选定的"派发中撤销"语义：撤销请求与复核/派发共用同一临界区，因此表现为
        /// **阻塞到 `dispatch` 返回**，之后才生效；已经发生在途的输入**不**被改记为 `not_sent`
        /// （`dispatch_if_current` 的返回值不被改写），只阻止后续新输入。
        #[test]
        fn revoke_requested_while_dispatching_blocks_until_dispatch_returns() {
            let outcome = run_with_timeout(
                "派发中撤销的阻塞语义",
                Duration::from_secs(30),
                || {
                    let broker = Arc::new(InteractiveInputLeaseBroker::default());
                    let scope = "dispatch-in-flight-revoke";
                    let lease = broker.acquire(scope, "run-a").expect("lease");
                    let epoch = lease.owner_epoch();
                    let dispatched = Arc::new(AtomicUsize::new(0));
                    let revoke_submitted = Arc::new(AtomicBool::new(false));
                    let revoke_returned = Arc::new(AtomicBool::new(false));
                    let revoke_accepted = Arc::new(AtomicBool::new(false));

                    let (entered_tx, entered_rx) = mpsc::channel::<()>();
                    let revoker = {
                        let broker = Arc::clone(&broker);
                        let submitted = Arc::clone(&revoke_submitted);
                        let returned = Arc::clone(&revoke_returned);
                        let accepted = Arc::clone(&revoke_accepted);
                        thread::spawn(move || {
                            entered_rx
                                .recv_timeout(Duration::from_secs(10))
                                .expect("派发必须先进入临界区");
                            submitted.store(true, Ordering::SeqCst);
                            if broker.revoke_owner_for_test(scope, epoch) {
                                accepted.store(true, Ordering::SeqCst);
                            }
                            returned.store(true, Ordering::SeqCst);
                        })
                    };

                    let started = std::time::Instant::now();
                    let result = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                        dispatched.fetch_add(1, Ordering::SeqCst);
                        entered_tx.send(()).expect("通知撤销线程");
                        let deadline = std::time::Instant::now() + Duration::from_secs(5);
                        while !revoke_submitted.load(Ordering::SeqCst) {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "撤销线程没有按期发起撤销"
                            );
                            thread::yield_now();
                        }
                        thread::sleep(Duration::from_millis(150));
                        // 关键证据：撤销已**发起**却仍未返回——锁在我们手里，它插不进来。
                        assert!(
                            !revoke_accepted.load(Ordering::SeqCst),
                            "撤销不得在派发临界区内生效"
                        );
                        assert!(
                            !revoke_returned.load(Ordering::SeqCst),
                            "撤销必须阻塞到本次派发返回（而不是插进复核与派发之间）"
                        );
                        epoch
                    });
                    let elapsed = started.elapsed();
                    assert_eq!(
                        result,
                        Ok(epoch),
                        "在途输入已被正式接纳，返回值不得被随后的撤销改写"
                    );
                    assert!(elapsed >= Duration::from_millis(150), "实际 {elapsed:?}");

                    revoker.join().expect("撤销线程不得 panic");
                    assert!(revoke_returned.load(Ordering::SeqCst));
                    assert!(
                        revoke_accepted.load(Ordering::SeqCst),
                        "撤销最终必须被接受并生效"
                    );
                    assert!(!lease.is_current(), "撤销在派发返回之后必须可见");
                    assert_eq!(dispatched.load(Ordering::SeqCst), 1);
                    let post =
                        broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                            dispatched.fetch_add(1, Ordering::SeqCst)
                        });
                    assert_eq!(
                        post,
                        Err(InputDispatchRefusal::ScopeRevoked {
                            scope: scope.into(),
                            expected_epoch: epoch,
                        }),
                        "撤销之后同一 token 不得再派发新输入"
                    );
                    assert_eq!(dispatched.load(Ordering::SeqCst), 1);
                    assert!(
                        broker.acquire(scope, "run-next").is_ok(),
                        "撤销之后 scope 必须可被新 owner 取得"
                    );
                },
            );
            let _ = outcome;
        }

        /// 重入：`dispatch`/守卫回调再进 broker 必须得到**明确错误**而不是自锁（整条测试有超时保护）。
        #[test]
        fn reentrant_broker_calls_are_refused_instead_of_self_deadlocking() {
            run_with_timeout("重入检测", Duration::from_secs(30), || {
                let broker = InteractiveInputLeaseBroker::default();
                let scope = "reentrant-scope";
                let other_scope = "reentrant-other-scope";
                let lease = broker.acquire(scope, "run-a").expect("lease");
                let epoch = lease.owner_epoch();
                let other_lease = broker.acquire(other_scope, "run-b").expect("other lease");

                let value = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                    // (1) 重入同一条边界 → Reentrant（不是死锁）。
                    let nested = broker.dispatch_if_current(scope, "run-a", epoch, &[], || 0);
                    assert!(
                        matches!(
                            &nested,
                            Err(InputDispatchRefusal::Reentrant { dispatching_scope, .. })
                                if dispatching_scope == scope
                        ),
                        "实际 {nested:?}"
                    );

                    // (2) 重入 acquire → 立刻返回 busy，且标明是重入。
                    let busy = broker
                        .acquire("reentrant-third-scope", "run-c")
                        .expect_err("重入 acquire 必须返回错误而不是自锁");
                    assert_eq!(busy.reentrant_dispatching_scope.as_deref(), Some(scope));
                    assert!(
                        busy.to_string().contains("reentrant"),
                        "Display 必须点明重入：{busy}"
                    );

                    // (3) 重入 is_current → fail-closed 的 false（绝不放行输入）。
                    assert!(!lease.is_current());
                    assert!(!other_lease.is_current());

                    // (4) 重入 revoke → 被接受，由本次派发返回前补做（不死锁、不丢撤销）。
                    assert!(broker.revoke_owner_for_test(scope, epoch));

                    // (5) 重入 release/Drop → 同样延迟到返回前生效。
                    drop(other_lease);

                    7usize
                });

                assert_eq!(value, Ok(7), "返回值不得被撤销改写");
                assert!(
                    !lease.is_current(),
                    "延迟的撤销必须在派发返回前由同一临界区补做"
                );
                let post = broker.dispatch_if_current(scope, "run-a", epoch, &[], || 0);
                assert!(matches!(post, Err(InputDispatchRefusal::ScopeRevoked { .. })));
                assert!(broker.acquire(scope, "run-next").is_ok(), "scope 必须已被释放");
                assert!(
                    broker.acquire(other_scope, "run-next").is_ok(),
                    "延迟的 release 也必须已经生效"
                );
            });
        }

        /// `dispatch` panic 不得把 broker 毒化成"永久不可用"，且临界区内发起的释放仍然生效。
        #[test]
        fn panicking_dispatch_does_not_poison_the_broker_and_still_applies_deferred_removals() {
            let broker = InteractiveInputLeaseBroker::default();
            let scope = "panic-dispatch";
            let other_scope = "panic-other-scope";
            let lease = broker.acquire(scope, "run-a").expect("lease");
            let epoch = lease.owner_epoch();
            let other = broker.acquire(other_scope, "run-b").expect("other lease");

            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = broker.dispatch_if_current(scope, "run-a", epoch, &[], || {
                    drop(other);
                    panic!("dispatch body panicked");
                });
            }));
            assert!(panicked.is_err(), "dispatch panic 必须照常向外传播");

            // 中毒容忍 + 延迟移除补做：另一个 scope 必须已经释放，本 scope 仍由本 owner 持有。
            assert!(
                broker.acquire(other_scope, "run-next").is_ok(),
                "panic 期间发起的 release 必须已经生效"
            );
            assert!(lease.is_current(), "panic 不得撤销仍然有效的 owner");
            assert_eq!(
                broker
                    .acquire(scope, "run-next")
                    .expect_err("owner 仍然有效")
                    .owner_id,
                "run-a"
            );
            assert_eq!(
                broker.dispatch_if_current(scope, "run-a", epoch, &[], || 5u8),
                Ok(5),
                "panic 之后 broker 必须仍然可用（不得因中毒永久失效）"
            );
        }

        /// 并发压力：多线程反复 acquire / dispatch / revoke，断言**不出现"已撤销却仍派发成功"**。
        ///
        /// 每轮按 `round % 3` 取三种时序，**两种顺序都被确定性地覆盖**，第三种是真竞争：
        /// * `mode 0`：撤销**先被接受并返回**，然后才发起派发 → 必须被拒、输入调用次数为零；
        /// * `mode 1`：派发先进入临界区（撤销在临界区内才被发起）→ 必须是 `Ok`，且派发的**决策**
        ///   早于撤销返回（ticket 序），已经发生的输入不被改记；
        /// * `mode 2`：两边同时起跑，真竞争 → 只断言不变量（不出现"已撤销却仍派发"、被拒即零输入、
        ///   成功即决策早于撤销返回），并计数两种结果各出现多少次。
        ///
        /// 每轮的 epoch 与派发调用次数都被记录，轮末还必须验证"成功过的 epoch 不得复活"。
        #[test]
        fn stress_concurrent_revoke_and_dispatch_never_dispatch_after_an_accepted_revocation() {
            const THREADS: usize = 4;
            const ROUNDS: usize = 120;
            const TEN_SECONDS: Duration = Duration::from_secs(10);

            /// 每轮共享的事实（撤销线程与派发线程各写一半）。
            #[derive(Default)]
            struct RoundFacts {
                ticket: AtomicU64,
                dispatch_calls: AtomicUsize,
                decision_ticket: AtomicU64,
                revoke_return_ticket: AtomicU64,
                dispatched_after_accept: AtomicBool,
                revoke_accepted: AtomicBool,
            }

            impl RoundFacts {
                fn next_ticket(&self) -> u64 {
                    self.ticket.fetch_add(1, Ordering::SeqCst) + 1
                }
            }

            let broker = Arc::new(InteractiveInputLeaseBroker::default());
            let mut totals = (0usize, 0usize, 0usize); // (派发成功, 被拒, 撤销被接受)

            for thread_index in 0..THREADS {
                let broker = Arc::clone(&broker);
                let (dispatched, refused, accepted, raced) = thread::spawn(move || {
                    let mut dispatched_total = 0usize;
                    let mut refused_total = 0usize;
                    let mut accepted_total = 0usize;
                    let mut raced_rounds = 0usize;

                    for round_index in 0..ROUNDS {
                        // 每轮独立 scope：排除"别人持有"这种与本题无关的噪音。
                        let mode = round_index % 3;
                        let scope = format!("stress-{thread_index}-{round_index}");
                        let owner = format!("run-{thread_index}");
                        let lease = broker.acquire(scope.clone(), owner.clone()).expect("acquire");
                        let epoch = lease.owner_epoch();
                        let facts = Arc::new(RoundFacts::default());

                        let (entered_tx, entered_rx) = mpsc::channel::<()>();
                        let barrier = Arc::new(Barrier::new(2));
                        let revoker = {
                            let broker = Arc::clone(&broker);
                            let scope = scope.clone();
                            let facts = Arc::clone(&facts);
                            let barrier = Arc::clone(&barrier);
                            thread::spawn(move || {
                                match mode {
                                    // mode 0：立刻撤销。
                                    0 => {}
                                    // mode 1：等派发进入临界区之后再发起撤销（此时它必然被阻塞）。
                                    1 => {
                                        let _ = entered_rx.recv_timeout(TEN_SECONDS);
                                    }
                                    // mode 2：与派发同时起跑。
                                    _ => {
                                        barrier.wait();
                                    }
                                }
                                if broker.revoke_owner_for_test(&scope, epoch) {
                                    facts.revoke_accepted.store(true, Ordering::SeqCst);
                                }
                                facts
                                    .revoke_return_ticket
                                    .store(facts.next_ticket(), Ordering::SeqCst);
                            })
                        };
                        let mut revoker = Some(revoker);
                        if mode == 0 {
                            // 撤销必须先被接受（返回）之后才发起派发。
                            revoker
                                .take()
                                .expect("revoker")
                                .join()
                                .expect("撤销线程不得 panic");
                        } else if mode == 2 {
                            barrier.wait();
                        }

                        let outcome = broker.dispatch_if_current(
                            &scope,
                            &owner,
                            epoch,
                            &[],
                            || {
                                facts.dispatch_calls.fetch_add(1, Ordering::SeqCst);
                                let _ = entered_tx.send(());
                                // 撤销线程只在 revoke 返回后才置位；而本闭包在临界区内运行，
                                // 撤销此时不可能已返回 —— 这里若为真即为"已撤销却仍派发"。
                                if facts.revoke_accepted.load(Ordering::SeqCst) {
                                    facts.dispatched_after_accept.store(true, Ordering::SeqCst);
                                }
                                facts
                                    .decision_ticket
                                    .store(facts.next_ticket(), Ordering::SeqCst);
                                epoch
                            },
                        );
                        if let Some(revoker) = revoker.take() {
                            revoker.join().expect("撤销线程不得 panic");
                        }

                        assert!(
                            !facts.dispatched_after_accept.load(Ordering::SeqCst),
                            "scope {scope} (mode {mode}): 撤销已被接受，却在同一临界区内仍然派发了输入"
                        );
                        assert!(
                            facts.dispatch_calls.load(Ordering::SeqCst) <= 1,
                            "scope {scope}: 每轮最多一次输入调用"
                        );
                        assert!(
                            facts.revoke_accepted.load(Ordering::SeqCst),
                            "scope {scope}: 撤销时记录必须仍在（否则说明记录被别的原因清掉了）"
                        );
                        accepted_total += 1;
                        let calls = facts.dispatch_calls.load(Ordering::SeqCst);
                        let decision = facts.decision_ticket.load(Ordering::SeqCst);
                        let revoked = facts.revoke_return_ticket.load(Ordering::SeqCst);
                        match outcome {
                            Ok(observed_epoch) => {
                                assert_ne!(
                                    mode, 0,
                                    "scope {scope}: 撤销先被接受，输入绝不允许派发"
                                );
                                assert_eq!(observed_epoch, epoch);
                                assert_eq!(calls, 1);
                                assert!(
                                    decision >= 1 && decision < revoked,
                                    "scope {scope}: 派发成功说明本次输入已被接纳并开始派发，\
                                     撤销只能发生在此之后（decision={decision} revoke={revoked}）"
                                );
                                dispatched_total += 1;
                            }
                            Err(refusal) => {
                                assert_ne!(
                                    mode, 1,
                                    "scope {scope}: 派发先进入临界区，撤销不得插进决策之前：{refusal:?}"
                                );
                                assert!(
                                    matches!(refusal, InputDispatchRefusal::ScopeRevoked { .. }),
                                    "被拒原因必须只有'已被撤销'一类，实际 {refusal:?}"
                                );
                                assert_eq!(
                                    calls, 0,
                                    "scope {scope} (mode {mode}): 被拒时输入调用次数必须为零"
                                );
                                refused_total += 1;
                            }
                        }
                        if mode == 2 {
                            raced_rounds += 1;
                        }
                        // 轮末：成功过的 epoch 不得复活，记录必须已经消失。
                        let post = broker.dispatch_if_current(&scope, &owner, epoch, &[], || 0);
                        assert!(
                            matches!(post, Err(InputDispatchRefusal::ScopeRevoked { .. })),
                            "scope {scope}: 轮末同一 token 不得再派发，实际 {post:?}"
                        );
                        drop(lease);
                    }
                    (
                        dispatched_total,
                        refused_total,
                        accepted_total,
                        raced_rounds,
                    )
                })
                .join()
                .expect("压力线程不得 panic");

                totals.0 += dispatched;
                totals.1 += refused;
                totals.2 += accepted;
                println!(
                    "[stress thread {thread_index}] dispatched={dispatched} refused={refused} revoke_accepted={accepted} raced_rounds={raced}"
                );
            }

            assert_eq!(
                totals.0 + totals.1,
                THREADS * ROUNDS,
                "每轮恰好一个结果（派发成功或被拒）"
            );
            assert_eq!(
                totals.2,
                THREADS * ROUNDS,
                "每轮的撤销都必须在记录仍在时生效"
            );
            // 两种顺序都必须被覆盖到（mode 0 全部被拒、mode 1 全部成功，另有真竞争轮）。
            assert!(
                totals.0 >= THREADS * (ROUNDS / 3),
                "mode 1（派发先进入临界区）必须产生成功的派发，实际 {}",
                totals.0
            );
            assert!(
                totals.1 >= THREADS * (ROUNDS / 3),
                "mode 0（撤销先被接受）必须产生零输入拒绝，实际 {}",
                totals.1
            );
            println!(
                "[stress summary] threads={THREADS} rounds={ROUNDS} dispatched={} refused={} revoke_accepted={}",
                totals.0, totals.1, totals.2
            );
        }

        /// 组合所有权（进程内 epoch + 跨进程内核对象）走的是同一条决策边界。
        #[test]
        fn scoped_ownership_boundary_refuses_after_lease_revocation_with_zero_input() {
            let broker = InteractiveInputLeaseBroker::default();
            let scope = probe_scope("dispatch-boundary");
            let ownership =
                ScopedInputOwnership::acquire(&broker, &scope, "run-a", ACQUIRE_TIMEOUT)
                    .expect("组合取得");
            let epoch = ownership.owner_epoch();
            let calls = AtomicUsize::new(0);

            assert_eq!(
                ownership.dispatch_if_current(&[], || calls.fetch_add(1, Ordering::SeqCst) + 1),
                Ok(1),
                "当前 owner 必须被接纳"
            );
            assert!(broker.revoke_owner_for_test(&scope, epoch));
            let refused = ownership.dispatch_if_current(&[], || calls.fetch_add(1, Ordering::SeqCst));
            assert_eq!(
                refused,
                Err(InputDispatchRefusal::ScopeRevoked {
                    scope: scope.clone(),
                    expected_epoch: epoch,
                })
            );
            assert_eq!(
                calls.load(Ordering::SeqCst),
                1,
                "失效之后输入调用次数必须为零（新增的 1 次是失效前的）"
            );
        }
    }
}
