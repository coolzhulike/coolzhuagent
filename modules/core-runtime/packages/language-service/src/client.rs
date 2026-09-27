use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
#[cfg(windows)]
use std::sync::Mutex as StdMutex;
use std::time::Duration;

use lsp_types::{
    Diagnostic, GotoDefinitionResponse, Location, LocationLink, Position, PublishDiagnosticsParams,
};
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{oneshot, Mutex};
use tokio::time::timeout;

#[cfg(windows)]
use coolzhu_windows_process_guard::ChildProcessJob;

use crate::error::LspError;
use crate::types::{LspServerConfig, SymbolLocation};

const MAX_MESSAGE_BYTES: usize = 16 * 1024 * 1024;
const MAX_HEADER_LINE_BYTES: usize = 8 * 1024;
const MAX_HEADER_BYTES: usize = 32 * 1024;
const MAX_HEADER_LINES: usize = 32;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const CHILD_EXIT_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct LspClient {
    config: LspServerConfig,
    writer: Arc<Mutex<BufWriter<ChildStdin>>>,
    child: Mutex<Child>,
    #[cfg(windows)]
    job: Arc<StdMutex<Option<ChildProcessJob>>>,
    alive: Arc<AtomicBool>,
    pending_requests: Arc<Mutex<BTreeMap<i64, oneshot::Sender<Result<Value, LspError>>>>>,
    diagnostics: Arc<Mutex<BTreeMap<String, Vec<Diagnostic>>>>,
    open_documents: Mutex<BTreeMap<PathBuf, i32>>,
    next_request_id: AtomicI64,
}

impl LspClient {
    pub(crate) async fn connect(config: LspServerConfig) -> Result<Self, LspError> {
        let mut command = Command::new(&config.command);
        command
            .args(&config.args)
            .current_dir(&config.workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .envs(config.env.clone());

        command.kill_on_drop(true);
        #[cfg(windows)]
        let (mut child, job) = ChildProcessJob::spawn_managed_async(&mut command)?;
        #[cfg(not(windows))]
        let mut child = command.spawn()?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Protocol("missing LSP stdin pipe".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Protocol("missing LSP stdout pipe".to_string()))?;
        let stderr = child.stderr.take();

        let client = Self {
            config,
            writer: Arc::new(Mutex::new(BufWriter::new(stdin))),
            child: Mutex::new(child),
            #[cfg(windows)]
            job: Arc::new(StdMutex::new(Some(job))),
            alive: Arc::new(AtomicBool::new(true)),
            pending_requests: Arc::new(Mutex::new(BTreeMap::new())),
            diagnostics: Arc::new(Mutex::new(BTreeMap::new())),
            open_documents: Mutex::new(BTreeMap::new()),
            next_request_id: AtomicI64::new(1),
        };

        client.spawn_reader(stdout);
        if let Some(stderr) = stderr {
            client.spawn_stderr_drain(stderr);
        }
        if let Err(error) = client.initialize().await {
            client.terminate_child().await;
            return Err(error);
        }
        Ok(client)
    }

    pub(crate) async fn ensure_document_open(&self, path: &Path) -> Result<(), LspError> {
        if self.is_document_open(path).await {
            return Ok(());
        }

        let contents = std::fs::read_to_string(path)?;
        self.open_document(path, &contents).await
    }

    pub(crate) async fn open_document(&self, path: &Path, text: &str) -> Result<(), LspError> {
        let uri = file_url(path)?;
        let language_id = self
            .config
            .language_id_for(path)
            .ok_or_else(|| LspError::UnsupportedDocument(path.to_path_buf()))?;

        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text,
                }
            }),
        )
        .await?;

        self.open_documents
            .lock()
            .await
            .insert(path.to_path_buf(), 1);
        Ok(())
    }

    pub(crate) async fn change_document(&self, path: &Path, text: &str) -> Result<(), LspError> {
        if !self.is_document_open(path).await {
            return self.open_document(path, text).await;
        }

        let uri = file_url(path)?;
        let next_version = {
            let mut open_documents = self.open_documents.lock().await;
            let version = open_documents
                .entry(path.to_path_buf())
                .and_modify(|value| *value += 1)
                .or_insert(1);
            *version
        };

        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {
                    "uri": uri,
                    "version": next_version,
                },
                "contentChanges": [{
                    "text": text,
                }],
            }),
        )
        .await
    }

    pub(crate) async fn save_document(&self, path: &Path) -> Result<(), LspError> {
        if !self.is_document_open(path).await {
            return Ok(());
        }

        self.notify(
            "textDocument/didSave",
            json!({
                "textDocument": {
                    "uri": file_url(path)?,
                }
            }),
        )
        .await
    }

    pub(crate) async fn close_document(&self, path: &Path) -> Result<(), LspError> {
        if !self.is_document_open(path).await {
            return Ok(());
        }

        self.notify(
            "textDocument/didClose",
            json!({
                "textDocument": {
                    "uri": file_url(path)?,
                }
            }),
        )
        .await?;

        self.open_documents.lock().await.remove(path);
        Ok(())
    }

    pub(crate) async fn is_document_open(&self, path: &Path) -> bool {
        self.open_documents.lock().await.contains_key(path)
    }

    pub(crate) async fn go_to_definition(
        &self,
        path: &Path,
        position: Position,
    ) -> Result<Vec<SymbolLocation>, LspError> {
        self.ensure_document_open(path).await?;
        let response = self
            .request::<Option<GotoDefinitionResponse>>(
                "textDocument/definition",
                json!({
                    "textDocument": { "uri": file_url(path)? },
                    "position": position,
                }),
            )
            .await?;

        Ok(match response {
            Some(GotoDefinitionResponse::Scalar(location)) => {
                location_to_symbol_locations(vec![location])
            }
            Some(GotoDefinitionResponse::Array(locations)) => {
                location_to_symbol_locations(locations)
            }
            Some(GotoDefinitionResponse::Link(links)) => location_links_to_symbol_locations(links),
            None => Vec::new(),
        })
    }

    pub(crate) async fn find_references(
        &self,
        path: &Path,
        position: Position,
        include_declaration: bool,
    ) -> Result<Vec<SymbolLocation>, LspError> {
        self.ensure_document_open(path).await?;
        let response = self
            .request::<Option<Vec<Location>>>(
                "textDocument/references",
                json!({
                    "textDocument": { "uri": file_url(path)? },
                    "position": position,
                    "context": {
                        "includeDeclaration": include_declaration,
                    },
                }),
            )
            .await?;

        Ok(location_to_symbol_locations(response.unwrap_or_default()))
    }

    pub(crate) async fn diagnostics_snapshot(&self) -> BTreeMap<String, Vec<Diagnostic>> {
        self.diagnostics.lock().await.clone()
    }

    pub(crate) async fn shutdown(&self) -> Result<(), LspError> {
        let _ = self
            .request_with_timeout::<Value>("shutdown", json!({}), SHUTDOWN_TIMEOUT)
            .await;
        let _ = self.notify("exit", Value::Null).await;
        self.terminate_child().await;
        Ok(())
    }

    pub(crate) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    pub(crate) fn workspace_root(&self) -> &Path {
        &self.config.workspace_root
    }

    async fn terminate_child(&self) {
        self.alive.store(false, Ordering::Release);
        #[cfg(windows)]
        if let Ok(mut job) = self.job.lock() {
            job.take();
        }
        let mut child = self.child.lock().await;
        let _ = child.start_kill();
        let _ = timeout(CHILD_EXIT_TIMEOUT, child.wait()).await;
    }

    fn spawn_reader(&self, stdout: ChildStdout) {
        let diagnostics = &self.diagnostics;
        let pending_requests = &self.pending_requests;
        let alive = self.alive.clone();
        let writer = self.writer.clone();
        #[cfg(windows)]
        let job = self.job.clone();

        let diagnostics = diagnostics.clone();
        let pending_requests = pending_requests.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let result = async {
                while let Some(message) = read_message(&mut reader).await? {
                    if let Some(method) = message.get("method").and_then(Value::as_str) {
                        if let Some(id) = message.get("id") {
                            let reply = json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "error": { "code": -32601, "message": "Method not found" }
                            });
                            timeout(SHUTDOWN_TIMEOUT, send_payload_to_writer(&writer, &reply))
                                .await
                                .map_err(|_| {
                                    LspError::Protocol(format!(
                                        "timed out replying to server request `{method}`"
                                    ))
                                })??;
                            continue;
                        }
                        if method != "textDocument/publishDiagnostics" {
                            continue;
                        }
                        let params = message.get("params").cloned().unwrap_or(Value::Null);
                        let notification =
                            serde_json::from_value::<PublishDiagnosticsParams>(params)?;
                        let mut diagnostics_map = diagnostics.lock().await;
                        if notification.diagnostics.is_empty() {
                            diagnostics_map.remove(&notification.uri.to_string());
                        } else {
                            diagnostics_map
                                .insert(notification.uri.to_string(), notification.diagnostics);
                        }
                        continue;
                    }
                    if let Some(id) = message.get("id").and_then(Value::as_i64) {
                        let response = if let Some(error) = message.get("error") {
                            Err(LspError::Protocol(error.to_string()))
                        } else {
                            Ok(message.get("result").cloned().unwrap_or(Value::Null))
                        };

                        if let Some(sender) = pending_requests.lock().await.remove(&id) {
                            let _ = sender.send(response);
                        }
                        continue;
                    }
                }
                Ok::<(), LspError>(())
            }
            .await;

            alive.store(false, Ordering::Release);
            #[cfg(windows)]
            if let Ok(mut job) = job.lock() {
                job.take();
            }
            let reason = result.err().map_or_else(
                || "server stdout closed".to_string(),
                |error| error.to_string(),
            );
            let mut pending = pending_requests.lock().await;
            for (_, sender) in std::mem::take(&mut *pending) {
                let _ = sender.send(Err(LspError::Protocol(reason.clone())));
            }
        });
    }

    fn spawn_stderr_drain<R>(&self, stderr: R)
    where
        R: AsyncRead + Unpin + Send + 'static,
    {
        tokio::spawn(async move {
            let mut stderr = stderr;
            let _ = tokio::io::copy(&mut stderr, &mut tokio::io::sink()).await;
        });
    }

    async fn initialize(&self) -> Result<(), LspError> {
        let workspace_uri = file_url(&self.config.workspace_root)?;
        let _ = self
            .request::<Value>(
                "initialize",
                json!({
                    "processId": std::process::id(),
                    "rootUri": workspace_uri,
                    "rootPath": self.config.workspace_root,
                    "workspaceFolders": [{
                        "uri": workspace_uri,
                        "name": self.config.name,
                    }],
                    "initializationOptions": self.config.initialization_options.clone().unwrap_or(Value::Null),
                    "capabilities": {
                        "textDocument": {
                            "publishDiagnostics": {
                                "relatedInformation": true,
                            },
                            "definition": {
                                "linkSupport": true,
                            },
                            "references": {}
                        },
                        "workspace": {
                            "configuration": false,
                            "workspaceFolders": true,
                        },
                        "general": {
                            "positionEncodings": ["utf-16"],
                        }
                    }
                }),
            )
            .await?;
        self.notify("initialized", json!({})).await
    }

    async fn request<T>(&self, method: &str, params: Value) -> Result<T, LspError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        self.request_with_timeout(method, params, REQUEST_TIMEOUT)
            .await
    }

    pub(crate) async fn request_with_timeout<T>(
        &self,
        method: &str,
        params: Value,
        deadline: Duration,
    ) -> Result<T, LspError>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        if !self.is_alive() {
            return Err(LspError::Protocol(
                "language server is no longer running".to_string(),
            ));
        }
        let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending_requests.lock().await.insert(id, sender);

        let result = timeout(deadline, async {
            self.send_message(&json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params,
            }))
            .await?;
            receiver
                .await
                .map_err(|_| LspError::Protocol(format!("request channel closed for {method}")))?
        })
        .await;
        self.pending_requests.lock().await.remove(&id);
        let response = match result {
            Ok(response) => response?,
            Err(_) => {
                self.terminate_child().await;
                return Err(LspError::Protocol(format!(
                    "request `{method}` exceeded {} ms and language server was stopped",
                    deadline.as_millis()
                )));
            }
        };
        Ok(serde_json::from_value(response)?)
    }

    async fn notify(&self, method: &str, params: Value) -> Result<(), LspError> {
        self.send_message(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    async fn send_message(&self, payload: &Value) -> Result<(), LspError> {
        if !self.is_alive() {
            return Err(LspError::Protocol(
                "language server is no longer running".to_string(),
            ));
        }
        send_payload_to_writer(&self.writer, payload).await
    }
}

async fn send_payload_to_writer(
    writer: &Mutex<BufWriter<ChildStdin>>,
    payload: &Value,
) -> Result<(), LspError> {
    let body = serde_json::to_vec(payload)?;
    if body.len() > MAX_MESSAGE_BYTES {
        return Err(LspError::Protocol(format!(
            "outbound LSP message exceeds {MAX_MESSAGE_BYTES} bytes"
        )));
    }
    let mut writer = writer.lock().await;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    writer.write_all(&body).await?;
    writer.flush().await?;
    Ok(())
}

async fn read_message<R>(reader: &mut BufReader<R>) -> Result<Option<Value>, LspError>
where
    R: AsyncRead + Unpin,
{
    let mut content_length = None;
    let mut header_bytes = 0usize;
    let mut header_lines = 0usize;

    loop {
        if header_lines >= MAX_HEADER_LINES {
            return Err(LspError::Protocol("too many LSP header lines".to_string()));
        }
        let line = read_header_line(reader).await?;
        let Some(line) = line else {
            if header_lines == 0 {
                return Ok(None);
            }
            return Err(LspError::Protocol(
                "LSP header ended unexpectedly".to_string(),
            ));
        };
        header_lines += 1;
        header_bytes += line.len();
        if header_bytes > MAX_HEADER_BYTES {
            return Err(LspError::Protocol("LSP headers exceed limit".to_string()));
        }

        if line == b"\r\n" || line == b"\n" {
            break;
        }

        let line = std::str::from_utf8(&line)
            .map_err(|_| LspError::Protocol("LSP header is not UTF-8".to_string()))?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some((name, value)) = trimmed.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                let value = value.trim().to_string();
                if content_length.is_some() {
                    return Err(LspError::Protocol(
                        "duplicate LSP Content-Length header".to_string(),
                    ));
                }
                content_length = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| LspError::InvalidContentLength(value.clone()))?,
                );
            }
        } else {
            return Err(LspError::InvalidHeader(trimmed.to_string()));
        }
    }

    let content_length = content_length.ok_or(LspError::MissingContentLength)?;
    if content_length == 0 || content_length > MAX_MESSAGE_BYTES {
        return Err(LspError::InvalidContentLength(content_length.to_string()));
    }
    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body).await?;
    Ok(Some(serde_json::from_slice(&body)?))
}

async fn read_header_line<R>(reader: &mut BufReader<R>) -> Result<Option<Vec<u8>>, LspError>
where
    R: AsyncRead + Unpin,
{
    let mut line = Vec::new();
    while line.len() < MAX_HEADER_LINE_BYTES {
        match reader.read_u8().await {
            Ok(byte) => {
                line.push(byte);
                if byte == b'\n' {
                    return Ok(Some(line));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
                return if line.is_empty() {
                    Ok(None)
                } else {
                    Err(LspError::Protocol(
                        "LSP header line ended unexpectedly".to_string(),
                    ))
                };
            }
            Err(error) => return Err(LspError::Io(error)),
        }
    }
    Err(LspError::Protocol(
        "LSP header line exceeds limit".to_string(),
    ))
}

fn file_url(path: &Path) -> Result<String, LspError> {
    url::Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|()| LspError::PathToUrl(path.to_path_buf()))
}

fn location_to_symbol_locations(locations: Vec<Location>) -> Vec<SymbolLocation> {
    locations
        .into_iter()
        .filter_map(|location| {
            uri_to_path(&location.uri.to_string()).map(|path| SymbolLocation {
                path,
                range: location.range,
            })
        })
        .collect()
}

fn location_links_to_symbol_locations(links: Vec<LocationLink>) -> Vec<SymbolLocation> {
    links
        .into_iter()
        .filter_map(|link| {
            uri_to_path(&link.target_uri.to_string()).map(|path| SymbolLocation {
                path,
                range: link.target_selection_range,
            })
        })
        .collect()
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    url::Url::parse(uri).ok()?.to_file_path().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn parse_frame(frame: Vec<u8>) -> Result<Option<Value>, LspError> {
        let (mut writer, reader) = tokio::io::duplex(frame.len().max(64));
        writer.write_all(&frame).await.expect("write test frame");
        drop(writer);
        read_message(&mut BufReader::new(reader)).await
    }

    #[tokio::test]
    async fn oversized_frame_and_header_are_rejected_before_allocation() {
        let frame = format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1);
        assert!(matches!(
            parse_frame(frame.into_bytes()).await,
            Err(LspError::InvalidContentLength(_))
        ));

        let mut frame = b"X-Header: ".to_vec();
        frame.extend(vec![b'a'; MAX_HEADER_LINE_BYTES]);
        frame.extend_from_slice(b"\r\nContent-Length: 2\r\n\r\n{}");
        assert!(matches!(
            parse_frame(frame).await,
            Err(LspError::Protocol(message)) if message.contains("header line exceeds limit")
        ));
    }

    #[tokio::test]
    async fn duplicate_length_and_partial_header_are_rejected() {
        assert!(matches!(
            parse_frame(b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}".to_vec()).await,
            Err(LspError::Protocol(message)) if message.contains("duplicate")
        ));
        assert!(matches!(
            parse_frame(b"Content-Length: 2\r\nX-Header:".to_vec()).await,
            Err(LspError::Protocol(message)) if message.contains("ended unexpectedly")
        ));
    }
}
