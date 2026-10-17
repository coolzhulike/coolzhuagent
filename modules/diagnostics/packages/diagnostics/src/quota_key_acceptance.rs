//! 关键验收：实际生产写入口跨进程恢复、有界清单、非登记数据保留及导出脱敏。
use super::*;
use std::process::{Command, Stdio};
use std::time::Instant;

#[test]
#[ignore = "仅由关键验收父测试以独立进程启动"]
fn production_writer_child() {
    let root = PathBuf::from(std::env::var_os("QUOTA_KEY_ROOT").unwrap());
    let name = std::env::var("QUOTA_KEY_NAME").unwrap();
    assert!(PRODUCT_LOG_FAMILIES.contains(&name.as_str()));
    append_shared_with_budget(&root.join(name), &vec![b'D'; 3000], 3, 4096).unwrap();
}

fn run_child(root: &Path, name: &str) -> u32 {
    let log = File::create(root.join(format!("{name}.child-result.txt"))).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "quota::key_acceptance::production_writer_child", "--ignored", "--nocapture"])
        .env("QUOTA_KEY_ROOT", root).env("QUOTA_KEY_NAME", name)
        .stdin(Stdio::null()).stdout(log.try_clone().unwrap()).stderr(log).spawn().unwrap();
    let pid = child.id();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() { assert!(status.success(), "子进程失败：{status}"); break; }
        if Instant::now() >= deadline {
            child.kill().unwrap(); let _ = child.wait(); panic!("本测试子进程未在30秒内完成");
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    pid
}

#[test]
fn production_restart_keeps_registry_bounded_and_authoritative_files_safe() {
    let parent = std::env::var_os("QUOTA_KEY_EVIDENCE_ROOT").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let root = parent.join(format!("quota-key-{}-{}", std::process::id(), crate::unix_millis()));
    fs::create_dir(&root).unwrap();
    let first = run_child(&root, "coolzhu-web-console.jsonl");
    let old = root.join("coolzhu-web-console.jsonl");
    File::options().write(true).open(&old).unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(120)).unwrap();
    let audit = root.join("tool-audit.jsonl");
    fs::write(&audit, vec![b'A'; 100]).unwrap();
    File::options().write(true).open(&audit).unwrap()
        .set_modified(SystemTime::now() - Duration::from_secs(240)).unwrap();
    // 模拟不可信清单把权威文件列为候选：只计数，绝不能获得删除资格。
    OpenOptions::new().append(true).open(root.join(QUOTA_MEMBERS_NAME)).unwrap()
        .write_all(b"tool-audit.jsonl\n").unwrap();
    let second = run_child(&root, "coolzhu-tauri-shell.jsonl");
    assert_ne!(first, second);
    assert!(!old.exists(), "真实生产写入口应回收退出进程的旧产品日志族");
    assert_eq!(fs::metadata(root.join("coolzhu-tauri-shell.jsonl")).unwrap().len(), 3000);
    assert_eq!(fs::read(&audit).unwrap(), vec![b'A'; 100], "清单伪造不得授权删除权威数据");
    let mut quota = DiagnosticQuota::new(&root, 4096).unwrap();
    let before = fs::read(root.join(QUOTA_MEMBERS_NAME)).unwrap();
    for _ in 0..100 { quota.register_rotated("coolzhu-tauri-shell.jsonl", 3).unwrap(); }
    assert_eq!(fs::read(root.join(QUOTA_MEMBERS_NAME)).unwrap(), before, "重复登记不增长");
    assert_eq!(quota.accounted_bytes().unwrap(), 3100);
    assert!(quota.register_file("new.log\ntool-audit.jsonl").is_err());
    assert_eq!(fs::read(root.join(QUOTA_MEMBERS_NAME)).unwrap(), before);

    let source = root.join("sample-source.txt");
    fs::write(&source, "正常运行\napi_key=FIXTURE_SECRET_6_1_ONLY\n").unwrap();
    let bundle = root.join("redacted-bundle");
    let report = crate::export_bundle(&bundle, &[crate::ExportAttachment {
        source, bundle_path: "logs/sample.txt".into(),
    }], crate::ExportLimits::default()).unwrap();
    assert_eq!(report.entries[0].status, crate::EntryStatus::Exported);
    let exported = fs::read_to_string(bundle.join("logs/sample.txt")).unwrap();
    assert!(!exported.contains("FIXTURE_SECRET_6_1_ONLY"));
    assert!(exported.contains("[REDACTED]") && exported.contains("正常运行"));

    // 超限清单拒读拒登记，不能为缩短清单而默默忘记已有账目。
    OpenOptions::new().write(true).open(root.join(QUOTA_MEMBERS_NAME)).unwrap()
        .set_len(MAX_MANIFEST_BYTES + 1).unwrap();
    assert!(quota.accounted_bytes().is_err());
    assert!(quota.register_file("new.log").is_err());
    assert_eq!(fs::read(&audit).unwrap(), vec![b'A'; 100]);
    println!("DIAG_QUOTA_KEY first_pid={first} second_pid={second} recovered_old_family=true authoritative_unchanged=true duplicate_registrations=100 manifest_growth=0 oversized_refused=true redacted_export=true root={}", root.display());
}
