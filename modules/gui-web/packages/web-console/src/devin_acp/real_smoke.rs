//! 需用户明确授权并手动开启的真实免费模型验收；常规回归不会登录或发请求。
use super::{journal::{Binding, Journal}, process::ManagedProcess, session::{SessionService, Update}};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

#[tokio::test]
#[ignore = "需已登录账号、固定 CLI 与用户授权，仅运行真实免费 SWE-2 文本测试"]
async fn real_swe2_basic_conversation() {
    assert_eq!(std::env::var("COOLZHU_DEVIN_REAL_SMOKE").as_deref(), Ok("1"), "必须显式开启真实账号测试");
    let binary = super::discovery::binary().unwrap();
    let root = std::env::current_dir().unwrap().join("tmp").join(format!("devin-real-smoke-{}", crate::unix_timestamp_millis()));
    let cwd = root.join("workspace"); std::fs::create_dir_all(cwd.join(".devin")).unwrap();
    let version = super::transport::run_readonly(&binary, &["--version"], &cwd, Duration::from_secs(5)).await.unwrap();
    assert_eq!(std::str::from_utf8(&version).unwrap().trim(), "devin 3000.10.48 (fcf7ba39)");
    let catalog = super::transport::run_readonly(&binary, &["models","list","--format","json"], &cwd, Duration::from_secs(30)).await.unwrap();
    let catalog: Value = serde_json::from_slice(&catalog).unwrap();
    let requested = "swe-2-medium";
    assert!(catalog["families"].as_array().unwrap().iter().flat_map(|f| f["variants"].as_array().unwrap())
        .any(|v| v["model_uid"] == requested && v["cost_tier"] == "Free"), "没有目录证明的免费模型时不发送提示");
    let imports = json!({"agents_standard":false,"cursor":false,"windsurf":false,"claude":false,"copilot":false,"opencode":false,"zed":false});
    let deny = json!(["read","write","edit","exec","grep","glob","fetch","mcp"]);
    let config = root.join("config.json");
    std::fs::write(&config, serde_json::to_vec(&json!({"version":1,"auto_update":false,"subagents_enabled":false,
        "read_config_from":imports,"permissions":{"allow":[],"deny":deny,"ask":[]}})).unwrap()).unwrap();
    std::fs::write(cwd.join(".devin/config.json"), serde_json::to_vec(&json!({"read_config_from":imports,"hooks":{},
        "permissions":{"allow":[],"deny":deny,"ask":[]}})).unwrap()).unwrap();
    // 专项只发合成文本，不加载正式工程或挂载宿主工具；本机全局 MCP 必须为空。
    let global = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap()).join("devin/mcp_config.json");
    assert!(!global.exists(), "存在全局 MCP 配置时请先独立审查，smoke 不覆盖用户配置");
    let journal = Journal::open(&root.join("attempts.sqlite3")).unwrap();
    let binding = Binding {remote_session_id:None,cwd:cwd.to_string_lossy().into(),
        cli_identity:std::str::from_utf8(&version).unwrap().trim().into(),context_digest:"synthetic-text-only-v1".into()};
    let nonce = crate::random_hex_identifier(8, "会话测试标记").unwrap();
    let prompts = [format!("这是纯文本会话测试。不要调用任何工具。请记住本次标记 {nonce}。只回复：你好，已记住。"),
        "继续同一会话：上一轮让我记住的标记是什么？只回复这个标记，不要调用任何工具。".into(),
        "继续同一会话：请算 17 + 25，只回复数字，不要调用任何工具。".into()];
    let mut reports = vec![];
    for (index, prompt) in prompts.iter().enumerate() {
        let mut scope = super::journal::tests::scope(&format!("real-{}-{index}", crate::unix_timestamp_millis()));
        scope.turn_id = format!("turn-{index}");
        let claim = journal.claim(scope, &binding).unwrap();
        let config_arg = config.to_str().unwrap();
        let (mut process, transport) = ManagedProcess::spawn_fixture(&binary,
            &["--config",config_arg,"--permission-mode","auto","acp","--model",requested], &cwd, journal.clone(),claim.clone()).await.unwrap();
        let mut events = vec![];
        let connected = SessionService::connect(transport,journal.clone(),claim.clone(),requested,vec![],
            tokio::time::Instant::now()+Duration::from_secs(90),|e| {events.push(e);Ok(())}).await;
        let mut service = match connected {Ok(service)=>service,Err(error)=>{process.drain().await.unwrap();panic!("真实 ACP 配置失败：{error}");}};
        let result = service.prompt(prompt,Arc::new(crate::ChatTurnCancellation::new()),Duration::from_secs(5),|e|{events.push(e);Ok(())}).await;
        process.drain().await.unwrap();
        let result = result.unwrap();
        let reply: String = events.iter().filter(|e| !e.replay).filter_map(|e|match &e.update {
            Update::Text{text,thought:false,..}=>Some(text.as_str()),_=>None}).collect();
        let chunks = events.iter().filter(|e| !e.replay && matches!(&e.update,Update::Text{thought:false,..})).count();
        assert!(!events.iter().any(|e| matches!(&e.update,Update::ToolObservation{..})), "纯文本测试出现工具通知");
        let passed = match index {0=>reply.contains("你好"),1=>reply.contains(&nonce),_=>reply.trim()=="42"};
        reports.push(json!({"turn":index+1,"requested":requested,"effective":result.model.effective,
            "resolved_model":result.model.resolved_model,"stop_reason":result.stop_reason,"reply":reply,
            "text_chunks":chunks,"load_replay_events":events.iter().filter(|e|e.replay).count(),
            "process_drained":journal.status(&claim).unwrap().process_drained,"passed":passed}));
        std::fs::write(root.join("report.json"),serde_json::to_vec_pretty(&json!({"cli_version":"devin 3000.10.48 (fcf7ba39)",
            "cost_tier":"Free","cost_source":"authenticated CLI catalog","tests":reports,
            "scope":"临时工程内真实 ACP 文本与跨进程恢复；未开放正式聊天路由或工具"})).unwrap()).unwrap();
        println!("真实 SWE-2：turn={} chunks={} restored={} passed={}",index+1,chunks,index>0,passed);
        assert_eq!(result.stop_reason,"end_turn"); assert!(passed,"真实文本验收未达到预期，见 tmp 内报告");
    }
    println!("真实会话报告：{}", root.join("report.json").display());
}
