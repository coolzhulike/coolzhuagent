// 1.3 关键验收：真实子进程持锁、竞争失败、进程退出后接管；旧未知记录不得被改成安全。
#[cfg(windows)]
#[test]
#[ignore = "仅由父验收用例带隔离目录启动"]
fn lease_process_acceptance_child() {
    let root = std::path::PathBuf::from(std::env::var_os("COOLZHU_LEASE_ACCEPTANCE_ROOT").expect("root"));
    let tag = std::env::var("COOLZHU_LEASE_ACCEPTANCE_TAG").expect("tag");
    let scoped = unique_scope(&tag);
    let coordinator = coordinator_for_scope(&root, &tag, &scoped, "old-recovery");
    coordinator.store().establish_incident_authorized(
        &incident_for(&scoped, "old-unknown"), coordinator.control()
    ).expect("记录未知释放状态");
    let ready = serde_json::json!({
        "pid": std::process::id(), "epoch": coordinator.control().epoch(),
        "coordinator": coordinator.control().coordinator_id(),
    });
    std::fs::write(root.join("ready.tmp"), ready.to_string()).expect("ready");
    std::fs::rename(root.join("ready.tmp"), root.join("ready.json")).expect("publish ready");
    // 父进程用自己持有的 Child 句柄结束本 fixture，模拟未释放锁的异常退出。
    loop { std::thread::sleep(std::time::Duration::from_millis(100)); }
}

#[cfg(windows)]
#[test]
fn real_process_lease_competition_reclaims_without_clearing_unknown() {
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    struct OwnedChild(Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            if matches!(self.0.try_wait(), Ok(None)) { let _ = self.0.kill(); }
            let _ = self.0.wait();
        }
    }
    let root = temp_root();
    let tag = format!("real-process-{}-{}", std::process::id(), now_unix_ms());
    let scoped = unique_scope(&tag);
    let child_log = std::fs::File::create(root.path().join("child.log")).expect("log");
    let mut child = OwnedChild(Command::new(std::env::current_exe().expect("test exe"))
        .args(["--exact", "input_safety_store::tests::lease_process_acceptance_child", "--ignored", "--nocapture"])
        .env("COOLZHU_LEASE_ACCEPTANCE_ROOT", root.path())
        .env("COOLZHU_LEASE_ACCEPTANCE_TAG", &tag)
        .stdout(Stdio::from(child_log.try_clone().expect("clone log")))
        .stderr(Stdio::from(child_log)).creation_flags(0x08000000)
        .spawn().expect("spawn owned fixture"));
    let deadline = Instant::now() + Duration::from_secs(20);
    while !root.path().join("ready.json").is_file() {
        assert!(child.0.try_wait().expect("child status").is_none(), "子进程提前结束：{}",
            std::fs::read_to_string(root.path().join("child.log")).unwrap_or_default());
        assert!(Instant::now() < deadline, "子进程未取得锁");
        std::thread::sleep(Duration::from_millis(30));
    }
    let ready: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.path().join("ready.json")).expect("ready bytes")
    ).expect("ready JSON");
    assert_eq!(ready["pid"].as_u64(), Some(u64::from(child.0.id())));
    assert_ne!(child.0.id(), std::process::id());
    let live_identity = windows_process_guard::capture_live_process_identity(child.0.id()).expect("持有者真实存活");
    assert!(default_holder_is_alive(child.0.id(), live_identity.creation_time_filetime()));
    assert!(live_holder_result_matches(1, Err(windows_process_guard::ProcessIdentityError::AccessDenied {
        pid: child.0.id(), source: std::io::Error::from_raw_os_error(5),
    })), "查询拒绝不能回收");
    assert!(live_holder_result_matches(1, Err(windows_process_guard::ProcessIdentityError::Os {
        pid: child.0.id(), source: std::io::Error::from_raw_os_error(6),
    })), "查询未知不能回收");
    let before = InputSafetyStore::open_at(root.path()).expect("parent opens store");
    let old_incident = before.incident("old-unknown").expect("incident").expect("exists");
    let old_recovery = before.recovery_operation("old-recovery").expect("recovery").expect("exists");
    let busy = InputSafetyCoordinator::begin_with_coordination_scope(root.path(),
        &coordination_scope_for(&tag), &scoped, "new-recovery", &["converge_legacy_cu_run"],
        Duration::from_millis(100)).err().expect("活着的子进程持锁时竞争者必须被拒");
    assert_eq!(busy.code(), "input_safety_coordinator_busy");
    assert!(before.recovery_operation("new-recovery").expect("new record").is_none());
    let old_epoch = old_recovery.recovery_epoch;
    let child_pid = child.0.id();
    child.0.kill().expect("仅结束本测试持有的子进程");
    child.0.wait().expect("确认子进程已退出");
    // Child句柄仍持有时Windows仍可读到退出进程身份；身份存在不等于进程存活。
    assert!(windows_process_guard::capture_process_identity(child_pid).is_ok());
    assert!(!default_holder_is_alive(child_pid, live_identity.creation_time_filetime()));
    let next = InputSafetyCoordinator::begin_with_coordination_scope(root.path(),
        &coordination_scope_for(&tag), &scoped, "new-recovery", &["converge_legacy_cu_run"],
        Duration::from_secs(2)).expect("真实持有者死亡后接管");
    assert!(next.control().epoch() > old_epoch);
    let after = next.store().incident("old-unknown").expect("incident").expect("保留未知记录");
    assert_eq!(after.state, IncidentState::Pending);
    assert_eq!(after.original_run_ref, old_incident.original_run_ref);
    assert_eq!(after.resolved_at_unix_ms, old_incident.resolved_at_unix_ms);
    assert_eq!(after.created_at_unix_ms, old_incident.created_at_unix_ms);
    let old_after = next.store().recovery_operation("old-recovery").expect("old recovery").expect("保留旧操作");
    assert_eq!(old_after.recovery_epoch, old_epoch);
    assert_eq!(old_after.coordinator_instance_id, old_recovery.coordinator_instance_id);
    assert_eq!(old_after.disposition, old_recovery.disposition);
    assert_eq!(old_after.committed, old_recovery.committed);
    assert!(!next.store().resource_state(&scoped).expect("resource state").accepts_new_input);
    assert_eq!(next.store().advance_recovery_operation_authorized("old-recovery", next.control(),
        RecoveryStage::IntentPersistedAndIntakeClosed).expect_err("新资格不得直接改写旧操作").code(),
        "input_safety_recovery_unauthorized");
    println!("INPUT_LEASE_ACCEPTANCE parent_pid={} child_pid={} old_epoch={} new_epoch={} contender_denied=true old_unknown_preserved=true intake_closed=true", std::process::id(), child_pid, old_epoch, next.control().epoch());
}
