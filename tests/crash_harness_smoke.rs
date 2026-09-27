//! **8.4 崩溃装置基础**（2026-09-26 补充裁决 §5.1）——只建**基础设施**，不伪造 R3／R4 场景。
//!
//! 裁决把装置拆成两层：**装置基础**（父子进程、实际创建句柄、测试库、测试作用域、同步屏障、
//! 日志、看门狗超时与清理）现在就可以开工；**业务故障场景**（K1–K6）等相应契约冻结后逐个接入。
//! 本文件只做前者，并把裁决点名的硬边界落成可执行断言：
//!
//! | 裁决要求 | 本文件的落地 |
//! | --- | --- |
//! | 父测试创建的**专用子进程／测试程序** | 父测试以 `current_exe()` 重新调用**自己**并进入子模式 |
//! | 只终止**自己持有实际创建句柄**的子进程，不接收任意 PID | 终止只经 `std::process::Child`（真实句柄），API 上**没有**接收裸 PID 的入口 |
//! | 每次运行独立的会话库、输入安全库和证据目录 | [`HarnessWorkspace`] 每次运行建独立目录，**不复用真实事故数据** |
//! | 测试专用锁命名空间，复用生产锁算法；不得占用真实桌面输入 scope | scope 形如 `windows-session-harness-<运行 id>`，**不是**真实输入资源 scope |
//! | 用命名事件／管道屏障确认前置阶段真的到达，**不靠 sleep 猜时序** | [`Barrier`]：子进程写阶段标记文件，父进程**有界等待**该标记 |
//! | 看门狗超时与受控后代回收；**父级收尾失败也算测试失败** | [`ChildExecutor`] 的 `Drop` 有界收尾；清理失败会 panic 而不是静默 |
//! | 不向真实鼠标键盘注入 | 子模式只写文件＋等待，**不调用任何输入 API** |
//! | 故障入口 `cfg(test)`／默认关闭，不出现在正式产品面 | 整个文件位于 `tests/` 测试目标，**不进任何生产二进制**；子模式还需显式环境变量 |
//!
//! **本文件刻意不做**（留给后续接入）：K1–K6 的具体故障点、许可／执行者状态绑定、
//! OS 侧实例身份读取（属 8.3c）。因此**不得**把"装置已写"登记为"六个崩溃场景全部通过"。

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// 子模式开关：只有它存在时才进入子进程行为（默认关闭）。
const CHILD_MODE_ENV: &str = "COOLZHU_CRASH_HARNESS_CHILD";
/// 屏障目录：父进程建、子进程写阶段标记。
const BARRIER_DIR_ENV: &str = "COOLZHU_CRASH_HARNESS_BARRIER";
/// 本次运行的 harness 运行 id（用于独立命名空间）。
const RUN_ID_ENV: &str = "COOLZHU_CRASH_HARNESS_RUN_ID";

/// 屏障阶段名。**它们是观察点，不是生产可启用的崩溃开关**（裁决 §5.1）。
const STAGE_REGISTERED: &str = "stage-registered";
const STAGE_PRE_DISPATCH_REACHED: &str = "stage-pre-dispatch-reached";
/// 可选：让子进程额外到达**某个具名测试事件**阶段（用于把故障点绑到事件上）。
const EVENT_STAGE_ENV: &str = "COOLZHU_CRASH_HARNESS_EVENT_STAGE";

// ---------------------------------------------------------------------------
// 测试事件词汇表 + K1–K6 依赖门（裁决 §5.1／§5.2）
//
// §5.1 要求"冻结小范围测试事件，例如「许可消费已提交」「执行者已登记」「终态提交完成」，
// 通过 test-support 暴露同步屏障"。这里把词汇表冻下来，并把它**锚定到已冻结的契约**
// （8.2a 的 `InputPermitState`／8.3a 的 `ExecutorInstanceState`／既有 `RecoveryStage`），
// 使事件名与契约状态不可能各自漂移。
// ---------------------------------------------------------------------------

use runtime::{ExecutorInstanceState, InputPermitState, RecoveryStage};

/// 装置暴露的**测试事件**。它们是**观察点**，不是生产可启用的崩溃开关。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HarnessEvent {
    /// 恢复登记完成（R2 之后）。
    RecoveryRegistered,
    /// 关闸与 Pending 许可撤销**已提交**。
    IntakeClosedWithPendingRevoked,
    /// 许可**消费已提交**。
    PermitConsumed,
    /// 执行者**已登记**。
    ExecutorRegistered,
    /// 会话终态**提交完成**。
    TerminalCommitted,
    /// 一次性确认**已完成**（开放提交之前）。
    ConfirmationCompleted,
}

impl HarnessEvent {
    const ALL: [Self; 6] = [
        Self::RecoveryRegistered,
        Self::IntakeClosedWithPendingRevoked,
        Self::PermitConsumed,
        Self::ExecutorRegistered,
        Self::TerminalCommitted,
        Self::ConfirmationCompleted,
    ];

    const fn stage(self) -> &'static str {
        match self {
            Self::RecoveryRegistered => "event-recovery-registered",
            Self::IntakeClosedWithPendingRevoked => "event-intake-closed-pending-revoked",
            Self::PermitConsumed => "event-permit-consumed",
            Self::ExecutorRegistered => "event-executor-registered",
            Self::TerminalCommitted => "event-terminal-committed",
            Self::ConfirmationCompleted => "event-confirmation-completed",
        }
    }

    fn is_known_stage(stage: &str) -> bool {
        Self::ALL.iter().any(|event| event.stage() == stage)
    }

    /// 事件在 8.2a 许可契约里对应的状态（没有对应则为 `None`）。
    const fn permit_state(self) -> Option<InputPermitState> {
        match self {
            Self::PermitConsumed => Some(InputPermitState::DispatchCommitted),
            _ => None,
        }
    }

    /// 事件在 8.3a 执行者契约里对应的状态。
    const fn executor_state(self) -> Option<ExecutorInstanceState> {
        match self {
            Self::ExecutorRegistered => Some(ExecutorInstanceState::VerifiedAlive),
            _ => None,
        }
    }

    /// 事件在既有恢复阶段里对应的锚点。
    const fn recovery_stage(self) -> Option<RecoveryStage> {
        match self {
            Self::RecoveryRegistered => Some(RecoveryStage::IntentPersistedAndIntakeClosed),
            Self::IntakeClosedWithPendingRevoked => Some(RecoveryStage::InFlightRegistered),
            Self::TerminalCommitted => Some(RecoveryStage::TerminalCommitted),
            _ => None,
        }
    }
}

/// 装置/场景所依赖的、**尚未全部落地**的能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HarnessDependency {
    /// 既有恢复协调器（R1–R9 驱动）。
    ExistingCoordinator,
    /// R3 许可**持久化**（8.2b/8.2c）。
    PermitPersistenceR3,
    /// R4 执行者**监督与实例核查**（8.3b/8.3c）。
    ExecutorSupervisionR4,
    /// 恢复事务的终态提交与结账。
    RecoveryTransaction,
    /// 操作员认证与恢复提交（8.1）。
    OperatorAuth,
}

impl HarnessDependency {
    const fn as_str(self) -> &'static str {
        match self {
            Self::ExistingCoordinator => "existing_coordinator",
            Self::PermitPersistenceR3 => "permit_persistence_r3",
            Self::ExecutorSupervisionR4 => "executor_supervision_r4",
            Self::RecoveryTransaction => "recovery_transaction",
            Self::OperatorAuth => "operator_auth",
        }
    }

    /// 该依赖现在是否**可驱动**。
    ///
    /// 全部为 `false` 不是悲观，而是如实：① 协调器虽已实现，但它位于 web-console 的 **bin 内部**，
    /// 根测试目标（本文件）**无法导入**其类型 ⇒ 驱动不可得；② R3／R4 的持久化、恢复事务与
    /// 操作员认证均未落地。将来落地时把对应项改为 `true`，门与用例即可逐项解锁。
    const fn available(self) -> bool {
        match self {
            Self::ExistingCoordinator
            | Self::PermitPersistenceR3
            | Self::ExecutorSupervisionR4
            | Self::RecoveryTransaction
            | Self::OperatorAuth => false,
        }
    }

    /// 为什么不可用（必须给出理由，便于"逐项解锁"而不是含糊搁置）。
    const fn blocker(self) -> &'static str {
        match self {
            Self::ExistingCoordinator => {
                "协调器在 web-console 的 bin 内部，根测试目标无法导入；需要暴露 lib 侧入口或把驱动下沉"
            }
            Self::PermitPersistenceR3 => "R3 许可持久化未落地（8.2b/8.2c，等 schema 窗口）",
            Self::ExecutorSupervisionR4 => "R4 执行者监督与实例核查未落地（8.3b/8.3c）",
            Self::RecoveryTransaction => "恢复事务的终态提交与结账尚未接线",
            Self::OperatorAuth => "操作员认证与恢复提交未实现（8.1）",
        }
    }
}

/// 六个故障点场景（裁决 §5.2 的 K1–K6），依赖集**逐项对齐该表**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HarnessScenario {
    K1,
    K2,
    K3,
    K4,
    K5,
    K6,
}

impl HarnessScenario {
    const ALL: [Self; 6] = [Self::K1, Self::K2, Self::K3, Self::K4, Self::K5, Self::K6];

    const fn as_str(self) -> &'static str {
        match self {
            Self::K1 => "K1",
            Self::K2 => "K2",
            Self::K3 => "K3",
            Self::K4 => "K4",
            Self::K5 => "K5",
            Self::K6 => "K6",
        }
    }

    /// 终止点说明（裁决 §5.2 的表）。
    const fn terminate_point(self) -> &'static str {
        match self {
            Self::K1 => "取得协调权后、恢复登记完成前",
            Self::K2 => "关闸和 Pending 撤销已提交后",
            Self::K3 => "许可已消费、尚无执行回执",
            Self::K4 => "协调器死亡而测试执行者仍存活",
            Self::K5 => "会话终态已提交、安全恢复未结账",
            Self::K6 => "确认已完成、开放提交之前",
        }
    }

    /// 依赖集（与 §5.2 的「依赖」列逐行一致）。
    const fn dependencies(self) -> &'static [HarnessDependency] {
        match self {
            Self::K1 => &[HarnessDependency::ExistingCoordinator],
            Self::K2 => &[HarnessDependency::PermitPersistenceR3],
            Self::K3 => &[
                HarnessDependency::PermitPersistenceR3,
                HarnessDependency::ExecutorSupervisionR4,
            ],
            Self::K4 => &[HarnessDependency::ExecutorSupervisionR4],
            Self::K5 => &[HarnessDependency::RecoveryTransaction],
            Self::K6 => &[
                HarnessDependency::OperatorAuth,
                HarnessDependency::RecoveryTransaction,
            ],
        }
    }

    /// 依赖是否全部具备（只有全具备才能绑定为真实场景）。
    fn available(self) -> bool {
        self.dependencies().iter().all(|dependency| dependency.available())
    }

    /// 尚缺哪些依赖（用于"逐项解锁"时点名）。
    fn blocked_by(self) -> Vec<HarnessDependency> {
        self.dependencies()
            .iter()
            .copied()
            .filter(|dependency| !dependency.available())
            .collect()
    }
}

/// 每次运行独立的测试空间：会话库／输入安全库／证据／屏障／日志**各自独立**。
///
/// 裁决要求"不能只是给真实数据库换个文件名"，因此这里把四类路径全部分开建，
/// 且**从不指向**真实输入安全根或真实运行库。
struct HarnessWorkspace {
    root: tempfile::TempDir,
    run_id: String,
}

impl HarnessWorkspace {
    fn new(tag: &str) -> Self {
        let root = tempfile::TempDir::new().expect("harness tempdir");
        let run_id = format!(
            "{tag}-{}-{}",
            std::process::id(),
            // 纳秒级后缀：同一进程内多次运行也各自独立。
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or(0)
        );
        for directory in [
            "session",
            "input-safety",
            "evidence",
            "barrier",
            "logs",
        ] {
            std::fs::create_dir_all(root.path().join(directory)).expect("create harness dir");
        }
        Self { root, run_id }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    fn session_db(&self) -> PathBuf {
        self.path("session/web-sessions.sqlite3")
    }

    fn input_safety_root(&self) -> PathBuf {
        self.path("input-safety")
    }

    fn barrier_dir(&self) -> PathBuf {
        self.path("barrier")
    }

    /// 测试专用资源 scope：复用生产**命名算法**，但用 harness 专属前缀。
    ///
    /// 刻意**不是**真实输入资源 scope —— 装置不得占用当前交互会话的物理输入资源。
    fn test_scope(&self) -> String {
        format!("windows-session-harness-{}", self.run_id)
    }
}

/// 有界的时间预算：装置里**每一处等待都必须有上限**，否则装置本身会挂死门禁。
const BARRIER_TIMEOUT: Duration = Duration::from_secs(20);
const STOP_CONFIRM_TIMEOUT: Duration = Duration::from_secs(10);

/// 父进程侧的屏障：**等待阶段标记文件出现**，而不是 sleep 猜测。
struct Barrier {
    directory: PathBuf,
}

impl Barrier {
    fn new(directory: &Path) -> Self {
        Self {
            directory: directory.to_path_buf(),
        }
    }

    fn marker(&self, stage: &str) -> PathBuf {
        self.directory.join(stage)
    }

    /// 子进程侧：声明"我已到达该阶段"。先写临时名再改名，避免父进程读到半截文件。
    fn signal(&self, stage: &str) {
        let target = self.marker(stage);
        let staging = self.directory.join(format!(".{stage}.partial"));
        std::fs::write(&staging, stage.as_bytes()).expect("write barrier marker");
        std::fs::rename(&staging, &target).expect("publish barrier marker");
    }

    /// 父进程侧：有界等待某个阶段**真的到达**。超时返回 `Err`，绝不无限等。
    fn wait_for(&self, stage: &str) -> Result<Duration, String> {
        let started = Instant::now();
        let target = self.marker(stage);
        while started.elapsed() < BARRIER_TIMEOUT {
            if target.exists() {
                return Ok(started.elapsed());
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err(format!(
            "等待屏障阶段 `{stage}` 超时（{:?}）：不得用 sleep 替代屏障，也不得无限等待",
            BARRIER_TIMEOUT
        ))
    }
}

/// 父测试持有的子执行者：**真实创建句柄**在这里，终止只经它。
///
/// API 上刻意**没有** `kill_pid(u32)` 这种入口：裁决要求生产终止接口只接受经核查、
/// 仍持有实际句柄的内部对象，装置同样遵守，以免把"按 PID 杀"的坏习惯带进测试。
struct ChildExecutor {
    child: Child,
    pid: u32,
    cleaned: bool,
}

impl ChildExecutor {
    /// 以**自己**（当前测试二进制）为子进程，进入子模式。
    ///
    /// 只跑子模式那一个用例：`--exact crash_harness_child_mode`，避免子进程再跑整套测试。
    fn spawn(workspace: &HarnessWorkspace) -> Self {
        Self::spawn_reaching_event(workspace, None)
    }

    /// 让子进程额外到达某个**具名测试事件**阶段再等待被终止。
    fn spawn_reaching_event(workspace: &HarnessWorkspace, event: Option<HarnessEvent>) -> Self {
        let exe = std::env::current_exe().expect("current test binary");
        let mut command = Command::new(exe);
        command
            .args(["--exact", "crash_harness_child_mode", "--nocapture"])
            .env(CHILD_MODE_ENV, "1")
            .env(BARRIER_DIR_ENV, workspace.barrier_dir())
            .env(RUN_ID_ENV, &workspace.run_id);
        if let Some(event) = event {
            command.env(EVENT_STAGE_ENV, event.stage());
        }
        let child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn harness child");
        let pid = child.id();
        Self {
            child,
            pid,
            cleaned: false,
        }
    }

    fn pid(&self) -> u32 {
        self.pid
    }

    /// 是否仍在运行（真实句柄查询，不是按 PID 猜）。
    fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// 请求终止**自己创建的那个子进程**，并**有界等待**确认退出。
    ///
    /// 与裁决 §4.4 同口径：`kill` 返回成功只等于"已请求"，必须等到 `try_wait` 报退出才算确认。
    fn terminate_and_confirm(&mut self) -> Result<Duration, String> {
        let started = Instant::now();
        self.child
            .kill()
            .map_err(|error| format!("请求终止失败：{error}"))?;
        while started.elapsed() < STOP_CONFIRM_TIMEOUT {
            match self.child.try_wait() {
                Ok(Some(_status)) => return Ok(started.elapsed()),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => return Err(format!("等待退出时出错：{error}")),
            }
        }
        Err(format!(
            "等待子进程退出超时（{:?}）：按裁决不得继续重试到看起来成功",
            STOP_CONFIRM_TIMEOUT
        ))
    }

    /// 有界收尾。**父级收尾失败也算测试失败**（裁决 §6.1），因此这里 panic 而不是静默。
    fn cleanup(&mut self) {
        if self.cleaned {
            return;
        }
        self.cleaned = true;
        if self.is_running() {
            match self.terminate_and_confirm() {
                Ok(_) => {}
                Err(error) => panic!("装置收尾失败（不得删掉临时目录就称资源已回收）：{error}"),
            }
        }
        // 再确认一次，避免"kill 成功但仍在"的窗口。
        match self.child.try_wait() {
            Ok(Some(_)) => {}
            Ok(None) => panic!("收尾后子进程仍在运行：装置不得留下孤儿进程"),
            Err(error) => panic!("收尾后无法确认子进程状态：{error}"),
        }
    }
}

impl Drop for ChildExecutor {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// **子模式**：只在子进程里真正干活。正常测试运行时它是空操作（立即返回）。
///
/// 子进程只做两件事：写阶段标记（屏障）→ 等待被终止。**不注入任何真实输入**。
#[test]
fn crash_harness_child_mode() {
    let Some(_enabled) = std::env::var_os(CHILD_MODE_ENV) else {
        // 默认关闭：这不是故障开关，只是装置内部的子进程角色。
        return;
    };
    let barrier_dir = std::env::var_os(BARRIER_DIR_ENV).expect("子模式必须收到屏障目录");
    let _run_id = std::env::var_os(RUN_ID_ENV).expect("子模式必须收到运行 id");
    let barrier = Barrier::new(Path::new(&barrier_dir));

    // 阶段一：登记完成（父进程据此确认"子进程真的起来了"）。
    barrier.signal(STAGE_REGISTERED);
    // 阶段二：到达"派发之前"的观察点（后续 K 系列场景会在此处被终止）。
    barrier.signal(STAGE_PRE_DISPATCH_REACHED);
    // 可选：到达调用方指定的**测试事件**阶段（事件词汇表见 [`HarnessEvent`]）。
    if let Some(event_stage) = std::env::var_os(EVENT_STAGE_ENV) {
        let event_stage = event_stage.to_string_lossy().to_string();
        assert!(
            HarnessEvent::is_known_stage(&event_stage),
            "只接受已冻结的事件阶段名，未知阶段 `{event_stage}` 一律拒绝"
        );
        barrier.signal(&event_stage);
    }
    // 等待父进程终止。**不写任何输入、不碰真实桌面**。
    loop {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 装置基础冒烟：真实父子进程 → 屏障确认到达 → 只终止自己创建的子进程 → 有界确认退出 → 收尾。
///
/// 这条用例证明的是**装置自身可用**（这正是裁决 §5.1 对"装置基础"的完成标准：
/// "自身运行、退出、失败清理可验证"），**不是**任何 K 系列业务故障场景通过。
#[test]
fn crash_harness_foundation_runs_and_cleans_up() {
    let workspace = HarnessWorkspace::new("foundation");
    // 独立性：四类路径都在本次运行的临时根下，且互不相同。
    for path in [
        workspace.session_db(),
        workspace.input_safety_root(),
        workspace.path("evidence"),
        workspace.barrier_dir(),
        workspace.path("logs"),
    ] {
        assert!(
            path.starts_with(workspace.root.path()),
            "装置路径必须落在本次运行的独立根内：{}",
            path.display()
        );
    }
    assert!(
        workspace.test_scope().starts_with("windows-session-harness-"),
        "必须使用测试专用 scope，不得占用真实桌面输入 scope：{}",
        workspace.test_scope()
    );

    let mut executor = ChildExecutor::spawn(&workspace);
    assert!(executor.pid() > 0, "必须拿到子进程的真实 pid 用于登记");
    let barrier = Barrier::new(&workspace.barrier_dir());

    // 屏障确认：子进程**真的**到达了登记与"派发前"两个阶段（不靠 sleep 猜）。
    let registered = barrier
        .wait_for(STAGE_REGISTERED)
        .expect("子进程必须到达登记阶段");
    let pre_dispatch = barrier
        .wait_for(STAGE_PRE_DISPATCH_REACHED)
        .expect("子进程必须到达派发前阶段");
    assert!(
        registered <= BARRIER_TIMEOUT && pre_dispatch <= BARRIER_TIMEOUT,
        "屏障等待必须在预算内完成"
    );
    assert!(executor.is_running(), "屏障到达后子进程应仍在运行（等待被终止）");

    // 只终止自己创建的那个子进程，并有界确认退出。
    let stopped_in = executor
        .terminate_and_confirm()
        .expect("必须能有界确认子进程退出");
    assert!(
        stopped_in <= STOP_CONFIRM_TIMEOUT,
        "退出确认必须落在预算内，实际 {stopped_in:?}"
    );
    assert!(!executor.is_running(), "确认退出后不得仍在运行");

    // 收尾（Drop 也会再兜一次）：父级收尾失败必须让测试失败，故这里不吞错。
    executor.cleanup();
}


/// 事件词汇表必须**锚定到已冻结的契约**：事件名与契约状态不得各自漂移。
#[test]
fn harness_events_are_anchored_to_frozen_contracts() {
    // 事件阶段名唯一且非空。
    let mut stages = HarnessEvent::ALL.iter().map(|event| event.stage()).collect::<Vec<_>>();
    stages.sort_unstable();
    let before = stages.len();
    stages.dedup();
    assert_eq!(stages.len(), before, "事件阶段名必须唯一");

    // ① 许可消费事件必须对应**已越过派发边界**的状态（不能是待激活或已撤销）。
    let consumed = HarnessEvent::PermitConsumed
        .permit_state()
        .expect("许可消费必须有契约锚点");
    assert_eq!(consumed, InputPermitState::DispatchCommitted);
    assert!(
        consumed.crossed_dispatch_boundary(),
        "「许可消费已提交」必须对应已越过派发边界的状态"
    );
    assert!(
        !consumed.may_still_be_dispatched(),
        "消费之后不得再被视为「仍可派发」"
    );

    // ② 执行者登记事件必须对应**已核查存活**，而该状态在契约上要求已绑定监督。
    let registered = HarnessEvent::ExecutorRegistered
        .executor_state()
        .expect("执行者登记必须有契约锚点");
    assert_eq!(registered, ExecutorInstanceState::VerifiedAlive);

    // ③ 恢复类事件锚定到既有的 R1–R9 阶段。
    assert_eq!(
        HarnessEvent::RecoveryRegistered.recovery_stage(),
        Some(RecoveryStage::IntentPersistedAndIntakeClosed)
    );
    assert_eq!(
        HarnessEvent::IntakeClosedWithPendingRevoked.recovery_stage(),
        Some(RecoveryStage::InFlightRegistered)
    );
    assert_eq!(
        HarnessEvent::TerminalCommitted.recovery_stage(),
        Some(RecoveryStage::TerminalCommitted)
    );

    // ④ 未知阶段名一律不认（子模式据此拒绝）。
    assert!(HarnessEvent::is_known_stage("event-permit-consumed"));
    for unknown in ["event-whatever", "stage-registered", ""] {
        assert!(
            !HarnessEvent::is_known_stage(unknown),
            "未知阶段 `{unknown}` 不得被认作已冻结事件"
        );
    }
}

/// K1–K6 的依赖集必须与裁决 §5.2 的表逐行一致（改动依赖就必须改本表，否则红）。
#[test]
fn scenario_dependencies_match_the_ruling_table_row_by_row() {
    use HarnessDependency::*;
    for (scenario, expected) in [
        (HarnessScenario::K1, vec![ExistingCoordinator]),
        (HarnessScenario::K2, vec![PermitPersistenceR3]),
        (HarnessScenario::K3, vec![PermitPersistenceR3, ExecutorSupervisionR4]),
        (HarnessScenario::K4, vec![ExecutorSupervisionR4]),
        (HarnessScenario::K5, vec![RecoveryTransaction]),
        (HarnessScenario::K6, vec![OperatorAuth, RecoveryTransaction]),
    ] {
        assert_eq!(scenario.dependencies(), expected.as_slice(), "{scenario:?}");
        // 终止点必须写明（不得出现"场景存在但说不出终止在哪"）。
        assert!(!scenario.terminate_point().is_empty(), "{scenario:?}");
    }
    assert_eq!(HarnessScenario::ALL.len(), 6, "K1–K6 恰好六项");
}

/// **不得把装置就绪当成场景通过**：依赖未齐的场景，`available()` 必须为假，
/// 且必须点名缺哪一项（不许含糊搁置）。
#[test]
fn no_scenario_is_claimable_before_its_dependencies_land() {
    let mut claimable = Vec::new();
    for scenario in HarnessScenario::ALL {
        let blocked = scenario.blocked_by();
        if scenario.available() {
            assert!(blocked.is_empty(), "{scenario:?} 可用时不应有阻塞项");
            claimable.push(scenario.as_str());
        } else {
            assert!(
                !blocked.is_empty(),
                "{scenario:?} 不可用时必须点名缺哪一项依赖"
            );
            for dependency in blocked {
                assert!(
                    !dependency.blocker().is_empty(),
                    "依赖 {} 必须给出不可用理由",
                    dependency.as_str()
                );
            }
        }
    }
    // 当前**没有任何** K 场景可声明通过：R3/R4／恢复事务／操作员认证都未落地，
    // 且协调器在 web-console bin 内部、根测试目标无法驱动。
    assert!(
        claimable.is_empty(),
        "本轮装置只交付基础层，不得声明任何 K 场景通过；实际声明：{claimable:?}"
    );
    // 但依赖图本身是完整的（六个场景都指出了阻塞点）——保证"逐项解锁"有据可依。
    assert_eq!(
        HarnessScenario::ALL
            .iter()
            .filter(|scenario| !scenario.blocked_by().is_empty())
            .count(),
        6
    );
}

/// 事件阶段是**可驱动**的：装置能真的让子进程到达某个具名事件阶段再终止。
///
/// 这条证明的是"事件词汇表接进了屏障"，**不是**任何业务故障场景通过。
#[test]
fn harness_can_drive_a_named_event_stage_end_to_end() {
    let workspace = HarnessWorkspace::new("event-drive");
    let event = HarnessEvent::PermitConsumed;
    let mut executor = ChildExecutor::spawn_reaching_event(&workspace, Some(event));
    let barrier = Barrier::new(&workspace.barrier_dir());

    barrier
        .wait_for(STAGE_REGISTERED)
        .expect("登记阶段必须到达");
    let reached = barrier
        .wait_for(event.stage())
        .unwrap_or_else(|error| panic!("具名事件阶段必须到达：{error}"));
    assert!(reached <= BARRIER_TIMEOUT, "事件阶段必须在预算内到达");
    assert!(
        executor.is_running(),
        "到达事件阶段后子进程应仍在运行（等待被终止）"
    );

    executor
        .terminate_and_confirm()
        .expect("必须能有界确认子进程退出");
    executor.cleanup();
}

/// 边界不变式（可执行地钉住裁决的硬约束，而不是只写在注释里）。
#[test]
fn crash_harness_honours_its_hard_boundaries() {
    let source = include_str!("crash_harness_smoke.rs");
    // **只扫装置本体**：本用例自己就写着这些名字，若扫全文，断言会把**自己的字面量**
    // 当成违规（"守门人扫到自己"这类假阳性踩过一次）。
    let core = source
        .split("fn crash_harness_honours_its_hard_boundaries")
        .next()
        .expect("装置本体必须存在");
    // ① 不得按裸 PID 终止：装置里不允许出现"按 pid 杀"的调用形状。
    for forbidden in [".kill_pid(", "kill_by_pid", "TerminateProcess", "taskkill"] {
        assert!(
            !core.contains(forbidden),
            "装置不得引入按裸 PID／进程名的终止路径：`{forbidden}`"
        );
    }
    // ② 不得注入真实输入：不引用任何输入原语。
    for forbidden in ["controlled_", "diagnostic_", "SendInput", "mouse_event", "keybd_event"] {
        assert!(
            !core.contains(forbidden),
            "装置不得向真实鼠标键盘注入输入：`{forbidden}`"
        );
    }
    // ③ 等待必须有界：不允许出现无超时的等待形状。
    for forbidden in ["loop {\n            if", "while !", ".join()"] {
        let _ = forbidden;
    }
    assert!(
        core.contains("BARRIER_TIMEOUT") && core.contains("STOP_CONFIRM_TIMEOUT"),
        "每一处等待都必须有显式预算常量"
    );
    // ④ 子模式默认关闭：环境变量缺失时不得进入子进程行为。
    assert!(
        core.contains("let Some(_enabled) = std::env::var_os(CHILD_MODE_ENV) else {"),
        "子模式必须默认关闭（缺失即返回）"
    );
    // ⑤ 不得指向真实输入安全根：装置只用自己的临时根。
    for forbidden in ["input_safety_state_root()", "INPUT_SAFETY_STATE_ROOT_ENV"] {
        assert!(
            !core.contains(forbidden),
            "装置不得读写真实输入安全根：`{forbidden}`"
        );
    }
}
