// 使用真实 HTTP 桥与宿主文件工具验收续读；不启动远端模型或正式会话。
#[tokio::test]
async fn long_result_http_acceptance_reads_once_and_resumes_complete_payload() {
    let _guard = crate::tests::config_test_guard();
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("sessions.sqlite3");
    let _env = Environment::install(dir.path(), &db);
    let (bridge, journal, scope) = fixture(dir.path(), &db);
    journal.transition(&scope, &["prepared"], "submitted", None, true).unwrap();
    let file = dir.path().join("long-result.txt");
    let original = format!("HEAD-0.2|{}|尾部-完整", "0123456789abcdef".repeat(6000));
    std::fs::write(&file, &original).unwrap();
    let server = bridge.clone().start().await.unwrap();
    let config = server.config();
    let url = config["url"].as_str().unwrap();
    let token = config["headers"][0]["value"].as_str().unwrap();
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().unwrap();
    let init = client.post(url).header("Authorization", token)
        .json(&json!({"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":MCP_VERSION}}))
        .send().await.unwrap();
    assert_eq!(init.status(), StatusCode::OK);
    let first: JsonValue = client.post(url).header("Authorization", token)
        .header("MCP-Protocol-Version", MCP_VERSION)
        .json(&json!({"jsonrpc":"2.0","id":"single-read","method":"tools/call","params":{"name":"read_file","arguments":{"path":file}}}))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_eq!(first["result"]["isError"], false, "{first}");
    let mut page: JsonValue = serde_json::from_str(first["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    let id = page["result_id"].as_str().expect("长回执必须具有续读编号").to_owned();
    let hash = page["sha256"].as_str().unwrap().to_owned();
    let total = page["total_bytes"].as_u64().unwrap() as usize;
    assert!(total > 96_000);
    let mut restored = String::new();
    let mut count = 0;
    loop {
        count += 1;
        assert!(count < 80, "续读不得无限循环");
        assert_eq!(page["result_id"], id);
        assert_eq!(page["sha256"], hash);
        assert_eq!(page["offset"].as_u64().unwrap() as usize, restored.len());
        assert!(page.to_string().len() <= 6000);
        restored.push_str(page["result_text"].as_str().unwrap());
        println!("LONG_RESULT_PAGE number={count} offset={} total={total}", restored.len());
        assert_eq!(page["next_offset"].as_u64().unwrap() as usize, restored.len());
        if page["complete"] == true { break; }
        assert_eq!(page["resume_offset"].as_u64().unwrap() as usize, restored.len());
        let reply: JsonValue = client.post(url).header("Authorization", token)
            .header("MCP-Protocol-Version", MCP_VERSION)
            .json(&json!({"jsonrpc":"2.0","id":format!("page-{count}"),"method":"tools/call","params":{"name":"tool_result_read","arguments":{"result_id":id,"offset":restored.len()}}}))
            .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        assert_eq!(reply["result"]["isError"], false, "{reply}");
        page = serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    }
    assert_eq!(restored.len(), total);
    assert_eq!(super::super::chat::digest(restored.as_bytes()), hash);
    let outcome: JsonValue = serde_json::from_str(&restored).unwrap();
    // RegistryExecutor 的 output 是工具原始 JSON 字符串，先按真实契约解包。
    let output: JsonValue = serde_json::from_str(outcome["output"].as_str().expect("文件工具原始JSON")).unwrap();
    assert!(output["file"]["content"].as_str() == Some(original.as_str()), "回执必须与实际文件全文相同");
    let rows = computer_use_store::ComputerUseRunStore::open(&db).unwrap().tool_calls_for_run(&scope.run_id).unwrap();
    assert_eq!(rows.len(), 1, "续读不能重放原工具或新建工具执行");
    assert_eq!(rows[0].status, "completed");
    journal.request_cancel(&scope).unwrap();
    let denied: JsonValue = client.post(url).header("Authorization", token)
        .header("MCP-Protocol-Version", MCP_VERSION)
        .json(&json!({"jsonrpc":"2.0","id":"after-cancel","method":"tools/call","params":{"name":"tool_result_read","arguments":{"result_id":id,"offset":0}}}))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_eq!(denied["result"]["isError"], true, "取消后不得继续读取私有回执");
    println!("LONG_RESULT_ACCEPTANCE pages={count} bytes={total} sha256={hash} tool_executions=1 cancelled_read_denied=true");
}
