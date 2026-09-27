//! 仅登记本启动器持有的真实 shell 子进程，不接受网页传来的实例证明。
use std::{io, process::Child, time::Duration};
use windows_process_guard::{capture_child_process_identity, local_recovery_pipe_request, recovery_pipe_name, ProcessIdentity};

pub fn register_shell(server: &ProcessIdentity, shell: &Child) -> io::Result<()> {
    let identity = capture_child_process_identity(shell).map_err(|error| io::Error::new(io::ErrorKind::PermissionDenied, error.to_string()))?;
    let request = serde_json::json!({"op":"register_shell","pid":identity.pid(),"creation_time_filetime":identity.creation_time_filetime()});
    let bytes = local_recovery_pipe_request(&recovery_pipe_name(server), server, request.to_string().as_bytes(), Duration::from_secs(5))?;
    let response: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "恢复通道登记响应无效"))?;
    if response.get("ok").and_then(serde_json::Value::as_bool) == Some(true) { return Ok(()); }
    Err(io::Error::new(io::ErrorKind::PermissionDenied, response.get("error").and_then(serde_json::Value::as_str).unwrap_or("后台拒绝登记桌面实例").chars().take(500).collect::<String>()))
}
