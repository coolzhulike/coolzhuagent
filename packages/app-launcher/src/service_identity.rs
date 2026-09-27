//! 复用既有后台服务时的**身份与绑定核对**（裁决第六节）。
//!
//! launcher 复用后台服务时**不能只核对端口健康**。复用判定至少核对：
//!
//! 1. **实际用户/实例归属**（监听进程名 + 进程属主用户）
//! 2. **可兼容的构建与协议**（`/api/system/info` 协议形状 + 后台自报构建版本）
//! 3. **workspace 身份与规范路径**（后台自报 workspace 与本次解析结果规范化比较）
//! 4. **实际数据/数据库绑定**（后台自报 `paths.session_db` 与上次观测值比较）
//! 5. **当前启动或切换状态**（健康状态、活动会话数，仅记录）
//!
//! 任何一项**不匹配 → 报告冲突、不静默连接、不按端口杀进程**。
//! 启动自检同时记录**请求的路径**与**后台实际使用的路径**（只打印配置值不足以证明生效）。

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{json, Value};

use crate::launch_paths::same_canonical;
use crate::ListenerOwner;

/// `/api/system/info` 路径（web-console 既有只读接口，用于核对实例身份）。
pub const SYSTEM_INFO_PATH: &str = "/api/system/info";
/// 单次回读响应体上限，避免异常服务把内存打满。
pub const MAX_BODY_BYTES: usize = 512 * 1024;

/// 后台服务自报身份 + 监听者归属。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ServiceIdentity {
    pub port: u16,
    pub health_ready: bool,
    pub listener_pid: Option<u32>,
    pub listener_alive: bool,
    pub listener_process: Option<String>,
    /// 监听进程的属主用户（`DOMAIN\user`），来自 `Win32_Process.GetOwner()`。
    pub listener_owner_user: Option<String>,
    pub reported_workspace: Option<PathBuf>,
    pub reported_port: Option<u16>,
    pub reported_build_version: Option<String>,
    pub reported_active_sessions: Option<u64>,
    pub reported_session_db: Option<PathBuf>,
    pub reported_health_status: Option<String>,
    /// `/api/system/info` + `/api/diagnostics/health` 的形状是否都符合本版本预期。
    pub protocol_shape_ok: bool,
    pub notes: Vec<String>,
}

impl ServiceIdentity {
    /// 从两份响应体构造（纯函数：便于无网络测试）。
    #[must_use]
    pub fn from_bodies(system_info: Option<&str>, health: Option<&str>, port: u16) -> Self {
        let mut notes = Vec::new();
        let system = system_info.and_then(|body| serde_json::from_str::<Value>(body).ok());
        let health = health.and_then(|body| serde_json::from_str::<Value>(body).ok());
        if system.is_none() {
            notes.push("未取到 /api/system/info 的合法 JSON".to_string());
        }
        if health.is_none() {
            notes.push("未取到 /api/diagnostics/health 的合法 JSON".to_string());
        }
        let reported_workspace = system
            .as_ref()
            .and_then(|value| value.get("workspace"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        let reported_port = system
            .as_ref()
            .and_then(|value| value.get("port"))
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok());
        let reported_build_version = system
            .as_ref()
            .and_then(|value| value.get("build_version"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
        let reported_active_sessions = system
            .as_ref()
            .and_then(|value| value.get("active_sessions"))
            .and_then(Value::as_u64);
        let reported_session_db = health
            .as_ref()
            .and_then(|value| value.get("paths"))
            .and_then(|paths| paths.get("session_db"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        let reported_health_status = health
            .as_ref()
            .and_then(|value| value.get("summary"))
            .and_then(|summary| summary.get("status"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let protocol_shape_ok = reported_workspace.is_some()
            && reported_port.is_some()
            && reported_build_version.is_some()
            && reported_session_db.is_some();
        Self {
            port,
            health_ready: health.is_some(),
            listener_pid: None,
            listener_alive: false,
            listener_process: None,
            listener_owner_user: None,
            reported_workspace,
            reported_port,
            reported_build_version,
            reported_active_sessions,
            reported_session_db,
            reported_health_status,
            protocol_shape_ok,
            notes,
        }
    }

    /// 附上监听者归属信息。
    #[must_use]
    pub fn with_listener(mut self, owner: Option<&ListenerOwner>, owner_user: Option<&str>) -> Self {
        if let Some(owner) = owner {
            self.listener_pid = Some(owner.pid);
            self.listener_alive = owner.alive;
            self.listener_process = owner.process_name.clone();
        }
        self.listener_owner_user = owner_user.map(str::to_string);
        self
    }
}

/// 单条复用核对项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReuseCheck {
    pub name: String,
    pub expected: String,
    pub observed: String,
    /// `match` / `mismatch` / `unverified`
    pub verdict: String,
}

impl ReuseCheck {
    fn new(name: &str, expected: impl Into<String>, observed: impl Into<String>, verdict: &str) -> Self {
        Self {
            name: name.to_string(),
            expected: expected.into(),
            observed: observed.into(),
            verdict: verdict.to_string(),
        }
    }
}

/// 复用判定结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReuseDecision {
    /// 可复用：只把 `--show-console` 转发给既有实例。
    Reuse { verification: ReuseVerification },
    /// 端口上没有可复用的既有实例：正常启动。
    StartFresh,
    /// 有监听者但不匹配：报告冲突，不连接、不杀进程。
    Conflict { reason: String, details: Vec<String> },
}

/// 复用判定证据（写入自检）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReuseVerification {
    pub port: u16,
    /// `reuse` / `conflict` / `start_fresh`
    pub decision: String,
    pub reason: Option<String>,
    pub checks: Vec<ReuseCheck>,
}

impl ReuseVerification {
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "port": self.port,
            "decision": self.decision,
            "reason": self.reason,
            "checks": self.checks.iter().map(|check| json!({
                "name": check.name,
                "expected": check.expected,
                "observed": check.observed,
                "verdict": check.verdict,
            })).collect::<Vec<_>>(),
        })
    }

    fn mismatch_details(&self) -> Vec<String> {
        self.checks
            .iter()
            .filter(|check| check.verdict == "mismatch")
            .map(|check| {
                format!(
                    "{}：期望 {}，实际 {}",
                    check.name, check.expected, check.observed
                )
            })
            .collect()
    }
}

/// 判定"端口上的既有服务能否复用"。
///
/// 只做判定，不产生任何副作用（不连接、不杀进程、不改选择）。
#[must_use]
pub fn decide_reuse(
    expected_workspace: &Path,
    expected_session_db: Option<&Path>,
    expected_build_version: Option<&str>,
    current_user: Option<&str>,
    identity: &ServiceIdentity,
) -> ReuseDecision {
    let port = identity.port;
    let mut checks = Vec::new();

    if !identity.health_ready {
        checks.push(ReuseCheck::new(
            "health",
            "ready",
            "not_ready",
            "unverified",
        ));
        return ReuseDecision::StartFresh;
    }
    checks.push(ReuseCheck::new("health", "ready", "ready", "match"));

    let process_name = identity.listener_process.clone().unwrap_or_default();
    let is_our_process = Path::new(&process_name)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| {
            value.eq_ignore_ascii_case("coolzhu-web-console.exe")
                || value.eq_ignore_ascii_case("coolzhu-web-console")
        });
    checks.push(ReuseCheck::new(
        "listener_process",
        "coolzhu-web-console.exe",
        if process_name.is_empty() {
            "unknown".to_string()
        } else {
            process_name.clone()
        },
        if is_our_process { "match" } else { "mismatch" },
    ));
    if !is_our_process {
        let verification = ReuseVerification {
            port,
            decision: "conflict".to_string(),
            reason: Some("端口上的监听者不是本产品的 web-console 进程".to_string()),
            checks: checks.clone(),
        };
        return ReuseDecision::Conflict {
            reason: format!(
                "端口 {port} 被其它进程（pid={}，name={}）占用：不复用、不按端口杀进程",
                identity
                    .listener_pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                if process_name.is_empty() {
                    "unknown"
                } else {
                    process_name.as_str()
                }
            ),
            details: verification.mismatch_details(),
        };
    }
    if !identity.listener_alive {
        // 死进程留下的监听：交给既有端口回收路径处理（不是"复用"）。
        return ReuseDecision::StartFresh;
    }

    // 1. 实际用户/实例归属。
    let user_verdict = match (
        identity.listener_owner_user.as_deref(),
        current_user,
    ) {
        (Some(owner_user), Some(current)) => {
            if owner_user.eq_ignore_ascii_case(current) {
                "match"
            } else {
                "mismatch"
            }
        }
        _ => "unverified",
    };
    checks.push(ReuseCheck::new(
        "instance_owner_user",
        current_user.unwrap_or("unknown"),
        identity
            .listener_owner_user
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        user_verdict,
    ));

    // 2. 可兼容的构建与协议。
    checks.push(ReuseCheck::new(
        "protocol_shape",
        "/api/system/info + /api/diagnostics/health 形状完整",
        if identity.protocol_shape_ok {
            "完整".to_string()
        } else {
            identity.notes.join("；")
        },
        if identity.protocol_shape_ok {
            "match"
        } else {
            "mismatch"
        },
    ));
    let build_verdict = match (
        identity.reported_build_version.as_deref(),
        expected_build_version,
    ) {
        (Some(observed), Some(expected)) => {
            if observed == expected {
                "match"
            } else {
                "mismatch"
            }
        }
        (Some(_), None) => "unverified",
        _ => "unverified",
    };
    checks.push(ReuseCheck::new(
        "build_identity",
        expected_build_version.unwrap_or("unknown（尚无本机构建记录）"),
        identity
            .reported_build_version
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        build_verdict,
    ));

    // 3. workspace 身份与规范路径。
    let workspace_verdict = match identity.reported_workspace.as_deref() {
        Some(reported) => {
            if same_canonical(reported, expected_workspace) {
                "match"
            } else {
                "mismatch"
            }
        }
        None => "mismatch",
    };
    checks.push(ReuseCheck::new(
        "workspace_canonical_path",
        expected_workspace.display().to_string(),
        identity
            .reported_workspace
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        workspace_verdict,
    ));

    // 4. 实际数据/数据库绑定（launcher 不固定业务库，因此没有期望值时只记录不阻断）。
    let db_verdict = match (
        identity.reported_session_db.as_deref(),
        expected_session_db,
    ) {
        (Some(observed), Some(expected)) => {
            if same_canonical(observed, expected) {
                "match"
            } else {
                "mismatch"
            }
        }
        (Some(_), None) => "unverified",
        (None, _) => "unverified",
    };
    checks.push(ReuseCheck::new(
        "session_db_binding",
        expected_session_db
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "由工作区配置决定（launcher 不固定）".to_string()),
        identity
            .reported_session_db
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        db_verdict,
    ));

    // 5. 当前启动/切换状态（只记录）。
    checks.push(ReuseCheck::new(
        "launch_state",
        "任意（仅记录）",
        format!(
            "health={} active_sessions={}",
            identity
                .reported_health_status
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            identity
                .reported_active_sessions
                .map(|count| count.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        "match",
    ));

    let mismatches: Vec<&ReuseCheck> = checks
        .iter()
        .filter(|check| check.verdict == "mismatch")
        .collect();
    if !mismatches.is_empty() {
        let reason = "端口上的既有实例与本次启动的身份/绑定不一致".to_string();
        let verification = ReuseVerification {
            port,
            decision: "conflict".to_string(),
            reason: Some(reason.clone()),
            checks,
        };
        return ReuseDecision::Conflict {
            reason,
            details: verification.mismatch_details(),
        };
    }

    let verification = ReuseVerification {
        port,
        decision: "reuse".to_string(),
        reason: None,
        checks,
    };
    ReuseDecision::Reuse { verification }
}

/// 监听进程属主用户（`DOMAIN\user`）：本进程用户身份，用于与监听者比较。
#[must_use]
pub fn current_process_user() -> Option<String> {
    let lookup = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
    let user = lookup("USERNAME")?;
    match lookup("USERDOMAIN") {
        Some(domain) => Some(format!("{domain}\\{user}")),
        None => Some(user),
    }
}

/// 把 `/api/diagnostics/health` 换成同源同端口的 `/api/system/info`。
#[must_use]
pub fn system_info_url(health_url: &str) -> Option<String> {
    let rest = health_url.strip_prefix("http://")?;
    let (authority, _) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    Some(format!("http://{authority}{SYSTEM_INFO_PATH}"))
}

/// 真实回读：`/api/system/info` + `/api/diagnostics/health`（best-effort，不做任何写操作）。
#[must_use]
pub fn fetch_service_identity(health_url: &str) -> ServiceIdentity {
    let port = crate::health_endpoint_port(health_url).unwrap_or(0);
    let health_body = http_get_body(health_url);
    let system_body = system_info_url(health_url).and_then(|url| http_get_body(&url));
    ServiceIdentity::from_bodies(system_body.as_deref(), health_body.as_deref(), port)
}

/// 极简 HTTP GET：只用于本机回读，支持 `Content-Length` 与 `chunked`。
#[must_use]
pub fn http_get_body(url: &str) -> Option<String> {
    let (host, port, path) = crate::parse_http_url(url)?;
    let addr: SocketAddr = format!("{host}:{port}").parse().ok()?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2)).ok()?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut raw = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                raw.extend_from_slice(&buffer[..read]);
                if raw.len() >= MAX_BODY_BYTES {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status_ok = head
        .lines()
        .next()
        .is_some_and(|line| line.contains(" 200 ") || line.ends_with(" 200"));
    if !status_ok {
        return None;
    }
    let chunked = head
        .lines()
        .any(|line| line.to_ascii_lowercase().contains("transfer-encoding: chunked"));
    if chunked {
        Some(decode_chunked(body))
    } else {
        Some(body.to_string())
    }
}

fn decode_chunked(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    loop {
        let Some(index) = rest.find("\r\n") else {
            break;
        };
        let size_line = rest[..index].trim();
        let size = usize::from_str_radix(size_line.split(';').next().unwrap_or("").trim(), 16)
            .unwrap_or(0);
        if size == 0 {
            break;
        }
        let start = index + 2;
        if start + size > rest.len() {
            out.push_str(&rest[start..]);
            break;
        }
        out.push_str(&rest[start..start + size]);
        rest = &rest[start + size..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYSTEM_INFO: &str = r#"{
        "workspace": "C:\\Users\\me\\coolzhuagent",
        "port": 8765,
        "build_version": "abc1234 · 2026-09-25",
        "active_sessions": 3
    }"#;

    const HEALTH: &str = r#"{
        "summary": {"status": "ok", "ok": 9, "warn": 0, "error": 0},
        "paths": {
            "workspace": "C:\\Users\\me\\coolzhuagent",
            "session_db": "C:\\Users\\me\\coolzhuagent\\.coolzhu\\web-sessions.sqlite3",
            "legacy_session_json": "C:\\Users\\me\\coolzhuagent\\.coolzhu\\web-sessions.json",
            "latest_capture": "C:\\Users\\me\\coolzhuagent\\.coolzhu\\latest-capture.png",
            "desktop_pet_exe": null
        }
    }"#;

    fn owner(pid: u32, alive: bool, name: &str) -> ListenerOwner {
        ListenerOwner {
            pid,
            alive,
            process_name: Some(name.to_string()),
        }
    }

    fn identity_for(workspace: &Path, build: &str, user: &str) -> ServiceIdentity {
        let mut identity =
            ServiceIdentity::from_bodies(Some(SYSTEM_INFO), Some(HEALTH), 8765);
        identity.reported_workspace = Some(workspace.to_path_buf());
        identity.reported_build_version = Some(build.to_string());
        identity.listener_owner_user = Some(user.to_string());
        identity
    }

    // P09：后台健康但绑定另一工作区 ⇒ 不复用、不按端口杀、明确报告冲突。
    #[test]
    fn p09_healthy_service_bound_to_another_workspace_is_a_conflict_not_a_reuse() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        let other = PathBuf::from(r"D:\other-workspace");
        let mut identity = identity_for(&other, "abc1234 · 2026-09-25", "MACHINE\\me");
        identity = identity.with_listener(Some(&owner(8188, true, "coolzhu-web-console.exe")), Some("MACHINE\\me"));

        let decision = decide_reuse(
            &expected,
            None,
            Some("abc1234 · 2026-09-25"),
            Some("MACHINE\\me"),
            &identity,
        );
        match decision {
            ReuseDecision::Conflict { reason, details } => {
                assert!(reason.contains("身份/绑定不一致"));
                assert!(details.iter().any(|detail| detail.contains("workspace_canonical_path")));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn fully_matching_instance_is_reusable_and_records_every_check() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        let mut identity = identity_for(&expected, "abc1234 · 2026-09-25", "MACHINE\\me");
        identity = identity.with_listener(Some(&owner(8188, true, "COOLZHU-WEB-CONSOLE.EXE")), Some("MACHINE\\me"));

        let decision = decide_reuse(
            &expected,
            Some(Path::new(
                r"C:\Users\me\coolzhuagent\.coolzhu\web-sessions.sqlite3",
            )),
            Some("abc1234 · 2026-09-25"),
            Some("machine\\ME"),
            &identity,
        );
        match decision {
            ReuseDecision::Reuse { verification } => {
                assert_eq!(verification.decision, "reuse");
                assert_eq!(verification.checks.len(), 8);
                assert!(verification
                    .checks
                    .iter()
                    .any(|check| check.name == "session_db_binding" && check.verdict == "match"));
                assert!(verification
                    .checks
                    .iter()
                    .any(|check| check.name == "launch_state"));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn foreign_listener_is_never_reused_or_killed() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        let mut identity = identity_for(&expected, "abc1234", "MACHINE\\me");
        identity = identity.with_listener(Some(&owner(4242, true, "other-service.exe")), Some("MACHINE\\me"));

        let decision = decide_reuse(&expected, None, None, Some("MACHINE\\me"), &identity);
        match decision {
            ReuseDecision::Conflict { reason, .. } => {
                assert!(reason.contains("不复用、不按端口杀进程"));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn instance_owned_by_another_user_is_a_conflict() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        let mut identity = identity_for(&expected, "abc1234", "MACHINE\\someone-else");
        identity = identity.with_listener(
            Some(&owner(8188, true, "coolzhu-web-console.exe")),
            Some("MACHINE\\someone-else"),
        );

        let decision = decide_reuse(&expected, None, None, Some("MACHINE\\me"), &identity);
        match decision {
            ReuseDecision::Conflict { details, .. } => {
                assert!(details
                    .iter()
                    .any(|detail| detail.contains("instance_owner_user")));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn incompatible_protocol_or_build_is_a_conflict() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        // 协议形状不完整（缺 /api/system/info）。
        let mut identity = ServiceIdentity::from_bodies(None, Some(HEALTH), 8765);
        identity.reported_workspace = Some(expected.clone());
        identity.reported_build_version = Some("old".into());
        identity.reported_session_db = Some(expected.join(".coolzhu").join("web-sessions.sqlite3"));
        identity.listener_alive = true;
        identity.listener_pid = Some(8188);
        identity.listener_process = Some("coolzhu-web-console.exe".into());
        identity.protocol_shape_ok = false;
        let decision = decide_reuse(&expected, None, None, None, &identity);
        match decision {
            ReuseDecision::Conflict { details, .. } => {
                assert!(details.iter().any(|detail| detail.contains("protocol_shape")));
            }
            other => panic!("unexpected decision: {other:?}"),
        }

        // 构建版本不同 → 冲突；无从核对 → 允许复用但记录 unverified。
        let mut identity = identity_for(&expected, "sha-new", "MACHINE\\me");
        identity.listener_alive = true;
        identity.listener_pid = Some(8188);
        identity.listener_process = Some("coolzhu-web-console.exe".into());
        match decide_reuse(&expected, None, Some("sha-old"), None, &identity) {
            ReuseDecision::Conflict { details, .. } => {
                assert!(details.iter().any(|detail| detail.contains("build_identity")));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
        match decide_reuse(&expected, None, None, None, &identity) {
            ReuseDecision::Reuse { verification } => {
                let check = verification
                    .checks
                    .iter()
                    .find(|check| check.name == "build_identity")
                    .unwrap();
                assert_eq!(check.verdict, "unverified");
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn unverifiable_workspace_binding_is_a_conflict_not_a_silent_connect() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        let mut identity = ServiceIdentity::from_bodies(None, Some(HEALTH), 8765);
        identity.listener_alive = true;
        identity.listener_pid = Some(8188);
        identity.listener_process = Some("coolzhu-web-console.exe".into());
        identity.reported_session_db = Some(expected.join(".coolzhu").join("web-sessions.sqlite3"));
        let decision = decide_reuse(&expected, None, None, None, &identity);
        match decision {
            ReuseDecision::Conflict { details, .. } => {
                assert!(details.iter().any(|detail| detail.contains("workspace_canonical_path")));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn data_binding_change_between_launches_is_a_conflict() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        let mut identity = identity_for(&expected, "abc1234", "MACHINE\\me");
        identity = identity.with_listener(
            Some(&owner(8188, true, "coolzhu-web-console.exe")),
            Some("MACHINE\\me"),
        );
        identity.reported_session_db = Some(
            expected
                .join("somewhere-else")
                .join("web-sessions.sqlite3"),
        );
        // 上次观测到的库在 .coolzhu 下，这次后台实际用的是别的目录 ⇒ 报告冲突而不是"连接成功"。
        let decision = decide_reuse(
            &expected,
            Some(&expected.join(".coolzhu").join("web-sessions.sqlite3")),
            Some("abc1234"),
            Some("MACHINE\\me"),
            &identity,
        );
        match decision {
            ReuseDecision::Conflict { details, .. } => {
                assert!(details
                    .iter()
                    .any(|detail| detail.contains("session_db_binding")));
            }
            other => panic!("unexpected decision: {other:?}"),
        }
    }

    #[test]
    fn unhealthy_or_dead_listener_starts_fresh() {
        let expected = PathBuf::from(r"C:\Users\me\coolzhuagent");
        let identity = ServiceIdentity::from_bodies(None, None, 8765);
        assert_eq!(
            decide_reuse(&expected, None, None, None, &identity),
            ReuseDecision::StartFresh
        );

        let mut identity = identity_for(&expected, "abc1234", "MACHINE\\me");
        identity.listener_alive = false;
        identity.listener_pid = Some(99);
        identity.listener_process = Some("coolzhu-web-console.exe".into());
        assert_eq!(
            decide_reuse(&expected, None, None, None, &identity),
            ReuseDecision::StartFresh
        );
    }

    #[test]
    fn system_info_url_keeps_host_and_port() {
        assert_eq!(
            system_info_url("http://127.0.0.1:8765/api/diagnostics/health").unwrap(),
            "http://127.0.0.1:8765/api/system/info"
        );
        assert_eq!(system_info_url("https://x/health"), None);
    }

    #[test]
    fn chunked_and_plain_bodies_are_both_decoded() {
        assert_eq!(decode_chunked("5\r\nhello\r\n0\r\n\r\n"), "hello");
        assert_eq!(
            decode_chunked("b\r\nhello world\r\n0\r\n\r\n"),
            "hello world"
        );
    }

    #[test]
    fn identity_requires_full_protocol_shape() {
        let identity = ServiceIdentity::from_bodies(Some(SYSTEM_INFO), Some(HEALTH), 8765);
        assert!(identity.protocol_shape_ok);
        assert_eq!(identity.reported_port, Some(8765));
        assert_eq!(identity.reported_active_sessions, Some(3));
        assert_eq!(
            identity.reported_workspace,
            Some(PathBuf::from(r"C:\Users\me\coolzhuagent"))
        );
        assert_eq!(
            identity.reported_health_status.as_deref(),
            Some("ok")
        );
        let partial = ServiceIdentity::from_bodies(Some(SYSTEM_INFO), None, 8765);
        assert!(!partial.protocol_shape_ok);
    }
}
