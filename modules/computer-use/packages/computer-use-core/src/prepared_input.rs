//! 原生 helper 与宿主授权之间的窄接口；本模块不访问 Web、SQLite 或会话存储。
use std::time::{Duration, Instant};

/// 只在真实 helper 已 READY 且尚未获准输入时借给宿主。不可复制执行能力。
#[derive(Debug)]
pub struct PreparedNativeInput {
    pub process: runtime::ProcessInstanceEvidence,
    pub helper: runtime::HelperIdentity,
    pub request_id: String,
    pub nonce: String,
    pub ready_at_unix_ms: u64,
    pub deadline_unix_ms: u64,
    pub protocol_version: u32,
    pub supervision_bound: bool,
}

/// 宿主已持久登记、签发并消费的许可绑定；core 不自行签发业务许可。
#[derive(Debug)]
pub struct NativeInputPermit {
    pub permit_id: String,
    pub attempt_id: String,
    pub executor_instance_id: String,
    pub expires_at_unix_ms: u64,
}

#[derive(Debug)]
pub struct NativeInputCompletion {
    pub request_id: String,
    pub process: Option<runtime::ProcessInstanceEvidence>,
    pub execute_notified: bool,
    pub process_exit_confirmed: bool,
    pub input_release: runtime::InputReleaseStatus,
    pub trusted_final: bool,
    pub error: Option<String>,
}

/// 异常提前返回也不能遗弃运行中的 helper；正常结束仍由各引擎记录真实退出事实。
pub(crate) struct SupervisedHelperChild {
    child: std::process::Child,
    cancel: std::path::PathBuf,
    #[cfg(windows)]
    _job: windows_process_guard::ChildProcessJob,
}

impl SupervisedHelperChild {
    #[cfg(windows)]
    pub(crate) fn new(child: std::process::Child, job: windows_process_guard::ChildProcessJob, cancel: &std::path::Path) -> Self {
        Self {
            child,
            cancel: cancel.into(),
            _job: job,
        }
    }
}

impl std::ops::Deref for SupervisedHelperChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl std::ops::DerefMut for SupervisedHelperChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

impl Drop for SupervisedHelperChild {
    fn drop(&mut self) {
        if !matches!(self.child.try_wait(), Ok(Some(_))) {
            let _ = std::fs::write(&self.cancel, b"cancel");
            let _ = self.child.kill();
        }
    }
}

/// 两次回调均同步且不跨线程保存，不要求实现方持有 Send/Sync 的数据库或 lease。
/// authorize 必须在输入 lease 外执行；dispatch 只在短 lease 中复核并调用一次 notify，
/// 不得在该临界区访问数据库、等待 helper 或重试。未调用 notify 的成功返回也算拒绝。
pub trait NativeInputAuthorization {
    fn authorize(&self, prepared: &PreparedNativeInput) -> Result<NativeInputPermit, String>;
    fn dispatch(
        &self,
        prepared: &PreparedNativeInput,
        permit: &NativeInputPermit,
        notify: &mut dyn FnMut() -> Result<(), String>,
    ) -> Result<(), String>;
    /// 引擎完成真实收尾后在 lease 外对账。输入和释放事实仍由原始 outcome 返回给适配器。
    fn completed(&self, _completion: &NativeInputCompletion) -> Result<(), String> {
        Ok(())
    }
}

/// helper 由原有输入引擎持有与监督；此对象只拥有这一次握手文件和一次性通知状态。
pub(crate) struct PreparedInputSession {
    nonce: String,
    ready: std::path::PathBuf,
    permit: std::path::PathBuf,
    deadline: Instant,
    deadline_unix_ms: u64,
    attempted: bool,
    pub(crate) notified: bool,
}

impl PreparedInputSession {
    pub(crate) fn new(nonce: &str, timeout: Duration) -> Self {
        Self {
            nonce: nonce.into(),
            ready: std::env::temp_dir().join(format!("coolzhu-input-ready-{nonce}.json")),
            permit: std::env::temp_dir().join(format!("coolzhu-input-permit-{nonce}.json")),
            deadline: Instant::now() + timeout,
            deadline_unix_ms: crate::cleanup::unix_ms()
                .saturating_add(timeout.as_millis().min(u128::from(u64::MAX)) as u64),
            attempted: false,
            notified: false,
        }
    }

    pub(crate) fn apply_request(&self, request: &mut serde_json::Value) {
        request["two_phase_helper"] = serde_json::json!(true);
        request["two_phase_nonce"] = serde_json::json!(self.nonce);
        request["ready_file"] = serde_json::json!(self.ready.to_string_lossy());
        request["permit_file"] = serde_json::json!(self.permit.to_string_lossy());
        request["input_deadline_unix_ms"] = serde_json::json!(self.deadline_unix_ms);
    }

    #[cfg(windows)]
    pub(crate) fn poll(
        &mut self,
        authorization: &dyn NativeInputAuthorization,
        identity: Option<&windows_process_guard::ProcessIdentity>,
        script: &str,
        cancelled: &dyn Fn() -> bool,
        child: &mut std::process::Child,
    ) -> Result<(), String> {
        use sha2::{Digest, Sha256};
        if self.attempted || !self.ready.exists() {
            return Ok(());
        }
        self.attempted = true;
        if cancelled() || Instant::now() >= self.deadline {
            return Err("输入授权前已取消或超时".into());
        }
        let identity = identity.ok_or("无法取得 helper 的真实进程身份")?;
        let metadata = std::fs::metadata(&self.ready).map_err(|_| "无法读取 helper READY")?;
        if metadata.len() > 4096 {
            return Err("helper READY 超过允许大小".into());
        }
        let ready: runtime::HelperReadySignal = serde_json::from_slice(
            &std::fs::read(&self.ready).map_err(|_| "无法读取 helper READY")?,
        )
        .map_err(|_| "helper READY 协议无效")?;
        ready.validate().map_err(|_| "helper READY 协议无效")?;
        if ready.nonce != self.nonce
            || ready.pid != identity.pid()
            || ready.helper_protocol_version != runtime::TWO_PHASE_HELPER_PROTOCOL_VERSION
        {
            return Err("helper READY 与本次进程身份不符".into());
        }
        let prepared = PreparedNativeInput {
            process: runtime::ProcessInstanceEvidence {
                pid: identity.pid(),
                creation_time_filetime: identity.creation_time_filetime(),
            },
            helper: runtime::HelperIdentity {
                host_process_path: identity
                    .image_path()
                    .ok_or("无法取得 helper 实际路径")?
                    .into(),
                script_or_program_digest: format!("sha256:{:x}", Sha256::digest(script.as_bytes())),
            },
            request_id: self.nonce.clone(),
            nonce: self.nonce.clone(),
            ready_at_unix_ms: ready.timestamp_unix_ms,
            deadline_unix_ms: self.deadline_unix_ms,
            protocol_version: ready.helper_protocol_version,
            supervision_bound: true,
        };
        let permit = authorization.authorize(&prepared)?;
        if [
            &permit.permit_id,
            &permit.attempt_id,
            &permit.executor_instance_id,
        ]
        .iter()
        .any(|v| v.trim().is_empty())
        {
            return Err("宿主没有提供完整的输入许可绑定".into());
        }
        let expires = permit.expires_at_unix_ms.min(self.deadline_unix_ms);
        if expires <= crate::cleanup::unix_ms() {
            return Err("宿主输入许可已经过期".into());
        }
        // 数据库操作已经完成后才准备通知内容；短 lease 内仅复核和原子改名。
        let staging = self.permit.with_extension("tmp");
        let payload = serde_json::json!({"type":"execute", "nonce":self.nonce,
            "permit_id":permit.permit_id, "attempt_id":permit.attempt_id,
            "executor_instance_id":permit.executor_instance_id,"expires_at_unix_ms":expires});
        std::fs::write(&staging, payload.to_string()).map_err(|_| "无法准备 helper 执行通知")?;
        let mut notify_attempted = false;
        let mut notify = || {
            if notify_attempted {
                return Err("helper 执行通知只能尝试一次".into());
            }
            notify_attempted = true;
            if cancelled() || Instant::now() >= self.deadline || crate::cleanup::unix_ms() >= expires {
                return Err("输入通知前已取消或超时".into());
            }
            if child
                .try_wait()
                .map_err(|_| "无法核查 helper 存活状态")?
                .is_some()
            {
                return Err("helper 已退出".into());
            }
            std::fs::rename(&staging, &self.permit).map_err(|_| "helper 执行通知未成功提交")?;
            self.notified = true;
            Ok(())
        };
        let result = authorization.dispatch(&prepared, &permit, &mut notify);
        drop(notify);
        let _ = std::fs::remove_file(staging);
        result?;
        if !self.notified {
            return Err("宿主没有提交 helper 执行通知".into());
        }
        Ok(())
    }
}

impl Drop for PreparedInputSession {
    fn drop(&mut self) {
        let mut ready_staging = self.ready.as_os_str().to_os_string();
        ready_staging.push(".tmp");
        for path in [
            &self.ready,
            &self.permit,
            &std::path::PathBuf::from(ready_staging),
            &self.permit.with_extension("tmp"),
        ] {
            let _ = std::fs::remove_file(path);
        }
    }
}
