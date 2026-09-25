# 功能全量清单（核对基线）

**用途**：逐项列出两侧的**全部**模块/包，供核对"是否有遗漏"。完成度标记：`[完整]` `[部分]` `[stub]` `[实验性]` `[未接线]` `[死代码]`。
配套主文档：`01-coolzhu-dsh-架构差异与整合决策.md`

- A 侧 = `C:\Users\zhupu\Desktop\coolzhuagent`：Cargo workspace **28 个成员** = 18 个模块 crate + 9 个插件 crate + 根包；另有 1 个独立的 `tauri-shell` Cargo 项目（不在 workspace 内）、3 个仅 manifest 的插件目录、1 个纯 JS 的 `browser-extension`、3 个 skill
- B 侧 = `C:\Users\zhupu\Desktop\dsh`：**291 个 workspace 包**（复核：`find packages -mindepth 3 -maxdepth 3 -name package.json | wc -l` = 291）

---

# A 侧 coolzhu 全量清单

## A1 modules/core-runtime（3 crate）

### coolzhu-core-runtime `[完整]`
- 职责：会话运行时、权限、记忆、配置、MCP、OAuth、hooks。
- 对外：`Session`、`ConversationMessage`、`ToolExecutor`、`ConfigLoader`（`src/lib.rs:108-113`）；`ConversationRuntime<C: ApiClient, T: ToolExecutor>`（`conversation.rs:104`，`run_turn` `:176`）；`AgentEvent` 6 变体。
- 文件面：`conversation.rs`、`session.rs`（`version: u32` `:48`）、`permissions.rs`（`PermissionMode` `:7-13`、`PermissionProfile` `:30-34`、`PermissionPolicy::authorize` `:129-174`）、`tool.rs`（`PermissionDecision` `:93-99`、`PermissionGateReport.affected_paths` `:126-135`）、`hooks.rs`（`HookRunner::run_pre_tool_use`/`post_tool_use` `:76,88`）、`memory.rs`（`MemoryLayer` L0-L4 `:6-37`、`decide_memory_write` `:229-300`、`query_memory_beads` `:461-527`）、`semantic.rs`（`hash_embed` `:38-50`、`BruteForceCosineIndex` `:112,124`）、`config.rs`（`ConfigLoader` 分层 `:11-16,213-254`）、`mcp.rs`(300 行)、`mcp_client.rs`(234 行)、`mcp_stdio.rs`(2,060 行)、`oauth.rs`(585 行)、`usage.rs`、`bootstrap.rs`、`path_effect.rs`（`TargetPathsExtractor` `:20`、`extractor_for` `:109`）、`agent_guide`（内联常量）。
- 缺口：`ConfigLoader`/`RuntimeConfig` 标 `#[allow(dead_code)]`。

### coolzhu-agent-server `[未接线]`
- 职责：axum 内存会话托管 + SSE。
- 对外：`POST/GET /sessions`、`GET /sessions/{id}`、`GET /sessions/{id}/events`(SSE)、`POST /sessions/{id}/message`（`src/lib.rs:140-145`）。
- 缺口：442 行单文件；`AppState{sessions: Arc<RwLock<HashMap>>, next_session_id: AtomicU64}`、`BROADCAST_CAPACITY=64`；**无任何 crate 依赖它**（仅 `Cargo.toml:59` 声明 `workspace.dependencies.server`；唯一使用 `tests/module_linkage_smoke.rs:31`）；无持久化、无认证、不依赖 llm-adapter。

### coolzhu-language-service `[未接线]`
- 职责：stdio JSON-RPC LSP 客户端 + prompt 片段渲染。
- 对外：`LspManager`（`manager.rs:15-21`）、`open_document`/`sync_document_from_disk`/`change_document`/`save_document`/`close_document`/`go_to_definition`(`:85`)/`find_references`(`:99`)/`collect_workspace_diagnostics`(`:114`)/`context_enrichment`(`:147`)/`shutdown`；`LspContextEnrichment::render_prompt_section`（`types.rs:98`）。
- 缺口：**全仓无生产构造点**（`LspManager::new` 仅 `#[cfg(test)]` `src/lib.rs:197,258`，测试用内嵌 Python mock LSP server，需 python3）；无 completion/hover/rename。

## A2 modules/llm-adapter（1 crate）

### coolzhu-llm-adapter `[完整]`
- 职责：模型 Provider、流式响应、嵌入适配。
- 对外：`ProviderKind` enum（10 变体 `providers/mod.rs:30-40`）、`trait Provider { type Stream }`（`:11`）、`ProviderClient` enum（`client.rs:29-36`）；`PROVIDER_CATALOG`(8)、`MODEL_REGISTRY`(~50)；`RequestParameters`（`request_parameters.rs:6-12`，`validate_for_model` `:17`、`apply` `:41`）；`StreamEvent`/`ContentBlockDelta`（`types.rs:246`）；`api::embed_texts`（`embeddings.rs:33-70`）。
- 关键机制：OpenAI-compat 流式 tool call 状态机（`providers/openai_compat.rs:520-770`，`tool_calls: BTreeMap<u32, ToolCallState>`）；wire DTO `:798-870`；`sse.rs` 非流式。
- 缺口：新增 provider 需改 ~10 处（`providers/mod.rs`、`openai_compat.rs:83-163`），**无注册 API**。

## A3 modules/tooling（4 crate）

### coolzhu-tool-registry `[完整]`
- 职责：工具注册、模型可见性、路径抽取。
- 对外：`mvp_tool_specs()` **20 个 `ToolSpec`**（`src/lib.rs:281-598`）；`GlobalToolRegistry::definitions(allowed_tools)`（`:183-205`）；`path_effect::TargetPathsExtractor`。
- 缺口：`GlobalToolRegistry` 用 `OnceLock<RwLock<Option<PathBuf>>>` 全局单例（`:88-97`）；插件工具与内置重名**直接报错**（`:116-123`）。

### coolzhu-plugin-system `[完整]`
- 职责：插件 manifest、加载、生命周期、hook、子进程工具执行。
- 对外：`Plugin` trait（`:400-408`）、`BuiltinPlugin`/`BundledPlugin`/`ExternalPlugin`（`:376-398`）；manifest `.claw-plugin/plugin.json`（`:20-21`，字段 `:106-122`，`tools[]` `:158-168`，`commands[]` `:207-212`）；`HookRunner`（`hooks.rs:65,77`）。
- 缺口：`PluginPermission` 运行时**无强制点**；`requiredPermission` 缺省 `danger-full-access`（`:341-343`）；hook 契约 exit 0/2/其他（`hooks.rs:178-195`）。

### coolzhu-command-router `[完整]`
- 职责：slash 命令解析与 skill 发现。
- 对外：`SlashCommand::parse`（`:378`）、`slash_command_specs()`（`:474`）；skill 扫描（`:1314-1370`）+ `shadowed_by` 去重（`:1485-1554`）。

### coolzhu-compatibility-harness `[完整]`
- 职责：扫描上游 TS 仓库 claw-code 的 `src/commands.ts`/`src/tools.ts`/`src/entrypoints/cli.tsx`，正则抽取 command/tool 清单与 bootstrap 计划（`src/lib.rs:35-47,88-98`），仓库根靠 `CLAW_CODE_UPSTREAM` 等候选路径解析（`:57-86`）。
- 注意：**与插件兼容性无关**，用途是"对齐上游功能面"。

## A4 modules/vision（2 crate）

### coolzhu-vision-service `[完整 + stub]`
- 职责：视觉后端抽象与 grounding 文本解析（不含截图、不执行输入）。
- 对外：`VisionBackend`（`:232`）→ `ZhipuVisionBackend`（默认 `glm-4.6v-flash` `:299`）、`LocalOpenAiVisionBackend`（`:366,1422`）；`DetectionBackend`（`:237`）→ `HttpDetectionBackend`（UI-DETR-1 `:479`）；`parse_grounding_result`(`:752`)、`parse_detection_elements`(`:768`)、`relative_point_to_pixel`(`:830`)；CLI `src/main.rs`（`--showui-ground`）、`bin/latest_desktop.rs`。
- 缺口：`Describe`/`Ocr`/`RealtimePerception` = `Reserved`，返回 "model calls are disabled for this round"（`:598,605,626`）；**无 OCR 引擎**；**无元素缓存**；`uncrop_point`/`anchor_region` 仅自测引用（`local_backend.rs:106,126`）。
- 双模式路由实现在 web-console（不在本 crate）：pipeline 默认 `["uia","ocr_template","local_vlm","remote_vlm"]`（`main.rs:5617`）；`cluster_median(eps=48px, min=2)` DBSCAN（`:20869`）；`cross_verify_tolerance_px` `[死代码]`（`:5600`）。

### coolzhu-uia-resolver `[部分]`
- 职责：UIA 树快照、元素解析、窗口聚焦。
- 对外：`snapshot_foreground_window(limit)`（`:86`→`windows_impl.rs:348`）、`resolve_query`（`:126`）、`resolve_system_control`（`:174`）。
- 缺口：**仅实现 `StartButton`**（`:192-279`，写临时 `.ps1` 调 .NET `UIAutomationClient`），其余 10 个 `SystemControlId` `[stub]`；无元素缓存/句柄复用/Invoke pattern。

## A5 modules/computer-use（1 crate）

### coolzhu-computer-use-core `[完整 + 死代码]`
- 职责：鼠标/键盘/坐标/笔画 + 预算熔断 + 审批策略。
- 文件面：`input.rs`(1,209 行，13 个原语)、`input_stroke.rs` + `input_stroke_native.cs`(101 行)、`supervisor.rs`、`controller.rs`(1,501 行)、`contracts.rs`、`lib.rs`(回归资材)、`bin/check.rs`。
- 对外：`ComputerUsePlanner`/`ComputerUseAdapter`/`ComputerUseEventSink`/`ComputerUseClock`/`ComputerUseApprovalPolicy`（`controller.rs:11-90`）；`host_sensitive_semantic_category`（`:92`）；`TurnComputerUseSupervisor`（`supervisor.rs:72`）；`RunBudgetGuard`（`:174`，硬上限 `max_actions=12`/`max_replans=2`/`max_same_signature=2`/`max_no_progress_steps=2`/`timeout_ms=120_000`/`max_calls_per_turn=2`，`contracts.rs:194-203`）。
- 关键机制：**无 Rust 侧 Win32 FFI**，每动作拼 PowerShell + 内联 C# P/Invoke（`input.rs:313,317,1069`）；后端 `CLAW_MOUSE_BACKEND=auto|sendinput|interception`（`:238-267`）。
- 缺口：`move_mouse_absolute`/`mouse_button_down_point` `[死代码]`（`:86,90`）；**不处理 DPI/多显示器，截图仅主屏**；**无输入内容安全控制**。

## A6 modules/gui-web（3 crate）

### coolzhu-web-console `[完整，单文件巨型]`
- 职责：产品主路径。HTTP 路由（232 条）、SSE、会话运行时、权限闸门、SQLite schema 与迁移（v2→v20）、Goal DAG、前端资源内联、桌宠状态、clawbot 网关、vision 路由、IDE 索引、诊断自检。
- 规模：`src/main.rs` **87,025 行 / 3.47 MB**；`src/app.js` 20,416 行 / 765 KB；`index.html` 1,478 行；`styles.css` 17,809 行；`src/model_settings.js`、`src/chat_insights.rs`、`src/chat_experience.js`、`src/realtime_voice_capture.js`、`src/browser_bridge.rs`、`src/wuxia_layout.css`、`assets/bamboo-*.js`。
- 缺口：`grep -c ConversationRuntime` = **0**；`api_plugins_install` `[stub]`；`/api/diagnostics/stream` `[stub]`；`/api/web/cards` 硬编码清单前端不调用。

### coolzhu-clawbot-sidecar `[完整]`
- 职责：微信通道 provider 适配（HTTP 客户端 + 本地队列轮询）。
- 对外：provider 契约 `/login/refresh`/`/login/logout`/`/updates`(45s 长轮询)/`/send_text`/`/send_file`（`lib.rs:743-818`）；自身 `/health,/version,/tick,/login/refresh,/login/logout`；`GatewayClient`（`:932`）、`spawn_polling_loop`（`:1141`）。
- 缺口：真实微信侧实现不在本仓（provider 端需外部服务）；`MockClawbotProvider` 供测试。

### coolzhu-windows-process-guard `[完整]`
- 职责：web-console 所需 Windows 句柄操作的**唯一 `unsafe` 容器**（其余 workspace 禁止 unsafe，`lib.rs:1-7`）。
- 对外：`make_socket_non_inheritable`/`socket_is_inheritable`（`Set/GetHandleInformation` 清 `HANDLE_FLAG_INHERIT`）；`ChildProcessJob`（`CreateJobObjectW` + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，`Drop` 时 `CloseHandle` 使子进程随 Web Console 退出而终止）。
- 被谁用：web-console（`main.rs:849,3495-3498`）；**0 个单元测试**。

## A7 modules/gui-desktop（2 crate）

### coolzhu-tauri-shell `[完整]`
- 职责：桌面壳 + 托盘 + 桌宠窗口 + 内嵌 web console WebView。
- 对外：12 个 IPC 命令（`main.rs:202-215`：`toggle_console`、`show_console_command`、`hide_console_command`、`quit_app`、`start_pet_dragging`、`report_throne_zone`、`stabilize_pet_window_command`、`pet_status`、`set_pet_action`、`pet_drop_uploaded`、`browser_window_command`、`open_browser_window`）；窗口 `console`+`pet`；capabilities 仅授权这两窗 + `remote.urls=["http://127.0.0.1:8765/*","http://localhost:8765/*"]`。
- 缺口：**0 个单元测试**。

### coolzhu-desktop-console `[未接线]`
- 职责：eframe+egui 独立 GUI，**不经 web 层**。
- 面板：左栏 session/agent，右栏 inner_vision/outer_vision/tool/test_lab，另有 config 面板（`app.rs:773-1366`）；`desktop_capture.rs:214`（powershell CopyFromScreen）、`desktop_anchor.rs`、`input_backend.rs`(4 行 re-export)。
- 缺口：**全仓无任何 crate/脚本引用它，`package-manifest.json` 也不构建它** → 仅手工 `cargo run` 入口。

## A8 modules/cli（1 crate）

### coolzhu-command-line `[完整 + 死代码]`
- 职责：CLI 入口 + REPL。**唯一使用 `ConversationRuntime` 的生产端**。
- 对外：argv parser（`main.rs:245`，**未用 clap**）；flags `--model`/`--output-format text|json`/`--permission-mode`/`--dangerously-skip-permissions`/`-p`/`--print`/`--allowedTools`/`-V`；子命令 `dump-manifests`/`bootstrap-plan`/`agents`/`skills`/`system-prompt`/`login`/`logout`/`init`/`prompt`/默认 REPL（`main.rs:184,459`）；`run_repl`（`:1095`）、`handle_repl_command` **25 个 slash 变体**（`:1397`）、`LiveCli::run_turn`（`:1329`）、`run_prompt_json`（`:1354`）。
- 规模：8,676 行 `main.rs` + `input.rs` 1,195 行。
- 缺口：`args.rs`(104 行，clap 版) `[死代码]`；`app.rs`(402 行，第二套 REPL) `[死代码]`；`branch`/`worktree`/`commit-push-pr` 只打印 `render_mode_unavailable`（`:1512-1526`）；流式**阻塞非增量**（`conversation.rs:39`）。

## A9 modules/diagnostics（1 crate）

### coolzhu-diagnostics `[完整]`
- 职责：进程级结构化日志/trace/span。
- 对外：`init`(`:84`)、`log_path`(`:80`)、`set_gui_callback`(`:122`)、`error/warn/info/debug/trace/emit`(`:127-186`)、`error_event`(`:142`)；`LogEntry{timestamp_ms,level,app,module,event,message,trace_id,span_id,fields}`(`:63-72`)；`LogLevel` 1–5(`:28-36`)；`start_span`(`span.rs:146`)/`start_span_with_parent`(`:177`)/`SpanGuard::record|event`(`:46,52`)/`SpanClosed`(`:133`)；`TraceId(u128)`/`SpanId(u64)`(`trace_id.rs:4-8,51`)。
- 输出：`COOLZHU_LOG_DIR`|`CLAW_LOG_DIR`|`{USERPROFILE|HOME|tmp}/.coolzhu/logs/{app}.jsonl`(`lib.rs:277-284`)；`COOLZHU_LOG_CONSOLE`。
- 缺口：**无轮转/无清理**（`output.rs:22-27`）；Span 生产使用**仅 1 处**（`web-console/src/main.rs:27132`）；日志**无读取端**。

## A10 modules/browser-extension（无 crate）

### browser 扩展 `[完整]`
- `manifest.json`：MV3，`permissions=[activeTab,scripting,nativeMessaging,tabs,storage]`，`host_permissions=["http://*/*","https://*/*"]`，content_script 于所有 http(s) `document_idle` 注入 `all_frames:false`。
- `content_script.js`：DOM 快照生成候选元素表 `dom-N`（`:284`）、`MutationObserver` 维护 revision（`:6,8`）、可执行动作固定白名单 12 个（`:380-393`）、按键限 14 个（`browser_bridge_protocol.rs:272`）。**无任意脚本执行、无截图**。
- `service_worker.js`：tab 生命周期 `open(owner_token)`/`activate`/`close_owned`，`chrome.storage.session` 持久化 owned tabs 并在重启后回收（`:25-90`），非 owned tab 拒绝操作；导航/前进后退走 `chrome.tabs`（`:267-275`）。
- Rust 侧桥：`browser_bridge.rs` 的 `BrowserNativeBridge` 实现 `ComputerUseAdapter`（`:402`），`ComputerUseAction→BrowserAction`（`:644`），broker `request_id`+`reply_token` 配对 + `wait_for_bridge_response`（`:224`），HTTP 回包兜底 `POST /api/computer-use/browser/response`（`main.rs:56115`），`self_test`（`:1164-1461`）。
- 握手：native host 校验 argv `chrome-extension://akpgmkdkaofanikngahmfbhddpppicfi/`（`bin/browser_native_host.rs:22`）+ `runtime-bridge-nonce` 的 `Hello{nonce}`（`browser_bridge.rs:264`）。

## A11 packages/app-launcher（1 crate）

### coolzhu-app-launcher `[完整]`
- 职责：安装后唯一用户入口 `COOLZHU-AGENT.exe`，编排 web console + tauri shell。
- 对外：`LauncherConfig::from_json_with_env`（`lib.rs:201`）、`launch()`（`:400`）、`poll_health`（`:370`）、selfcheck JSON（`:420`）；`preflight_web_console_port` + `classify_listener_recovery`（`main.rs:101-223`）。
- 缺口：单实例靠 listener+health 探测，**无 mutex**。

## A12 .coolzhu/plugins（9 插件，全部 `[未接线]`）

| 插件 | manifest `tools`/`hooks` | 声明权限 | 状态 |
|---|---|---|---|
| coolzhu-tdd-runner | 无 | — | `[未接线]` workspace 成员 |
| coolzhu-git-workflow | 无 | — | 同上 |
| coolzhu-code-review | 无 | — | 同上 |
| coolzhu-orchestrator | 无 | — | 同上；**实现完整但运行时无调用方** |
| coolzhu-docgen | 无 | — | 同上 |
| coolzhu-db-tools | 无 | — | 同上 |
| coolzhu-debug-diag | 无 | — | 同上 |
| coolzhu-monitor | 无 | — | 同上 |
| coolzhu-marketplace | 无 | read, write | 同上；`TemplateMarket` 内存 HashMap，`install()` 只 `downloads += 1` |
| coolzhu-agents-md-updater | 无 | — | `[stub]` 仅 manifest，未列入 workspace |
| coolzhu-claude-compat | 无 | — | `[stub]` 同上 |
| coolzhu-opencode-sync | 无 | — | `[stub]` 同上 |

**已核实**：`grep -l '"tools"\|"hooks"' .coolzhu/plugins/*/plugin.json` → 无命中。全部 9 个 workspace 成员插件注册 **0 个扩展点**。

## A13 skills（3 skill）

| skill | 职责 |
|---|---|
| coolzhu-goal-session-chain | 锚定 workspace/chat room/session/artifact 路径与 phase 交接，禁止静默换房间或推断相对路径，重试时先修上次校验指出的缺失文件 |
| coolzhu-goal-tool-execution | 工具 schema 顶层字段纪律、Windows/PowerShell 命令规范、临时脚本放 `tmp/`、输出重定向到 `tmp/logs/`、命令必须显式超时 |
| coolzhu-goal-model-reasoning | scope 收敛（最小实现、禁改无关模块）、上下文接近阈值时保留决策/路径/错误、重试前先诊断、三次等价失败即停并报阻断 |

注入方式：**作为 baseline 无条件注入同一段 phase prompt**（`main.rs:44400-44416`），不通过 skill 工具按需加载。

## A14 其他支撑面

| 路径 | 职责 | 完成度 |
|---|---|---|
| `tests/module_linkage_smoke.rs` | 唯一根测试，4 断言 | `[完整]` |
| `src/lib.rs` | 5 行纯注释，仅为 tests/ 存在 | `[完整]` |
| `config/package-launcher.json` | launcher 配置（executable/health_url/超时/log_dir/runtime_dir） | `[完整]` |
| `config/package-manifest.json` | 10 artifacts + 12 resources 构建清单 | `[完整]`（`gui-web.stt-models` 源目录不存在，optional 静默跳过） |
| `scripts/package-all.ps1` | `package/` staging + 独立 target 构建 + SHA256 增量 + backup_keep=10 + package-report.json | `[完整]` |
| `scripts/package-safety.ps1` | 隐私扫描（blockedPathPattern + 4 类凭证正则逐行扫） | `[完整]` |
| `scripts/build-msi.ps1` | WiX 5.0.2 MSI 打包 + installer-report.json | `[完整]` |
| `scripts/project-delivery.ps1` | 源码交付 + SHA256 清单 + ZIP 往返校验 | `[部分]`（默认 curation map 不存在时 throw；`SOURCE-MANIFEST.json` 已过期） |
| `scripts/setup-browser-bridge.ps1` | 写 native messaging 注册表项 | `[完整]` |
| 7 个 `scripts/test-clawbot-*.mjs` | mock ilink provider + 本地端口 | `[完整]` |
| `scripts/test-package-safety/manifest/project-delivery/powershell-script-compat` | 脚本级测试 | `[完整]` |
| `SOURCE-MANIFEST.json` | 983 文件/136.8MB 清单 | `[部分]` 2026-07-28 生成，与源码不同步 |
| `SOURCE-EXCLUSIONS.md` | 排除说明（固定 4 段文案） | `[完整]` |
| `.coolzhu/` | 仓库内**仅 `plugins/`**（`.gitignore` 为 `.coolzhu/*` 全忽略 + plugins 白名单） | `[完整]` |
| 运行时代码写入 | `web-sessions.sqlite3`、legacy `web-sessions.json`、`attachments/`、`tool-audit.jsonl`、`computer-use-audit.jsonl`、`ide-index/`、`logs/local-gemma.log`、`self-update/staging`(仅 plan 引用) | `[完整]`（sqlite 与 legacy json 双路径并存） |
| 用户级状态 | `%USERPROFILE%/.coolzhu/logs/<app>.jsonl`；`%TEMP%/coolzhu-session-lock.log` | `[完整]` |
| 模型权重 | **不在** `.coolzhu`；本地基座目录 `~/llama.cpp`；`modules/vision/resources/` 仅脚本/模板 | `[完整]` |
| 环境变量覆写 | `COOLZHU_WEB_SESSION_DB`、`COOLZHU_WEB_SESSION_STORE`、`COOLZHU_LOG_DIR/LEVEL/CONSOLE`、`COOLZHU_CLAWBOT_PROVIDER_KIND/URL`、`CLAW_MOUSE_BACKEND`、`CLAW_LOG_DIR`、`CLAW_CODE_UPSTREAM` | `[完整]`（与 `development-standard.md` 的 config-first 约定并存） |
| `AGENTS.md` / `CLAUDE.md` | 内容相同仅首行对象不同（**无 symlink**） | `[完整]` |
| `AGENT.md` | Agent Tool Calling Guide（`antml:` 协议），属使用约定 | `[完整]` |
| `docs/development-standard.md` | 12 节开发规范 | `[完整]` |
| `docs/interface-contracts.md` + 9 个模块 `INTERFACE.md` | 跨模块接口契约 | `[完整]` |
| `.github/` | **仅 ISSUE_TEMPLATE，无 CI workflow** | `[部分]` |
| `tmp/2026-09-19-agent-fixes/pr-checkout/` | 整仓副本（污染 find 结果，建议清理） | — |

### A 侧调试/验证入口清单（模型可见工具 20 个）
bash、read_file、write_file、edit_file、glob_search、grep_search、WebFetch、WebSearch、TodoWrite、Skill、Agent、ToolSearch、NotebookEdit、Sleep、SendUserMessage、Config、StructuredOutput、REPL、PowerShell（+ 定义在 `tool-registry/src/lib.rs:281-598`）

### A 侧 HTTP 路由面（232 条，主要分组）
`/api/chat/*`、`/api/tools/*`（pending/approve/reject/events/runtime-execute）、`/api/goals/*`、`/api/sessions/*`、`/api/plugins/*`、`/api/channels/clawbot/*`、`/api/computer-use/browser/*`、`/api/vision/*`、`/api/realtime/session/*`、`/api/pet/*`、`/api/diagnostics/*`、`/api/web/cards`、`/api/file*`、`/api/config/*`、`/api/system/*`

### A 侧 SQLite 表清单
`sessions`、`chat_rooms`、`session_messages`、`chat_room_messages`、`chat_room_permissions`、`runtime_runs`、`runtime_run_events`、`goals`、`goal_phases`、`goal_role_configs`、`chat_handoffs`、`goal_events`、`memory_beads`、`memory_vectors`、`memory_access`、`memory_meta`、`memory_edges`、`memory_settings`、`memory_jobs`、`attachment_refs`、`chat_usage_events`、`chat_message_timing`、`clawbot_login_snapshot`、`clawbot_inbox`、`clawbot_outbox`、`clawbot_group_member_grants`、`clawbot_contacts`、`clawbot_operation_administrators`、`clawbot_groups`、`clawbot_group_audit`、`clawbot_denial_notices`

---

# B 侧 dsh 全量清单（291 包）

> 每包一行：包名 — 职责。带标记者为已知非完整面。

## acp（1）
- `acp` — automation-only ACP v1 stdio server。`[完整]`

## api（7）
- `api-gateway` — 双侧 Typert RPC 端点；Host `ctx.typertGateway`、Client `ctx.remote`；`/api/remote.mux` WebSocket。`[完整]`（SRC mode 为开发回退 `[部分]`）
- `api-remotes` — 应用级 Remote 装配单点；Host 注册转发事件源，Client 逐个 `$mount()`。`[完整]`
- `api-session-controller` — Session 生命周期/历史/模型目录/skill/文件引用；cold reads + live control transport。`[完整]`
- `api-settings-controller` — settings/credentials 脱敏读写 + native 打开。`[完整]`
- `api-terminal-controller` — Session 工作区交互终端（10 个 Remote，含 bounded screen recovery）。`[完整]`
- `api-workspace-controller` — Workspace 导航变更 + 完整投影 follow；拥有 `ctx.directoryPickerController`。`[完整]`
- `api-workspace-files` — Web 文件预览（7 个 Remote：stat/read/readBytes/readAll/readRelated/list/changes）。`[完整]`

## attachment（2）
- `attachment` — 不可变附件存储 seam。`[完整]`
- `attachment-local` — 内容寻址 `sha256:<64hex>`、hard-link 发布、去重、只读、编码阶梯、请求变体缓存。`[完整]`

## boot（4）
- `app-boot` — 共享启动（`.env` 分层、fail-loud Loader、`sanitizeProfile()`、`--dump-config`）。`[完整]`
- `cmdline` — 不可变 `ctx.cmdlineArgs` 快照、`ctx.appExit`、`ctx.appReady`、`exitOnStdinEnd`。`[完整]`
- `hmr` — 协调模块与 profile 配置热重载；`runExclusive()` 单队列。`[完整]`
- `plugin-manager` — 当前 profile 的插件/bundle 管理（install spec、pnpm 调用、回滚）。`[完整]`

## browser-use（1）
- `browser-use` — exclusive named provider 注册服务（`ctx.browserUse`）。`[完整]`（**base 包无实际 provider**）

## bundle（6）
- `base` — 512 行 patch，base-backed 四 profile 共享。`[完整]`
- `web-app` — 516 行 patch，挂 Web rows。`[完整]`
- `headless` — 34 行，`headless-runner`。`[完整]`
- `sdk-app` — 25 行，SDK JSON-RPC server。`[完整]`
- `sdk-minimal` — 158 行，**不继承 base**，自含完整树。`[完整]`
- `acp-app` — 24 行，ACP + coding-agent persona。`[完整]`

## client（55）
- `client-connection` — 认证 RPC 传输与 generation 生命周期。`[完整]`
- `client-file-upload` — Agent 作用域浏览器文件上传、流式接收、暂存收据。`[完整]`
- `client-hmr` — Web client graph 同步与重建 bundle 重载。`[完整]`
- `client-locale` — Host-backed 偏好 + 可扩展语言目录。`[完整]`
- `client-modules` — Client 模块系统双面；Host 合成 `__DSH_BOOT__`。`[完整]`
- `client-resources` — 统一 client 资源模型（`dsh-resource://` + `useResource`）。`[完整]`
- `client-store` — React-free observable/snapshot 原语。`[完整]`
- `client-ui-agent-preset` — Agent preset 界面。`[完整]`
- `client-ui-approval` — 审批 composer takeover。`[完整]`
- `client-ui-attachment` — 附件呈现。`[完整]`
- `client-ui-brand-official` — 官方品牌占位。`[完整]`
- `client-ui-chat` — Chat Conversation target、node 定义、渲染器、详情。`[完整]`
- `client-ui-commands` — 命令面（全局目录缓存、`/` 源、三类命令）。`[完整]`
- `client-ui-conversation` — Target-neutral Conversation 组装、shell、composer、queue、view。`[完整]`
- `client-ui-deliverables` — changed-files card + 逐文件对比 tab + 交付 card。`[完整]`
- `client-ui-directory-picker-browse` — 应用内目录浏览。`[完整]`
- `client-ui-directory-picker-native` — 原生目录选择（renderless）。`[完整]`
- `client-ui-dockkit` — split-tree 布局引擎（可逆操作）。`[完整]`
- `client-ui-goal` — GoalBar。`[完整]`
- `client-ui-input-trigger` — `/` 与 `@` 检测、候选菜单、pick 路由。`[完整]`
- `client-ui-jobs` — Session header 后台任务列表。`[完整]`
- `client-ui-layout` — 三列 AppFrame + 拖拽 handle + `ctx.layout`。`[完整]`
- `client-ui-message-feedback` — 逐条 Like/Dislike。`[完整]`
- `client-ui-model-selection` — 模型选择面。`[完整]`
- `client-ui-open-in-app` — Session header "Open In..."。`[完整]`
- `client-ui-permission-presets` — 权限面（新 session 默认 + 当前 session）。`[完整]`
- `client-ui-plan` — plan 模式控件 + transcript plan card + sidebar Markdown。`[完整]`
- `client-ui-plugin-manager` — 侧栏 Plugins 面板。`[完整]`
- `client-ui-primitives` — 纯 React atoms（控件/图标/markdown/JS 工具）。`[完整]`
- `client-ui-reference` — 统一 `@file` 与 `@session` 引用源。`[完整]`
- `client-ui-renderer` — React slot 绑定、`ctx.uiRenderer`。**Host 侧 `apply()` 空实现** `[stub]`
- `client-ui-schedule` — 只读 Schedule 目录（Session header）。`[完整]`
- `client-ui-session` — Session Controller 的 React 适配 + session 作用域 slot。`[完整]`
- `client-ui-settings` — Settings domain 基础。`[完整]`
- `client-ui-settings-general` — General 段 + 产品引导。`[完整]`
- `client-ui-settings-models` — Models 设置 + 共享引导对话框。`[完整]`
- `client-ui-settings-plugin-inventory` — 只读 Cordis Loader inventory tab。`[完整]`
- `client-ui-settings-plugins` — Plugins 设置段（feature-owned tabs）。`[完整]`
- `client-ui-settings-unarchive-sessions` — 归档 session 设置页。`[完整]`
- `client-ui-sidebar` — session 多级树、搜索、分组、状态点。`[完整]`
- `client-ui-sidebar-browser` — 沙箱化 Web 浏览器 tab。`[完整]`
- `client-ui-sidebar-documentpreview` — Office/Markdown/高亮文档预览。`[完整]`
- `client-ui-sidebar-files` — Workspace 文件树 tab。`[完整]`
- `client-ui-sidebar-right` — 右栏 docking 容器与会话绑定状态。`[完整]`
- `client-ui-sidebar-terminal` — 交互 shell tab。`[完整]`
- `client-ui-skill` — skill 引用 + 专用 skill 工具行。`[完整]`
- `client-ui-slots` — Slot 注册表纯核心（typed Slot + Component Factory + `SlotCore`）。`[完整]`
- `client-ui-subagent` — subagent 会话目录、续接路由 UI、`@` 引用。`[完整]`
- `client-ui-theme` — 主题插件（pre-plugin 调色板 bootstrap + DOM-free Theme）。`[完整]`
- `client-ui-tool` — Tool 调用树渲染器 + keyed per-tool 呈现槽。`[完整]`
- `client-ui-trajectory` — Trajectory 事件 ledger + 交互时间轴（纯消费投影）。`[完整]`
- `client-ui-user-questions` — ask_user_question composer takeover + plan-review 呈现。`[完整]`
- `client-ui-workflow-run` — durable workflow-run Conversation Node + 嵌套成员披露。`[完整]`
- `client-ui-workspace` — Workspace picker。`[完整]`
- `client-web` — Web boot kernel（静态模块表 + Cordis loader + framework-free boot 页）。`[完整]`

## compaction（5）
- `command-compact` — `/compact` slash 命令；不耗模型 turn。`[完整]`
- `compaction` — 抽象 compaction seam（brand/checkpoint/tool-pairing）。`[完整]`
- `compaction-basic` — token-meter 驱动的压缩策略 + LLM 摘要后端。`[完整]`
- `compaction-image-offload` — 图像路由的 durable 图片卸载（**永久**替换为文本）。`[完整]`
- `compaction-tool-result-pruner` — replay-safe 零模型调用 head/middle/tail 剪枝。`[完整]`

## computer-use（1）
- `computer-use` — exclusive named provider 注册服务（`ctx.computerUse`）。`[完整]`（**base 包无实际 provider**）

## context（6）
- `agent-instructions` — AGENTS.md/CLAUDE.md 作为 durable user/message 注入。`[完整]`
- `file-reference` — 文件引用发现契约与共享 `@file` 语法（零 FS 访问）。`[完整]`
- `file-reference-local` — 本地 FS provider（有界模糊索引）。`[完整]`
- `session-reference` — 跨会话快照引用与 durable untrusted 模型上下文。`[完整]`
- `time-context` — opt-in 每步时间与 elapsed 上下文。`[完整]`
- `tmux-context` — opt-in 每步 tmux pane/window 上下文。`[完整]`

## core（8）
- `agent` — Agent 接口、注册表、initiator 作用域、事件词汇。`[完整]`
- `agent-default-model` — 默认模型选择。`[完整]`
- `agent-loop` — 具体 agent loop 插件。`[完整]`
- `agent-tool-presentation` — Agent 面呈现选择器（把工具组合为 PTC 标记）。`[完整]`
- `scope` — 作用域上下文注册原语（scope tag、scope 过滤事件）。`[完整]`
- `session` — 事件溯源 session store（19 个 session 族包之一，实体在此）。`[完整]`
- `system-prompt` — system prompt 组装注册表。`[完整]`
- `tools` — 工具注册表与执行流水线。`[完整]`

## credentials（3）
- `authorization` — 插件拥有的授权流 seam。`[完整]`（**无 shipped flow**）
- `credentials` — 抽象凭据 seam（settings 只带引用）。`[完整]`
- `credentials-local` — `$DSH_HOME/.credentials.yaml`（POSIX 强制 0600）。`[完整]`（Windows 跳过权限检查）

## deliverables（2）
- `tool-present` — 显式 workspace 文件交付声明（工具 `present`，`maxFiles=8`）。`[完整]`
- `workspace-changes` — git 快照驱动的每 turn 文件变更 → `workspace/changes`。`[完整]`

## document（1）
- `office-to-pdf` — LibreOffice 转换（18 Config 字段、队列、内容寻址缓存、gen 失效）。`[完整]`

## experimental（17）
- `experimental-agent-team` — Agent Teams roster + durable mailbox + task board。`[实验性]`
- `experimental-agent-team-profile` — Teams profile bundle（**禁用**普通 subagent 委派）。`[实验性]`
- `experimental-agent-team-web-profile` — Teams Web 层。`[实验性]`
- `experimental-auto-review` — per-tool LLM 授权审查（Auto 权限）。`[实验性]`
- `experimental-browser-use-chrome-devtools-mcp` — Chromium 工具（chrome-devtools MCP）。`[实验性]`
- `experimental-browser-use-playwright-mcp` — Chromium 工具（Playwright MCP）。`[实验性]`
- `experimental-browser-use-runtime` — Session 级浏览器资源生命周期与 MCP 集成（库，无 ctx key）。`[实验性]`
- `experimental-browser-use-stagehand-native` — Stagehand 原生工具（`stagehand_<method>`）。`[实验性]`
- `experimental-client-ui-agent-team` — Teams roster/board/teammate 导航。`[实验性]`
- `experimental-computer-use-cua-driver-mcp` — Cua Driver MCP（沿用其原工具名）。`[实验性]`
- `experimental-computer-use-cua-driver-native` — Cua Driver 原生嵌入。`[实验性]`
- `experimental-inspector` — 跨 realm CDP hub（Host 调试 + Client Runtime）。`[实验性]`
- `experimental-ptc-runtime-python` — CPython 子进程 PTC（fd3 JSON-lines；**无文件沙箱**）。`[实验性]`
- `experimental-tool-agent-team` — 9 个 Teams 工具 + `team:policy` 提示段。`[实验性]`
- `experimental-webworker-packer` — 构建期 packer（浏览器 runtime 的 base VFS + 有序模块）。`[实验性]`
- `experimental-webworker-runtime` — 浏览器内 harness runtime（内存 VFS、模块 transform/load）。`[实验性]`
- （`/experimental/README.md` 记录其中 5 个 browser/computer provider **无 shipped profile 接线**）

## extensions（4）
- `cordis-host-runner` — 动态包定义注册表 + host 半边 `node:vm` 沙箱生命周期（`vmTimeoutMs=5000`；自述**非 containment**）。`[完整]`
- `cordis-client-runner` — 动态双半包插件包的浏览器半边。`[完整]`
- `tool-cordis` — 只读运行时 API 检查（`cordis_inspect_list`/`cordis_inspect_query`）。`[完整]`
- `client-ui-cordis` — Cordis 动态插件定义 card。`[完整]`

## feedback（2）
- `command-feedback` — log-only session 反馈（`/feedback` + `sessionFeedback`）。**永不进模型上下文或派生历史**。`[完整]`
- `message-feedback` — 逐条 assistant message 评分。**绝不进模型历史或 telemetry**。`[完整]`

## fs（7）
- `fs` — 文件系统 seam（方法集 + `fs/write-intent`/`fs/edit-intent`/`fs/observed`）。`[完整]`
- `fs-local` — 本地实现。`[完整]`
- `fs-observation-policy` — observed-state 新鲜度（read-before-edit no-clobber）。`[完整]`
- `fs-sandbox` — sandbox 强制实现。`[完整]`
- `tool-fs` — `read`/`read_image`/`write`/`edit`。`[完整]`
- `tool-fs-search` — `glob`/`grep`（打包 ripgrep）。`[完整]`
- `tool-str-replace-editor` — `view`/`create`/`str_replace`/`insert` 兼容工具。`[完整]`

## goal（4）
- `command-goal` — `/goal` 系列 UI 命令（结果不入模型请求）。`[完整]`
- `goal` — 事件溯源同 session goal 状态与生命周期。`[完整]`
- `goal-round-driver` — race-fenced 同 session 续轮驱动。`[完整]`
- `tool-goal` — `get_goal`/`create_goal`/`update_goal`（**执行期授权**）。`[完整]`

## guard（2）
- `repeat-tool-reminder` — 重复工具调用 advisory 提醒（阈值 `[3,5,8]`，不 veto）。`[完整]`
- `tool-call-timeout-policy` — per-tool 超时包裹（`TOOL_TIMEOUT`）。`[完整]`

## hooks（3）
- `hook-protocol` — 共享 Claude Code/Codex hook wire 协议（matcher 引擎、stdin/stdout 编解码）。`[完整]`
- `hooks-claude-code` — Claude Code hooks.json 兼容桥（7 事件；**30 个中 23 个不支持**）。`[完整]`
- `hooks-codex` — Codex hooks.json 兼容桥（5 事件；`stop_hook_active` 恒 false + TODO）。`[部分]`

## host（8）
- `host-directory-picker` — 目录选择 seam（`capability()` 判别联合）。`[完整]`
- `host-directory-picker-auto` — 自适应选择 native/browse（boot 期纯采样 + 挂真实 Loader entry）。`[完整]`
- `host-directory-picker-browse` — 应用内浏览后端。`[完整]`
- `host-directory-picker-native` — 原生 OS 选择器（osascript/Zenity/KDialog/`IFileOpenDialog`）。`[完整]`
- `host-frontend-static` — SPA dist 服务（**故意不做 SPA fallback**；index 需授权）。`[完整]`
- `host-open-in-app` — 打开外部应用（白名单 catalog + 三路由 + 三个 deadline 配置）。`[完整]`
- `host-plugin-inventory` — 只读 Remote 投影（实时读 `ctx.loader.entries()`）。`[完整]`
- `host-webserver` — HTTP/upgrade 路由注册 + index 注入 + fallback seat。`[完整]`

## identity（1）
- `anonymous-user-id` — 每 harness home 一个匿名 id（`.anonymous-user-id`）。`[完整]`

## interaction（5）
- `commands` — 插件拥有的人类命令注册表。`[完整]`
- `permission-presets` — 用户可见权限预设（默认表 2 项）。`[完整]`
- `tool-ask-user` — `ask_user_question`（1..n 问）。`[完整]`
- `user-approval` — 审批 seam（`ask|never`，fail-closed，allowed-once）。`[完整]`
- `user-questions` — 自由问答 seam。`[完整]`

## jobs（3）
- `jobs` — 后台 job 注册表（owner session 授权栅栏、终态通知）。`[完整]`
- `jobs-local` — 进程内实现（`maxConcurrentJobsPerOwner=10`，**不跨重启**）。`[完整]`
- `tool-jobs` — `job_output`/`job_list`/`job_kill`。`[完整]`

## llm（7）
- `deepseek-llm-api-extensions` — additive request-field 注册表。`[完整]`
- `llm` — provider-neutral LLM 服务接口（`LlmAdapter` + `registerAdapter`）。`[完整]`
- `llm-deepseek` — DeepSeek 适配器（Chat Completions + Messages 协议）。`[完整]`
- `llm-pi-ai` — pi-ai-backed 适配器（3 种 wire protocol + 手写 YAML 路由）。`[完整]`
- `llm-retry` — provider 路由重试策略。`[完整]`
- `plugin-package-inventory-deepseek` — 官方 DeepSeek LLM API 的 Loader-backed 插件包 inventory。`[完整]`
- `token-meter` — replay-aware token 计量（明示非计费）。`[完整]`

## lsp（3）
- `lsp` — LSP seam（仅 4 操作）。`[完整]`
- `lsp-stdio` — 通用 stdio 语言服务器 provider。`[完整]`
- `tool-lsp` — `lsp` 工具。`[完整]`

## mcp（2）
- `mcp-client` — MCP 客户端桥（一实例一 server，`mcp__<s>__<t>`）。`[完整]`
- `mcp-resources` — MCP 资源发现与读取（3 工具）。`[完整]`

## plan（1）
- `plan-mode` — logged per-agent plan 模式（**与 sandbox/approval 独立**，`exit_plan_mode`）。`[完整]`

## preset（2）
- `agent-presets` — per-session agent 组合（`agent.cordis.yml`；三来源合并）。`[完整]`（superseded generation 回收 TODO）
- `persona` — 组合作者声明的 deployment persona section。`[完整]`

## ptc-runtime（2）
- `ptc-runtime` — PTC 执行 seam。`[完整]`
- `ptc-runtime-node` — 沙箱化 Node 进程实现。`[完整]`

## runtime-diagnostics（1）
- `invariants` — 包自检不变量注册表（allowlist/blocklist 正则；失败归属违规包）。`[完整]`

## sandbox（4）
- `sandbox` — 进程沙箱 seam + escalation `WIDER_MODES`。`[完整]`
- `sandbox-local` — 本地后端（bwrap/Landlock/Seatbelt/ACL token；fail-closed）。`[完整]`
- `sandbox-policy` — per-call policy 解析器与当前模型上下文。`[完整]`
- `sandbox-windows-acl` — Windows ACL 写限制后端（restricted-token spawn，10 文件）。`[完整]`

## schedule（1）
- `schedule` — agent 作用域 durable 提醒（`after`/`at`/`every`，**无 cron/日历**，不发外部通知）。`[完整]`

## sdk（3）
- `sdk-client` — TS 客户端 SDK（进程生命周期归 client）。`[完整]`（**无 wire cancel / per-prompt result**）
- `sdk-protocol` — 共享 wire 协议（3 请求 + 4 通知）。`[完整]`（server→client request `[未接线]`）
- `sdk-jsonrpc-server` — stdio JSON-RPC server 插件。`[完整]`

## session（19）
- `session-checkpoint-policy` — 语义边界耐久 checkpoint（`llm/stream`/`tools/execute` 顶层/`agent/pre-step`）。`[完整]`
- `session-format` — 流式相邻迁移机制（强强制 `to = from + 1`）。`[完整]`（重映射 O(event count)）
- `session-format-catalog` — build-static 首方 codec 与相邻边装配（**运行期插件无法补缺边**）。`[完整]`
- `session-format-v0-to-v1` — 冻结 v0 codec + identity 迁移。`[完整]`
- `session-format-v1-to-v2` — 冻结 v1 codec + assistant-stream 迁移。`[完整]`
- `session-format-v2-to-v3` — system-prompt/canonical-envelope/PTC 迁移入 V3。`[完整]`
- `session-log-deepseek` — 增量无损 session-log 请求扩展。`[部分]`（两处 TODO；`Config.enabled` 默认 true）
- `session-persistence` — 持久化 seam（5 方法 + handle 契约 + 7 个错误类）。`[完整]`（**无删除/保留 API**）
- `session-persistence-jsonl` — JSONL 后端（zstd 帧、fsync、原子 publish、flock 租约、torn tail）。`[完整]`（Windows 无 lock 文件）
- `session-projection` — 投影 seam（merge-extensible 类型表 + drive + checkpoint/restore）。`[完整]`
- `session-projection-cache` — 持久化投影 checkpoint（3 强制写点 + 节流，fail-soft）。`[完整]`
- `session-stats` — `sessionStats` 投影（turns/steps/llmMs/toolMs/ttftMs/decodeMs/decodeTokens）。`[完整]`
- `session-telemetry` — telemetry seam（`ledger`/`ops` 通道，redaction waterfall，sharing 三态）。`[完整]`（`flush` 刻意未实现）
- `session-telemetry-otel` — OTel 后端（仅 FEEDBACK_ONLY/DISABLED）。`[部分]`（**无 full 模式实现**）
- `session-title` — log-backed 标题 + 确定性 fallback + 唯一 provider。`[完整]`
- `session-title-all-prompts-llm` — 全部 eligible 消息生成标题。`[完整]`
- `session-title-first-prompt-llm` — 首条消息生成标题。`[完整]`
- `session-title-llm` — 共享 LLM 标题生成策略（库）。`[完整]`
- `session-turn-outline` — `turnOutline` 投影（turns + draft）。`[完整]`

## session-query（4）
- `session-log-export` — `/export` 命令 + `/api/session.export` 流式 ZIP。`[完整]`
- `session-query` — 组合查询服务契约（精确读、trace、过滤）。`[完整]`
- `session-query-sqlite` — SQLite FTS5 后端（schema v8）。`[完整]`（`openAt:'never'` 时搜索禁用）
- `tool-session-query` — 模型面 session 历史检索/追踪/读取（5 工具 + workspace 授权）。`[完整]`

## settings（2）
- `settings` — 抽象用户设置 seam + 结构化 redaction。`[完整]`（union/transform secret 缺口 TODO）
- `settings-file` — `settings.yaml` provider（保注释 leaf diff、跨进程写锁）。`[完整]`

## shell（10）
- `bash-local` — 本地 subprocess bash 执行器。`[完整]`（`XXX(stateful-shell)` TODO）
- `bash-sandbox` — sandbox 消费型 bash 执行器（fail-closed `SANDBOX_UNAVAILABLE`）。`[完整]`
- `pwsh-local` — 本地 PowerShell 实现。`[完整]`
- `pwsh-sandbox` — sandbox 消费型 PowerShell 实现。`[完整]`
- `shell` — bash executor seam（`resolve(request): Spec`）。`[完整]`
- `shell-env` — 工具无关的托管 `DSH_*` 环境变量注册表。`[部分]`（`list()` 不含内建 TODO）
- `tool-bash` — 模型面 `bash` 工具（可选 background + sandbox 参数）。`[完整]`（部署策略归属 TODO）
- `tool-bash-persistent` — owner 作用域持久 Bash（PTY）。`[完整]`
- `tool-pwsh` — 模型面 `pwsh` 工具。`[完整]`
- `tool-pwsh-persistent` — owner 作用域持久 PowerShell。`[完整]`

## skill（5）
- `skill` — skill provider 注册表（rank 越小越优先）。`[完整]`
- `skill-badge` — 随包 "powered by dsh" 徽章 skill。`[完整]`
- `skill-filesystem` — 本地 FS provider（SKILL.md + frontmatter 校验 + legacy key 拒绝）。`[完整]`
- `skill-office` — Word/PowerPoint/Excel 工作流与结构检查。`[完整]`
- `tool-skill` — 模型面 skill 加载工具。`[完整]`

## spill（3）
- `spill` — spill 存储 seam（`saveText` 只存全文 + locator）。`[完整]`
- `spill-local` — 本地实现（`<tmp>/dsh-spill-*/session-<hash>/`）。`[完整]`
- `spill-policy` — 工具结果 spill 策略变换器（需配 `maxInlineBytes` 才生效）。`[完整]`

## ssh（4）
- `ssh` — 共享 OpenSSH 连接 + versioned POSIX remote helper（`SSH_PROTOCOL_VERSION=1`、`helperHash` 校验）。`[完整]`
- `fs-ssh` — 经共享 helper 的文件系统 provider。`[完整]`
- `sandbox-ssh` — 远端 POSIX 沙箱 argv provider。`[完整]`
- `subprocess-ssh` — 经共享 helper 的 subprocess 与 terminal provider。`[完整]`

## storage（4）
- `storage` — storage hub（backend 注册表 + data form 挂载）。`[完整]`
- `storage-domain` — schema 校验、change 发射的 KV domain（先耐久、再内存、后 emit）。`[完整]`
- `storage-json` — JSON 文件 KV 后端（single / per-record 两布局）。`[完整]`
- `storage-sqlite` — SQLite KV 后端（schema v1、`open(path,'wx',0o600)`）。`[完整]`

## subagent（10）
- `subagent` — 委派 seam（命名 provider 注册表；`maxDepth` 默认 1、`maxActiveSubagents` 默认 8）。`[完整]`
- `subagent-acp` — 进程外 ACP 子 agent 后端。`[完整]`
- `subagent-claude-code` — Claude Code one-shot provider（官方 Agent SDK）。`[完整]`
- `subagent-codex` — Codex one-shot provider（app-server 协议）。`[完整]`
- `subagent-dsh-sdk` — 进程外 SDK 子 agent 后端（完整 Harness 子进程）。`[完整]`
- `subagent-fork-in-process` — fork 后端（**父已完成 turn 作一次性 seed**）。`[完整]`
- `subagent-in-process-driver` — 共享进程内子 agent 运行驱动。`[完整]`
- `subagent-spawn-in-process` — spawn 后端（无 seed）。`[完整]`
- `tool-subagent` — 模型面委派工具（one-shot/continuable）。`[完整]`
- `tool-subagent-control` — `send_message`/`interrupt_agent`/`list_agents`。`[完整]`

## subprocess（3）
- `subprocess` — 子进程 seam（`resolveExecutable`/`spawn`/`spawnTerminal`/`terminalEnvironment`）。`[完整]`
- `subprocess-local` — 本地实现（Linux systemd scope / Windows Job / node-pty）。`[完整]`
- `win32-process` — 共享 Win32 进程/stdio/Job Object 原语（koffi FFI）。`[完整]`

## terminal（3）
- `terminal` — 持久 PTY seam（owner=Agent 精确围栏，6 个错误码）。`[完整]`
- `terminal-bash` — bash/pwsh PTY 后端（`@xterm/headless` + 私有 marker 判 ready）。`[完整]`（三处 TODO）
- `tool-terminal` — 6 个模型面 PTY 工具。`[完整]`

## test-support（7）
- `agent-loop-testkit` — 生产 AgentLoop driver + 可替换 Inbox。`[完整]`
- `client-test-runtime` — jsdom slot bench + 真实 web bundle roster + endpoint 级 Remote mock。`[完整]`
- `llm-mock-server` — 可编排 OpenAI 兼容 HTTP/SSE 故障服务器。`[完整]`
- `llm-replay` — 无 key 重放插件（从录制 session JSONL 重建 model chunks）。`[完整]`
- `loader-smoke` — 真实 bin + `cordis.yml` 的无 key Loader 执行 harness。`[完整]`
- `remote-mock` — endpoint 命名 Remote mock（unary + stream）。`[完整]`
- `session-snapshot` — session-log snapshot 核心 + **ACP 协议 adapter** + 四协议支持。`[完整]`

## todo（1）
- `tool-todo` — `todo_write`（整表替换、session 所有）。`[完整]`

## typert（4）
- `typert-generator` — TS 项目分析器 + 模型驱动 Typert 产物生成。`[部分]`（通配 export、跨 face namespace re-export、泛型/computed Zod 根、跨 face schema 运行期 import 未支持）
- `typert-loader` — 生成 Typert 包贡献的 Loader 集成。`[部分]`（仅发现 host `./typert`）
- `typert-protocol` — 编译器无关 Remote 元数据与 Typert provider 协议。`[完整]`
- `typert-registry` — 生成包反射与 Zod schema 的运行时注册表。`[完整]`

## util（16）
- `atomic-write` — 原子文件替换（独占创建随机后缀 + `withFileLock`）。`[完整]`
- `brand` — 无状态 branded 原始类型。`[完整]`
- `chunked-list` — 持久 append-only 分块列表（有界复制 + JSON 校验）。`[完整]`
- `util-crypto` — 零依赖浏览器安全 UUID 与字节编码。`[完整]`
- `deque` — 零依赖循环 deque。`[完整]`
- `home-paths` — `DSH_HOME` 与共享路径助手。`[完整]`
- `http-proxy` — 进程级出站 HTTP 代理策略（loopback 直连）。`[完整]`
- `launch-environment` — 不可变启动环境（记录层来源；SSH 启动不采纳 .env）。`[完整]`
- `lazy-require` — 调用方相对、成功缓存的懒加载。`[完整]`
- `native-command` — 宿主命令与路径打开（**shell-free 执行**）。`[完整]`
- `output-retention` — 有界保留原语（`ItemRetainer`/`TextRetainer`）。`[完整]`
- `package-manifest` — `package.json.dsh` 配置字段类型声明。`[完整]`
- `time` — wire 边界共享时间词汇（`canonicalClientTimeZone`）。`[完整]`
- `timeout` — 超时/截止原语（`clampTimeout`/`deadline`/`idleWatchdog`）。`[完整]`
- `util-values` — 重复安装安全的值原语（`snapshotJsonValue`/`deepFreeze`/`deepEqualJson`）。`[完整]`
- `util-workspace-path` — 浏览器安全 Workspace 路径与显示助手。`[完整]`

## web（6）
- `tool-web` — `web_search`/`web_fetch`。`[完整]`
- `web` — 抽象 web 访问 seam（含 `WEB_PROVIDER_AMBIGUOUS`）。`[完整]`
- `web-fetch-http` — 匿名公共 HTTP(S) 抓取 provider。`[完整]`
- `web-search-deepseek` — DeepSeek 后端搜索。`[完整]`
- `web-search-exa` — Exa 后端搜索。`[完整]`
- `web-search-perplexity` — Perplexity 后端搜索。`[完整]`

## webhook（2）
- `webhook` — fire-and-forget webhook 规则运行时（**无队列/重试/去重/完成状态**）。`[完整]`
- `webhook-github` — 签名 GitHub HTTP adapter（HMAC 先校验后 parse；返回 202）。`[完整]`

## workflow（4）
- `tool-ralph` — 模型面 fresh-agent Ralph 循环（`maxRounds` 默认兼上限 256）。`[完整]`
- `tool-workflow` — 模型面 workflow 工具（`meta`/`script`/`args`；**阻塞父 turn**）。`[完整]`
- `workflow` — workflow capability seam（`agent`/`parallel`/`pipeline`/`phase`/`log`）。`[完整]`
- `workflow-ptc` — 共享沙箱 Node PTC runtime 中的 workflow 编排。`[完整]`

## workspace（1）
- `workspace` — Workspace 实体注册表（命名有序、`pendingMutation` 两写可恢复）。`[完整]`（`create(path,title?)` 的 title 失去调用者）

---

## B 侧非 workspace 目录

| 路径 | 内容 |
|---|---|
| `apps/cli` | `dsh` CLI：`parseDshArgs` → profile/`plugin`/dump-config；`src/profile-boot.ts` 被导出供 Desktop 复用。`[完整]` |
| `apps/web` | Vite 构建入口（`new AppWebEntry(el).run()`）；Desktop 分支等 `dshDesktopBoot.ready()`。`[完整]` |
| `apps/desktop` | Electron 壳（独占 `$DSH_HOME/profiles/desktop`、bundled Python/Node/pnpm、Office skills 默认注册）。`[完整]`（无独立 README） |
| `apps/desktop-host` | `@deepseek-ai/dsh-desktop-host`（private）：`loadProfileDirectory` + `runProfile({profile:'desktop', resolutionMode, args})`。`[完整]` |
| `python/sdk` | Python SDK（`api/client/models/errors.py`，含 `next_request/respond`）；协议**镜像而非 import** TS 类型。`[完整]` |
| `python/sdk-runtime` | 平台 wheel（`dsh` console command + `deepseek_harness_runtime`；要求非空 `DSH_HOME`）。`[完整]`（`runtime/node/` dev-only） |
| `native/system` | `@deepseek-ai/node-addon-system`：`landlock-run`（launcherPath/probe/grantArgs）+ `flock`（`tryLockExclusive`）；平台包 darwin/linux × x64/arm64；**安装不编译原生代码**。`[完整]` |
| `benchmarks/` | 7 场景（`agent-continuation`/`conversation-fold`/`session-open`/`long-session-browser`/`active-stream-reconnect`/`terminal-io`）+ `support/built-worker` + `calibration`。`[完整]` |
| `snapshots/` | 顶层 `session/`（约 40+ 场景）、`acp/`、`sdk/`、`web/`；record/replay 引擎在 `test-support/session-snapshot/src/suite.ts`（mode `replay\|record\|refresh`）。`[完整]` |
| `scripts/` | 聚合门禁 `run-gates.ts`（多模式，含并发与 fail-fast）；~14 个静态 verify 脚本（runtime-closure/cordis-config/client-domain-graph/module-graph/default-product-isolation/application-entrypoints/package-dependencies/dsh-package-licenses/package-invariants/optional-dependency-imports/client-packages/client-ui-i18n/no-bare-dispatcher/node-next-types）；快照/python-sdk 生成器。`[完整]` |
| `docs/` | 543 文件 + `docs/AGENTS.md` 规范；`architecture.md`/`glossary.md`/`defensive-patterns.md`/`testing.md`/`development.md`/`cordis-primer.md`/`rescope.md`/`tool-execution-pipeline.md`/`subsystems/client-modules.md`/`cookbook/`。`[完整]` |
| `website/` | VitePress 文档投影（build 兼作死链检查）。`[完整]` |
| `.agents/notes/` | 决策记录（implemented/ 分类；archived 冻结）。`[完整]` |
| `.agents/skills/` | 12 个 skill（`references/` + `templates/`；**无 `scripts/`**）。`[完整]` |
| `cordis.yml` / `cordis.patch.yml` | 插件与 bundle 组合配置。`[完整]` |
| `pnpm-workspace.yaml` | workspace 定义（vendor/packages/native/apps/website）。`[完整]` |
| `patches/` | pnpm patch 补丁。`[完整]` |
| `vendor/` | 6 个 vendored 包（pinned 源拷贝 + 上游 SHA）。`[完整]` |

### B 侧模型可见工具清单（按来源包汇总）
- shell：`bash`、`pwsh`（+ persistent 变体，按配置出现 `run_in_background`、`sandbox_permissions`+`justification`）
- fs：`read`、`read_image`、`write`、`edit`、`glob`、`grep`、`str_replace_editor`
- terminal：`terminal_open`、`terminal_send`、`terminal_read`、`terminal_signal`、`terminal_close`、`terminal_list`
- web：`web_search`、`web_fetch`
- lsp：`lsp`
- mcp：`mcp__<server>__<tool>` + `list_mcp_resources`、`list_mcp_resource_templates`、`read_mcp_resource`
- subagent：`subagent`（可配名）、`send_message`、`interrupt_agent`、`list_agents`
- jobs：`job_output`、`job_list`、`job_kill`
- schedule：`schedule_create`、`schedule_list`、`schedule_delete`
- workflow：`workflow`、`ralph`
- goal：`get_goal`、`create_goal`、`update_goal`
- todo：`todo_write`
- interaction：`ask_user_question`
- deliverables：`present`
- session-query：`session_search`、`session_event_search`、`session_trace`、`session_event_trace`、`session_event_read`
- skill：`skill`
- extensions：`cordis_inspect_list`、`cordis_inspect_query`、`cordis_define`
- plugin-manager：`plugin_manager`
- agent-team（实验）：`spawn_teammate`、`team_task_create`、`team_task_list`、`team_task_get`、`team_task_update`（+ 与 subagent 控制同名互斥）

### B 侧事件名清单（主要）
`turn/start`、`step/start`、`step/end`、`turn/end`、`agent/pre-step`、`agent/request`、`agent/turn-stopping`、`agent/created`、`llm/stream`、`tool/call`、`tool/result`、`tools/pre-execute`、`tools/execute`、`tools/post-execute`、`session/event`、`session/created`、`session/disposed`、`session/flush`、`session/title`、`session/title-llm-request`、`compaction/start|summary|end`、`approval/asked|decided`、`approval/policy`、`permission/preset`、`plan/mode`、`goal/*`、`todo/write`、`command/*`、`feedback/record`、`feedback/message-put|delete`、`workspace/changes`、`deliverables/presented`、`fs/observed`、`fs/write-intent`、`fs/edit-intent`、`domain/changed`、`session-telemetry/record`、`webserver/index-inject`、`subagent/provider-added|removed`、`workflow/phase`、`hmr/change`、`hmr/reload`、`credentials/reference-updated`、`credentials/record-updated`、`user-questions/request`、`approval/request`、`agent-error`、`session-log-deepseek/delivery-accepted`

### B 侧 SQLite 使用清单（全部为派生或 storage）
`session-query-sqlite`：`search_state`、`persisted_sessions`、FTS5 `persisted_docs`、TEMP `live_sessions`、TEMP `live_docs`（schema v8，版本不符则 `resetDerivedSchema`）
`storage-sqlite`：`unit_globals`、`u_<unit>_<table>`（schema v1）
storage-json（per-record 布局）：`<root>/<name>/` 目录树，如 `session_projcache/sessions/`
