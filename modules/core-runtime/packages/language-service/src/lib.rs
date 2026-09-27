mod client;
mod error;
mod manager;
mod types;

pub use error::LspError;
pub use manager::LspManager;
pub use types::{
    FileDiagnostics, LspContextEnrichment, LspServerConfig, SymbolLocation, WorkspaceDiagnostics,
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use lsp_types::{DiagnosticSeverity, Position};

    use crate::{LspManager, LspServerConfig};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("lsp-{label}-{nanos}"))
    }

    /// 真实探测一个可用的 Python 解释器：环境覆盖优先，其次 PATH 上的通用名；
    /// **每个候选都真实执行 `--version` 并校验退出状态**（不信任环境变量的值本身）。
    ///
    /// 顺序与 core-runtime 的 MCP stdio 测试保持一致：`MCP_TEST_PYTHON` → `PYTHON3` → `PYTHON`
    /// → `python3` → `python` → `/usr/bin/python3`。
    fn python_interpreter() -> Option<String> {
        let from_env = ["MCP_TEST_PYTHON", "PYTHON3", "PYTHON"]
            .iter()
            .filter_map(|key| std::env::var(key).ok())
            .filter(|value| !value.trim().is_empty());
        let fallback = ["python3", "python", "/usr/bin/python3"]
            .iter()
            .map(|candidate| (*candidate).to_string());
        from_env
            .chain(fallback)
            .find(|candidate| interpreter_works(candidate))
    }

    fn interpreter_works(program: &str) -> bool {
        Command::new(program)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
    }

    /// ## 本 crate 测试已声明的环境要求（RPR-01b）
    ///
    /// 1. `std::env::temp_dir()`（Windows 上是 `TEMP`/`TMP`）必须可写：mock LSP 服务器脚本与
    ///    临时工作区都建在它下面；
    /// 2. **需要本机 Python 解释器**：两个用例的 mock LSP 服务器是用 Python 写的，
    ///    解释器按 [`python_interpreter`] 的顺序逐个真实探测。
    ///
    /// ## 缺失前置时为什么是**失败**而不是跳过
    ///
    /// 改前这两个用例写的是 `let Some(python) = python3_path() else { return; };` ——
    /// libtest 把"什么都没做就返回"报成 **ok**：本机 `python3` 并不存在（只有 `python`），
    /// 于是 `cargo test -p coolzhu-language-service` 显示 `2 passed`，而 `LspManager` 的
    /// 端到端行为其实**一个断言都没跑**。这正是裁决 §5.2 禁止的"用 skip 令发布门禁全绿"：
    /// 这两个用例是本 crate 目前**唯一**验证 `LspManager` 的用例，缺前置 ⇒ 该能力在本机
    /// **验收未完成**，必须让门禁看见，而不能由跳过掩盖。
    ///
    /// 因此这里显式失败，并给出可直接执行的原因与可用的环境覆盖。这不是产品缺陷。
    fn require_python_interpreter(test_name: &str) -> String {
        match python_interpreter() {
            Some(program) => program,
            None => panic!(
                "[env-missing] {test_name}: 本机未验证——找不到可用的 Python 解释器\
                 （已按 MCP_TEST_PYTHON / PYTHON3 / PYTHON / python3 / python / /usr/bin/python3 \
                 顺序逐个执行 `--version` 校验）。本用例的 mock LSP 服务器由 Python 实现，\
                 缺解释器时它**不验证任何行为**，因此**不是通过**：该能力在本机验收未完成。\
                 可用 MCP_TEST_PYTHON 指向具体解释器（或设 PYTHON3/PYTHON）。\
                 独立统计口径：失败输出里的 [env-missing] 行，不计入通过。"
            ),
        }
    }

    fn write_mock_server_script(root: &std::path::Path) -> PathBuf {
        let script_path = root.join("mock_lsp_server.py");
        fs::write(
            &script_path,
            r#"import json
import sys


def read_message():
    headers = {}
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line == b"\r\n":
            break
        key, value = line.decode("utf-8").split(":", 1)
        headers[key.lower()] = value.strip()
    length = int(headers["content-length"])
    body = sys.stdin.buffer.read(length)
    return json.loads(body)


def write_message(payload):
    raw = json.dumps(payload).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(raw)}\r\n\r\n".encode("utf-8"))
    sys.stdout.buffer.write(raw)
    sys.stdout.buffer.flush()


while True:
    message = read_message()
    if message is None:
        break

    method = message.get("method")
    if method == "initialize":
        write_message({
            "jsonrpc": "2.0",
            "id": message["id"],
            "result": {
                "capabilities": {
                    "definitionProvider": True,
                    "referencesProvider": True,
                    "textDocumentSync": 1,
                }
            },
        })
    elif method == "initialized":
        continue
    elif method == "textDocument/didOpen":
        document = message["params"]["textDocument"]
        write_message({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {
                "uri": document["uri"],
                "diagnostics": [
                    {
                        "range": {
                            "start": {"line": 0, "character": 0},
                            "end": {"line": 0, "character": 3},
                        },
                        "severity": 1,
                        "source": "mock-server",
                        "message": "mock error",
                    }
                ],
            },
        })
    elif method == "textDocument/didChange":
        continue
    elif method == "textDocument/didSave":
        continue
    elif method == "textDocument/definition":
        uri = message["params"]["textDocument"]["uri"]
        write_message({
            "jsonrpc": "2.0",
            "id": message["id"],
            "result": [
                {
                    "uri": uri,
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 3},
                    },
                }
            ],
        })
    elif method == "textDocument/references":
        uri = message["params"]["textDocument"]["uri"]
        write_message({
            "jsonrpc": "2.0",
            "id": message["id"],
            "result": [
                {
                    "uri": uri,
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 3},
                    },
                },
                {
                    "uri": uri,
                    "range": {
                        "start": {"line": 1, "character": 4},
                        "end": {"line": 1, "character": 7},
                    },
                },
            ],
        })
    elif method == "shutdown":
        write_message({"jsonrpc": "2.0", "id": message["id"], "result": None})
    elif method == "exit":
        break
"#,
        )
        .expect("mock server should be written");
        script_path
    }

    async fn wait_for_diagnostics(manager: &LspManager) {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if manager
                    .collect_workspace_diagnostics()
                    .await
                    .expect("diagnostics snapshot should load")
                    .total_diagnostics()
                    > 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("diagnostics should arrive from mock server");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn collects_diagnostics_and_symbol_navigation_from_mock_server() {
        let python = require_python_interpreter(
            "collects_diagnostics_and_symbol_navigation_from_mock_server",
        );

        // given
        let root = temp_dir("manager");
        fs::create_dir_all(root.join("src")).expect("workspace root should exist");
        let script_path = write_mock_server_script(&root);
        let source_path = root.join("src").join("main.rs");
        fs::write(&source_path, "fn main() {}\nlet value = 1;\n")
            .expect("source file should exist");
        let manager = LspManager::new(vec![LspServerConfig {
            name: "rust-analyzer".to_string(),
            command: python,
            args: vec![script_path.display().to_string()],
            env: BTreeMap::new(),
            workspace_root: root.clone(),
            initialization_options: None,
            extension_to_language: BTreeMap::from([(".rs".to_string(), "rust".to_string())]),
        }])
        .expect("manager should build");
        manager
            .open_document(
                &source_path,
                &fs::read_to_string(&source_path).expect("source read should succeed"),
            )
            .await
            .expect("document should open");
        wait_for_diagnostics(&manager).await;

        // when
        let diagnostics = manager
            .collect_workspace_diagnostics()
            .await
            .expect("diagnostics should be available");
        let definitions = manager
            .go_to_definition(&source_path, Position::new(0, 0))
            .await
            .expect("definition request should succeed");
        let references = manager
            .find_references(&source_path, Position::new(0, 0), true)
            .await
            .expect("references request should succeed");

        // then
        assert_eq!(diagnostics.files.len(), 1);
        assert_eq!(diagnostics.total_diagnostics(), 1);
        assert_eq!(
            diagnostics.files[0].diagnostics[0].severity,
            Some(DiagnosticSeverity::ERROR)
        );
        assert_eq!(definitions.len(), 1);
        assert_eq!(definitions[0].start_line(), 1);
        assert_eq!(references.len(), 2);

        manager.shutdown().await.expect("shutdown should succeed");
        fs::remove_dir_all(root).expect("temp workspace should be removed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn renders_runtime_context_enrichment_for_prompt_usage() {
        let python = require_python_interpreter("renders_runtime_context_enrichment_for_prompt_usage");

        // given
        let root = temp_dir("prompt");
        fs::create_dir_all(root.join("src")).expect("workspace root should exist");
        let script_path = write_mock_server_script(&root);
        let source_path = root.join("src").join("lib.rs");
        fs::write(&source_path, "pub fn answer() -> i32 { 42 }\n")
            .expect("source file should exist");
        let manager = LspManager::new(vec![LspServerConfig {
            name: "rust-analyzer".to_string(),
            command: python,
            args: vec![script_path.display().to_string()],
            env: BTreeMap::new(),
            workspace_root: root.clone(),
            initialization_options: None,
            extension_to_language: BTreeMap::from([(".rs".to_string(), "rust".to_string())]),
        }])
        .expect("manager should build");
        manager
            .open_document(
                &source_path,
                &fs::read_to_string(&source_path).expect("source read should succeed"),
            )
            .await
            .expect("document should open");
        wait_for_diagnostics(&manager).await;

        // when
        let enrichment = manager
            .context_enrichment(&source_path, Position::new(0, 0))
            .await
            .expect("context enrichment should succeed");
        let rendered = enrichment.render_prompt_section();

        // then
        assert!(rendered.contains("# LSP context"));
        assert!(rendered.contains("Workspace diagnostics: 1 across 1 file(s)"));
        assert!(rendered.contains("Definitions:"));
        assert!(rendered.contains("References:"));
        assert!(rendered.contains("mock error"));

        manager.shutdown().await.expect("shutdown should succeed");
        fs::remove_dir_all(root).expect("temp workspace should be removed");
    }
}
