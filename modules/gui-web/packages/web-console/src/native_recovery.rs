//! 原生桌面恢复确认：浏览器只申请 challenge，已注册 Tauri 通过本机管道批准一次。
//! 这是当前用户控制面的确认，不宣称 Windows Hello / 重新认证，也不以同 SID 代替人工。
use super::*;
use crate::native_recovery_store::RecoverySnapshot;
use sha2::{Digest, Sha256};
use std::fs;
use windows_process_guard::{
    process_peer_identity, LocalProcessPeer, LocalRecoveryPipeServer, ProcessIdentityError,
};

const CONFIRMATION_TTL_MS: u64 = 90_000;
const MAX_PENDING: usize = 64;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(super) struct RecoveryChallengeRequest {
    pub reason: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    #[serde(default)]
    pub acknowledged_block_ids: Vec<String>,
    #[serde(default)]
    pub acknowledged_run_ids: Vec<String>,
}
impl RecoveryChallengeRequest {
    fn normalize(mut self) -> Result<Self, String> {
        self.reason = self.reason.trim().into();
        if self.reason.is_empty() || self.reason.len() > 4000 || self.reason.contains('\0') {
            return Err("请填写简短、明确且不含控制字符的恢复原因".into());
        }
        for list in [
            &mut self.evidence_refs,
            &mut self.acknowledged_block_ids,
            &mut self.acknowledged_run_ids,
        ] {
            if list.len() > 128
                || list
                    .iter()
                    .any(|value| value.len() > 1024 || value.contains('\0'))
            {
                return Err("恢复申请内容过长或含控制字符".into());
            }
            for value in list.iter_mut() {
                *value = value.trim().into();
            }
            list.retain(|value| !value.is_empty());
            list.sort();
            list.dedup();
        }
        Ok(self)
    }
    fn digest(&self) -> Result<String, String> {
        Ok(format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).map_err(|e| e.to_string())?)
        ))
    }
}
#[derive(Clone)]
struct Challenge {
    id: String,
    request: RecoveryChallengeRequest,
    digest: String,
    snapshot: RecoverySnapshot,
    expires: u64,
}
#[derive(Clone)]
struct Approval {
    challenge: Challenge,
    operator: String,
    shell: LocalProcessPeer,
}
#[derive(Default)]
struct ControlState {
    launcher: Option<LocalProcessPeer>,
    shell: Option<LocalProcessPeer>,
    expected_shell: Option<PathBuf>,
    challenges: HashMap<String, Challenge>,
    proofs: HashMap<String, Approval>,
    unavailable: Option<String>,
}
fn state() -> &'static Mutex<ControlState> {
    static STATE: OnceLock<Mutex<ControlState>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(ControlState::default()))
}
fn secret() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
fn same_path(actual: Option<&str>, expected: &Path) -> bool {
    actual
        .and_then(|v| fs::canonicalize(v).ok())
        .zip(fs::canonicalize(expected).ok())
        .is_some_and(|(a, b)| {
            a.to_string_lossy()
                .eq_ignore_ascii_case(&b.to_string_lossy())
        })
}
fn same_peer(actual: &LocalProcessPeer, expected: &LocalProcessPeer) -> bool {
    actual.process.is_same_instance(&expected.process)
        && actual.user_sid == expected.user_sid
        && actual.process.image_path() == expected.process.image_path()
}
fn verified_shell(state: &ControlState, peer: &LocalProcessPeer) -> Result<(), String> {
    match &state.shell {
        Some(shell) if same_peer(peer, shell) => Ok(()),
        _ => Err("只有本次启动登记的桌面控制台可以确认恢复".into()),
    }
}
fn root_scope() -> Result<(PathBuf, runtime::InputSafetyResourceScope), String> {
    Ok((
        input_safety_store::input_safety_state_root().ok_or("输入安全状态根未注入")?,
        input_safety_store::physical_input_resource_scope().map_err(|e| e.reason())?,
    ))
}
fn snapshot() -> Result<RecoverySnapshot, String> {
    let (root, scope) = root_scope()?;
    input_safety_store::InputSafetyStore::open_at(&root)
        .map_err(|e| e.to_string())?
        .native_recovery_store()
        .snapshot(&scope)
}
fn view(challenge: &Challenge) -> serde_json::Value {
    json!({"challenge_id":challenge.id,"scope":challenge.snapshot.scope,"reason":challenge.request.reason,
        "acknowledged_block_ids":challenge.request.acknowledged_block_ids,"acknowledged_run_ids":challenge.request.acknowledged_run_ids,
        "evidence_refs":challenge.request.evidence_refs,"blocks":challenge.snapshot.blocks,
        "gate_revision":challenge.snapshot.gate_revision,"recovery_epoch":challenge.snapshot.recovery_epoch,
        "unsettled_permits":challenge.snapshot.permits,"executor_facts":challenge.snapshot.executors,
        "expires_at_unix_ms":challenge.expires})
}

pub(super) fn available() -> bool {
    let shell = state().lock().ok().and_then(|state| state.shell.clone());
    shell.is_some_and(|shell| {
        process_peer_identity(shell.process.pid()).is_ok_and(|actual| same_peer(&actual, &shell))
    })
}

/// 主函数最早阶段调用，冻结 OS 实际父 launcher；任何环境字符串都不是授权来源。
pub(super) fn start_control_plane() -> Result<(), String> {
    let own = process_peer_identity(std::process::id()).map_err(|e| e.to_string())?;
    let launcher = process_peer_identity(own.parent_pid)
        .map_err(|e| format!("恢复控制面未登记产品 launcher：{e}"))?;
    let own_path = env::current_exe().map_err(|e| e.to_string())?;
    let bin = own_path.parent().ok_or("后台目录不可用")?;
    let expected_launchers = [
        bin.join("COOLZHU-AGENT.exe"),
        bin.parent().unwrap_or(bin).join("COOLZHU-AGENT.exe"),
    ];
    if own.user_sid != launcher.user_sid
        || !expected_launchers
            .iter()
            .any(|p| same_path(launcher.process.image_path(), p))
    {
        return Err("当前后台不是由已安装产品 launcher 启动；恢复需完整重启桌面应用".into());
    }
    let name = windows_process_guard::recovery_pipe_name(&own.process);
    let mut server = LocalRecoveryPipeServer::bind(&name).map_err(|e| e.to_string())?;
    {
        let mut state = state().lock().map_err(|_| "恢复状态锁损坏")?;
        state.launcher = Some(launcher);
        state.expected_shell = Some(bin.join("coolzhu-tauri-shell.exe"));
        state.unavailable = None;
    }
    std::thread::Builder::new()
        .name("native-recovery-control".into())
        .spawn(move || loop {
            match server.serve_one(|peer, bytes| {
                let response = handle_pipe(peer, bytes)
                    .unwrap_or_else(|error| json!({"ok":false,"error":error}));
                serde_json::to_vec(&response)
                    .unwrap_or_else(|_| br#"{"ok":false,"error":"response_failed"}"#.to_vec())
            }) {
                Ok(true) => {}
                Ok(false) => std::thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    tracing::warn!("原生恢复管道连接拒绝：{error}");
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn handle_pipe(peer: LocalProcessPeer, bytes: &[u8]) -> Result<serde_json::Value, String> {
    let request: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "恢复控制面请求无效")?;
    let op = request
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or("缺少控制面操作")?;
    if op == "register_shell" {
        let (launcher, expected) = {
            let state = state().lock().map_err(|_| "恢复状态锁损坏")?;
            (
                state.launcher.clone().ok_or("未登记 launcher")?,
                state.expected_shell.clone().ok_or("未登记安装位置")?,
            )
        };
        if !same_peer(&peer, &launcher) {
            return Err("只有实际启动本后台的 launcher 可以登记控制台".into());
        }
        let pid = request
            .get("pid")
            .and_then(|v| v.as_u64())
            .and_then(|v| u32::try_from(v).ok())
            .ok_or("缺少 shell PID")?;
        let created = request
            .get("creation_time_filetime")
            .and_then(|v| v.as_u64())
            .ok_or("缺少 shell 创建时间")?;
        let shell = process_peer_identity(pid).map_err(|e| e.to_string())?;
        if shell.process.creation_time_filetime() != created
            || shell.parent_pid != launcher.process.pid()
            || shell.user_sid != launcher.user_sid
            || !same_path(shell.process.image_path(), &expected)
        {
            return Err("桌面实例的实际父进程、创建身份、用户或安装路径不符".into());
        }
        let mut state = state().lock().map_err(|_| "恢复状态锁损坏")?;
        if state
            .shell
            .as_ref()
            .is_some_and(|existing| !same_peer(existing, &shell))
        {
            return Err("本次后台已绑定其他桌面实例，请完整重启应用".into());
        }
        state.shell = Some(shell);
        return Ok(json!({"ok":true}));
    }
    let id = request
        .get("challenge_id")
        .and_then(|v| v.as_str())
        .ok_or("缺少恢复 challenge")?;
    let challenge = {
        let state = state().lock().map_err(|_| "恢复状态锁损坏")?;
        verified_shell(&state, &peer)?;
        state
            .challenges
            .get(id)
            .cloned()
            .ok_or("恢复申请不存在、已使用或已过期")?
    };
    if unix_timestamp_millis() >= challenge.expires {
        return Err("恢复申请已过期，请重新申请".into());
    }
    if snapshot()? != challenge.snapshot {
        return Err("阻断或执行状态已经变化，请重新申请".into());
    }
    match op {
        "inspect_challenge" => Ok(json!({"ok":true,"challenge":view(&challenge)})),
        "approve_recovery" => {
            let proof = secret()?;
            let mut state = state().lock().map_err(|_| "恢复状态锁损坏")?;
            verified_shell(&state, &peer)?;
            if state.challenges.remove(id).is_none() {
                return Err("恢复申请已经被使用".into());
            }
            state.proofs.insert(
                proof.clone(),
                Approval {
                    challenge,
                    operator: peer.user_sid.clone(),
                    shell: peer.clone(),
                },
            );
            Ok(json!({"ok":true,"proof":proof,"operator":peer.user_sid}))
        }
        _ => Err("未知恢复控制面操作".into()),
    }
}

pub(super) async fn challenge(
    Json(request): Json<RecoveryChallengeRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let request = request
        .normalize()
        .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?;
    let snapshot = snapshot().map_err(|e| api_error(StatusCode::SERVICE_UNAVAILABLE, &e))?;
    let expected = snapshot
        .blocks
        .iter()
        .map(|b| b.block_id.clone())
        .collect::<Vec<_>>();
    if request.acknowledged_block_ids != expected {
        return Err(api_error(
            StatusCode::CONFLICT,
            "阻断集合已变化，请刷新恢复页面后重试",
        ));
    }
    // 常驻浏览器与控制台共用进程，不能让用户先批准一个注定无法提交的申请。
    // 这里只读核验；完整退出后仍须重新申请人工确认，不在此处清账或开放输入。
    confirmed_executor_exits(&snapshot)
        .map_err(|e| api_error(StatusCode::CONFLICT, &e))?;
    let now = unix_timestamp_millis();
    let challenge = Challenge {
        id: secret().map_err(|e| api_error(StatusCode::SERVICE_UNAVAILABLE, &e))?,
        digest: request
            .digest()
            .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?,
        request,
        snapshot,
        expires: now + CONFIRMATION_TTL_MS,
    };
    let native_available = available();
    let mut state = state()
        .lock()
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "恢复状态锁损坏"))?;
    state.challenges.retain(|_, value| value.expires > now);
    state
        .proofs
        .retain(|_, value| value.challenge.expires > now);
    if state.challenges.len() + state.proofs.len() >= MAX_PENDING {
        return Err(api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "恢复申请过多，请等待旧申请失效",
        ));
    }
    let mut result = view(&challenge);
    result["native_available"] = json!(native_available);
    if serde_json::to_vec(&result)
        .map_err(|e| api_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .len()
        > 28 * 1024
    {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "待确认事实过多，无法完整显示在原生确认中，请先完成执行状态核查",
        ));
    }
    state
        .challenges
        .insert(challenge.id.clone(), challenge.clone());
    if !native_available {
        result["native_unavailable_reason"]=json!("请从安装目录的 COOLZHU-AGENT 启动桌面应用；普通浏览器不能批准恢复。后台复用但控制台未登记时需完整退出后重启。");
    }
    Ok(Json(result))
}

pub(super) struct ConfirmedRecovery {
    challenge: Challenge,
    pub operator: String,
}
impl ConfirmedRecovery {
    pub(super) fn operation_id(&self) -> String {
        format!("operator-release-{}", self.challenge.id)
    }
}
pub(super) fn consume_confirmation(
    request: &ReleaseIsolationRequest,
) -> ApiResult<ConfirmedRecovery> {
    let approval = state()
        .lock()
        .map_err(|_| api_error(StatusCode::SERVICE_UNAVAILABLE, "恢复状态锁损坏"))?
        .proofs
        .remove(&request.proof)
        .ok_or_else(|| {
            api_error(
                StatusCode::FORBIDDEN,
                "需要桌面原生确认；确认凭据缺失、已用或已过期",
            )
        })?;
    let payload = RecoveryChallengeRequest {
        reason: request.reason.clone(),
        evidence_refs: request.evidence_refs.clone(),
        acknowledged_block_ids: request.acknowledged_block_ids.clone(),
        acknowledged_run_ids: request.acknowledged_run_ids.clone(),
    }
    .normalize()
    .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?;
    let current = process_peer_identity(approval.shell.process.pid())
        .map_err(|e| api_error(StatusCode::FORBIDDEN, &e.to_string()))?;
    if !same_peer(&current, &approval.shell)
        || request.operator != approval.operator
        || request.challenge_id != approval.challenge.id
        || unix_timestamp_millis() >= approval.challenge.expires
        || payload
            .digest()
            .map_err(|e| api_error(StatusCode::BAD_REQUEST, &e))?
            != approval.challenge.digest
    {
        return Err(api_error(
            StatusCode::FORBIDDEN,
            "恢复确认不属于本次桌面实例/申请内容，或已经过期",
        ));
    }
    Ok(ConfirmedRecovery {
        challenge: approval.challenge,
        operator: approval.operator,
    })
}

pub(super) fn commit_confirmed_release(
    coordinator: &input_safety_store::InputSafetyCoordinator,
    confirmation: &ConfirmedRecovery,
    decision: &runtime::ReleaseIsolationDecision,
) -> Result<runtime::ReleaseIsolationDecision, String> {
    if decision.scope.as_str() != confirmation.challenge.snapshot.scope {
        return Err("确认不能跨资源使用".into());
    }
    // 申请时的检查不能替代提交时的 OS 身份复核。
    let exits = confirmed_executor_exits(&confirmation.challenge.snapshot)?;
    coordinator.store().native_recovery_store().commit_release(
        &confirmation.challenge.snapshot,
        decision,
        coordinator.control(),
        &exits,
        confirmation.challenge.expires,
    )
}

fn confirmed_executor_exits(
    snapshot: &RecoverySnapshot,
) -> Result<Vec<(String, runtime::ProcessInstanceEvidence)>, String> {
    let mut exits = Vec::new();
    for executor in &snapshot.executors {
        if executor.state == "exited_confirmed" {
            continue;
        }
        let created = executor
            .created
            .filter(|v| *v > 0)
            .ok_or("执行者缺少可信创建身份，不能自动判断已退出")?;
        if executor.pid == 0 {
            return Err("执行者缺少可信PID".into());
        }
        let exited = match windows_process_guard::capture_live_process_identity(executor.pid) {
            Ok(current) => current.creation_time_filetime() != created,
            Err(ProcessIdentityError::NotFound { .. }) => true,
            Err(error) => return Err(format!("执行者状态无法核查，保持隔离：{error}")),
        };
        if !exited {
            return Err(match executor.kind {
                crate::persistent_panel_executor::InputExecutorKind::PersistentNativePanel => format!(
                    "内置浏览器的旧操作仍绑定桌面宿主（PID {}）。请通过窗口右上角关闭按钮或托盘“退出”完整退出 CoolzhuAgent，再从桌面快捷方式重新启动后申请恢复。仅关闭浏览器扩展栏不能结束该宿主。历史未知结果将保留，重启不会自动放行输入。",
                    executor.pid
                ),
                crate::persistent_panel_executor::InputExecutorKind::EphemeralHelper => format!(
                    "执行者 {}（PID {}）仍在运行，请先停止当前任务并确认执行进程退出后再申请恢复；输入保持隔离。",
                    executor.id, executor.pid
                ),
            });
        }
        exits.push((
            executor.id.clone(),
            runtime::ProcessInstanceEvidence {
                pid: executor.pid,
                creation_time_filetime: created,
            },
        ));
    }
    Ok(exits)
}

pub(super) fn settle_attempt(
    coordinator: &input_safety_store::InputSafetyCoordinator,
    opened: bool,
    detail: &str,
) -> Result<(), String> {
    coordinator
        .store()
        .settle_recovery_operation_authorized(
            coordinator.control().recovery_id(),
            coordinator.control(),
            if opened {
                runtime::RecoveryDisposition::Recovered
            } else {
                runtime::RecoveryDisposition::HumanReviewRequired
            },
            detail,
        )
        .map_err(|e| e.to_string())?;
    coordinator
        .store()
        .release_recovery_epoch(&input_safety_store::OwnedRecoveryEpoch {
            scope: coordinator.control().resource_scope().clone(),
            epoch: coordinator.control().epoch(),
            coordinator_instance_id: coordinator.control().coordinator_id().into(),
        })
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn live_executor_snapshot() -> RecoverySnapshot {
        let process = windows_process_guard::capture_live_process_identity(std::process::id())
            .expect("读取真实测试进程身份");
        RecoverySnapshot {
            scope: "native-recovery-preflight-test".into(),
            gate_revision: 1,
            recovery_epoch: 1,
            resource_epoch: 1,
            blocks: vec![],
            permits: vec![],
            executors: vec![crate::native_recovery_store::RecoveryExecutor {
                id: "live-executor".into(),
                pid: process.pid(),
                created: Some(process.creation_time_filetime()),
                revision: 1,
                state: "verified_alive".into(),
                kind: crate::persistent_panel_executor::InputExecutorKind::PersistentNativePanel,
            }],
        }
    }

    #[test]
    fn live_panel_requires_complete_restart_and_helper_requires_stop() {
        // 用真实 OS 进程核验，不伪造退出、不向桌面发送任何输入。
        let mut snapshot = live_executor_snapshot();
        let before = snapshot.clone();
        let error = confirmed_executor_exits(&snapshot).unwrap_err();
        assert!(error.contains("完整退出 CoolzhuAgent") && error.contains("重启不会自动放行"));
        assert!(error.contains("仅关闭浏览器扩展栏不能"));
        assert_eq!(snapshot, before, "预检不得修改原始事实");
        snapshot.executors[0].kind =
            crate::persistent_panel_executor::InputExecutorKind::EphemeralHelper;
        let error = confirmed_executor_exits(&snapshot).unwrap_err();
        assert!(error.contains("停止当前任务") && !error.contains("完整退出 CoolzhuAgent"));
    }

    #[test]
    fn gone_instance_and_reused_pid_produce_original_exit_evidence() {
        let mut snapshot = live_executor_snapshot();
        // PID 已复用时按原创建身份结账；不能将现在的进程身份写进旧记录。
        snapshot.executors[0].created = Some(snapshot.executors[0].created.unwrap() + 1);
        let exits = confirmed_executor_exits(&snapshot).unwrap();
        assert_eq!(exits[0].1.creation_time_filetime, snapshot.executors[0].created.unwrap());
        snapshot.executors[0].pid = 0xFFFF_FFF0; // Windows 不会分配的 PID。
        let exits = confirmed_executor_exits(&snapshot).unwrap();
        assert_eq!(exits[0].1.pid, 0xFFFF_FFF0);
        assert_eq!(snapshot.executors[0].state, "verified_alive", "核验不直接清账");
    }

    #[test]
    fn missing_process_identity_still_refuses_recovery() {
        let mut snapshot = live_executor_snapshot();
        snapshot.executors[0].created = None;
        assert!(confirmed_executor_exits(&snapshot).unwrap_err().contains("创建身份"));
        snapshot.executors[0].created = Some(1);
        snapshot.executors[0].pid = 0;
        assert!(confirmed_executor_exits(&snapshot).unwrap_err().contains("可信PID"));
    }

    #[test]
    fn native_confirmation_is_single_use_payload_bound_and_expires() {
        let root = tempfile::tempdir().unwrap();
        let scope = runtime::InputSafetyResourceScope::parse("native-proof-test").unwrap();
        let store = input_safety_store::InputSafetyStore::open_at(root.path()).unwrap();
        let snapshot = store.native_recovery_store().snapshot(&scope).unwrap();
        let peer = process_peer_identity(std::process::id()).unwrap();
        let payload = RecoveryChallengeRequest {
            reason: "我已核对执行者和未确认结果".into(),
            evidence_refs: vec![],
            acknowledged_block_ids: vec![],
            acknowledged_run_ids: vec![],
        };
        let make = |expires| {
            let challenge = Challenge {
                id: secret().unwrap(),
                digest: payload.digest().unwrap(),
                request: payload.clone(),
                snapshot: snapshot.clone(),
                expires,
            };
            let proof = secret().unwrap();
            let request = ReleaseIsolationRequest {
                challenge_id: challenge.id.clone(),
                proof: proof.clone(),
                operator: peer.user_sid.clone(),
                reason: payload.reason.clone(),
                evidence_refs: vec![],
                acknowledged_block_ids: vec![],
                acknowledged_run_ids: vec![],
            };
            state().lock().unwrap().proofs.insert(
                proof,
                Approval {
                    challenge,
                    operator: peer.user_sid.clone(),
                    shell: peer.clone(),
                },
            );
            request
        };
        let request = make(unix_timestamp_millis() + 90_000);
        assert!(consume_confirmation(&request).is_ok());
        assert!(
            consume_confirmation(&request).is_err(),
            "凭据原子一次消费，重复提交必须拒绝"
        );
        let mut altered = make(unix_timestamp_millis() + 90_000);
        altered
            .acknowledged_run_ids
            .push("未显示给用户的运行".into());
        assert!(
            consume_confirmation(&altered).is_err(),
            "不得替换确认过的风险集合"
        );
        altered.acknowledged_run_ids.clear();
        assert!(
            consume_confirmation(&altered).is_err(),
            "失败的证明同样已消耗"
        );
        assert!(
            consume_confirmation(&make(unix_timestamp_millis())).is_err(),
            "过期不可放行"
        );
        let mut nul = payload;
        nul.reason.push('\0');
        assert!(nul.normalize().is_err(), "原生对话框不能被NUL截断风险文本");
    }
}
