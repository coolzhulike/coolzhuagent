# CLAUDE.md — coolzhu code agent 协作约定

本文件供 Claude Code 在本仓库工作时参考。**始终用中文思考与回复。**

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
- `src/main.rs`（≈1.36 MB，本地 HTTP 服务 + 全部业务后端，**巨型单文件**）

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
3. **大文件编辑要谨慎**：`main.rs` 1.36 MB、`app.js` 266 KB。仓库有 `tmp/backups/main-rs-corrupted-*` 损坏-恢复前科。
   - 单文件超 256 KB 必须用 Read 的 offset/limit 分段读。
   - 改前先 Read 确认确切上下文，**不要凭记忆/假设函数名**（曾误以为 web-console 里有 `decode_text_lossy` 导致编译失败）。
4. **改完必编译**：`cargo build -p <crate> --offline`。注意增量缓存可能让 `cargo check` 误报 exit 0；涉及新符号/依赖时用 `cargo build` 验证。
5. **每个 crate 各自独立**：`decode_console_output` 这类辅助函数在 web-console 和 tool-registry 中**各有一份**（它们不共享 util），不要假设跨 crate 可见。
6. **不要把易失败的命令塞进大批并行调用**：一个 `Get-Process` 返回 exit 1 会连带取消整批并行工具调用。探测类命令单独发。
7. **工具调用格式硬约束（防"运行后停住"）**：每个工具调用必须用带 `antml:` 命名空间前缀的标签
   （antml:function_calls / antml:invoke name="工具名" / antml:parameter name="参数名"）。
   严禁写成裸 `call` 或缺前缀的 `invoke`/`parameter`——harness 无法解析时会当普通文本打印，
   表现为"脚本运行后就停住"。本会话多次因此中断，每次发工具调用前自查前缀是否齐全。

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
- 现状：plan→execute→verify 基本单向，缺失败回退（`CUR-GOAL-LOOP-001`）。
  目标增强：implementer 受阻 → 回 planner 重规划；verifier 不通过 → 回 implementer 重改（均带原因/证据 + 重试上限）。

## 文档与日志

- 需求与未闭环问题：`docs/current-issues-and-unfinished-requirements-2026-05-21.md`
- work-log 放 `docs/work-logs/`，按日期命名。
- 本轮 work-log：`docs/work-logs/2026-05-29-coolzhu-interactive-dev-encoding-and-roadmap.md`
