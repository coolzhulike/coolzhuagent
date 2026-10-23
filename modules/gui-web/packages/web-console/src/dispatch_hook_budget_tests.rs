// 通过完整派发服务验证 hook 的真实进程、根预算与取消；不调用外部模型。
#[cfg(windows)]
mod hook_budget_acceptance {
    use super::*;
    use crate::root_execution_budget::{self, RootExecutionBudget};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn slow_hook(directory: &std::path::Path) -> (String, std::path::PathBuf, std::path::PathBuf) {
        let started = directory.join("hook-started.pid");
        let late = directory.join("hook-late.txt");
        // 本测试程序作为有限子进程，不依赖 PowerShell 的脚本策略或外部运行时。
        let executable = std::env::current_exe().unwrap();
        (format!("set \"COOLZHU_HOOK_BUDGET_PROBE={}\" && \"{}\" --exact tests::hook_budget_acceptance::hook_process_probe --nocapture",
            directory.display(), executable.display()), started, late)
    }

    #[test]
    fn hook_process_probe() {
        let Some(path) = std::env::var_os("COOLZHU_HOOK_BUDGET_PROBE") else { return; };
        let path = std::path::PathBuf::from(path);
        std::fs::write(path.join("hook-started.pid"), std::process::id().to_string()).unwrap();
        std::thread::sleep(Duration::from_secs(8));
        std::fs::write(path.join("hook-late.txt"), "late").unwrap();
    }

    async fn wait_hook_started(started: &std::path::Path) {
        tokio::time::timeout(Duration::from_secs(6), async {
            while hook_pid(started).is_none() { tokio::time::sleep(Duration::from_millis(10)).await; }
        }).await.expect("必须有真实 hook 入场记录");
        assert!(windows_process_guard::capture_live_process_identity(hook_pid(started).unwrap()).is_ok(),
            "先确认进程确实存活且有查询能力，不能把查询拒绝误当作退出");
    }

    fn hook_pid(started: &std::path::Path) -> Option<u32> {
        std::fs::read_to_string(started).ok()?.trim().parse().ok()
    }

    fn assert_hook_exited(started: &std::path::Path) {
        let pid = std::fs::read_to_string(started).unwrap().trim().parse::<u32>().unwrap();
        assert!(windows_process_guard::capture_live_process_identity(pid).is_err(),
            "派发返回前 hook 进程必须已经退出，pid={pid}");
    }

    fn interrupted_hook_case(post: bool, cancel: bool, longer_parent: bool) {
        let _guard = config_test_guard();
        let _dev_open = DevOpenPermissionsTestGuard::enable();
        let fixture = hook_dispatch_fixture();
        let _ = crate::session_store();
        let _ = crate::computer_use_store::ComputerUseRunStore::open(&crate::default_session_sqlite_path()).unwrap();
        crate::workspace_config().lock().unwrap().tool.dev_open_permissions = true;
        let target = fixture.tmp.path().join("target.txt");
        let following = fixture.tmp.path().join("following.txt");
        let (command, started, late) = slow_hook(fixture.tmp.path());
        let commands = vec![command, format!("echo following>\"{}\"", following.display())];
        let runner = if post { authorized_dispatch_hook_runner(Vec::new(), commands) }
            else { authorized_dispatch_hook_runner(commands, Vec::new()) };
        let _hook_guard = dispatch_hook_override_guard(runner);
        let cancellation = Arc::new(crate::ChatTurnCancellation::new());
        let _scope = crate::ToolTurnCancellationScope::install("hook-budget-turn", cancellation.clone());
        let input = serde_json::json!({"path":target,"content":"真实目标回执"});
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let mut parent = crate::FrozenParentContext::new("hook-budget-test",
            &crate::workspace_identity(fixture.tmp.path()), None, None, None, None).unwrap();
        parent.root_budget = Some(RootExecutionBudget::establish(60_000));
        let budget = RootExecutionBudget::establish(if cancel { 15_000 } else { 4_000 });
        let began = Instant::now();
        let outcome = rt.block_on(root_execution_budget::scope(budget, async {
            let dispatch = crate::run_model_tool_dispatch_for_session_with_identity(
                "write_file", &input, None, Some("provider-hook-budget"), Some("hook-budget-turn"), None,
                longer_parent.then_some(&parent), None);
            let observe = async {
                let seen = tokio::time::timeout(Duration::from_secs(6), async {
                    while hook_pid(&started).is_none() { tokio::time::sleep(Duration::from_millis(10)).await; }
                }).await.is_ok();
                if seen { assert!(windows_process_guard::capture_live_process_identity(hook_pid(&started).unwrap()).is_ok()); }
                if seen && cancel { cancellation.request(); }
                seen
            };
            let (result, seen) = tokio::join!(dispatch, observe);
            assert!(seen, "hook 未真实入场，实际派发结果：{result:?}");
            result
        }));
        let error = outcome.expect_err("中止 hook 不能报告工具整体成功");
        assert_eq!(error.0, if cancel { axum::http::StatusCode::CONFLICT } else { axum::http::StatusCode::REQUEST_TIMEOUT });
        assert!(began.elapsed() < Duration::from_secs(8), "不能等完整 hook 睡眠结束才响应");
        assert_hook_exited(&started);
        assert!(!late.exists(), "中止后不得完成 hook 的迟到写入");
        assert!(!following.exists(), "中止后不得启动下一 hook");
        if post {
            assert_eq!(std::fs::read_to_string(&target).unwrap(), "真实目标回执",
                "后置失败不得抹掉已发生的目标效果");
        } else { assert!(!target.exists(), "前置中止必须阻止目标工具"); }
    }

    #[test]
    fn dispatch_hook_budget_pre_deadline_stops_target_and_following_hook() { interrupted_hook_case(false, false, false); }
    #[test]
    fn dispatch_hook_budget_post_deadline_preserves_target_effect() { interrupted_hook_case(true, false, false); }
    #[test]
    fn dispatch_hook_budget_pre_cancel_yields_and_stops_target() { interrupted_hook_case(false, true, false); }
    #[test]
    fn dispatch_hook_budget_post_cancel_yields_and_preserves_effect() { interrupted_hook_case(true, true, false); }

    #[test]
    fn dispatch_hook_budget_longer_frozen_parent_cannot_extend_current_deadline() { interrupted_hook_case(false, false, true); }

    #[test]
    fn dispatch_hook_budget_dropped_wait_cancels_real_process() {
        let _guard = config_test_guard();
        let fixture = hook_dispatch_fixture();
        let _ = crate::session_store();
        let _ = crate::computer_use_store::ComputerUseRunStore::open(&crate::default_session_sqlite_path()).unwrap();
        let (command, started, late) = slow_hook(fixture.tmp.path());
        let _hook_guard = dispatch_hook_override_guard(authorized_dispatch_hook_runner(vec![command], Vec::new()));
        let input = serde_json::json!({"path":fixture.file_path});
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        rt.block_on(async {
            let mut dispatch = Box::pin(crate::run_model_tool_dispatch("read_file", &input));
            tokio::select! {
                result = &mut dispatch => panic!("等待撤销前不应完成：{result:?}"),
                () = wait_hook_started(&started) => {}
            }
            drop(dispatch);
            tokio::time::sleep(Duration::from_millis(500)).await;
            assert_hook_exited(&started);
            tokio::time::sleep(Duration::from_secs(8)).await;
            assert!(!late.exists(), "丢弃 HTTP 等待后不能留下继续执行的 hook");
        });
    }

    #[test]
    fn dispatch_hook_budget_live_root_allows_real_pre_target_post() {
        let _guard = config_test_guard();
        let _dev_open = DevOpenPermissionsTestGuard::enable();
        let fixture = hook_dispatch_fixture();
        let _ = crate::session_store();
        crate::workspace_config().lock().unwrap().tool.dev_open_permissions = true;
        let pre = fixture.tmp.path().join("pre.txt");
        let post = fixture.tmp.path().join("post.txt");
        let target = fixture.tmp.path().join("target.txt");
        let _hook_guard = dispatch_hook_override_guard(authorized_dispatch_hook_runner(
            vec![format!("echo pre>\"{}\"", pre.display())], vec![format!("echo post>\"{}\"", post.display())]));
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let input = serde_json::json!({"path":target,"content":"正常目标"});
        let outcome = rt.block_on(root_execution_budget::scope(RootExecutionBudget::establish(15_000),
            crate::run_model_tool_dispatch("write_file", &input))).unwrap();
        assert_eq!(outcome.route, "runtime-executed");
        assert_eq!(outcome.status, "ok");
        assert!(pre.exists() && post.exists(), "正常根必须真正执行前、后置 hook");
        assert_eq!(std::fs::read_to_string(target).unwrap(), "正常目标");
    }
}
