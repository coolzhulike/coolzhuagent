// 关键闭环使用正式启动配置及派发入口；不注入测试 HookRunner、不启用全开放权限。
#[cfg(windows)]
mod hook_startup_acceptance {
    use super::*;
    use sha2::{Digest,Sha256};
    use std::path::PathBuf;

    #[test]
    fn hook_command_probe() {
        let Ok(root)=std::env::var("HOOK_CONFIG_PROBE_ROOT") else {return;};
        let root=PathBuf::from(root);
        let event=std::env::var("HOOK_EVENT").unwrap();
        assert_eq!(std::env::var("HOOK_TOOL_NAME").unwrap(),"read_file");
        if event=="PreToolUse" {
            std::fs::write(std::env::var("HOOK_CONFIG_PROBE_TARGET").unwrap(),"由前置hook写入，供目标读取").unwrap();
        } else {
            let output=std::env::var("HOOK_TOOL_OUTPUT").unwrap();
            assert!(output.contains("由前置hook写入，供目标读取"),"后置必须看到目标真实回执：{output}");
        }
        let mut events=std::fs::OpenOptions::new().create(true).append(true).open(root.join("events.txt")).unwrap();
        std::io::Write::write_all(&mut events,format!("{event}\n").as_bytes()).unwrap();
    }

    #[test]
    fn startup_configuration_reaches_real_dispatch() {
        let Ok(mode)=std::env::var("HOOK_CONFIG_ACCEPTANCE_CHILD") else {
            let temporary=tempfile::tempdir().unwrap();
            let root=std::env::var_os("HOOK_CONFIG_ACCEPTANCE_ROOT").map(PathBuf::from).unwrap_or_else(||temporary.path().to_path_buf());
            std::fs::create_dir_all(&root).unwrap();
            for mode in ["ungranted","granted"] {
                let case=root.join(mode);std::fs::create_dir(&case).unwrap();
                let output=std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact","tests::hook_startup_acceptance::startup_configuration_reaches_real_dispatch","--nocapture","--test-threads=1"])
                    .env("HOOK_CONFIG_ACCEPTANCE_CHILD",mode).env("HOOK_CONFIG_PROBE_ROOT",&case)
                    .output().unwrap();
                std::fs::write(case.join("test.stdout"),&output.stdout).unwrap();
                std::fs::write(case.join("test.stderr"),&output.stderr).unwrap();
                assert!(output.status.success(),"{mode}: {} {}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
            }
            return;
        };
        let _guard=config_test_guard();
        assert!(!crate::dev_open_tool_permissions_enabled());
        assert!(crate::tool_dispatch_service::DISPATCH_HOOK_RUNNER_OVERRIDE.lock().unwrap().is_none());
        let fixture=hook_dispatch_fixture();
        let root=PathBuf::from(std::env::var_os("HOOK_CONFIG_PROBE_ROOT").unwrap());
        std::env::set_var("HOOK_CONFIG_PROBE_TARGET",&fixture.file_path);
        let command=format!("\"{}\" --exact tests::hook_startup_acceptance::hook_command_probe --nocapture",std::env::current_exe().unwrap().display());
        let bytes=serde_json::to_vec(&serde_json::json!({"version":1,"workspace_root":fixture.tmp.path(),"hooks":{"PreToolUse":[command],"PostToolUse":[command]}})).unwrap();
        let config=root.join("hooks.json");std::fs::write(&config,&bytes).unwrap();
        std::env::set_var("COOLZHU_TOOL_HOOKS_CONFIG",&config);
        std::env::remove_var("COOLZHU_TOOL_HOOKS_PRE_SHA256");std::env::remove_var("COOLZHU_TOOL_HOOKS_POST_SHA256");
        let digest=format!("{:x}",Sha256::digest(&bytes));
        if mode=="granted" {
            std::env::set_var("COOLZHU_TOOL_HOOKS_PRE_SHA256",&digest);
            std::env::set_var("COOLZHU_TOOL_HOOKS_POST_SHA256",&digest);
        }
        crate::dispatch_hooks::initialize(fixture.tmp.path()).unwrap();
        // 初始化后文件改变不能把新命令带入本进程的既有授权。
        std::fs::write(&config,b"changed after startup").unwrap();
        assert_eq!(crate::dispatch_hooks::runner("different-workspace"),runtime::HookRunner::default());
        let _=crate::session_store();
        let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let response=runtime.block_on(crate::run_model_tool_dispatch("read_file",&serde_json::json!({"path":fixture.file_path}))).unwrap();
        assert_eq!(response.status,"ok");assert_eq!(response.route,"runtime-executed");
        let output=response.tool_result_text.unwrap();
        let events=std::fs::read_to_string(root.join("events.txt")).unwrap_or_default();
        if mode=="granted" {assert_eq!(events,"PreToolUse\nPostToolUse\n");assert!(output.contains("由前置hook写入，供目标读取"));}
        else {assert!(events.is_empty());assert!(output.contains("hello-llm-runtime"));}
        std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&serde_json::json!({"mode":mode,"config_sha256":digest,"production_startup_loader":true,"runner_override":false,"dev_open_permissions":false,"events":events,"target_output":output,"status":response.status,"route":response.route,"cross_workspace_no_hooks":true,"config_snapshot_immutable":true})).unwrap()).unwrap();
    }
}
