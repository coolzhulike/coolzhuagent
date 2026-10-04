//! 真实固定伴随资源的隔离工程测试。受控调用元数据不冒充模型回包，不触碰正式会话。
use super::*;
use crate::tests::{config_test_guard, context_test_agent};

const SESSION: &str = "dsh-engineering-agent";
const ROOM: &str = "dsh-engineering-room";
const RUN: &str = "dsh-engineering-run";
const TURN: &str = "dsh-engineering-turn";

struct IsolatedScope {
    workspace: PathBuf,
    config: WorkspaceConfig,
    db: Option<PathBuf>,
    store: Option<SessionStore>,
}
impl IsolatedScope {
    fn install(root: &Path, db: PathBuf) -> Self {
        let workspace = std::mem::replace(
            &mut workspace_state().lock().unwrap().current,
            root.to_owned(),
        );
        let mut config = WorkspaceConfig::default();
        config.model.enable_llm_tools = true;
        config.model.llm_tool_exposure = Some("all".into());
        let config = std::mem::replace(&mut *workspace_config().lock().unwrap(), config);
        let previous_db = replace_session_db_path_override_for_test(Some(db.clone()));
        let session = PersistedSession {
            id: SESSION.into(),
            name: "DSH独立工程会话".into(),
            provider: "Custom".into(),
            model: "local-dsh-engineering".into(),
            avatar: None,
            base_url: Some("http://127.0.0.1:1/v1".into()),
            endpoint: None,
            reasoning_effort: "auto".into(),
            model_type: "text".into(),
            api_key_ref: String::new(),
            memory_beads: Vec::new(),
            created_at: 1,
            updated_at: 1,
            context_reset_at: 0,
            messages: Vec::new(),
        };
        let state = PersistedSessionState {
            sessions: vec![session],
            active_session_id: Some(SESSION.into()),
            active_vision_session_id: None,
            chat_rooms: vec![PersistedChatRoom {
                id: ROOM.into(),
                name: "DSH独立工程房间".into(),
                created_at: 1,
                updated_at: 1,
                messages: Vec::new(),
            }],
            active_chat_room_id: Some(ROOM.into()),
        };
        let store = SessionStore {
            history_edits: Vec::new(),
            committed_state: None,
            path: db,
            legacy_json_path: root.join("empty-session.json"),
            capacity: SessionStoreCapacity::default(),
            state,
        };
        let store = std::mem::replace(&mut *session_store().lock().unwrap(), store);
        Self {
            workspace,
            config,
            db: previous_db,
            store: Some(store),
        }
    }
}
impl Drop for IsolatedScope {
    fn drop(&mut self) {
        clear_pending_approvals_for_test();
        replace_session_db_path_override_for_test(self.db.take());
        *workspace_config().lock().unwrap() = self.config.clone();
        workspace_state().lock().unwrap().current = self.workspace.clone();
        if let Some(previous) = self.store.take() {
            *session_store().lock().unwrap() = previous;
        }
    }
}

async fn enable_request(id: &str, fingerprint: &str, workspace: &str) -> JsonValue {
    enable(Json(EnableRequest {
        expected_workspace: workspace.into(),
        id: id.into(),
        expected_source_sha256: fingerprint.into(),
        session_id: SESSION.into(),
        chat_room_id: ROOM.into(),
        config: json!({}),
    }))
    .await
    .unwrap()
    .0
}
fn approval(id: &str) -> PendingApprovalRecord {
    pending_approvals()
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .expect("原审批应存在")
}
fn is_enabled(manager: &plugins::PluginManager, id: &str) -> bool {
    manager
        .list_installed_plugins()
        .unwrap()
        .iter()
        .find(|item| item.metadata.id == id)
        .unwrap()
        .enabled
}
fn frozen_parent(
    db: &Path,
    workspace: &str,
) -> (
    FrozenParentContext,
    Arc<host_child_agent::HostModelSnapshot>,
) {
    let mut parent = FrozenParentContext::new(
        "dsh-engineering-dispatch",
        workspace,
        Some(ROOM),
        Some(SESSION),
        Some(TURN),
        Some(RUN),
    )
    .unwrap();
    parent.runtime_db_path = Some(db.into());
    parent.root_budget = Some(root_execution_budget::RootExecutionBudget::establish(
        180_000,
    ));
    let mut agent = context_test_agent();
    agent.id = SESSION.into();
    agent.provider = "Custom".into();
    agent.model = "local-dsh-engineering".into();
    agent.base_url = Some("http://127.0.0.1:1/v1".into());
    agent.memory_beads.clear();
    let host = Arc::new(
        host_child_agent::HostModelSnapshot::capture(&agent, Some(ROOM), &parent).unwrap(),
    );
    parent.host_model_snapshots = Arc::new(HashMap::from([(SESSION.into(), host.clone())]));
    (parent, host)
}
async fn dispatch(
    parent: &FrozenParentContext,
    host: &Arc<host_child_agent::HostModelSnapshot>,
    name: &str,
    provider_id: &str,
    input: JsonValue,
) -> ApiResult<ToolDispatchResponse> {
    tool_invocation_identity::scope(
        format!("{TURN}/engineering-request"),
        run_model_tool_dispatch_for_session_with_identity(
            name,
            &input,
            Some(SESSION),
            Some(provider_id),
            Some(TURN),
            Some(ROOM),
            Some(parent),
            Some(&host_child_agent::HostToolScope::Parent(host.clone())),
        ),
    )
    .await
}
fn ledger(db: &Path, provider_id: &str) -> (String, String, String, String) {
    let connection = open_session_connection(db).unwrap();
    connection.query_row("SELECT tool_call_id,status,run_id,source_request_key FROM tool_calls WHERE provider_tool_call_id=?1",
        params![provider_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap()
}
async fn pending_model(
    parent: &FrozenParentContext,
    host: &Arc<host_child_agent::HostModelSnapshot>,
    name: &str,
    provider_id: &str,
    expression: &str,
) -> PendingApprovalRecord {
    let response = dispatch(
        parent,
        host,
        name,
        provider_id,
        json!({"expression":expression}),
    )
    .await
    .unwrap();
    assert_eq!(
        response.route,
        "runtime-dry-run",
        "{}",
        response.notes.join("; ")
    );
    assert!(!response.dispatch_plan.unwrap().audit.executed);
    let (execution, status, run, scope) =
        ledger(parent.runtime_db_path.as_ref().unwrap(), provider_id);
    assert_ne!(
        execution, provider_id,
        "宿主编号与原provider编号必须分别保留"
    );
    assert_eq!(status, "awaiting_approval", "等待审批不能记成执行成功");
    assert_eq!(run, RUN);
    assert_eq!(scope, format!("{TURN}/engineering-request"));
    approval(&execution)
}

#[tokio::test]
async fn missing_provider_identity_is_refused_before_database_registration() {
    let result = run_model_tool_dispatch_for_session_with_identity(
        "dsh__unadmitted",
        &json!({}),
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    assert!(matches!(result, Err((StatusCode::CONFLICT, _))));
}

async fn verify_handshake_cancel(
    output: &Path,
    ipc: &Path,
    db: &Path,
    workspace: &str,
    name: &str,
    disable_snapshot: bool,
) -> JsonValue {
    let live_cancel = Arc::new(ChatTurnCancellation::new());
    let _live_cancel_scope = ToolTurnCancellationScope::install(TURN, live_cancel.clone());
    let (live_parent, live_host) = frozen_parent(db, workspace);
    let provider_id = if disable_snapshot {
        "engineering-provider-handshake-disable"
    } else {
        "engineering-provider-handshake-cancel"
    };
    let plugin_id = live_parent
        .dsh_bindings
        .get(name)
        .unwrap()
        .snapshot
        .plugin_id
        .clone();
    let live_record =
        pending_model(&live_parent, &live_host, name, provider_id, "12 * (3 + 5)").await;
    let observation = async {
        let deadline = Instant::now() + Duration::from_secs(25);
        let mut seen = BTreeMap::<String, JsonValue>::new();
        loop {
            for entry in std::fs::read_dir(ipc).unwrap().flatten() {
                if !entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("coolzhu-dsh-call-")
                {
                    continue;
                }
                let request = std::fs::read(entry.path().join("request.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<JsonValue>(&bytes).ok());
                let manifest = std::fs::read(entry.path().join("manifest.json"))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<JsonValue>(&bytes).ok());
                seen.insert(entry.file_name().to_string_lossy().into_owned(), json!({
                    "context":request.as_ref().map(|v| &v["context"]),"mode":request.as_ref().map(|v| &v["mode"]),
                    "manifest_present":manifest.is_some(),"generation":manifest.as_ref().map(|v| &v["manifest"]["generation"])}));
                if let (Some(request), Some(manifest)) = (request, manifest) {
                    if request["context"]["call_id"] == provider_id
                        && request["context"]["run_id"] == RUN
                        && request["mode"] == "execute"
                        && manifest["manifest"]["generation"].is_string()
                    {
                        if disable_snapshot {
                            extension_market::manager(&active_workspace_path())
                                .unwrap()
                                .disable(&plugin_id)
                                .unwrap();
                        } else {
                            live_cancel.request();
                        }
                        return json!({"observed_real_node_handshake":true,"context":request["context"],
                            "revocation":if disable_snapshot {"真实停用撤销快照"} else {"原父轮取消令牌"},
                            "generation":manifest["manifest"]["generation"],
                            "execute_envelope_present":entry.path().join("execute.json").is_file(),
                            "scope":"仅证明真实握手后取消，不声称calculator函数已经执行"});
                    }
                }
            }
            if Instant::now() >= deadline {
                live_cancel.request();
                return json!({"observed_real_node_handshake":false,"samples":seen,
                    "std_temp_dir":std::env::temp_dir(),"tempfile_temp_dir":tempfile::env::temp_dir()});
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    };
    let execution = async {
        let outcome = execute_approved_pending_record(&live_record, true).await;
        std::fs::write(
            output.join("live-execution-result.json"),
            serde_json::to_vec_pretty(&outcome).unwrap(),
        )
        .unwrap();
        outcome
    };
    let (cancelled, observation) = tokio::join!(execution, observation);
    let evidence = json!({"observation":observation,"outcome":cancelled.output});
    std::fs::write(
        output.join("live-cancel-observation.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    assert_eq!(
        evidence["observation"]["observed_real_node_handshake"], true,
        "必须观察实际Node握手，不能把未启动当取消成功：{evidence}"
    );
    assert_eq!(cancelled.status, ToolOutcomeStatus::Failed);
    assert_eq!(cancelled.output["code"], "cancelled");
    assert_eq!(
        cancelled.output["cleanup_confirmed"], true,
        "{}",
        cancelled.summary_text
    );
    assert_eq!(ledger(db, provider_id).1, "failed");
    assert!(!cancelled.output.to_string().contains("\"value\":96"));
    evidence
}

/// 复制测试二进制到独立bin，旁边放真实已核验的dsh-runtime；使用生产资源选择器。
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "需要DSH_DISPATCH_PROBE_ROOT独立空目录及官方固定源码，测试二进制伴随完整固定资源"]
async fn official_fixed_runtime_web_dispatch_and_stale_approval() {
    let _serial = config_test_guard();
    let output = PathBuf::from(std::env::var_os("DSH_DISPATCH_PROBE_ROOT").expect("工程输出目录"));
    std::fs::create_dir_all(&output).unwrap();
    assert_eq!(
        std::fs::read_dir(&output).unwrap().count(),
        0,
        "只使用独立空目录"
    );
    let root = output.join("coolzhuagent");
    std::fs::create_dir(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let db = root.join("web-sessions.sqlite3");
    let config_home = output.join("config-home");
    std::fs::create_dir(&config_home).unwrap();
    let _home = crate::test_env::set("CLAW_CONFIG_HOME", Some(config_home.as_os_str()));
    let ipc = output.join("private-ipc");
    std::fs::create_dir(&ipc).unwrap();
    let _temp = crate::test_env::set("TEMP", Some(ipc.as_os_str()));
    let _tmp = crate::test_env::set("TMP", Some(ipc.as_os_str()));
    assert_eq!(
        std::env::temp_dir().canonicalize().unwrap(),
        ipc.canonicalize().unwrap(),
        "协议观察只能读取本测试的私有临时目录"
    );
    let _scope = IsolatedScope::install(&root, db.clone());
    let fixed_source =
        PathBuf::from(std::env::var_os("DSH_DISPATCH_FIXED_SOURCE").expect("已审查官方源码"));
    let previous: JsonValue = serde_json::from_slice(
        &std::fs::read(std::env::var_os("DSH_DISPATCH_SOURCE_RECEIPT").expect("已审查来源回执"))
            .unwrap(),
    )
    .unwrap();
    let package: plugins::DshPackage =
        serde_json::from_value(previous["snapshot"]["package"].clone()).unwrap();
    package.verify(&fixed_source).unwrap();
    let verified = runtime::dsh_runtime::installed().expect("生产资源选择器应找到真实伴随固定资源");
    let workspace = workspace_identity(&root);
    let mut manager = extension_market::manager(&root).unwrap();
    let install = manager
        .install_dsh(&fixed_source, &package, "DSH独立工程验证")
        .unwrap();
    let plugin = install.plugin_id.clone();
    assert!(!is_enabled(&manager, &plugin));
    assert!(model_bindings(&root).unwrap().is_empty());
    let connection = open_session_connection(&db).unwrap();
    initialize_session_schema(&connection).unwrap();
    connection.execute("INSERT INTO sessions(id,name,provider,model,api_key_ref,created_at,updated_at) VALUES(?1,'DSH工程','Custom','local-dsh-engineering','',1,1)", params![SESSION]).unwrap();
    connection
        .execute(
            "INSERT INTO chat_rooms(id,name,created_at,updated_at) VALUES(?1,'DSH工程',1,1)",
            params![ROOM],
        )
        .unwrap();
    drop(connection);
    let fingerprint = package.source_fingerprint().unwrap();
    let request = enable_request(&plugin, &fingerprint, &workspace).await;
    assert_eq!(request["enabled"], false);
    let enable_record = approval(
        request["pending_call_id"]
            .as_str()
            .expect("默认必须明确审批"),
    );
    assert!(enable_record.dsh_action.is_some());
    assert!(!is_enabled(&manager, &plugin));
    let enabled = execute_approved_pending_record(&enable_record, true).await;
    assert_eq!(
        enabled.status,
        ToolOutcomeStatus::Ok,
        "{}",
        enabled.summary_text
    );
    let snapshot = manager.dsh_snapshot(&plugin, &workspace).unwrap().unwrap();
    assert_eq!(snapshot.runtime_lock_sha256, verified.runtime_lock_sha256);
    assert_eq!(
        execute_approved_pending_record(&enable_record, true)
            .await
            .status,
        ToolOutcomeStatus::Failed
    );
    create_chat_runtime_run_sqlite(
        &db,
        RUN,
        "engineering-claim",
        &workspace,
        Some(SESSION),
        ROOM,
        TURN,
    )
    .unwrap();
    assert!(start_chat_runtime_run_sqlite(&db, RUN, "engineering-claim").unwrap());
    let (parent, host) = frozen_parent(&db, &workspace);
    let (name, binding) = parent
        .dsh_bindings
        .iter()
        .next()
        .expect("实际SDK工具必须进入冻结父轮");
    let name = name.clone();
    assert_eq!(binding.raw_name, "calculator");
    let definition = host
        .parent_definitions
        .iter()
        .find(|tool| tool.name == name)
        .unwrap();
    assert_eq!(definition.input_schema, binding.definition.input_schema);
    let cancellation = Arc::new(ChatTurnCancellation::new());
    let _cancel_scope = ToolTurnCancellationScope::install(TURN, cancellation.clone());
    if std::env::var_os("DSH_DISPATCH_CANCEL_ONLY").is_some() {
        drop(_cancel_scope);
        let evidence = verify_handshake_cancel(&output, &ipc, &db, &workspace, &name, false).await;
        std::fs::write(
            output.join("result.json"),
            serde_json::to_vec_pretty(&evidence).unwrap(),
        )
        .unwrap();
        return;
    }
    let success_record = pending_model(
        &parent,
        &host,
        &name,
        "engineering-provider-success",
        "12 * (3 + 5)",
    )
    .await;
    let success = execute_approved_pending_record(&success_record, true).await;
    assert_eq!(
        success.status,
        ToolOutcomeStatus::Ok,
        "{}",
        success.summary_text
    );
    assert_eq!(success.output["result"]["isError"], false);
    assert_eq!(success.output["result"]["value"], 96);
    assert_eq!(
        success.output["provider_tool_call_id"],
        "engineering-provider-success"
    );
    assert_eq!(
        success.output["execution_id"],
        success_record.invoke.call_id
    );
    assert_eq!(ledger(&db, "engineering-provider-success").1, "completed");
    let raced_identity = tool_invocation_identity::ModelToolIdentity::from_source(
        Some(RUN),
        &format!("{TURN}/engineering-request"),
        "engineering-provider-immediate-approval",
    )
    .unwrap();
    let raced_id = raced_identity.execution_id.clone();
    let observer = tokio::spawn(async move {
        let deadline = Instant::now() + Duration::from_secs(25);
        loop {
            let record = pending_approvals().lock().unwrap().get(&raced_id).cloned();
            if let Some(record) = record {
                return execute_approved_pending_record(&record, true).await;
            }
            assert!(Instant::now() < deadline, "必须收到实际审批记录");
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    let raced_dispatch = dispatch(
        &parent,
        &host,
        &name,
        "engineering-provider-immediate-approval",
        json!({"expression":"12 * (3 + 5)"}),
    )
    .await
    .unwrap();
    assert_eq!(raced_dispatch.route, "runtime-dry-run");
    let raced = observer.await.unwrap();
    assert_eq!(
        raced.status,
        ToolOutcomeStatus::Ok,
        "{}",
        raced.summary_text
    );
    assert_eq!(raced.output["result"]["value"], 96);
    assert_eq!(
        ledger(&db, "engineering-provider-immediate-approval").1,
        "completed",
        "原派发收尾不能覆盖并发审批执行结果"
    );
    let duplicate = execute_approved_pending_record(&success_record, true).await;
    assert_eq!(duplicate.status, ToolOutcomeStatus::Failed);
    assert_eq!(ledger(&db, "engineering-provider-success").1, "completed");
    assert!(dispatch(
        &parent,
        &host,
        &name,
        "engineering-provider-success",
        json!({"expression":"99"})
    )
    .await
    .is_err());
    let failure_record = pending_model(
        &parent,
        &host,
        &name,
        "engineering-provider-failure",
        "unknownFunction(1)",
    )
    .await;
    let failure = execute_approved_pending_record(&failure_record, true).await;
    assert_eq!(failure.status, ToolOutcomeStatus::Failed);
    assert_eq!(failure.output["result"]["isError"], true);
    assert_eq!(ledger(&db, "engineering-provider-failure").1, "failed");
    // 仅本独立测试进程使用已有开发权限守卫；正式应用权限配置从未改动。
    let direct = {
        let _dev_open = crate::tests::DevOpenPermissionsTestGuard::enable();
        let (allowed_parent, allowed_host) = frozen_parent(&db, &workspace);
        let direct = dispatch(
            &allowed_parent,
            &allowed_host,
            &name,
            "engineering-provider-direct",
            json!({"expression":"12 * (3 + 5)"}),
        )
        .await
        .unwrap();
        assert_eq!(direct.status, "ok", "{}", direct.notes.join("; "));
        assert_eq!(
            direct.dispatch_plan.as_ref().unwrap().dry_run_input["result"]["value"],
            96
        );
        assert_eq!(ledger(&db, "engineering-provider-direct").1, "completed");
        direct.dispatch_plan.unwrap().dry_run_input
    };
    let mut expired = parent.clone();
    expired.root_budget = Some(root_execution_budget::RootExecutionBudget::from_started_at(
        1, 1,
    ));
    assert!(dispatch(
        &expired,
        &host,
        &name,
        "engineering-provider-expired",
        json!({"expression":"96"})
    )
    .await
    .is_err());
    let connection = open_session_connection(&db).unwrap();
    assert_eq!(connection.query_row("SELECT count(*) FROM tool_calls WHERE provider_tool_call_id='engineering-provider-expired'", [], |row| row.get::<_, i64>(0)).unwrap(), 0);
    drop(connection);
    let ended_record =
        pending_model(&parent, &host, &name, "engineering-provider-ended", "96").await;
    let connection = open_session_connection(&db).unwrap();
    connection
        .execute(
            "UPDATE runtime_runs SET state='completed' WHERE id=?1",
            params![RUN],
        )
        .unwrap();
    assert_eq!(
        execute_approved_pending_record(&ended_record, true)
            .await
            .status,
        ToolOutcomeStatus::Failed
    );
    assert_eq!(ledger(&db, "engineering-provider-ended").1, "failed");
    connection
        .execute(
            "UPDATE runtime_runs SET state='running' WHERE id=?1",
            params![RUN],
        )
        .unwrap();
    drop(connection);
    let disabled_record =
        pending_model(&parent, &host, &name, "engineering-provider-disabled", "96").await;
    manager.disable(&plugin).unwrap();
    assert!(model_bindings(&root).unwrap().is_empty());
    assert_eq!(
        execute_approved_pending_record(&disabled_record, true)
            .await
            .status,
        ToolOutcomeStatus::Failed
    );
    let obsolete_enable = enable_request(&plugin, &fingerprint, &workspace).await;
    let obsolete_enable = approval(obsolete_enable["pending_call_id"].as_str().unwrap());
    let reinstall = manager
        .install_dsh(&fixed_source, &package, "DSH独立同版重装")
        .unwrap();
    assert_ne!(install.operation_id, reinstall.operation_id);
    assert_eq!(
        execute_approved_pending_record(&obsolete_enable, true)
            .await
            .status,
        ToolOutcomeStatus::Failed
    );
    assert!(!is_enabled(&manager, &plugin));
    let fresh = enable_request(&plugin, &fingerprint, &workspace).await;
    let fresh = approval(fresh["pending_call_id"].as_str().unwrap());
    let reenabled = execute_approved_pending_record(&fresh, true).await;
    assert_eq!(
        reenabled.status,
        ToolOutcomeStatus::Ok,
        "{}",
        reenabled.summary_text
    );
    let new_snapshot = manager.dsh_snapshot(&plugin, &workspace).unwrap().unwrap();
    assert_ne!(snapshot.activation_id, new_snapshot.activation_id);
    assert!(dispatch(
        &parent,
        &host,
        &name,
        "engineering-provider-old-snapshot",
        json!({"expression":"96"})
    )
    .await
    .is_err());
    let (new_parent, new_host) = frozen_parent(&db, &workspace);
    let cancelled_record = pending_model(
        &new_parent,
        &new_host,
        &name,
        "engineering-provider-cancelled",
        "96",
    )
    .await;
    cancellation.request();
    let cancelled = execute_approved_pending_record(&cancelled_record, true).await;
    assert_eq!(cancelled.status, ToolOutcomeStatus::Failed);
    assert_eq!(ledger(&db, "engineering-provider-cancelled").1, "failed");
    assert!(!cancelled.output.to_string().contains("\"value\":96"));
    drop(_cancel_scope);
    let live_cancel_evidence =
        verify_handshake_cancel(&output, &ipc, &db, &workspace, &name, false).await;
    let live_disable_evidence =
        verify_handshake_cancel(&output, &ipc, &db, &workspace, &name, true).await;
    assert!(manager.dsh_snapshot(&plugin, &workspace).unwrap().is_none());
    std::fs::write(output.join("result.json"), serde_json::to_vec_pretty(&json!({
        "evidence_kind":"独立工程测试，非模型回包、非正式安装、非GUI验收",
        "model_requests":0,"credentials_created":0,"workspace_id":workspace,
        "runtime_lock_sha256":verified.runtime_lock_sha256,"sdk_lock_sha256":verified.sdk_lock_sha256,
        "source_fingerprint":fingerprint,"snapshot":snapshot,"new_snapshot":new_snapshot,
        "checks":{"default_disabled":true,"explicit_enable_existing_approval":true,"enable_replay_refused":true,
            "frozen_schema_matches_actual_sdk":true,"separate_canonical_and_raw_provider_ids":true,
            "awaiting_approval_not_completed":true,"model_replay_refused":true,"actual_sdk_failure":failure.output,
            "expired_root_no_registration":true,"ended_parent_refuses_old_approval":true,
            "disabled_old_snapshot_refused":true,"reinstall_invalidates_old_enable":true,"cancelled_parent_refused":true},
        "production_web_dispatch_calculate_96":success.output,
        "concurrent_approval_before_dispatch_returns":raced.output,
        "production_direct_dispatch_with_test_only_existing_permission_guard":direct,
        "node_handshake_then_cancel":live_cancel_evidence,
        "node_handshake_then_disable":live_disable_evidence
    })).unwrap()).unwrap();
}
