//! 有界 ACP JSON-RPC 流；超时使连接失效，不在旧连接上继续发送任务。
use serde_json::{Value, json};
use std::{io, process::Stdio, time::Duration};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt,
};

pub(super) const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_DIRECTORY_BYTES: u64 = 4 * 1024 * 1024;

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

pub(super) async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> io::Result<Value> {
    let mut bytes = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return Err(invalid("ACP 输出提前结束。"));
        }
        let count = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |pos| pos + 1);
        if bytes.len().saturating_add(count) > MAX_FRAME_BYTES {
            return Err(invalid("ACP 消息超过大小限制。"));
        }
        let ended = buffer[count - 1] == b'\n';
        bytes.extend_from_slice(&buffer[..count]);
        reader.consume(count);
        if ended {
            break;
        }
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|_| invalid("ACP 输出不是合法 JSON。"))?;
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || !value.is_object() {
        return Err(invalid("ACP 输出不是 JSON-RPC 2.0 消息。"));
    }
    Ok(value)
}

pub(super) struct RpcConnection<R, W> {
    reader: R,
    writer: W,
    next_id: u64,
    closed: bool,
}

impl<R: AsyncBufRead + Unpin, W: AsyncWrite + Unpin> RpcConnection<R, W> {
    pub(super) fn new(reader: R, writer: W) -> Self {
        Self {
            reader,
            writer,
            next_id: 0,
            closed: false,
        }
    }

    async fn write(&mut self, value: &Value) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(value)?;
        if bytes.len() >= MAX_FRAME_BYTES {
            return Err(invalid("ACP 请求超过大小限制。"));
        }
        bytes.push(b'\n');
        self.writer.write_all(&bytes).await?;
        self.writer.flush().await
    }

    /// 串行请求不复用 ID；后台请求全部拒绝，不调用任何宿主 executor。
    pub(super) async fn request(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> io::Result<Value> {
        if self.closed {
            return Err(invalid("ACP 连接已失效，需要新一代连接。"));
        }
        if !matches!(
            method,
            "initialize" | "session/new" | "session/load" | "session/set_config_option"
        ) {
            return Err(invalid("当前 ACP 连接仅用于握手和配置，不能提交任务。"));
        }
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| invalid("ACP 请求 ID 已耗尽。"))?;
        let id = self.next_id;
        let result = tokio::time::timeout(timeout, async {
            self.write(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
                .await?;
            let mut notifications = 0;
            loop {
                let frame = read_frame(&mut self.reader).await?;
                if frame.get("method").is_some() {
                    notifications += 1;
                    if notifications > 4096 {
                        return Err(invalid("ACP 非响应消息过多。"));
                    }
                    if frame.get("id").is_some() {
                        let denied =
                            super::protocol::denied_client_request(&frame).map_err(invalid)?;
                        self.write(&denied).await?;
                    }
                    // 此连接仅供握手/配置，不把任何工具通知投影成已执行。
                    continue;
                }
                if frame.get("id").and_then(Value::as_u64) != Some(id) {
                    return Err(invalid("ACP 响应与当前请求身份不一致。"));
                }
                if frame.get("error").is_some() {
                    return Err(invalid(
                        "ACP 请求被拒绝；未记录远端原始错误以避免泄露凭据。",
                    ));
                }
                return frame
                    .get("result")
                    .cloned()
                    .ok_or_else(|| invalid("ACP 响应缺少结果。"));
            }
        })
        .await;
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => {
                self.closed = true;
                Err(error)
            }
            Err(_) => {
                self.closed = true;
                Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "ACP 请求超时，连接已失效。",
                ))
            }
        }
    }
}

async fn bounded_output(reader: impl AsyncRead + Unpin) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_DIRECTORY_BYTES + 1)
        .read_to_end(&mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_DIRECTORY_BYTES {
        return Err(invalid("Devin CLI 输出超过 4 MiB 限制。"));
    }
    Ok(bytes)
}

fn readonly_failure(stderr: &[u8]) -> &'static str {
    let diagnostic = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if diagnostic.contains("not logged in") || diagnostic.contains("not authenticated") {
        "Devin 尚未登录。请在插件页或模型设置中点击“登录 Devin”，完成授权后重新获取模型。"
    } else {
        // 只返回固定诊断，不将远端原始错误、账号信息或凭据带入 API。
        "Devin CLI 查询失败或输出超限；请检查 CLI 登录、网络及版本。"
    }
}

/// 不经过 shell，不传提示词；输出有界、无交互输入，Windows Job 管理整个子树。
pub(super) async fn run_readonly(
    binary: &std::path::Path,
    args: &[&str],
    cwd: &std::path::Path,
    timeout: Duration,
) -> Result<Vec<u8>, &'static str> {
    let mut command = tokio::process::Command::new(binary);
    command
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    let (mut child, _job) = windows_process_guard::ChildProcessJob::spawn_managed_async(
        &mut command,
    )
    .map_err(
        |_| "无法启动 Devin CLI，请确认已安装原生可执行文件，并设置 COOLZHU_DEVIN_CLI 的绝对路径。",
    )?;
    #[cfg(not(windows))]
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动 Devin CLI，请检查安装和路径。")?;
    let stdout = child.stdout.take().ok_or("Devin CLI 标准输出不可用。")?;
    let stderr = child.stderr.take().ok_or("Devin CLI 错误输出不可用。")?;
    let operation = tokio::time::timeout(timeout, async {
        let (stdout, stderr, status) =
            tokio::try_join!(bounded_output(stdout), bounded_output(stderr), child.wait())
                .map_err(|_| "Devin CLI 查询失败或输出超限；请检查 CLI 登录、网络及版本。")?;
        if !status.success() {
            return Err(readonly_failure(&stderr));
        }
        Ok(stdout)
    })
    .await;
    let result = match operation {
        Ok(result) => result,
        Err(_) => Err("Devin CLI 查询超时，已停止受管进程。"),
    };
    if result.is_err() {
        let _ = child.start_kill();
        let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::BufReader;
    #[test]
    fn readonly_login_errors_are_actionable_without_reflecting_raw_credentials() {
        assert!(readonly_failure(b"Error: Not logged in. Run auth login. token=secret").contains("尚未登录"));
        assert!(!readonly_failure(b"Error: Not logged in. token=secret").contains("secret"));
        assert!(!readonly_failure(b"network failure token=secret").contains("secret"));
        assert!(!readonly_failure(b"invalid model").contains("尚未登录"));
    }
    #[tokio::test]
    async fn bounded_frames_reject_noise_oversize_and_partial_json() {
        for bytes in [
            b"log line\n".to_vec(),
            b"{\"jsonrpc\":\"2.0\"}".to_vec(),
            vec![b'x'; MAX_FRAME_BYTES + 1],
        ] {
            assert!(
                read_frame(&mut BufReader::new(bytes.as_slice()))
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn request_rejects_host_calls_and_never_interprets_tool_notice_as_execution() {
        let (client, server) = tokio::io::duplex(4096);
        let (cr, cw) = tokio::io::split(client);
        let mut rpc = RpcConnection::new(BufReader::new(cr), cw);
        let peer = tokio::spawn(async move {
            let (sr, mut sw) = tokio::io::split(server);
            let mut reader = BufReader::new(sr);
            let request = read_frame(&mut reader).await.unwrap();
            sw.write_all(
                b"{\"jsonrpc\":\"2.0\",\"id\":\"p1\",\"method\":\"session/request_permission\"}\n",
            )
            .await
            .unwrap();
            let denied = read_frame(&mut reader).await.unwrap();
            assert_eq!(denied["result"]["outcome"]["outcome"], "cancelled");
            let response = format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":request["id"],"result":{"protocolVersion":1,"agentCapabilities":{}}})
            );
            sw.write_all(response.as_bytes()).await.unwrap();
        });
        let result = rpc
            .request(
                "initialize",
                super::super::protocol::initialize_params(),
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        super::super::protocol::validate_initialize(&result).unwrap();
        peer.await.unwrap();
    }
    #[tokio::test]
    async fn timeout_invalidates_connection_and_wrong_request_id_is_rejected() {
        let bytes = b"{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":{}}\n";
        let mut rpc = RpcConnection::new(BufReader::new(bytes.as_slice()), tokio::io::sink());
        assert!(
            rpc.request("initialize", json!({}), Duration::from_secs(1))
                .await
                .is_err()
        );
        assert!(rpc.closed);
        let (_keep_open, idle) = tokio::io::duplex(1000);
        let mut rpc = RpcConnection::new(BufReader::new(idle), tokio::io::sink());
        assert_eq!(
            rpc.request("initialize", json!({}), Duration::from_millis(10))
                .await
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(
            rpc.request("initialize", json!({}), Duration::from_secs(1))
                .await
                .is_err()
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn cli_timeout_reclaims_managed_process_without_waiting_for_full_sleep() {
        let binary = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let cwd = tempfile::tempdir().unwrap();
        let clock = std::time::Instant::now();
        let error = run_readonly(
            &binary,
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 30",
            ],
            cwd.path(),
            Duration::from_millis(50),
        )
        .await
        .unwrap_err();
        assert!(error.contains("超时"));
        assert!(clock.elapsed() < Duration::from_secs(5));
    }
}
