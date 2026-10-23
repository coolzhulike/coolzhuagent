// 由main::tests包含，复用真实Goal claim/finalizer夹具；不手工伪造命令成功。
fn ha09_save(name:&str,value:serde_json::Value) {
    if let Some(dir)=std::env::var_os("COOLZHU_HARNESS_EVIDENCE_DIR") {
        std::fs::write(std::path::PathBuf::from(dir).join(format!("ha09-{name}.json")),serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    }
}

#[tokio::test]
async fn ha09_phase_consumption_requires_actual_command_receipts() {
    let _serial=config_test_guard();
    for name in ["Passed","Failed","Disabled","Empty","Legacy","Absent","FilesExist"] {
        let temp=tempfile::tempdir().unwrap();let workspace=format!("ha09-{name}");
        let (mut store,context)=token_finalizer_fixture_with_role(&temp,&workspace,"implementer");
        let contract=match name {
            "Absent"=>None,
            "FilesExist"=>{let file=temp.path().join("real.txt");std::fs::write(&file,"actual artifact").unwrap();Some(json!({"type":"FilesExist","paths":[file]}))},
            "Empty"=>Some(json!({"type":"Command","commands":[]})),
            _=>Some(json!({"type":"Command","cwd":temp.path(),"timeout_ms":90_000,"commands":[if name=="Failed"{"exit 7"}else{"exit 0"}]})),
        };
        super::open_session_connection(&context.db_path).unwrap().execute(
            "UPDATE goal_phases SET verification_json=?1 WHERE goal_id=?2 AND id=?3",
            super::params![contract.as_ref().map(serde_json::Value::to_string),context.goal_id,context.phase_id]).unwrap();
        let result=if name=="Legacy" {super::goal_verification::Result::from(Vec::<String>::new())}
            else{super::goal_verification::evaluate(contract.as_ref(),name!="Disabled",temp.path()).await};
        let value=serde_json::to_value(&result).unwrap();
        let finalized=store.record_goal_phase_model_result_at_path_with_claim(&context.db_path,&workspace,&context.goal_id,&context.phase_id,&context.run_id,&context.claim_token,&context.agent,
            token_finalizer_response("工作结果待宿主验证"),Vec::new(),result).unwrap();
        let phase=finalized.status.goal.phases.iter().find(|p|p.id==context.phase_id).unwrap();
        ha09_save(name,json!({"gate":value,"status":phase.status,"verdict":phase.last_verdict,"evidence":phase.last_evidence}));
        let expected=matches!(name,"Passed"|"Absent"|"FilesExist");
        assert_eq!(phase.status=="completed",expected,"{name}: {:?}",phase.last_evidence);
        if name=="Passed" {assert_eq!(phase.last_evidence.as_ref().unwrap()["command_verification"]["receipts"][0]["exit_code"],0);}
        if !expected{assert_ne!(phase.last_verdict.as_deref(),Some("pass"));}
        if matches!(name,"Absent"|"FilesExist"){assert_eq!(value["state"],"not_required");assert!(value["receipts"].as_array().unwrap().is_empty());}
    }
}

#[tokio::test]
async fn ha09_missing_corrupt_contract_and_finalizer_storage_fault_are_not_passed() {
    let _serial=config_test_guard();let temp=tempfile::tempdir().unwrap();let workspace="ha09-faults";
    let (mut store,c)=token_finalizer_fixture_with_role(&temp,workspace,"implementer");
    let mut facts=Vec::new();
    for (goal,phase) in [("missing",c.phase_id.as_str()),(c.goal_id.as_str(),"missing")] {
        let r=super::run_goal_phase_command_gate(&c.db_path,workspace,goal,phase).await;
        assert_eq!(r.state,super::goal_verification::State::Unable);facts.push(serde_json::to_value(r).unwrap());
    }
    let db=super::open_session_connection(&c.db_path).unwrap();
    db.execute("UPDATE goal_phases SET verification_json='broken json' WHERE goal_id=?1",[&c.goal_id]).unwrap();
    let corrupt=super::run_goal_phase_command_gate(&c.db_path,workspace,&c.goal_id,&c.phase_id).await;
    assert_eq!(corrupt.state,super::goal_verification::State::Unable);facts.push(serde_json::to_value(corrupt).unwrap());
    let contract=json!({"type":"Command","commands":["exit 0"],"cwd":temp.path(),"timeout_ms":90_000});
    db.execute("UPDATE goal_phases SET verification_json=?1 WHERE goal_id=?2",super::params![contract.to_string(),c.goal_id]).unwrap();
    let r=super::goal_verification::evaluate(Some(&contract),true,temp.path()).await;
    assert_eq!(r.state,super::goal_verification::State::Passed);
    db.execute_batch("CREATE TRIGGER ha09_storage_fault BEFORE UPDATE ON goal_phases WHEN NEW.status='completed' BEGIN SELECT RAISE(FAIL,'HA09_EXPECTED_STORAGE_FAULT'); END;").unwrap();
    let failed=store.record_goal_phase_model_result_at_path_with_claim(&c.db_path,workspace,&c.goal_id,&c.phase_id,&c.run_id,&c.claim_token,&c.agent,
        token_finalizer_response("不能把存储失败说成完成"),Vec::new(),r);
    assert!(failed.is_err());let persisted: (String,Option<String>)=db.query_row("SELECT status,last_verdict FROM goal_phases WHERE goal_id=?1",[&c.goal_id],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    assert_eq!(persisted.0,"running");assert_ne!(persisted.1.as_deref(),Some("pass"));
    ha09_save("faults",json!({"unable_results":facts,"finalizer_error":failed.err().unwrap().1.error,"persisted_status":persisted.0,"persisted_verdict":persisted.1}));
}

#[tokio::test]
#[cfg(windows)]
async fn ha09_actual_cancel_and_timeout_have_receipts_and_no_late_effects() {
    let _serial=config_test_guard();let temp=tempfile::tempdir().unwrap();
    let timeout=super::goal_command::run_receipt("Start-Sleep -Seconds 10; Set-Content -LiteralPath late.txt late",temp.path(),250).await;
    assert_eq!(timeout.status,super::goal_command::Status::TimedOut);assert!(!temp.path().join("late.txt").exists());
    let token=std::sync::Arc::new(super::ChatTurnCancellation::new());let trace="ha09-actual-cancel";
    let _scope=super::ToolTurnCancellationScope::install(trace,token.clone());
    let mut pending=Box::pin(super::TURN_TRACE.scope(trace.into(),super::goal_command::run_receipt(
        "Set-Content -LiteralPath child.pid -Value $PID; Start-Sleep -Seconds 20; Set-Content -LiteralPath late.txt late",temp.path(),30_000)));
    let path=temp.path().join("child.pid");
    tokio::select!{
        r=&mut pending=>panic!("屏障前命令意外结束：{r:?}"),
        ()=async{tokio::time::timeout(std::time::Duration::from_secs(10),async{while std::fs::read_to_string(&path).ok().and_then(|s|s.trim().parse::<u32>().ok()).is_none(){tokio::time::sleep(std::time::Duration::from_millis(20)).await;}}).await.unwrap();}=>{}
    }
    let pid:u32=std::fs::read_to_string(path).unwrap().trim().parse().unwrap();token.request();
    let cancelled=tokio::time::timeout(std::time::Duration::from_secs(5),pending).await.unwrap();
    assert_eq!(cancelled.status,super::goal_command::Status::Cancelled);
    assert!(windows_process_guard::capture_live_process_identity(pid).is_err());assert!(!temp.path().join("late.txt").exists());
    ha09_save("cancel-timeout",json!({"timeout":timeout,"cancelled":cancelled,"child_pid":pid,"child_drained":true,"late_effect":false}));
}

#[cfg(windows)]
fn ha09_tool_message(call_id: &str) -> super::ChatMessageDto {
    super::ChatMessageDto {
        id: call_id.to_string(),
        author: "工具结果".to_string(),
        role: "assistant".to_string(),
        target: "测试".to_string(),
        content: "工具已执行".to_string(),
        kind: "tool-result".to_string(),
        attachments: Vec::new(),
    }
}

#[cfg(windows)]
fn ha09_run_powershell(call_id: &str, command: &str, cwd: &std::path::Path) -> (serde_json::Value, String) {
    use super::ToolInvocationExecutor;
    let outcome = super::RegistryExecutor::with_allowlist(vec!["PowerShell"]).execute(&super::ToolInvoke {
        call_id: call_id.to_string(),
        tool_name: "PowerShell".to_string(),
        input: json!({"command": command, "cwd": cwd, "timeout": 90}),
        caller: super::ToolCaller::Llm,
        workspace_id: "ha09-diagnostic".to_string(),
        session_id: Some("ha09-session".to_string()),
        user_authorized: true,
        user_confirmed_twice: false,
    });
    assert_eq!(outcome.status, runtime::ToolOutcomeStatus::Ok, "{}", outcome.summary_text);
    let text = outcome.output.as_str().expect("PowerShell 输出是 JSON 文本");
    let value = serde_json::from_str::<serde_json::Value>(text).expect("真实 PowerShell JSON");
    let receipt = value.get("executionReceipt").expect("工具真实回执必须存在");
    let id = receipt.get("id").and_then(serde_json::Value::as_str).expect("回执 ID").to_string();
    (value, id)
}

#[cfg(windows)]
fn ha09_line_count(path: &std::path::Path) -> usize {
    std::fs::read_to_string(path).map(|text| text.lines().count()).unwrap_or(0)
}

#[tokio::test]
#[cfg(windows)]
async fn ha09_phase_consumes_model_diagnostic_receipt_once() {
    let _serial = config_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let workspace = "ha09-phase-diagnostic";
    let (mut store, context) = token_finalizer_fixture_with_role(&temp, workspace, "implementer");
    let marker = temp.path().join("phase-diagnostic.txt");
    let command = format!("Add-Content -LiteralPath '{}' -Value phase; exit 0", marker.display());
    let (_, tool_receipt_id) = ha09_run_powershell("ha09-phase-call", &command, temp.path());
    assert_eq!(ha09_line_count(&marker), 1);
    let contract = json!({
        "type":"Command",
        "cwd":temp.path(),
        "timeout_ms":90_000,
        "commands":[command],
    });
    super::open_session_connection(&context.db_path).unwrap().execute(
        "UPDATE goal_phases SET verification_json=?1 WHERE goal_id=?2 AND id=?3",
        super::params![contract.to_string(), context.goal_id, context.phase_id],
    ).unwrap();
    let result = super::run_goal_phase_command_gate_with_receipts(
        &context.db_path, workspace, &context.goal_id, &context.phase_id,
        &[ha09_tool_message("ha09-phase-call")],
    ).await;
    assert_eq!(result.state, super::goal_verification::State::Passed, "{result:?}");
    assert_eq!(result.receipts[0].id, tool_receipt_id);
    let result_value = serde_json::to_value(&result).unwrap();
    let finalized = store.record_goal_phase_model_result_at_path_with_claim(
        &context.db_path, workspace, &context.goal_id, &context.phase_id,
        &context.run_id, &context.claim_token, &context.agent,
        token_finalizer_response("模型诊断已完成同一验证"), Vec::new(), result,
    ).unwrap();
    let phase = finalized.status.goal.phases.iter().find(|p| p.id == context.phase_id).unwrap();
    assert_eq!(phase.status, "completed");
    assert_eq!(phase.last_verdict.as_deref(), Some("pass"));
    let evidence = phase.last_evidence.as_ref().unwrap();
    assert_eq!(
        evidence["command_verification"]["receipts"][0]["id"].as_str(),
        Some(tool_receipt_id.as_str())
    );
    assert_eq!(ha09_line_count(&marker), 1, "phase finalizer 不得再次启动命令");
    ha09_save("phase-diagnostic", json!({
        "gate": result_value,
        "status": phase.status,
        "verdict": phase.last_verdict,
        "evidence": phase.last_evidence,
        "tool_receipt_id": tool_receipt_id,
        "line_count": 1,
    }));
}

#[tokio::test]
#[cfg(windows)]
async fn ha09_repair_result_receipt_is_reused_without_second_process() {
    let _serial = config_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let workspace = "ha09-repair-receipt";
    let (mut store, context) = token_finalizer_fixture_with_role(&temp, workspace, "implementer");
    let marker = temp.path().join("repair-once.txt");
    let command = format!("Add-Content -LiteralPath '{}' -Value repair; exit 0", marker.display());
    let prior = super::goal_command::run_receipt(&command, temp.path(), 90_000).await;
    assert_eq!(prior.status, super::goal_command::Status::Passed, "{prior:?}");
    assert_eq!(ha09_line_count(&marker), 1);
    let contract = json!({
        "type":"Command",
        "cwd":temp.path(),
        "timeout_ms":90_000,
        "commands":[command],
    });
    let db = super::open_session_connection(&context.db_path).unwrap();
    db.execute(
        "UPDATE goal_phases SET verification_json=?1, last_evidence_json=?2 WHERE goal_id=?3 AND id=?4",
        super::params![
            contract.to_string(),
            json!({"repair_result":{"run_id":context.run_id,"command_verification":{"state":"passed","receipts":[prior.clone()]}}}).to_string(),
            context.goal_id,
            context.phase_id,
        ],
    ).unwrap();
    let reused = super::run_goal_phase_command_gate_with_receipts(
        &context.db_path, workspace, &context.goal_id, &context.phase_id, &[],
    ).await;
    assert_eq!(reused.state, super::goal_verification::State::Passed, "{reused:?}");
    assert_eq!(reused.receipts.len(), 1);
    assert_eq!(reused.receipts[0].id, prior.id);
    assert_eq!(ha09_line_count(&marker), 1, "repair 回执被复用时不得再启动第二个进程");
    // 让 fixture 正常收尾，避免临时 store 在测试结束时持有未完成运行。
    let _ = store.save();
    ha09_save("repair-receipt", json!({
        "prior": prior,
        "reused": reused,
        "line_count": 1,
    }));
}

#[tokio::test]
#[cfg(windows)]
async fn ha09_diagnostic_receipt_is_consumed_once_or_runtime_executes() {
    let _serial = config_test_guard();
    let temp = tempfile::tempdir().unwrap();
    let cwd = temp.path();
    let reused_path = cwd.join("reused.txt");
    let reused_command = format!("Add-Content -LiteralPath '{}' -Value run; exit 0", reused_path.display());
    let (tool_value, tool_receipt_id) = ha09_run_powershell("ha09-reused-call", &reused_command, cwd);
    assert_eq!(ha09_line_count(&reused_path), 1);
    let contract = json!({"type":"Command","cwd":cwd,"timeout_ms":90_000,"commands":[reused_command]});
    let reused = super::goal_verification::evaluate_with_receipts(
        Some(&contract), true, cwd, &[ha09_tool_message("ha09-reused-call")],
    ).await;
    assert_eq!(reused.state, super::goal_verification::State::Passed, "{reused:?}");
    assert_eq!(reused.receipts.len(), 1);
    assert_eq!(reused.receipts[0].id, tool_receipt_id);
    assert_eq!(ha09_line_count(&reused_path), 1, "runtime 不得重复启动已回执命令");

    let fallback_path = cwd.join("fallback.txt");
    let fallback_command = format!("Add-Content -LiteralPath '{}' -Value run; exit 0", fallback_path.display());
    let fallback_contract = json!({"type":"Command","cwd":cwd,"timeout_ms":90_000,"commands":[fallback_command]});
    let fallback = super::goal_verification::evaluate_with_receipts(Some(&fallback_contract), true, cwd, &[]).await;
    assert_eq!(fallback.state, super::goal_verification::State::Passed, "{fallback:?}");
    assert_eq!(ha09_line_count(&fallback_path), 1);
    assert!(fallback.receipts[0].id.starts_with("goal-command-"));

    let mismatch_path = cwd.join("mismatch.txt");
    let mismatch_command = format!("Add-Content -LiteralPath '{}' -Value runtime; exit 0", mismatch_path.display());
    let (_, mismatch_tool_id) = ha09_run_powershell("ha09-mismatch-call", "Write-Output noop; exit 0", cwd);
    let mismatch_contract = json!({"type":"Command","cwd":cwd,"timeout_ms":90_000,"commands":[mismatch_command]});
    let mismatch = super::goal_verification::evaluate_with_receipts(
        Some(&mismatch_contract), true, cwd, &[ha09_tool_message("ha09-mismatch-call")],
    ).await;
    assert_eq!(mismatch.state, super::goal_verification::State::Passed, "{mismatch:?}");
    assert_eq!(ha09_line_count(&mismatch_path), 1);
    assert_ne!(mismatch.receipts[0].id, mismatch_tool_id, "不匹配回执必须由 runtime 重新执行");

    ha09_save("diagnostic-receipts", json!({
        "tool": tool_value,
        "reused": reused,
        "fallback": fallback,
        "mismatch": mismatch,
        "reused_line_count": 1,
        "fallback_line_count": 1,
        "mismatch_line_count": 1,
    }));
}
