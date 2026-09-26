//! **宿主 bin 的测试接缝 + K1 跨进程场景**（2026-09-26 授权 §5）。
//!
//! ## 为什么放在 bin 的测试模块里
//!
//! 协调器（`InputSafetyCoordinator`）是 bin 内部实现，外部 `tests/` 集成测试**不能** `use` 它。
//! 裁决 §5.1 因此选择最小方案：把需要访问协调器的父测试放在**所属 bin 的测试模块**中，
//! 再由该测试程序创建**自己的**受控子测试进程——不要求先把协调器抽成公共 library。
//!
//! ## 硬约束（裁决 §5.2）
//!
//! - 子进程调用**同一生产实现**（`begin_with_coordination_scope`），**不复制**协调器代码、
//!   **不重写** epoch／锁逻辑、**不反序列化**"已持权"对象。
//! - 隔离路径与测试 scope 经**显式参数／子进程环境**传入。
//! - 记录实际启动的程序路径，避免"执行错二进制仍报告通过"。
//! - 只终止**自己持有创建句柄**的子进程。
//!
//! ## 它验证什么／不验证什么
//!
//! 验证的是"**宿主测试二进制中的真实跨进程协调逻辑**"：子进程取得协调资格后**未完成登记即被终止**，
//! 新实例必须**重新取权**（并且不能把"锁可取得"当成"安全"）。
//! **不等于**正式安装程序的启动端到端验收——两者在报告里分列。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::input_safety_store::InputSafetyCoordinator;
use runtime::InputSafetyResourceScope;

/// 子模式开关（默认关闭；仅测试二进制的测试角色使用）。
const CHILD_MODE_ENV: &str = "COOLZHU_K1_HARNESS_CHILD";
/// 隔离输入安全根。
const ROOT_ENV: &str = "COOLZHU_K1_HARNESS_ROOT";
/// 测试专用协调范围（**不是**真实交互会话范围）。
const COORDINATION_ENV: &str = "COOLZHU_K1_HARNESS_COORDINATION_SCOPE";
/// 资源 scope。
const RESOURCE_ENV: &str = "COOLZHU_K1_HARNESS_RESOURCE_SCOPE";
/// 屏障目录。
const BARRIER_ENV: &str = "COOLZHU_K1_HARNESS_BARRIER";

const STAGE_COORDINATION_HELD: &str = "k1-coordination-held";
const BARRIER_TIMEOUT: Duration = Duration::from_secs(20);
const STOP_CONFIRM_TIMEOUT: Duration = Duration::from_secs(10);

fn barrier_signal(directory: &Path, stage: &str) {
    let staging = directory.join(format!(".{stage}.partial"));
    std::fs::write(&staging, stage.as_bytes()).expect("write barrier");
    std::fs::rename(&staging, directory.join(stage)).expect("publish barrier");
}

fn barrier_wait(directory: &Path, stage: &str) -> Result<Duration, String> {
    let started = Instant::now();
    let target = directory.join(stage);
    while started.elapsed() < BARRIER_TIMEOUT {
        if target.exists() {
            return Ok(started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(format!("等待屏障 `{stage}` 超时（不得用 sleep 替代屏障）"))
}

/// **子模式**：取得真实协调资格 → 声明屏障 → 等待被终止（**不完成登记**）。
///
/// 这正是 K1 的故障点："取得协调权后、恢复登记完成前"。
#[test]
fn k1_harness_child_mode() {
    let Some(_enabled) = std::env::var_os(CHILD_MODE_ENV) else {
        return; // 默认关闭
    };
    let root = PathBuf::from(std::env::var_os(ROOT_ENV).expect("隔离根必须显式传入"));
    let coordination = std::env::var(COORDINATION_ENV).expect("协调范围必须显式传入");
    let resource = std::env::var(RESOURCE_ENV).expect("资源 scope 必须显式传入");
    let barrier_dir = PathBuf::from(std::env::var_os(BARRIER_ENV).expect("屏障目录必须显式传入"));
    let scope = InputSafetyResourceScope::parse(&resource).expect("资源 scope");

    // **调用同一生产实现**取得协调资格。
    let coordinator = InputSafetyCoordinator::begin_with_coordination_scope(
        &root,
        &coordination,
        &scope,
        "recovery-k1-child",
        &["release_isolation", "open_new_input"],
        Duration::from_secs(5),
    )
    .expect("子进程必须能取得协调资格");
    // 把持有的 epoch 写进屏障文件，供父进程核对"确实持有"。
    let held = coordinator
        .store()
        .current_recovery_epoch(&scope)
        .expect("current")
        .expect("child holds");
    std::fs::write(
        barrier_dir.join("k1-held-epoch"),
        held.epoch.to_string(),
    )
    .expect("write epoch");
    barrier_signal(&barrier_dir, STAGE_COORDINATION_HELD);
    // 刻意**不**推进登记、不结账：模拟"取得协调权后、登记完成前"被终止。
    loop {
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// 父测试持有的子进程句柄：终止只经它，**不接受裸 PID**。
struct ChildHandle {
    child: Child,
}

impl ChildHandle {
    fn spawn(root: &Path, coordination: &str, resource: &str, barrier: &Path) -> Self {
        let exe = std::env::current_exe().expect("current test binary");
        eprintln!("[k1] 启动的子测试程序：{}", exe.display());
        let child = Command::new(exe)
            .args(["--exact", "input_safety_harness::k1_harness_child_mode", "--nocapture"])
            .env(CHILD_MODE_ENV, "1")
            .env(ROOT_ENV, root)
            .env(COORDINATION_ENV, coordination)
            .env(RESOURCE_ENV, resource)
            .env(BARRIER_ENV, barrier)
            .stdin(Stdio::null())
            // 子进程输出留档：诊断失败时必须能看到它**为什么**没能取得资格，
            // 而不是只看到父进程的超时（否则等于把证据丢掉）。
            .stdout(std::fs::File::create(barrier.join("child-stdout.log")).expect("stdout log"))
            .stderr(std::fs::File::create(barrier.join("child-stderr.log")).expect("stderr log"))
            .spawn()
            .expect("spawn child");
        Self { child }
    }

    fn terminate_and_confirm(&mut self) -> Result<Duration, String> {
        let started = Instant::now();
        self.child
            .kill()
            .map_err(|error| format!("请求终止失败：{error}"))?;
        while started.elapsed() < STOP_CONFIRM_TIMEOUT {
            match self.child.try_wait() {
                Ok(Some(_)) => return Ok(started.elapsed()),
                Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => return Err(format!("等待退出出错：{error}")),
            }
        }
        Err("等待子进程退出超时".to_string())
    }
}

impl Drop for ChildHandle {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            self.terminate_and_confirm()
                .unwrap_or_else(|error| panic!("K1 装置收尾失败：{error}"));
        }
    }
}

/// **K1**：取得协调权后、恢复登记完成前被终止 ⇒ 新实例必须**重新取权**；
/// 且"锁可取得"**不等于**"安全"。
#[test]
fn k1_new_instance_reacquires_after_holder_dies_before_registration() {
    let root = tempfile::TempDir::new().expect("tempdir");
    let barrier = tempfile::TempDir::new().expect("barrier");
    // 测试专用协调范围与资源 scope：**不占用**真实交互会话的输入资源。
    let coordination = format!("windows-session-harness-k1-{}", std::process::id());
    let resource = "windows-session-harness-k1-resource";
    let scope = InputSafetyResourceScope::parse(resource).expect("scope");

    let mut child = ChildHandle::spawn(root.path(), &coordination, resource, barrier.path());
    if let Err(error) = barrier_wait(barrier.path(), STAGE_COORDINATION_HELD) {
        let stderr = std::fs::read_to_string(barrier.path().join("child-stderr.log"))
            .unwrap_or_default();
        panic!("子进程必须取得协调资格：{error}
子进程 stderr：
{stderr}");
    }
    let held_epoch: u64 = std::fs::read_to_string(barrier.path().join("k1-held-epoch"))
        .expect("epoch 文件")
        .trim()
        .parse()
        .expect("epoch 是数字");

    // ① 子进程仍持有时，第二个实例**必须**被跨进程互斥挡住（真实跨进程，不是同进程模拟）。
    let busy = InputSafetyCoordinator::begin_with_coordination_scope(
        root.path(),
        &coordination,
        &scope,
        "recovery-k1-parent",
        &["release_isolation", "open_new_input"],
        Duration::from_millis(300),
    )
    .err()
    .expect("另一进程持有时不得再取得协调资格");
    assert_eq!(busy.code(), "input_safety_coordinator_busy", "{busy}");

    // ② **"锁可取得不等于安全"**：此刻资源并未因"有人在协调"而变成可接纳新输入。
    let store = crate::input_safety_store::InputSafetyStore::open_at(root.path()).expect("open");
    let state = store.resource_state(&scope).expect("resource state");
    assert!(
        !state.accepts_new_input,
        "仅凭「有人持有协调资格」不得让资源变成可接纳新输入"
    );

    // ③ 终止持有者（只终止自己创建的子进程），并**有界确认**退出。
    let stopped = child.terminate_and_confirm().expect("必须有界确认退出");
    assert!(stopped <= STOP_CONFIRM_TIMEOUT);

    // ④ 持有者死亡后的**重新取权**。这里如实覆盖两条分支，而不是假定必然成功：
    //
    // 存储的回收规则是 **fail-closed**（与裁决 §4.5 同口径）：只有在能**正面确认**
    // 旧持有者已消失（进程不存在或 PID 已被复用）时才回收陈旧资格；拿不到身份一律按
    // Unknown 处理，**不回收**、仍报 epoch 冲突。因此被强杀的持有者之后，新实例可能得到
    // 新 epoch，也可能被明确拒绝——两者都**不是**"锁一拿到就安全"。
    let attempted = InputSafetyCoordinator::begin_with_coordination_scope(
        root.path(),
        &coordination,
        &scope,
        "recovery-k1-restored",
        &["release_isolation", "open_new_input"],
        Duration::from_secs(2),
    );
    match attempted {
        Ok(restored) => {
            let new_epoch = restored
                .store()
                .current_recovery_epoch(&scope)
                .expect("current")
                .expect("restored holds")
                .epoch;
            assert!(
                new_epoch > held_epoch,
                "确认旧持有者已消失时，新实例必须取得**新** epoch（{held_epoch} → {new_epoch}），                 不得继承旧资格"
            );
        }
        Err(error) => {
            assert_eq!(
                error.code(),
                "input_safety_epoch_conflict",
                "无法确认旧持有者消失时必须**明确拒绝**（fail-closed），而不是悄悄接管：{error}"
            );
            eprintln!(
                "[k1] 环境中无法正面确认被杀持有者已消失 ⇒ 存储按 fail-closed 拒绝回收（预期行为之一）"
            );
        }
    }

    // 无论走哪条分支：**"锁可取得/资格可回收"都不等于"资源已安全"**。
    let after = store.resource_state(&scope).expect("resource state");
    assert!(
        !after.accepts_new_input,
        "崩溃后重新取权不得让资源自动变成可接纳新输入"
    );

    // ⑤ **崩溃者的痕迹必须仍然可见**（K1 的核心断言"新实例重新取权**核查**"）：
    // 取得协调资格本身会写下"恢复操作已开始"，而持有者被终止时**没有**结账，
    // 因此必须留下**未结账**的恢复操作供新实例核对——不得被静默清掉、也不得被伪造成已完成。
    let unsettled = store
        .unsettled_recovery_operations(&scope)
        .expect("unsettled");
    assert!(
        !unsettled.is_empty(),
        "被终止的持有者必须留下未结账的恢复操作（否则新实例无从核查）"
    );
    let crashed = unsettled
        .iter()
        .find(|operation| operation.coordinator_instance_id.contains("-"))
        .expect("必须能识别出崩溃者留下的那条操作");
    assert!(
        !crashed.committed,
        "崩溃者的恢复操作不得被标成已结账（未完成就是未完成）"
    );
    assert_eq!(
        crashed.recovery_epoch, held_epoch,
        "该操作必须绑定崩溃者当时持有的 epoch（{}），而不是被改写成别的资格",
        held_epoch
    );
    eprintln!(
        "[k1] 崩溃者留下的未结账恢复操作：id={} epoch={} committed={}",
        crashed.recovery_operation_id, crashed.recovery_epoch, crashed.committed
    );
}
