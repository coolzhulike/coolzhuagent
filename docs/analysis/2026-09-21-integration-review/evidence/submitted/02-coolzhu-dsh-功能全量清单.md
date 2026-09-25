# 功能全量清单（核对基线）

> **2026-09-21 审查修订版。** 已保留原始备份，并对影响决策的断言作正文纠正。原有 `[完整]` 仅表示所述代码/契约存在，不代表全部产品验收；本轮为全文阅读与重点源码抽查，未重新运行产品测试。
> 新增：[Computer Use 与 ZCode 差异及 Paint 专项](03-coolzhu-zcode-Computer-Use差异与Paint专项.md)。文末审计表使用修订前原文行号，源码引用以本次本地快照为准。涉及冲突时以本轮证据纠错为准。

**用途**：逐项列出两侧的**全部**模块/包，供核对"是否有遗漏"。完成度标记：`[完整]` `[部分]` `[stub]` `[实验性]` `[未接线]` `[死代码]`。
配套主文档：`01-coolzhu-dsh-架构差异与整合决策.md`

- A 侧 = `C:\Users\zhupu\Desktop\coolzhuagent`：Cargo workspace **28 个成员** = 18 个模块 crate + 9 个插件 crate + 根包；另有 1 个独立的 `tauri-shell` Cargo 项目（不在 workspace 内）、3 个仅 manifest 的插件目录、1 个纯 JS 的 `browser-extension`、3 个 skill
- B 侧 = `C:\Users\zhupu\Desktop\dsh`：**291 个包层成员**（仅 `packages/*/*/package.json`）；`pnpm-workspace.yaml` 另外匹配 22 个成员 manifest（vendor 9、native 6、apps 4、benchmarks/website/python-sdk-runtime 各 1），合计 **313 个 workspace 成员 manifest，不含仓库根包**。后文 B 侧逐包清单的分母仍为 291，不把其它层混入包层计数。

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
- 缺口：新增 provider 需改 ~10 处（`providers/mod.rs`、`openai_compat.rs:83-163`），**无注册 API**。 此处指新增 ProviderKind/适配器实现；**不等于新增模型或兼容连接也要改代码**。`client.rs:39` 的 `from_session_endpoint` 已支持显式协议、Endpoint 与未登记模型，统一会话参数页也已在 0.2.14 发布。

## A3 modules/tooling（4 crate）

### coolzhu-tool-registry `[完整]`
- 职责：工具注册、模型可见性、路径抽取。
- 对外：`mvp_tool_specs()` **19 个静态 `ToolSpec`**（`src/lib.rs:281-598`）；`GlobalToolRegistry::definitions(allowed_tools)`（`:183-205`）；`path_effect::TargetPathsExtractor`。
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

### coolzhu-computer-use-core `[基础执行已接线；端到端部分验证]`
- 职责：鼠标/键盘/坐标/笔画 + 预算熔断 + 审批策略。
- 文件面：`input.rs`(1,209 行，13 个原语)、`input_stroke.rs` + `input_stroke_native.cs`(101 行)、`supervisor.rs`、`controller.rs`(1,501 行)、`contracts.rs`、`lib.rs`(回归资材)、`bin/check.rs`。
- 对外：`ComputerUsePlanner`/`ComputerUseAdapter`/`ComputerUseEventSink`/`ComputerUseClock`/`ComputerUseApprovalPolicy`（`controller.rs:11-90`）；`host_sensitive_semantic_category`（`:92`）；`TurnComputerUseSupervisor`（`supervisor.rs:72`）；`RunBudgetGuard`（`:174`，硬上限 `max_actions=12`/`max_replans=2`/`max_same_signature=2`/`max_no_progress_steps=2`/`timeout_ms=120_000`/`max_calls_per_turn=2`，`contracts.rs:194-203`）。
- 关键机制：**无 Rust 侧 Win32 FFI**，每动作拼 PowerShell + 内联 C# P/Invoke（`input.rs:313,317,1069`）；后端 `CLAW_MOUSE_BACKEND=auto|sendinput|interception`（`:238-267`）。
- 缺口与边界：`move_mouse_absolute`/`mouse_button_down_point` 的调用范围仍需区分底层与正式桥接。0.2.14 已增加 UIA 观察的可恢复 PMv2 线程上下文、绑定窗口原图和物理像素/DPI/身份校验，不能再称“不处理 DPI、截图仅主屏”。混合 DPI、多显示器、负坐标及跨屏仍需专项实测。宿主已有敏感语义检查与输入目标/长度限制（`controller.rs:92,201,541`；`web-console/src/computer_use_desktop_bridge.rs:162`），但不等于完整内容安全方案。Paint 实测证明接口可执行，未证明目标绘图成功。

## A6 modules/gui-web（3 crate）

### coolzhu-web-console `[完整，单文件巨型]`
- 职责：产品主路径。HTTP 路由（232 条）、SSE、会话运行时、权限闸门、SQLite schema 与迁移（v2→v20）、Goal DAG、前端资源内联、桌宠状态、clawbot 网关、vision 路由、IDE 索引、诊断自检。
- 规模：`src/main.rs` **87,025 行 / 3.47 MB**；`src/app.js` 20,416 行 / 765 KB；`index.html` 1,478 行；`styles.css` 17,809 行；`src/model_settings.js`、`src/chat_insights.rs`、`src/chat_experience.js`、`src/realtime_voice_capture.js`、`src/browser_bridge.rs`、`src/wuxia_layout.css`、`assets/bamboo-*.js`。
- 已拆出的生产边界还包括：`src/chat_tool_history.rs`、`src/tool_loop_coordinator.rs`、`src/multimodal_input.rs`、`src/computer_use_planner.rs`、`src/computer_use_executor.rs`、`src/computer_use_adapters.rs`、`src/computer_use_desktop_bridge.rs`、`src/computer_use_store.rs`。分别承载工具历史投影、调用身份、图片路由、专用规划/视觉验收、真实步骤与桌面桥接，必须纳入整合保留面。
- 缺口：`grep -c ConversationRuntime` = **0**；`api_plugins_install` `[stub]`；`/api/diagnostics/stream` `[stub]`；`/api/web/cards` 硬编码清单前端不调用。

### coolzhu-clawbot-sidecar `[完整]`
- 职责：微信通道 provider 适配（HTTP 客户端 + 本地队列轮询）。
- 对外：provider 契约 `/login/refresh`/`/login/logout`/`/updates`(45s 长轮询)/`/send_text`/`/send_file`（`lib.rs:743-818`）；自身 `/health,/version,/tick,/login/refresh,/login/logout`；`GatewayClient`（`:932`）、`spawn_polling_loop`（`:1141`）。
- 缺口：真实微信侧实现不在本仓（provider 端需外部服务）；`MockClawbotProvider` 供测试。

### coolzhu-windows-process-guard `[完整]`
- 职责：封装 web-console 所需的 Windows 句柄与 Job Object 操作。它**不是唯一 unsafe 容器**：`modules/vision/packages/uia-resolver/Cargo.toml:25` 也允许 unsafe，`windows_impl.rs` 直接调用 UIA/COM/Win32 并管理同步观察 DPI 上下文；应分别审查这些原生边界。
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
| `tmp/2026-09-19-agent-fixes/pr-checkout/` | 发布源码及验证用独立 checkout；扫描应排除此目录以免重复计数，含共享构建 junction，清理须另行确认真实路径与链接范围 | 发布追溯资产，不作为本轮整合删除项 |

### A 侧静态工具注册清单（19 个 spec；实际模型可见集合动态生成）
bash、read_file、write_file、edit_file、glob_search、grep_search、WebFetch、WebSearch、TodoWrite、Skill、Agent、ToolSearch、NotebookEdit、Sleep、SendUserMessage、Config、StructuredOutput、REPL、PowerShell（定义在 `tool-registry/src/lib.rs:278-598`）

Web 主路径额外提供 `semantic_dispatch`、`computer_use_perform`、`chat_handoff`，并主动不向 LLM 暴露 `ToolSearch`（`web-console/src/main.rs:32419,32580,32627`）。最终可见集合受会话/房间能力、权限、暴露模式和 allowlist 控制；另需单列 MCP/插件动态工具及开发用 `computer.*` 目录，不能把目录可用状态等同于模型真实执行入口。

### A 侧 HTTP 路由面（232 条，主要分组）
`/api/chat/*`、`/api/tools/*`（pending/approve/reject/events/runtime-execute）、`/api/goals/*`、`/api/sessions/*`、`/api/plugins/*`、`/api/channels/clawbot/*`、`/api/computer-use/browser/*`、`/api/vision/*`、`/api/realtime/session/*`、`/api/pet/*`、`/api/diagnostics/*`、`/api/web/cards`、`/api/file*`、`/api/config/*`、`/api/system/*`

### A 侧 SQLite 表清单
`sessions`、`chat_rooms`、`session_messages`、`chat_room_messages`、`chat_room_permissions`、`runtime_runs`、`runtime_run_events`、`goals`、`goal_phases`、`goal_role_configs`、`chat_handoffs`、`goal_events`、`memory_beads`、`memory_vectors`、`memory_access`、`memory_meta`、`memory_edges`、`memory_settings`、`memory_jobs`、`attachment_refs`、`chat_usage_events`、`chat_message_timing`、`clawbot_login_snapshot`、`clawbot_inbox`、`clawbot_outbox`、`clawbot_group_member_grants`、`clawbot_contacts`、`clawbot_operation_administrators`、`clawbot_groups`、`clawbot_group_audit`、`clawbot_denial_notices`、`metadata`、`chat_room_diagnostics`、`computer_use_runs`、`computer_use_steps`、`computer_use_step_details`、`computer_use_planner_diagnostics`

上述为当前业务/审计表；迁移中临时替换表（如 `goal_phases_v6`）另列，不计入当前业务实体。证据：`main.rs:39653,39844` 与 `computer_use_store.rs:13,45,67,72`。

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
- `client-ui-renderer` — React slot 绑定、`ctx.uiRenderer`。`[接口与实现已存在]`；Host `apply()` 空实现是仅浏览器 renderer 的预期设计，Client `src/client/index.ts:92` 安装 slot renderer 并提供 mount，不能据 Host 空入口判定为 stub。
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

## experimental（16）
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
- `session-persistence-jsonl` — JSONL 后端（zstd 帧、fsync、原子 publish、flock 租约、torn tail）。`[完整]`（Windows 使用路径派生的命名内核信号量，**无锁文件但有跨进程排他**；名称按登录会话隔离，见 `src/lease.ts:1,80` 与 README:160）
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

## B 侧包层之外的目录（其中 apps/native/vendor 等仍属于 workspace）

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
| `vendor/` | 9 个 vendored 包（pinned 源拷贝 + 上游 SHA）。`[完整]` |

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


---

# 2026-09-21 独立审查补充

# 功能全量清单证据审查

审查日期：2026-09-21。输入：`C:/Users/zhupu/Desktop/02-coolzhu-dsh-功能全量清单.md`（667 行），并交叉核对本地 coolzhuagent、dsh、0.2.14 发布报告。这里只读取源码和既有证据，没有启动产品、请求云模型或执行桌面输入。

## 审查结论与证据等级

清单对包名的覆盖较好，适合作为代码资产目录；不能直接把 `[完整]` 汇总为产品完成率。A 侧遗漏了已经接线的关键产品能力，B 侧有把接口包、实验 provider 和默认产品能力混算的问题。应把每行扩展为“接口存在 / 生产接线 / 测试证据 / 已知边界 / 整合决策”五列。

本次通读了全部 667 行，并将 B 侧所有 291 个 `packages/*/*/package.json` 与清单逐名核对；只发现 `util/time` 的规范包名 `util-time` 使用了目录短名 `time`，并非遗漏了整个包。对架构决策影响最大的断言进行了下述源码抽查。没有对 291 个包逐文件穷尽审计，没有执行 dsh 测试，因此不能替原清单全部 `[完整]` 背书。

证据优先级：生产调用与执行结果 > 定向测试 > 契约及实现源码 > README/注释 > 名称或目录存在。纯静态审查可以确认接口与调用点，不能推出模型端到端成功率。

## 必须修正的事实

| 编号 | 原断言与问题 | 修订结论 | 证据 |
|---|---|---|---|
| F01 | B 侧“291 个 workspace 包”；后面又把 apps/native/vendor 列为非 workspace | 291 是 `packages/*/*` 包层数量。另有 22 个 workspace manifest：vendor 9、native/system 及其平台/入口 6、apps 4、benchmarks 1、website 1、python/sdk-runtime 1，总计 313 个匹配成员，不含仓库根包。保留“B 侧包层清单 291 包”这个准确分母 | `C:/Users/zhupu/Desktop/dsh/pnpm-workspace.yaml:1` 到各 glob；实际 manifest 枚举 |
| F02 | experimental 标 17，vendor 标 6 | 实际分别为 16 与 9。experimental 清单的最后一条是说明，不是第 17 个包 | `C:/Users/zhupu/Desktop/dsh/packages/experimental/README.md:25`，manifest 枚举；vendor 为 cordis、cosmokit、group、hmr、include、loader、logger-console、schemastery、timer |
| F03 | `mvp_tool_specs()` 有 20 项、模型可见工具 20 项 | 注册表静态 spec 实际 19 项。Web 模型工具面还附加 `semantic_dispatch`、`computer_use_perform`、`chat_handoff`，并明确过滤 ToolSearch；会话、房间、权限、暴露模式、allowlist 会改变最终请求工具数，不能用静态 spec 数代表模型可见工具数 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/tooling/packages/tool-registry/src/lib.rs:278`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:32419`、`:32580`、`:32605`、`:32627` |
| F04 | A5“不处理 DPI/多显示器，截图仅主屏” | DPI 结论过期。UIA 同步观察已使用可恢复 PMv2 线程上下文；正式桌面桥按绑定窗口采集原图并以物理像素 rect/DPI/身份校验。多显示器、负坐标、跨屏与混合缩放仍需要专项实测，不能反向宣称全支持 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/vision/packages/uia-resolver/src/windows_impl.rs:26`、`:52`、`:369`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:75`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/input_stroke_native.cs:57`、`:70` |
| F05 | A5“无输入内容安全控制” | 绝对断言错误：宿主收集目标/参数/可见节点文本，识别删除、支付、发送、凭据等敏感语义；文本输入限制 Edit/Document 和 4000 字节；另有动作能力、审批、限额。不能据此宣称具备完整语义安全方案，规则误报/漏报、密码框等仍待专项审查 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/computer-use/packages/computer-use-core/src/controller.rs:92`、`:201`、`:276`、`:541`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:162` |
| F06 | windows-process-guard 是唯一 unsafe 容器 | 错误。uia-resolver 明确允许 unsafe，直接调用 COM/UIA/Win32，并不只依赖该进程容器。应把不同 native 边界分别审查，不以全 workspace 禁止 unsafe 掩盖例外 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/vision/packages/uia-resolver/Cargo.toml:14`、`:25`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/vision/packages/uia-resolver/src/windows_impl.rs:34`、`:47` |
| F07 | A5 标“完整 + 死代码”且列底层原语，没有区分正式控制通道 | 改为“基础执行已接线，端到端部分验证”。正式控制通道已有结构化规划、图片观察、受控拖拽、取消、证据/状态/用量；Paint 的目标仍未通过，不得因核心单测通过而标产品完整 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_planner.rs:41`、`:524`、`:564`；`C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14-agent-fixes-report.md` |
| F08 | A6 文件面遗漏正在承载产品逻辑的拆出模块 | 补充 chat_tool_history、tool_loop_coordinator、multimodal_input、computer_use_planner/executor/adapters/desktop_bridge/store，以及对应测试。巨型 main 规模原值正确，但“只有巨型单文件”不能抹掉这些已经提取的边界 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:4`；`:9`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/chat_tool_history.rs:18`、`:75` |
| F09 | A 侧 SQLite “表清单”遗漏运行审计实体 | 至少补 metadata、chat_room_diagnostics、computer_use_runs、computer_use_steps、computer_use_step_details、computer_use_planner_diagnostics；迁移中 `goal_phases_v6` 等临时替换表不应算当前业务表 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:39653`、`:39844`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/computer_use_store.rs:13`、`:45`、`:67`、`:72` |
| F10 | B client-ui-renderer 因 Host apply 空而标 stub | 错误归类。Host 入口注释明确是仅浏览器 renderer；Client 实现安装 slot renderer 并提供 React mount。应记录“Host 无行为是设计”，而非功能未实现 | `C:/Users/zhupu/Desktop/dsh/packages/client/ui-renderer/src/index.ts:1`；`C:/Users/zhupu/Desktop/dsh/packages/client/ui-renderer/src/client/index.ts:92` |
| F11 | B JSONL 的“Windows 无 lock 文件”易被解释成无跨进程锁 | Windows 使用由路径派生的命名内核信号量；无文件足迹不等于无锁。仍需注意名称按登录会话隔离，不能直接保证跨用户/跨登录会话排他 | `C:/Users/zhupu/Desktop/dsh/packages/session/session-persistence-jsonl/src/lease.ts:1`、`:80`；`C:/Users/zhupu/Desktop/dsh/packages/session/session-persistence-jsonl/README.md:160` |
| F12 | A2“新增 provider 需改约10处”混同了新增模型/连接 | 新增 ProviderKind/原生适配分支确实涉及 enum 分发，但已有 `from_session_endpoint` 支持模型未登记时按显式协议构造连接；用户正常换模型 ID、Endpoint 不需要新增 provider。清单必须保留这个已实现能力，不能把统一配置再当待从 dsh 新建 | `C:/Users/zhupu/Desktop/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/client.rs:39`；`C:/Users/zhupu/Desktop/coolzhuagent/modules/llm-adapter/packages/llm-adapter/src/request_parameters.rs:1` |
| F13 | 把临时 PR checkout 当污染建议清理 | 这是本轮发布来源及历史验证资产，且包含共享 target 的 junction。应改为扫描排除；真正清理前独立检查路径/链接/未提交数据，不把删除作为架构整合前置 | `C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.14-agent-fixes-report.md` 的构建源码与发布追溯；本轮上下文已确认共享构建链接 |

## 原判断可保留但需限定

- A workspace 28 成员、main.rs 87025 行/3470755 字节、app.js 20416 行、styles.css 17809 行，当前文件实测相符；精确行数用包含空行的 splitlines，不能用 PowerShell Measure-Object -Line（会忽略空行）替换。
- Web 主路径没有调用 ConversationRuntime；CLI 的 ApiClient::stream 一次返回 Vec，的确不是逐个异步事件接口。证据：`C:/Users/zhupu/Desktop/coolzhuagent/modules/core-runtime/packages/core-runtime/src/conversation.rs:38`。这说明存在双运行时，不说明应立即把 Web 换成当前 CLI runtime。
- LspManager::new 的当前生产扫描无命中，仅 language-service/src/lib.rs:197、:258 测试构造；可保留“未接线”结论，整合应增加真实生产调用与生命周期测试。
- 插件远程安装尚未完成，甚至当前 install handler 仅检查非空 ID 就返回 installed=true，不能把响应字段当真实安装成功证据。证据：`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:18942`。
- diagnostics/stream 返回静态能力宣告，不能作为真正故障注入测试。证据：`C:/Users/zhupu/Desktop/coolzhuagent/modules/gui-web/packages/web-console/src/main.rs:18503`。
- dsh computer-use 基础包是独占 provider 注册服务，不实现动作；真实桌面 provider 位于 experimental。证据：`C:/Users/zhupu/Desktop/dsh/packages/computer-use/computer-use/src/index.ts:16`。其“完整”只适用于注册接口，不能推广为默认发行版桌面能力已可用。
- dsh “webhook 完整”“SDK 完整”“LSP 完整”仅是所列契约的实现完备，原文已列无队列/无取消/仅四操作等边界。应分别定义验收范围，不能与用户期望的完整产品功能同义。

## 缺失的产品能力维度与重复计数

| 维度 | A 已有事实 | 原清单应补的边界与整合要求 |
|---|---|---|
| 模型配置、图片路由 | 统一参数页、原图直传/默认视觉转述、思考参数等已在0.2.14发布 | 参数协议能力需与产品保存值/实际wire逐项对齐；图片开关不代表音视频全模态。保留用户百炼 Base URL、已有 Key、qwen3.8-flash；Agnes 仅是当前默认视觉配置名，架构不能写死该名 |
| 消息与工具呈现 | 暂态思考、独立状态、跨轮过滤、旧自动记忆降噪已实现 | 重构须保留 room/turn/call 作用域、终态不能倒退、旧历史不破坏及同轮原生工具配对；不要又造第二套 UI ledger |
| 搜索/定位/用量 | 房间全文子串检索和定位、逐轮耗时、provider已返回用量 | 不是 FTS、不是跨工作区搜索、不是准确账单。`chat_insights.rs:136` 是遍历内存消息；`:163` 房间范围；`:197`说明缺 usage 不计入。dsh 的检索属于可借鉴索引升级，不是“从0增加搜索” |
| Goal 与多 Agent | 主 Web 路径有 phase claim、重试、implementer blocked→planner、目标暂停及升级 | 不能因为 coolzhu-orchestrator 插件未接线就断言主产品没有编排。`main.rs:37675` 已存在失败回退及统一 retry_count，仍需幂等、并发、恢复测试证明闭环边界 |
| 浏览器与桌面 | MV3 owned tab bridge 和正式桌面 bridge 都实现 ComputerUseAdapter | 应独立列正式用户入口、模型工具入口、开发靶场/仅plan接口；`computer.*` 调试目录不能与 `computer_use_perform` 真实工具混为一张能力表 |
| 运行证据与打包 | 0.2.14有安装包源码身份、哈希、安装用户数据保留、1175通过证据 | 测试数不是功能覆盖率；Paint端到端失败、Anthropic在线跳过、原生窗口启动被审批拦截都必须与“通过”并列。dsh尚无本轮同条件实测，不能用包数量替代性能/可靠性比较 |

重复计数主要在“seam接口包+provider实现+tool暴露+UI呈现+bundle装配”被当作五项不同用户功能。例如 dsh computer-use 基础包+2个experimental provider不等于3套产品目标能力；session格式/持久化/投影族也不是19项用户功能。A 的tool-registry、Web动态工具、调试工具目录同理。建议每个用户能力只分配一个 feature_id，各包仅作为实施组成引用。

## 待验证项，禁止从静态清单推出

1. 原文 `[完整]` 每项是否进入目标发布 profile、是否有用户入口、是否打包了实际依赖、生命周期卸载是否可靠。
2. Windows 原生会话写锁的跨登录会话范围；目标整合应明确单用户本机会话边界，不能把命名内核信号量当分布式锁。
3. 统一配置与 dsh 参数注册/凭据引用整合的兼容迁移，尤其空 Key 不覆盖已存 Key、保留精确 Base URL、无效参数不静默忽略、旧历史/旧会话仍可打开。
4. 所有输入通道的授权一致性、敏感字段与密码控件识别；当前关键词规则和输入长度限制不是完整内容安全证明。
5. 中文 Windows 编码、100/125/150/200%缩放、负坐标与跨屏窗口、画布与工具栏识别、输入后稳定等待。既有150%DPI修复只证明该缺陷修复，未覆盖整个矩阵。
6. CU观察新鲜度导致失败究竟是窗口/控件真实变化，还是计划耗时/观察代际粒度造成误拒；不可简单延长时限或移除stale检查，应记录各阶段时间与身份变化。
7. UI与搜索在1万/10万消息、流式重连、切换房间/工程时的性能和作用域；目前子串遍历并不等于索引检索。
8. 异常退出后的工具/Goal/CU终态恢复、同一调用重放幂等、取消传播、未返回usage的可观测性。原包清单没有这些可测契约。

## 逐阶段整合候选与门槛

| 阶段 | 候选工作 | 为什么此时做 | 必须通过的门槛 |
|---|---|---|---|
| P0 基线与测量 | 修正文档标签和分母；建立 feature_id→生产入口→事件→存储→测试；保留0.2.14黄金样本；接最小CI | 先消除误判和重复开发，再评价dsh差异 | 现有1175测试基线可复现；安装版冒烟/失败样本单独报告；文档不再将注册表数量等同模型可见功能；API Key/历史不外泄 |
| P1 CU可靠性实验 | 观察/规划/输入/稳定等待/验收结构化trace；画布区域与工具状态显式表示；局部裁剪+原图映射；有限stale刷新；分层验收 | Paint仍未通过，存在Agent可检验改进假设，不能仅归因模型能力 | 先mock/靶场再Paint；同模型同初态A/B；报告物理输入率、有效画布落笔率、误报成功率、stale拒绝原因、耗时与token；不得提高画质要求或用预制图替代输入任务 |
| P2 保行为提边界 | 从Web提取 ToolLoopService、ConversationProjection、UsageLedger、SessionConfigService；与CLI建立共享契约，暂保HTTP/SSE/SQLite兼容 | 借鉴dsh seam/provider/tool分层，避免一次全栈迁移同时改变产品行为 | 黄金wire/SSE回放无退化；room/turn/call身份保持；工具参数不进入思考，跨轮记忆投影不退化；取消/终态矩阵通过 |
| P3 补高收益缺口 | FTS派生索引、稳定事件查询、LSP真实接线、插件安装结果与真实状态一致、诊断故障注入、日志保留 | 这些能在不替换已工作的UI/运行时前提下改善产品 | 索引可重建且按工作区授权；安装失败不报installed；LSP重启/退出无泄漏；诊断不以常量“ok”冒充测试；大历史性能门槛先测后定 |
| P4 受控扩展 | 评估dsh适配注册、PTC、子agent协议、workflow/SDK、实验CU provider作为可选桥接 | 只有接口契约稳定后再评估跨Rust/TS运行时成本 | 每项有依赖/进程/凭据/取消/持久化所有权；不能同时让两个runtime拥有同一会话；实验provider须打包与真实功能验证，禁止默认启用未经验证组合 |
| P5 UI和发行 | 保留玉石icon、竹林背景及对比度约束；在既有左右布局上优化；升级/回滚/运行证据统一 | 不重建已完成的统一配置和消息体验 | 低高度窗口可用、键盘可达、后台运行中切换保护、搜索定位回放、升级保留配置、签名/发布追溯；用户审批后才实施新视觉方案 |

推荐原则：优先迁移 dsh 的边界设计、可回放测试与声明式装配思想；不能根据291对28的数量差认定应整体重写。实际引入其包代码时另审依赖、许可证和部署成本。本文件只提出整合候选，不授权产品改动。


## 本轮新增专项及后续计划

原生Computer Use、ZCode接口契约、Paint R1–R6、Agent改进候选与实验门槛，见[03专项](03-coolzhu-zcode-Computer-Use差异与Paint专项.md)。整体执行计划将结合GPT6 Pro实际审查回复单独形成，不能把本节候选阶段当作已经实施。
