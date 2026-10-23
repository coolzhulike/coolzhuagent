# AGENT.md — coolzhu 内部会话约定

本文件是 coolzhu 内部模型会话（聊天室、Goal 阶段、chat_handoff 对端）的统一约定：
前半部分是工程纪律，后半部分是工具调用参考表。**始终用中文思考与回复。**

## 项目概览

`coolzhu code agent` 是一个 Rust workspace（见根 `Cargo.toml`），核心模块：

- `modules/core-runtime`：会话运行时、权限、记忆 beads、conversation
- `modules/llm-adapter`：Provider 适配、真实模型调用、流式输出
- `modules/tooling`：工具注册（`tool-registry`）、插件系统、命令路由
- `modules/vision`：截图、视觉模型、坐标解析、UIA
- `modules/computer-use`：鼠标键盘、安全点击、分辨率映射
- `modules/gui-web`：**主控制台**（`packages/web-console`）
- `modules/gui-desktop`：桌面入口 / 桌宠 / Tauri shell
- `modules/cli`、`modules/diagnostics`

### 主控制台前端（最常改动）

`modules/gui-web/packages/web-console/`：
- `index.html`（≈833 行，UI 骨架，UTF-8 无 BOM）
- `src/app.js`（≈266 KB，前端逻辑，**单文件**）
- `src/styles.css`（≈105 KB）
- `src/main.rs`（≈3.6 MB，本地 HTTP 服务 + 全部业务后端，**巨型单文件**）

`main.rs` 通过 `include_str!` 内联 `index.html / app.js / styles.css`，以 `charset=utf-8` 返回。
**改前端后必须重新 `cargo build -p coolzhu-web-console` 才能在浏览器看到效果**（资源是编译期内联，不是运行时读盘）。

### 运行方式

```powershell
cargo run -p coolzhu-web-console        # 默认 http://127.0.0.1:8765/
cargo run -p coolzhu-web-console -- --open   # 顺便打开浏览器
```

会话存储：`.coolzhu/web-sessions.json` 与 `.coolzhu/web-sessions.sqlite3`。
仓库内 JSON 只有 `mario-demo / agent-test001 / agent-test002`；**`test5/test6` 是运行时会话**，
只存在于实际运行 app 的工作目录/sqlite，需 app 启动后通过 API 或聊天室寻址驱动，离线脚本无法复现。

## 硬性约定

1. **中文优先**：思考、回复、代码注释、work-log 一律中文。
2. **临时脚本与日志放 `tmp/`**（已被 .gitignore 忽略）。分析中间产物落盘到 `tmp/analysis-*.txt` 再读，避免被工具结果摘要截断。
3. **大文件编辑要谨慎**：`main.rs` ≈3.6 MB、`app.js` 数百 KB。仓库有 `tmp/backups/main-rs-corrupted-*` 损坏-恢复前科。
   - 单文件超 256 KB 必须用 Read 的 offset/limit 分段读。
   - 改前先 Read 确认确切上下文，**不要凭记忆/假设函数名**（曾误以为 web-console 里有 `decode_text_lossy` 导致编译失败）。
4. **改完必编译**：`cargo build -p <crate> --offline`。注意增量缓存可能让 `cargo check` 误报 exit 0；涉及新符号/依赖时用 `cargo build` 验证。
5. **每个 crate 各自独立**：`decode_console_output` 这类辅助函数在 web-console 和 tool-registry 中**各有一份**（它们不共享 util），不要假设跨 crate 可见。
6. **不要把易失败的命令塞进大批并行调用**：一个 `Get-Process` 返回 exit 1 会连带取消整批并行工具调用。探测类命令单独发。

## 中文 Windows 编码（乱码根因）

- 静态前端文件本身是干净 UTF-8（已字节级核验，0 替换符）。
- 真正乱码源：后端用 `String::from_utf8_lossy` 直接解码 `git / powershell.exe / tasklist` 等子进程 stdout/stderr。
  中文 Windows 默认输出 **GBK/GB18030（代码页 936）**，UTF-8 强解即乱码（Diff 视图 / 终端窗口）。
- 修复模式：用 `decode_console_output(bytes)`（先 UTF-8，失败回退 GB18030，最后 lossy）。
  - web-console：`src/main.rs` 顶部已定义；git diff 的 `diff` 与 `stderr` 已接入。
  - tool-registry：`src/lib.rs` 已定义并接入 `run_process`/shell 输出；依赖加了 `encoding_rs = "0.8"`。
- 仍待补：终端窗口若仍乱码，排查 `core-runtime` 的 `bash.rs`（`execute_bash` 的 stdout/stderr 解码）。

## 最小验证命令

```powershell
cargo build -p coolzhu-web-console --offline
cargo test  -p coolzhu-web-console --offline
cargo check -p coolzhu-tool-registry --offline
cargo test  --test module_linkage_smoke --offline
```

## Goal 模式关键位置

- 路由 / handler：`modules/gui-web/packages/web-console/src/main.rs`
  - `api_goal_commander_review`、`api_run_goal_phase`、`api_run_next_goal_phase`、`api_run_all_goal_phases`、
    `run_goal_phase_once`、`next_runnable_goal_phase_id`、`dispatch_ready_goal_phases`
  - SQLite 状态：`*_goal_*_sqlite` 系列函数
- 前端任务链 / 任务卡片：`src/app.js`
  - `refreshOpenGoalTaskChain`、`openGoalTaskChain`、`syncTaskCardFromGoals`、`taskRenderHandoffSummary`
  - 阶段角色：commander / planner / implementer / verifier
- 现状：已实现失败回退链——implementer 受阻回 planner 重规划、verifier 确定性门（FilesExist/Command）不通过回
  implementer 重改（带原因/证据 + `retry_count` 重试上限，预算不重置）；普通 handoff 防环不误拒合法
  Goal 恢复交接。回归：`goal_phase_token_finalizer_repair_returns_to_verifier_without_resetting_budget`。

## 文档与日志

- 需求与未闭环问题：`docs/current-issues-and-unfinished-requirements-2026-05-21.md`
- work-log 放 `docs/work-logs/`，按日期命名。

## 工具调用参考表

Before any tool call, check the matching demo and pass parameters exactly as the
tool schema requires.

### Hard Rules

- Use the tool's top-level JSON fields directly. Do not wrap arguments in
  `raw` unless the tool schema explicitly requires a `raw` field.
- If a tool returns `missing field`, immediately retry with the required fields
  at the top level.
- For file creation or edits, prefer `write_file` or `edit_file` over shell
  redirection.
- For Windows shell work, prefer `PowerShell`. Use `bash` only for commands that
  are valid in the current shell runtime.
- Shell commands are OS-specific:
  - Windows: use `PowerShell` (`New-Item`, `Set-Content`, `Get-ChildItem`,
    `Test-Path`). Do not send POSIX-only syntax such as `mkdir -p`, heredocs,
    `cat > file <<EOF`, or bare `ls` to the Windows `bash` compatibility path.
  - Linux/macOS: use `bash`/POSIX shell syntax unless the task explicitly needs
    PowerShell.
- A successful directory creation is not a completed file task. If the expected
  artifact is a file, call `write_file`, `edit_file`, `PowerShell`, or `bash`
  until the exact file exists and has meaningful content.
- After writing a required artifact, verify with `read_file` or an explicit file
  existence/content check.

### Working Approach

- Plan first: for multi-step tasks, use `grep_search`/`glob_search` to discover the
  relevant context and outline the steps before acting. Do not assume file names or
  APIs — verify them.
- Call tools in parallel: batch independent `read_file`/`glob_search`/`grep_search`
  calls in a single turn instead of one at a time.
- Minimal change: do only what was asked. Do not over-engineer, add unrequested
  abstractions, or refactor unrelated code.
- Verify with signals: after changing code, run the relevant build/test and check the
  exit code — do not rely on "looks correct". For a Goal phase with Command
  verification, a non-zero exit auto-routes the phase back to implementer; fix until
  it passes.
- Be persistent: do not hand back before the task is fully done and verified. When
  blocked, diagnose the root cause — do not retry the same failing command in a loop.
- Decide vs ask: in autonomous contexts (Goal phases) proceed with sensible defaults
  instead of waiting for input; confirm first only for destructive or outward-facing
  actions (delete, overwrite, push).

### Common Tool Demos

| Tool | Use for | Correct parameters |
| --- | --- | --- |
| `write_file` | Create or replace a text file | `{"path":"goal-artifacts/demo.html","content":"<!doctype html><html><body>ok</body></html>"}` |
| `edit_file` | Replace known text in a file | `{"path":"src/app.js","old_string":"old text","new_string":"new text","replace_all":false}` |
| `read_file` | Read a text file | `{"path":"goal-artifacts/demo.html","offset":0,"limit":200}` |
| `glob_search` | Find files by glob | `{"pattern":"goal-artifacts/**/*.html","path":"."}` |
| `grep_search` | Search file contents | `{"pattern":"write_file","path":"modules","include":"*.rs"}` |
| `bash` | Run a POSIX shell command on Linux/macOS, or a Windows cmd-compatible command only when necessary | `{"command":"dir goal-artifacts","timeout":30000,"description":"List artifacts"}` |
| `PowerShell` | Run a PowerShell command on Windows | `{"command":"Get-ChildItem -LiteralPath goal-artifacts","timeout":30000,"description":"List artifacts"}` |
| `WebSearch` | Search the web | `{"query":"2026 mainstream AI models comparison"}` |
| `WebFetch` | Fetch a URL | `{"url":"https://example.com","prompt":"Summarize the page."}` |
| `chat_handoff` | Send a task to another configured chat agent | `{"to":"test3","intent":"Verify the output artifact","attach":"recent"}` |
| `tools_semantic_dispatch` | Legacy semantic dispatch or dry-run planning | `{"intent":"create file demo.txt with hello","execute":true,"target":"demo.txt","text":"hello"}` |

### Bad Patterns To Avoid

Do not call:

```json
{"raw":"{\"path\":\"demo.html\",\"content\":\"ok\"}"}
```

Call:

```json
{"path":"demo.html","content":"ok"}
```

Do not call:

```json
{"raw":"{\"command\":\"mkdir -p goal-artifacts\"}"}
```

Call:

```json
{"command":"New-Item -ItemType Directory -Force -Path goal-artifacts","timeout":30000}
```

### Goal Artifact Checklist

For a Goal phase with `FilesExist` verification:

1. Identify every required artifact path.
2. Create parent directories if needed.
3. Write the exact artifact file with meaningful content.
4. Read or inspect the exact artifact path.
5. Report the concrete path and verification evidence.
