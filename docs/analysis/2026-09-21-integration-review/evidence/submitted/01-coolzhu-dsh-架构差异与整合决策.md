# coolzhu × DeepSeek Harness 架构差异清单与统一整合预备文档

> **2026-09-21 审查修订版。** 已保留原始备份，并对影响决策的断言作正文纠正。原有 `[完整]` 仅表示所述代码/契约存在，不代表全部产品验收；本轮为全文阅读与重点源码抽查，未重新运行产品测试。
> 新增：[Computer Use 与 ZCode 差异及 Paint 专项](03-coolzhu-zcode-Computer-Use差异与Paint专项.md)。文末审计表使用修订前原文行号，源码引用以本次本地快照为准。涉及冲突时以本轮证据纠错为准。

**用途**：为两套 agent 系统的架构统一整合提供事实基础。本文只描述两侧的**实现逻辑与架构**，不做优劣排序。

**口径**：
- 完成度标记：`[完整]` `[部分]` `[stub]` `[实验性]` `[未接线]` `[死代码]`
- 所有结论附 `文件:行号`，可复核
- 未实现的功能也列出（这是统一整合时必须先决策的部分）
- 对照物：
  - **A 侧** = `C:\Users\zhupu\Desktop\coolzhuagent`（Rust workspace，Windows 桌面 Agent）
  - **B 侧** = `C:\Users\zhupu\Desktop\dsh`（DeepSeek Harness，TS/Cordis monorepo）
- 规模：
  - **A 侧**：Cargo workspace **28 个成员** = 18 个模块 crate（`modules/*/packages/*` + `packages/app-launcher`）+ 9 个插件 crate（`.coolzhu/plugins/*`）+ 根包 `.`；另有 **1 个不在 workspace 内的独立 Cargo 项目** `modules/gui-desktop/packages/tauri-shell/src-tauri`；3 个只有 `plugin.json` 的插件目录（非 crate）；1 个纯 JS 的 `modules/browser-extension`（非 crate）；3 个 skill
  - **B 侧**：291 个包层成员（`packages/<group>/<pkg>`）；整个 pnpm workspace 另含22个成员manifest，共313个，不含根包。详细分母见02清单本轮审查
- 完整模块/包清单见配套文件 `02-coolzhu-dsh-功能全量清单.md`

---

# 第一部分 跨范式差异（统一整合的根本约束）

这 9 条不是某个功能点的差异，而是两侧的底层范式差异。任何领域级的统一都必须先在这些维度上做选择，否则会反复返工。

| # | 维度 | A 侧 coolzhu | B 侧 dsh |
|---|---|---|---|
| P1 | **实现语言与进程模型** | Rust workspace，单进程多线程 + `tokio`；`Command::new` 拉起子进程（插件工具、MCP、浏览器 native host、clawbot sidecar） | TypeScript ESM，Node 单进程事件循环；子进程用于 MCP / 持久 shell / PTC runtime / subagent / SDK |
| P2 | **扩展机制** | manifest（`.claw-plugin/plugin.json`）+ **进程外子进程**，JSON 走 stdin，`CLAW_PLUGIN_*` 环境变量；扩展点仅 3 类 | **同进程** Cordis 插件；`ctx.effect()` / `ctx.on()` 注册，`register()` 返回 disposer；扩展点覆盖 service/事件/waterfall/UI slot/命令/prompt section |
| P3 | **依赖注入 / 解耦** | 手工 trait 注入（`ApiClient`/`ToolExecutor`/`Provider`/`PermissionPrompter`）+ `OnceLock<RwLock<…>>` 全局单例（`tool-registry/src/lib.rs:88-97`） | Cordis service container：Service Definition/Provider/Consumer 三层 seam，`ctx.<name>` 只对声明过的 `inject` 可见，可选 service 走 `ctx.get(name)`（`packages/AGENTS.md:5-6`） |
| P4 | **类型与契约传递** | Rust 编译期类型；跨进程契约靠 JSON + 手写 schema（`ToolSpec.input_schema: Value`） | TS 编译期类型 + **Typert 构建期代码生成**（`FaceModel`/`TypeGraph` → Zod 工厂 + `InvocationDescriptor`），解决 Host/Client 两个独立 tsc program 的类型桥接 |
| P5 | **权威状态模型** | **SQLite 关系表为权威**（`web-sessions.sqlite3`，`main.rs:39653-40717`），另有 `Session` JSON 带自己的 `version: u32`（`session.rs:48`） | **append-only 事件日志为权威**（JSONL + zstd 帧），SQLite 仅承载**派生**索引与缓存（`session-query-sqlite` schema v8、`storage-sqlite`） |
| P6 | **前端形态** | 原生 JS 单文件（`app.js` 20,416 行 / 765 KB）+ `index.html` + `styles.css`，**Rust 编译期内联**（`main.rs:25041,56686`）；无框架无构建器 | 55 个 `packages/client/*` UI 包（React + Vite 独立构建）+ 8 个 `packages/host/*`；插件在 `package.json` 声明 `dsh.client` |
| P7 | **平台覆盖** | **Windows only**（PowerShell + 内联 C# P/Invoke、UIA、Job Object、WiX MSI） | Linux/macOS/Windows 三平台（bwrap/Landlock、Seatbelt、Win32 ACL token + Job Object） |
| P8 | **分发形态** | MSI 安装包（WiX 5.0.2）+ `package/` staging + `COOLZHU-AGENT.exe` launcher | npm/pnpm 包（`@deepseek-ai/dsh`）+ 6 个 bundle + profile 组合；Python wheel；Electron 桌面壳 |
| P9 | **Agent 主循环数量** | **两套并存**：干净的 `ConversationRuntime`（`conversation.rs:104`）只被 CLI 与 `tool-registry` 子 agent 使用；产品主路径是 `web-console/main.rs` 内的内联循环（`grep -c ConversationRuntime` = 0） | **一套** `agent-loop`，所有 profile（web/headless/sdk/acp）共享；传输层与 runtime 分离 |

**统一含义**：P1/P2/P5/P6 是硬约束。特别是 **P2（进程外 vs 同进程）** 与 **P5（SQLite vs 事件日志）** 显著约束运行时、状态与扩展的整合方式；具体影响见第九部分决策点，不使用缺乏分母的比例。

---

# 第二部分 运行时与扩展

## 领域 1 进程启动与组合模型

**A 侧** `[完整]`
- 架构位置：`packages/app-launcher` 是安装后唯一用户入口 `COOLZHU-AGENT.exe`，读 `config/package-launcher.json`（`web_console.executable`、`tauri.executable`、`health_url`、超时/轮询、log_dir、runtime_dir），按序拉起 web-console 再拉起 tauri-shell（`lib.rs:400`），传 `--web-console-pid` 给 tauri。
- 端口与单实例：`preflight_web_console_port` 用"health 探测 × listener 归属"分类——复用既有的 / 清理陈旧进程 / 报错（`main.rs:101-223`）；`poll_health` 轮询 `health_url`（`lib.rs:370`）；**无 mutex 单实例**，靠 listener + health 判定；tauri 侧另有 `single-instance` 插件。启动自检 JSON 供排障（`lib.rs:420`）。
- 打包期声明：所有二进制/资源必须由 `config/package-manifest.json` 声明（10 artifacts + 12 resources），`docs/development-standard.md:57-65` 强制。
- 端口：web-console 默认 `http://127.0.0.1:8765/`。

**B 侧** `[完整]`
- 架构位置：`dsh <profile>` CLI → 4 层 patch 叠加：**bundle → profile `cordis.patch.yml` → `$DSH_HOME/cordis.patch.yml` → `--patch` overlay**（`apps/cli/src/profile-boot.ts:3,193,213`）。patch 按 row id 匹配，**整段替换 `config` 而非 merge**。
- 启动器：`packages/boot/app-boot` 做 `installFailLoud('dsh')`（`:633`）+ `boot()`（`:917`），`.env` 分层为"调用目录文件 > Harness-home 文件 > 继承环境"，且**决定进程启动的变量（`PATH`/`DSH_*`/`XDG_*`）禁止来自文件**，4 个 proxy 名只接受 home 文件。
- fail-loud：required entry 列表 = `agent-loop/webserver/modules/connection/headless-runner/acp/sdk-jsonrpc-server`；已启用 required entry 无法激活则 dispose 全 app + `exit 1`（`auditStartupEntries` `:870`）。
- 热重载：`packages/boot/hmr` 用 `runExclusive()` 单队列串行化配置变更、Loader 更新与自动 reload（`:139`），watch profile manifest + profile/home patch（`:235-236`）；`headless`/`sdk`/`acp` bundle 在 YAML 里 disable module HMR。
- 命令行：`packages/boot/cmdline` 提供不可变 `ctx.cmdlineArgs` 快照、`ctx.appExit`、`exitOnStdinEnd`；launcher 只解析自身 flag，首个不识别 token 起为 app 参数。
- 6 个 bundle：`base`(512 行 patch)、`web-app`(516)、`headless`(34)、`sdk-app`(25)、`acp-app`(24)、`sdk-minimal`(158，**不继承 base**，自含完整树)。
- profile：`web` / `headless` / `sdk` / `sdk-minimal` / `acp` / `desktop`（Electron 独占 `$DSH_HOME/profiles/desktop`）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D1.1 | 组合单位 | 固定的 2 进程（web + tauri），由 launcher JSON 描述 | 任意 bundle 叠加 + profile 命名组合，patch 4 层 | A 侧若要对齐需引入"组合清单"概念；B 侧无 MSI/launcher 概念 |
| D1.2 | 配置来源 | `package-launcher.json` + `package-manifest.json`（仓库内静态） | profile manifest + patch 文件 + home patch + CLI overlay | 两套"启动配置"语义不同：A 是**产物清单**，B 是**依赖图** |
| D1.3 | 热重载 | 无（改前端需重编 Rust crate；改配置需重启） | HMR：模块与 profile 配置热重载，单队列串行 | A 侧无对应能力 |
| D1.4 | fail-loud | 启动自检 + health 轮询 + 排障 JSON | required entry 白名单 + 无法激活即 exit 1 | 两者都有，机制不同 |
| D1.5 | 单实例 | listener + health 探测（无 mutex） | 未见统一机制；desktop 用 Electron 进程锁 | — |

## 领域 2 扩展机制（插件）

**A 侧** `[完整 + 未接线]`
- 抽象：`Plugin` trait（`plugin-system/src/lib.rs:400-408`：metadata/hooks/lifecycle/tools/validate/initialize/shutdown），三实现 `BuiltinPlugin`/`BundledPlugin`/`ExternalPlugin`（`:376-398`）。
- 工具执行：**外部进程**。`Command::new(command).args(args)`，输入 JSON 写 stdin，注入 `CLAW_PLUGIN_ID`/`CLAW_PLUGIN_NAME`/`CLAW_TOOL_NAME`/`CLAW_TOOL_INPUT`/`CLAW_PLUGIN_ROOT`（`:297-338`）。**无 .dll/wasm 加载**。
- manifest：`.claw-plugin/plugin.json`，字段 `name/version/description/permissions/defaultEnabled/hooks/lifecycle/tools/commands`（`plugin-system/src/lib.rs:106-122`）；`tools[]` 带 `inputSchema/command/args/requiredPermission`（`:158-168`）。
- **扩展点只有 3 类**：pre/post tool hooks、lifecycle init/shutdown、tools + slash commands。**没有 UI 扩展点**。
- hooks：shell 脚本，`exit 0 = allow`、`exit 2 = deny`、其他 = warn 继续（`hooks.rs:178-195`）；载荷经 stdin JSON + `HOOK_EVENT` 环境变量（`:107-171`）；Windows 走 `cmd /C`（`:230-250`）。**同一接口有两份实现**：`core-runtime/src/hooks.rs:76,88` 与 `plugin-system/src/hooks.rs:65,77`。
- 安装：来源 `LocalPath` / `GitUrl`（`:345-350`）；暂存目录→拷贝到 install_root（`:978-1019`）；install 记录 `installed.json`，启用状态 `settings.json`（`:18-19,913-935`）。
- 扫描来源：builtin（代码里只有 `example-builtin` 占位 `:1338-1353`）+ installed registry + external 目录（`:957-963`）。
- 权限：manifest `permissions`（read/write/execute）在解析时校验合法性（`:1496-1520`），`requiredPermission` 三档（`:170-196`），**缺省 `danger-full-access`**（`:341-343`）；**`PluginPermission` 运行时无任何强制点**（全仓 grep 仅命中定义/解析）；插件进程**无网络/文件沙箱**。
- **内置 9 插件完成度**：`.coolzhu/plugins/*/plugin.json` **全部无 `tools`/`hooks` 字段**（已核查 `grep -l '"tools"\|"hooks"'` 无命中）→ 注册 0 个扩展点；除 `Cargo.toml:29-38` workspace 成员外无任何代码引用 `[未接线]`。另有 3 个插件目录（`coolzhu-agents-md-updater`、`coolzhu-claude-compat`、`coolzhu-opencode-sync`）只有 `plugin.json` 且未列入 workspace `[stub]`。

**B 侧** `[完整]`
- 抽象：Cordis 插件。service 包 default-export service class；function plugin 必须 **named**-export `name`/`inject`/`Config`/`apply` 且无 default export（`packages/AGENTS.md:5`）；混用会被 Loader 丢弃 namespace（postmortem 0001）。可选 service 用 `ctx.get(name)`。
- **注册即 effect**：`register()` 末尾 `return this.layers.effect(this.ctx, layer => layer.tools.insert(...), { label: 'tools.register()' })`（`packages/core/tools/src/index.ts:1043,1063-1068`）——返回值就是 disposer。
- 扩展点覆盖面：service、merge-extensible 事件、waterfall listener、guard、UI slot / 右栏 tab、命令、prompt section、provider（LLM/sandbox/skill/web/session-title/…）。
- 插件分发：`dsh plugin --profile <name> add <package>`（`bundle/README.md:36`），install spec 接受 registry 名/范围、git host URL、tarball（`boot/plugin-manager/src/install-spec.ts:11,70-73`），底层调 pnpm；pnpm 11 阻止脚本时返回 `pendingBuilds` + `approvedBuilds` 批准通道；安装失败回滚 `package.json`/`pnpm-lock.yaml`（**故意不回滚** `pnpm-workspace.yaml`）。
- profile 作用域：plugin toggle 只改 profile `cordis.patch.yml` 最后一条匹配 override 的 `disabled`；bundle toggle 改 `package.json` 的 `dsh.profile.bundles` 有序列表。
- 动态插件：`packages/extensions/cordis-host-runner` 的 `DynamicCordisRegistry`（`registry.ts:141`），Host 半边在 `node:vm` 新 realm 执行（`sandbox.ts:142,254`，`vmTimeoutMs` 默认 5000）；guard 逐服务代理并 deny 未在 `inject` 声明的访问（`guard.ts:669,740`）；定义随重启消失。
- 沙箱自述：`extensions/cordis-host-runner/src/sandbox.ts:5-8` 明确"是 cooperative、可检查、可销毁，**但不是 containment**：host-realm helper functions 仍是逃逸路径"。已安装 Host 代码在进程内、workspace sandbox **之外**执行（`boot/plugin-manager/README.md:26`）；`plugin_manager` 每次操作要求 `danger-full-access` 或当次 approval。
- 无插件市场：无 `dsh-plugin` topic 约定的实现、无 registry 索引。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D2.1 | 隔离边界 | 进程外（语言无关；manifest 声明 command/args） | 同进程（必须是 JS/TS，Cordis 生命周期） | 两者的"插件"**不是同一种东西**；A 的插件可以是任意可执行文件，B 的插件必须是同构模块 |
| D2.2 | 扩展点数量 | 3 类（hooks/lifecycle/tools+commands） | 覆盖 service/事件/waterfall/UI/命令/prompt/provider | A 侧要做 UI 扩展需新增扩展点类型 |
| D2.3 | 生命周期语义 | init/shutdown + validate | `effect()` disposer + 原子 `replace()`；卸载是撤销 effect | 语义可映射但 A 无"撤销"概念 |
| D2.4 | 权限强制 | manifest `permissions` **无强制点**，默认全权 | `plugin_manager` 要求 `danger-full-access`/逐次审批；sandbox 自述非 containment | 两侧都存在"声明的权限与执行的权限不闭合"，统一时需定义权威强制点 |
| D2.5 | 分发 | LocalPath / GitUrl + installed.json | pnpm：registry / git / tarball + profile patch | B 侧复用 npm 生态，A 侧需自建或接 npm |
| D2.6 | 市场 | `coolzhu-marketplace` = 内存 `HashMap`（`publish/search/install`），`install()` 只 `downloads += 1`（`lib.rs:6-69`）；web-console `api_plugins_install` 是显式 stub 自述"P2 接入"（`main.rs:18942-18960`）`[stub]` | **无市场** | 两侧都没有可参考的市场实现 |
| D2.7 | 双份实现/重复 | `core-runtime` 与 `plugin-system` 各有一份 HookRunner | 未见同类重复 | — |

## 领域 3 Agent 主循环与会话生命周期

**A 侧** `[完整，两套并存]`
- 干净版：`ConversationRuntime<C: ApiClient, T: ToolExecutor>`，`run_turn()` 在 `conversation.rs:104,176`，循环体 `:213-337`；落 `AgentEvent`（TurnStarted/ContextSnapshot/ReasoningDelta/ToolCall/MessageDone/TurnCompleted）。
- 产品版：`web-console/src/main.rs`（87,025 行）：`api_chat_send`（`:16465`）、`api_chat_send_stream`（`:16836`）、工具反馈循环（`:17614`，上限 `tool_execution_policy().max_feedback_rounds` 默认 40、clamp 64，`:6386,6470`）、非流式循环（`:26534`）。链路：`prepare_chat_dispatch` → `create_chat_runtime_run_sqlite`（`:16849`）→ `start_chat_runtime_run_sqlite`（`:16876`）→ 模型调用 → `model_tool_requests_from_blocks`（`:26314`）→ `dispatch_model_tool_calls_parallel`（`:26354`）→ `runtime_tool_execute` → SSE。
- `ConversationRuntime` 的生产消费端只有两处：`modules/cli/packages/command-line/src/main.rs`、`tool-registry` 的子 agent（`lib.rs:2197-2215`）。
- 两套主循环的分工：CLI 走 `ConversationRuntime`（阻塞式 `ApiClient::stream() -> Vec<AssistantEvent>`，非增量，`conversation.rs:39`）；web 走内联 axum SSE 循环 + `tool_loop_coordinator.rs`。
- 运行记录：`runtime_runs` 表带 `state` CHECK（accepted/running/stop_requested/completed/failed/interrupted/orphaned）+ claim_token/lease（`main.rs:39938-39972`），事件表 `runtime_run_events`（kind ∈ `chat_turn`/`goal_phase`/`goal_loop`）。

**B 侧** `[完整]`
- 生命周期（`docs/architecture.md:88-107`）：`turn/start` → claim input → 组装 prompt sections + tool schemas → `agent/pre-step`（可 reject/rewrite）→ `step/start` → `agent/request` + `prepareCall()` 解析路由 → 提交 system/user messages → freeze model history → `llm/stream` → `tool/call*` → `tools/pre-execute` → `tools/execute` → `tools/post-execute` → `tool/result*` → `step/end` → `agent/turn-stopping` → `turn/end`。
- 术语：**step** = 一次模型请求 + 其工具调用；**turn** = 零或多个 step（`docs/architecture.md:86`）。
- 四个 waterfall：`agent/pre-step`、`agent/request`、`llm/stream`、`tools/*`；listener **必须调 `next()`** 才委派，不调即短路（`docs/cordis-primer.md:31`）。`agent/turn-stopping` 是 serial，无 `next()`（`docs/architecture.md:109`）。
- 机械不变式：`agent-loop/src/invariant.ts:24-58` 在 `llm/stream` 上 prepend 检查——request 必须 frozen、必须带 live session id、log 里必须有 `step/start` + `request/header`，且 `JSON.stringify(options.messages)` 必须等于 `session.deriveMessages()`，否则 fail（`log-reconstruction desync`）。
- crash 修复：`core/session/src/repair.ts` 合成 `TOOL_NOT_STARTED` / `TOOL_OUTCOME_UNKNOWN`；不变量断言 seq 严格递增、turn/step 包闭、tool call/result 配对（`core/session/src/invariant.ts:60,72,108,123`）。
- fork：仅切在闭合 turn 外的稳定边界（`core/session/src/index.ts:1236`）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D3.1 | 主循环数量 | 2 套（CLI / web），web 为主 | 1 套，所有 profile 共享 | A 侧统一的第一前提 |
| D3.2 | 循环可扩展性 | 无统一扩展点（hook 仅 pre/post tool） | 4 waterfall + 1 serial，语义有文档 | A 侧插入行为需改主循环 |
| D3.3 | 模型可见性校验 | 无 | `deriveMessages() === request.messages` 断言 | A 侧无对应物 |
| D3.4 | 轮次上限 | `max_feedback_rounds` 默认 40 / clamp 64 | 无固定上限（由 goal/plan 与 provider 决定） | 语义不同：A 是硬闸，B 是外部策略 |
| D3.5 | 崩溃恢复 | 已有启动恢复：遗留 run 收敛 orphaned，匹配 Goal claim/状态/事件在同一事务更新（`main.rs:796,41804-41824`）；未见与 B 等价的完整工具副作用重建协议 | 合成 TOOL_NOT_STARTED / TOOL_OUTCOME_UNKNOWN + 不变量断言 | 需补齐工具结果不确定性与防重执行，不应把 A 写成无恢复 |
| D3.6 | 流式粒度 | CLI 非增量；web 走 SSE | 统一 `llm/stream` waterfall | — |

## 领域 4 事件与传输协议

**A 侧** `[完整]`
- 传输：HTTP fetch + JSON（`requestJson`，`app.js:17663`）；聊天流是 `POST /api/chat/send/stream` 的 **SSE 手写解析**（`app.js:13758-13850`），事件 `started`/`message_start`/`message`/`message_delta`/`message_done`/`done`/`error`。
- 另有 4 条 EventSource：`/api/tools/events`（`app.js:3395`，permission-required/approved/rejected/lagged）、`/api/goals/{id}/events`（`:4976`）、`/api/realtime/session/events`（`:1327`）、`/api/vision/realtime/events`（`:1293`）。
- 服务端双轨：`broadcast::Receiver` + SQLite 轮询（`main.rs:2755,15164`）。
- 路由总量：**232 条**。
- 事件持久化：`runtime_run_events`（运行期）+ `goal_events` + `session_messages`/`chat_room_messages`（消息）+ `tool-audit.jsonl`（审计）。

**B 侧** `[完整]`
- 事件模型：`core/session` 的 append-only 类型化事件日志是**权威**。`SessionEventMap` 为 declaration-merging 表（`core/session/src/types.ts:269`），成员**默认 required-on-read**，未知类型需 envelope `ignorable?: true` 才可跳过（`:483`）。
- surface 语义：surface 类型（`system/message`/`user/message`/`assistant/message`/`tool/result`，`:415`）强制带 `surfaceOp`（`'append'` 或 `{ op:'replace', startSeq, endSeq }` 闭区间，`:441`）；log-only 事件禁止两个 surface 字段。`foldSurface()` 供离线重组（`core/session/src/surface.ts:544`）。
- 传输（Client↔Host）：`api-gateway` 双侧 Typert RPC——Host `ctx.typertGateway` 在 `/api` RPC carrier 注册 interceptor，并注册 `/api/remote.mux` **WebSocket upgrade**（`api/gateway/src/index.ts:205-215`）；unary 走 HTTP POST + `RemoteResult<T>`，stream 走 mux 多路复用逻辑流。
- 会话控制：`api-session-controller` 的 **cold reads**（只读持久化 header 与 projection cache 行，不 stat、不开 Session body，`src/list.ts:1,109`）+ **live control transport**（每 generation 先发完整进程内 baseline 再发增量，`src/control.ts:20,57`）。
- 重连语义：`client-connection` 的 generation 由"source 报 ready"才可见，失败/撤回/stop 先清 generation 再进 `ConnectionController` 重试策略。
- 其他协议入口：`sdk-protocol`（stdio newline-delimited JSON-RPC 2.0）、`acp`（标准 ACP v1）、`webhook`（HTTP）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D4.1 | 权威事件源 | SQLite 表（`runtime_run_events` 等）+ SSE 流 | append-only 事件日志（surface/log-only 分类） | 需先决定权威源 |
| D4.2 | 传输风格 | SSE + 4 条独立 EventSource | unary HTTP POST + WebSocket mux（同一 `Remote` 抽象） | — |
| D4.3 | 客户端类型契约 | 无（手写 JS 解析） | Typert 构建期生成 Zod + descriptor | A 侧无类型化 wire |
| D4.4 | 未知事件处理 | 未见策略 | required-on-read，需 `ignorable:true` 才可跳过 | B 侧显式版本策略 |
| D4.5 | 断线重连 | SSE 重连；SQLite 轮询兜底 | generation 生命周期 + baseline/增量 + `RemoteJournalStream` | — |
| D4.6 | 冷读免加载 | 无（SQLite 直接读） | cold reads（不构造 Session 对象） | B 侧优化点 |

## 领域 5 工具系统

**A 侧** `[完整]`
- 注册：静态 `mvp_tool_specs()` 共 **20 个 `ToolSpec`**（`tool-registry/src/lib.rs:281-598`）：bash、read_file、write_file、edit_file、glob_search、grep_search、WebFetch、WebSearch、TodoWrite、Skill、Agent、ToolSearch、NotebookEdit、Sleep、SendUserMessage、Config、StructuredOutput、REPL、PowerShell 等。每个带 `name: &'static str`、`input_schema: Value`、`required_permission: PermissionMode`（`:71-76`）。
- 全局状态：`GlobalToolRegistry` 用 `OnceLock<RwLock<Option<PathBuf>>>` 存 project config root（`:88-97`）。
- 模型可见性：`definitions(allowed_tools)` 生成 `ToolDefinition`（`:183-205`）；插件工具与内置**重名直接报错**（`:116-123`）。
- 暴露策略三档：`normalize_llm_tool_exposure` = `all` / `dispatch-only` / `whitelist`（默认）（`main.rs:32340-32348`）；`llm_tool_definitions_with_settings` 组装实际列表（`:32584`）；按权限过滤的 allowlist 在 `llm_tool_allowlist_for_permission`（`:32353`）。
- slash 命令：`command-router` 的 `SlashCommand::parse`（`:378`）与 `slash_command_specs()`（`:474`），**与模型工具无关**。
- 路径抽取：`path_effect.rs` 的 `TargetPathsExtractor` trait（`:20`）、`extractor_for(tool_name)`（`:109`），把工具入参映射成 `Vec<PathTarget>`；**静态入参解析、不读文件系统**（注释 `:11`）；对 `bash`/`PowerShell`/`REPL`/`Agent` **只取 `cwd`**，无 `cwd` 返回空 → 被判为"workspace 外"（`:25-42`、`permission_gate.rs:225-228`）。

**B 侧** `[完整]`
- 注册：`ctx.tools.register(definition)` 返回 disposer（`core/tools/src/index.ts:1043,1063-1068`）；`layers` 机制按层组织。
- 执行流水线（`docs/tool-execution-pipeline.md:8-52`）：`tools/pre-execute` waterfall（hooks / permission / sandbox）→ **monotonic guards**（只能 deny 或 abstain，identity 受保护）→ `ctx.approval` one-shot prompt → `tools/execute` → `tools/post-execute`。
- guard 注册入口 `guard()`（`core/tools/src/index.ts:1107`）；仅两个实现：`repeat-tool-reminder`（advisory 不 veto，阈值 `[3,5,8]`）、`tool-call-timeout-policy`（包裹 execute，超时替换为 `TOOL_TIMEOUT`）。
- response 呈现分离：Host 侧 `presentCall`/`presentResult` 返回 `card`-tagged 纯函数（`shell/tool-bash/src/index.ts:101,386`）；**Web 不用它们**，Client 插件把 wire tool name 注册进 keyed slot `tool.call.toolview`（`client/ui-tool/src/client/apply.ts:33-50`），未注册者回退 generic card。
- PTC 模式：`packages/ptc-runtime` 让模型写程序直接调用工具（`ctx.ptcRuntime`，`resolve(request)→run(spec)`）；binding namespace 名须跨语言可移植；失败分类 `exception|timeout|abort|worker-exit|invalid-output|output-limit|protocol|sandbox-unavailable`；Node provider 每程序起新进程，`danger-full-access` 不 confine 否则经 `ctx.sandbox.confine`；Python 实现为 `[实验性]`。
- 工具集来源：`shell`（bash/pwsh + persistent 变体）、`fs`（read/write/edit/read_image + glob/grep + str_replace_editor）、`terminal`（6 工具）、`web`（web_search/web_fetch）、`lsp`、`mcp__<server>__<tool>`、`subagent`/`send_message`/`interrupt_agent`/`list_agents`、`job_output`/`job_list`/`job_kill`、`schedule_*`、`workflow`/`ralph`、`get_goal`/`create_goal`/`update_goal`、`todo_write`、`ask_user_question`、`present`、`session_search`/`session_event_search`/`session_trace`/`session_event_trace`/`session_event_read`、`skill`、`run_in_background` 等。
- 工具名镜像：`tool-bash`/`tool-pwsh` 参数刻意镜像，源码内以 `jscpd:ignore` 标注。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D5.1 | 注册模型 | 静态数组 + 全局单例 | service + effect disposer | 影响测试隔离与多工作区 |
| D5.2 | 暴露策略 | 三档（all/dispatch-only/whitelist，默认 whitelist）+ 权限 allowlist | 无等价三档；由 sandbox/approval 决定 | 语义不同 |
| D5.3 | 工具呈现 | 散在前端 `app.js` | Host presenter 纯函数 + Client keyed slot 双轨 | — |
| D5.4 | 模型调用形态 | 纯 tool call | tool call + **PTC（模型写程序）** 两形态 | B 侧多一种范式 |
| D5.5 | 路径影响分析 | 静态入参抽取（Bash 类只取 cwd） | `fs/write-intent`/`fs/edit-intent` waterfall 占据单决策槽做 read-before-edit 新鲜度检查（`fs-observation-policy/src/index.ts:116-122`） | 两者解决的问题不同 |
| D5.6 | guard | 无 | 2 个（advisory 提醒 + 超时包裹） | — |
| D5.7 | 重名处理 | 插件与内置重名**报错** | 未见全局重名报错（MCP 前缀隔离） | 策略不同 |

---

# 第三部分 模型、配置与凭据

## 领域 6 模型 Provider 与适配

**A 侧** `[完整]`
- Provider 抽象：`ProviderKind` 是 **enum 而非 trait**（10 变体：ClawApi/Anthropic/Xai/OpenAi/ZhipuAi/AlibabaBailian/BaiduQianfan/ByteDanceArk/DeepSeek/Custom，`providers/mod.rs:30-40`）；另有 `trait Provider { type Stream; }`（`:11`）但运行时靠 enum match。`ProviderClient` 是 enum（`client.rs:29-36`），`from_session_endpoint` match 分发（`:38-66`）。
- 新增 provider 成本：需改 `ProviderKind`、`ProviderMetadata` 常量、`PROVIDER_CATALOG`、`MODEL_REGISTRY`、`provider_kind_from_name`、`resolve_model_alias`、`metadata_for_model`、`detect_provider_kind`、`model_token_limit`、`ProviderClient` 与 `OpenAiCompatConfig` 构造函数（`providers/openai_compat.rs:83-163`）——**无注册 API**。
- 目录：`PROVIDER_CATALOG` 8 个 UI 选项；`MODEL_REGISTRY` 约 50 条模型别名映射到 `ProviderMetadata`。
- 流式与 tool call：`StreamEvent`/`ContentBlockDelta` 在 `llm-adapter/src/types.rs:246`；工具调用解析状态机在 `providers/openai_compat.rs:520-770`（`tool_calls: BTreeMap<u32, ToolCallState>`，逐 index 累积 `arguments`，`delta_event()` 产 `InputJsonDelta` `:759-770`）；wire DTO `:798-870`；非流式走 `sse.rs`。
- 参数：`RequestParameters` 包含 `temperature`/`top_p`/`reasoning_mode`/`thinking_budget`（`request_parameters.rs:6-12`）；`reasoning_effort` 在请求对象另传，Qwen3.8原生编码有校验。`max_tokens` 不在该结构，但并非只能走静态表：web 会话 `SessionModelLimitOverride` 已含 `context_window/max_output_tokens`、协议/Endpoint、图片与工具策略（`main.rs:5154-5183`），`effective_model_limit_for` 优先显式会话覆盖再回退目录（`:27719-27733`）。
- 多模型：多会话"接力式群发"按 `session.relay_timeout_ms` 让多个 session 依次回复（`main.rs:5229,16204-16213,16952`）；`vision.rs` 的 `fallback_model` 仅限视觉（`vision-service/src/lib.rs:1323`）。**无 provider 级 fallback、无按任务路由**。
- 嵌入：`api::embed_texts`（POST `{base_url}/embeddings`，可接 Ollama bge-m3，`llm-adapter/src/embeddings.rs:33-70`）。

**B 侧** `[完整]`
- Provider 抽象：`abstract class LlmAdapter`，**唯一强制方法** `stream(options)`（`llm/llm/src/index.ts:266`）；可选覆写 `providerInfo`/`providerRetryPolicy`/`imageRequestPricing`/`listModels`/`resolveModel`/`prepareCall`。注册：`ctx.llm.registerAdapter(providers, adapter)` 返回带 disposer 与**原子 `replace()`** 的 handle（`:288-330,390`）。
- 适配器：`llm-deepseek`（Chat Completions + Messages 两种协议）、`llm-pi-ai`（三种 wire protocol：`openai-completions`/`openai-responses`/`anthropic-messages`，`src/provider.ts:48-50`；provider 目录来自安装的 pi-ai catalog `src/catalog.ts:173-191`，也支持手写 YAML 路由 `src/index.ts:17-55`）。
- 请求扩展：`deepseek-llm-api-extensions` 是 additive request-field 注册表（`session-log-deepseek` 用它挂 `dsh_session_log`）。
- 重试：`llm-retry` 指数退避 + jitter，`ResolvedRetryPolicy` 由各 provider 声明（`llm/src/index.ts:210-212`）。
- token 计量：`token-meter` 固定启发式 `CHARS_PER_TOKEN=4`、`BLOCK_OVERHEAD=4`（`src/estimate.ts:13-21`），`measure()` **优先复用 provider usage 作 anchor**（`src/index.ts:146-180`）；注释声明"不是计费或门控输入"（`src/projection.ts:20-29`）。
- 模型选择：per-session `model/selection` 事件（`core/session/src/known-event-types.ts:46`，写入点 `api/session-controller/src/agent.ts:334`）+ `core/agent-default-model`；UI 有 `client-ui-model-selection` 与 `client-ui-settings-models`。
- **无 provider 级 fallback 链、无按任务路由**。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D6.1 | Provider 扩展方式 | enum match，多点多文件改动，无注册 API | `registerAdapter` + handle（含原子 replace） | A 侧 trait 已存在但未用于运行时 |
| D6.2 | wire protocol 覆盖 | OpenAI-compatible 为主（`openai_compat.rs`）+ `sse.rs` | 3 种（openai-completions/responses、anthropic-messages） | — |
| D6.3 | 请求参数 | 已有思考、采样与会话输出预算等显式覆盖，目录值作为回退；统一配置页已发布 | adapter 自定 + `prepareCall`；extensions 注册表可加字段 | 优先收敛解析与wire校验，不重复建设现有参数页 |
| D6.4 | 重试 | 未找到 provider 级策略 | `llm-retry` per-provider policy + jitter | — |
| D6.5 | fallback/路由 | 无（仅 vision fallback_model + 多会话接力） | 无 | 两侧都缺 |
| D6.6 | token 计费 | `cache_read_input_tokens` 用于计费统计（`core-runtime/src/usage.rs:34`、`session.rs:346`） | 无计费（`token-meter` 明确声明非计费） | A 侧有 usage 统计，B 侧无 |
| D6.7 | 嵌入 | 有 `embed_texts`（可接 Ollama） | 无 | — |

## 领域 7 配置与设置

**A 侧** `[三套并存]`
- ① `AdapterConfig`（providers/models 两个 HashMap）从 `~/.coolzhu/config.json`、`./coolzhu.json`、`./.coolzhu/config.json` **首个非空命中（不合并）**（`llm-adapter/src/config.rs:55-78`）；`load_config` 与 `config_file_paths` 标了 `#[allow(dead_code)]`（`:54,68`）。
- ② `ConfigLoader`（core-runtime）真分层：`User` → `Project` → `Local`，`deep_merge_objects` 累加（`core-runtime/src/config.rs:11-16,213-254`），文件是 `.claw.json` / `settings.json` / `settings.local.json`（`:214-239`）。
- ③ `WorkspaceConfig`（web-console，**实际生效**）：`coolzhu.toml`，15+ section（`main.rs:5111,5117-5142`）。
- 会话级覆盖：`SessionModelLimitOverride` 按 `session_id` 存进 `coolzhu.toml` 的 `session_model_limits`（`main.rs:5146-5183`），`session_model_settings_for()` 读取（`:5185`）。
- 主题配置：`ConfigContextLifecycle`（`main.rs:5256-5319`）、`ConfigScheduledTask`（`:5983-6021`）、vision router pipeline（`:5617`）。
- 规范约束：`docs/development-standard.md` 要求 config-first、**禁新增 `COOLZHU_*`/`CLAW_*` 业务环境变量**、统一 workspace `coolzhu.toml`；但实际存在环境变量（`COOLZHU_WEB_SESSION_DB`、`COOLZHU_LOG_DIR`、`COOLZHU_LOG_LEVEL`、`COOLZHU_CLAWBOT_PROVIDER_KIND` 等）。

**B 侧** `[完整]`
- 单一 `ctx.settings`：namespace schema 分层 = **schema defaults → 注册者 composition `base` → 用户文档 section**（`settings/settings/src/index.ts:1-5`）。存储为单个 YAML/JSON 文档（`settings.yaml`），跨进程写锁 + **保注释的 leaf diff**（`settings-file/src/index.ts:1-5`）。
- 显式默认：**`resolve(request): Spec` 是一等公民**。三个实例——`sandbox-policy/src/index.ts:164-171`（`request.mode ?? session override ?? defaultMode`）、`shell/src/index.ts:85`（`run(spec)` 只接受 resolved spec，"never a raw request"）、`settings-file/src/index.ts:44`（"Fully resolved provider parameters; **defaulting happens here, never inline**"）。
- launch environment：`util/launch-environment` 层序 process → 调用目录 `.env` → `$DSH_HOME/.env`，**SSH 启动不采纳 .env**（`src/index.ts:13-18,121`）；`http-proxy` 读 `http_proxy/https_proxy/no_proxy/all_proxy`，loopback 直连，替换全局 dispatcher，**代理名只取自 launcher 快照**。
- 配置写作：`api-settings-controller` 暴露 `@Remote` 读写 + native 打开（`src/index.ts:116-225`）；命名空间在 provider 缺失时仍注册并返回**可操作的配置错误**（README:314）。
- `Config` 字段校验：`--dump-config` / `-dump-default-config` 预览（`boot/app-boot`）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D7.1 | 配置层数 | 3 套并存（其中 2 套含 `#[allow(dead_code)]`） | 1 套（schema defaults → base → user） | A 侧需先收敛 |
| D7.2 | 默认值归属 | 散落的 `unwrap_or_else` 链 | 显式 `resolve(request): Spec` | 语义可对齐 |
| D7.3 | 文件格式 | `coolzhu.toml`（生效）+ `config.json` + `.claw.json`/`settings.json` | `settings.yaml`（保注释 leaf diff） | — |
| D7.4 | 并发写 | 有进程内 `Mutex<WorkspaceConfig>` 与 `mutate_workspace_config`（`main.rs:6675,6692`）；跨进程文件写锁尚需核实 | 跨进程写锁 | 需分别验收进程内序列化、原子落盘与跨进程竞争 |
| D7.5 | 环境变量 | 规范禁业务环境变量，实际存在多处 | 显式分层（process/调用目录/home），启动关键变量禁止来自文件 | B 侧更严格 |
| D7.6 | 会话级覆盖 | `session_model_limits` 写回 `coolzhu.toml` | per-session `model/selection` 事件（落 session log） | A 侧覆盖写配置文件，B 侧写日志 |

## 领域 8 凭据与密钥

**A 侧** `[部分]`
- API key：适配器的默认入口可读取环境变量；web 产品会话还支持 `api_key_ref` 的内联字面值与文件引用（`main.rs:29913-29941`），统一参数页可保存且空输入默认保留旧密钥（`model_settings.js:186`）。不能概括为全部从 env 读取；`looks_like_secret` 仍是前缀/长度启发式。
- **无加密、无凭据存储库**。OAuth token（MCP 用）存 `credentials.json` **明文 JSON**（`core-runtime/src/oauth.rs:258,276`，585 行实现）。OAuth 配置经 `McpClientAuth::from_oauth`（`mcp_client.rs:108-110`）。
- 脱敏：`api_key_status` 只返回字符串常量"配置"/"未配置"（`main.rs:28585,28740-28811`）；有一条测试**硬性禁止**源码出现截断前缀逻辑（`assert!(!source.contains("first_4"))`，`main.rs:75052-75058`）。
- 忽略规则：`.gitignore` 拦 `credentials`/`secrets`/`token-cache`/`*.pem|key|pfx|p12`；`scripts/package-safety.ps1` 对 text 白名单逐行扫 credential/bearer/url-credential/private-key 四类正则（含 test/dummy/example 豁免，`:40-43`）。

**B 侧** `[完整]`
- seam：`ctx.credentials` —— **两个正交 key 空间**：`CredentialRef`（Branded）回答"环境变量名背后是什么"，分层于进程环境 + provider store + `.env`；`CredentialKey`（Branded）回答"此插件为此 id 持有什么凭据"，**无环境可叠，记录存在即是全部事实**，故 `modifyRecord` 是唯一写路径（读-决定-替换在同一锁内，用于 token refresh）（`credentials/src/index.ts:155-168`）。
- seam 级规则：空存储值到处视为 absent（`resolve` 跳过、`describe` 报未配置，`:159-161`）。解析是**逐操作**的，消费者不得跨操作缓存（`:178`）。
- 记录模型：`CredentialRecord = ApiKeyRecord | GrantRecord`，`GrantRecord.value` 对 seam 与其他插件不透明（`types.ts:37,52,55,60`）。事件 `credentials/reference-updated`/`credentials/record-updated`（`types.ts:90,102`）。
- provider：`credentials-local` 分层信任序 = 继承进程环境（只读，胜）> 调用目录 `.env` > `$DSH_HOME/.env`；文件 `.credentials.yaml`（`DOCUMENT_VERSION=1`）；POSIX 上若含 group/other 位则**启动失败**并提示 `chmod 600`（`src/index.ts:128-142`），**Windows 跳过该检查**（`:121,138`）；用 `parseDocument` 保留注释；`watch` 默认 true 热发布外部编辑，settle 100ms。
- 脱敏：`redactSecrets()` 按 schema 的 `role('secret')` 删字段，返回 `{ value, secrets[{ path, set }] }`（`settings/settings/src/redact.ts:79-98`）；**已知缺口**：union/transform 里的 secret 会被原样返回且有 TODO（`:61-63`）。
- 授权流：`authorization` seam —— **不是内置 OAuth**，而是"插件拥有授权流"：`flows: Map<CredentialKey, AuthorizationFlow>`，每 key 一个流、重名冲突（`authorization/src/index.ts:186-202`）；`AuthorizationSession` 每个成员限定在**一次 attempt**，流既不知道也不选择哪个 surface 在监听（`:87-90`）；**无 shipped flow**（无内置 OAuth 实现）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D8.1 | 存储 | API key 可来自适配器环境、会话字面值或文件引用；OAuth 明文 JSON | `.credentials.yaml` 实际存 refs 对应值和 tagged records；继承环境 > 托管凭据文件 > 调用目录.env > home.env（`credentials-local/src/index.ts:2-24,443-463`） | 两侧均需明确凭据来源、写入、脱敏与迁移，B并非只保存无密钥引用 |
| D8.2 | 文件权限 | 无（未见 0600 检查） | POSIX 强制 0600，否则启动失败；Windows 跳过 | — |
| D8.3 | 脱敏 | 二值状态 + 禁打印前缀（测试强制） | 结构化 redaction（删值留结构），有 union/transform 缺口 TODO | 两种思路不同 |
| D8.4 | 授权流 | `oauth.rs` 585 行实现（MCP 用） | `authorization` seam 抽象但**无 shipped flow** | 两侧正好互补 |
| D8.5 | 加密 | 无 | 无（仅文件权限） | **两侧都没有加密** |

---

# 第四部分 权限与安全

## 领域 9 权限、审批与沙箱

**A 侧** `[完整，三套档位]`
- 档位：`enum PermissionMode`（5 档：ReadOnly/WorkspaceWrite/DangerFullAccess/Prompt/Allow，kebab-case 序列化，`core-runtime/src/permissions.rs:7-13`）+ `enum PermissionProfile`（3 档，**实际使用**：WorkspaceAuto 默认/OutsideApproval/FullAccess，`:30-34,59-63`）+ chat room 三档字符串 `read-only`/`workspace-write`/`full-access`（`main.rs:2270-2272`）。**无 plan 模式**。
- 判定：`PermissionDecision` 五态（AllowAuto/AllowApproved/RequireApproval/RequireConfirm/Deny，`core-runtime/src/tool.rs:93-99`）；`PermissionPolicy::authorize(tool, input, prompter)` 三态（`permissions.rs:129-174`）；闸门主入口 `evaluate_permission(invoke, required, targets, workspace_root, protected_rules, session_grant, profile)`（`permission_gate.rs:208`），顺序：FullAccess 短路放行（`:242-251`）→ **Protected 命中优先**（直接 RequireConfirm，`:254-268`）→ 按 profile 分级（`:271-311`）。
- 审批 UI：`PendingApprovalRecord` + `/api/tools/pending` + `/api/tools/approve|reject`（`main.rs:1475,2572-2610,4738`）；**TTL 超时视为 Deny**（`:2264`）；提权到 full-access 需 `risk_acknowledged=true` **且** `confirmed_twice=true`（`:4721-4731`）；会话级授权缓存按 `(workspace_id, session_id, tool_name)` + `Instant` 截止（`:2219` 附近）。
- 路径门控：`path_effect.rs` 静态抽取 + 闸门判"是否在 workspace 内"（见 D5.5）。
- 沙箱：**无进程沙箱**。Computer Use 有预算与熔断（见领域 19），但无内容安全。
- 审计：`ToolAuditEntry` JSON Lines 追加（`main.rs:4440-4489`），字段含 ts/call_id/tool_name/caller/workspace_id/session_id/input_summary/status/elapsed_ms/**完整 permission（required/decision/reason/workspace_relative/protected_match/affected_paths）**/summary_text/evidence；**放行与拒绝都落审计**（`tool.rs:127` 注释，调用点 `main.rs:2516,2558,4945,5002,5102`）；另有 `computer_use_audit_log_path()`（`:22430,22461`）与 `clawbot_group_audit` 表（`clawbot_gateway.rs:478`，仅 append）。

**B 侧** `[完整，两正交旋钮]`
- 模型：**sandbox mode × approval policy 两正交维度**。sandbox mode = `read-only`（**默认**，fail-safe）/ `workspace-write` / `danger-full-access`（`sandbox-policy/src/index.ts:112-117`）；approval policy = `ask`（默认）/ `never`（`user-approval/src/index.ts:60`）。`permission-presets` 绑成用户可见选项（默认表只有 `workspace-write`=workspace-write+ask 与 `danger-full-access`=danger-full-access+never；`custom`/`auto` 是保留名，`permission-presets/src/index.ts:179-196,208-213`），写入路径是 `/permission` 命令（`:265-286`）；preset 定义 `{sandbox, approval, name?, description?}`（`:61-69`），切换写 durable `permission/preset`（`:57`）。
- 审批实现：`ctx.approval.request()` **必须在 open turn 内**（`user-approval/src/index.ts:210-216`，否则抛错，因审计要求 `approval/asked`+`approval/decided` 落在 turn 的 commit/replay 边界内）；**`'never'` policy 在 dispatch 之前由 service 自己决定**（`:263-268`）——注释说明这是**故意的**：一个 `prepend:true` 的 listener 会排在任何 gate listener 前面，所以 listener 形状的 gate **无法保证与注册顺序无关的确定性拒绝**；失败一律 fail closed → `'unavailable'`（`:278-285`）；**`allowed-once` 是唯一 grant**（`:206`）。
- 执行流水线：`tools/pre-execute` waterfall（hooks/permission/sandbox）→ **monotonic guards**（只能 deny 或 abstain，identity 受保护）→ approval one-shot → `tools/execute`。
- 沙箱 provider：`sandbox-local` 平台 runner 链——Linux 先 bwrap 后 Landlock、macOS Seatbelt（`sandbox-exec -p`）、Windows ACL restricted-token runner；参数在 `profiles.ts:15-44`（bwrap `--ro-bind / /` + workspace-write 时 `--bind <root>`；Seatbelt 共享 `writableRoots` 免得与 `dsh-fs-sandbox` 漂移）；**confinement 缺失或不可用则 fail closed，不返回原始 argv**（`:3-4`）。`sandbox-windows-acl` 是独立包（10 文件，含 FFI/token/ACL/workspace SID）。`sandbox-ssh` 把 `{argv, policy}` 发远端 helper 拿 `ConfinedArgv`。
- 提权：`sandbox/src/escalation.ts:24-31` 定义**严格更宽表** `WIDER_MODES`（read-only→{workspace-write, danger-full-access}，workspace-write→{danger-full-access}）；检查发生在**执行时，绝不烘进 tool schema**（`:22-24`，理由：schema 是 registry-global 而 effective mode 是 per-call）；`sandbox_permissions` 与 `justification` 必须成对且 justification 非空（`:44-58`）。
- 文件层：`fs-sandbox` 覆写 `sandboxMode` 与 writeText/editText，先按 per-call policy 检查 target 再委托 super，读透传（`fs-sandbox/src/index.ts:65-134`）；`fs-observation-policy` 做 read-before-edit 新鲜度（见 D5.5）。
- 实验：`experimental-auto-review` 做 per-tool LLM 授权审查。
- 插件边界：见 D2.4。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D9.1 | 档位模型 | 3 套并存（5 档 enum / 3 档 profile / 3 档字符串），语义重叠 | 2 正交维度 + 命名预设（默认表 2 项） | 需先约定映射表 |
| D9.2 | 默认档位 | `WorkspaceAuto`（工作区可写） | `read-only` + `ask`（fail-safe） | 默认安全边界不同 |
| D9.3 | 拒绝的时机 | 执行前判定，但走 hook 链 | **dispatch 前由 service 决定**，理由明确（listener 顺序不可靠） | B 侧有显式理由记录 |
| D9.4 | 授权粒度 | 会话级缓存 `(workspace, session, tool)` + TTL | `allowed-once` 唯一 grant | A 侧更宽 |
| D9.5 | 保护路径 | FullAccess先短路；Protected规则仅优先于普通workspace自动放行（`permission_gate.rs:241-268`） | 无同构Protected规则（依靠sandbox writableRoots等执行边界） | 若改变FullAccess与Protected的关系，必须作为显式行为变更 |
| D9.6 | 提权确认 | `risk_acknowledged` **且** `confirmed_twice` 双确认 | `WIDER_MODES` 严格更宽表 + `sandbox_permissions`/`justification` 成对 | 两者互补 |
| D9.7 | 进程沙箱 | **无** | 4 平台后端（bwrap/Landlock/Seatbelt/ACL token）+ fail-closed | B 侧独有 |
| D9.8 | 审计 | ToolAuditEntry（放行+拒绝+affected_paths）+ CU 审计 + 群聊审计 | 经 session event（`approval/asked`+`approval/decided` 落在 turn 边界内） | A 侧审计更独立完整 |
| D9.9 | plan 模式与权限 | 无 plan 模式 | `plan-mode` 是独立协作状态，**与 sandbox/approval 独立生效，不读不写 plan 状态**（`plan-mode/src/index.ts:3-4`） | 语义不同：plan 不是权限档 |
| D9.10 | guard | 无（有 CU 预算/熔断） | 2 个：advisory 重复提醒、超时包裹（协作式，硬停不可能） | — |

---

# 第五部分 状态与上下文

## 领域 10 会话持久化与版本迁移

**A 侧** `[完整]`
- 权威存储：**SQLite**（rusqlite）。默认路径 `default_session_sqlite_path()`，文件形如 `.coolzhu/web-sessions.sqlite3`（`permission_gate.rs:592` 测试可见）。工作区作用域默认 `<workspace>/.coolzhu`，可被 `coolzhu.toml` 的 `paths.data_dir` 覆盖（`main.rs:9394-9397`）。
- 表：`sessions`、`chat_rooms`、`session_messages`、`chat_room_messages`、`chat_room_permissions`、`runtime_runs`、`runtime_run_events`、`goals`、`goal_phases`、`memory_*`（6 张）、`attachment_refs`、`clawbot_*`（8 张）（`main.rs:39653-40717`）。
- 迁移：手工链式 v2→v20（`main.rs:39733-39751`），`PRAGMA user_version`，**无统一 schema 常量**，每步独立函数（`apply_session_migration_v12` `:39755` … v20 `:40003`）。
- 另有 `Session` JSON 结构带自己的 `version: u32`（`core-runtime/src/session.rs:48`，`new()` 固定为 1，`from_json` 缺失即报错 `:125-130`）；CLI 的会话持久化走 `Session::save_to_path` → `sessions_dir()`（`command-line/src/main.rs:2019`）。
- legacy 并存：`web-sessions.sqlite3`（主）与 `web-sessions.json`（legacy，`main.rs:9367,9373`）。
- 无备份/清理/保留策略代码（`api_system_self_update_plan` `main.rs:8915` 只返回静态 plan 文本）`[stub]`。

**B 侧** `[完整]`
- 权威存储：**append-only 事件日志**。`SESSION_FORMAT_VERSION = 3`（`core/session/src/types.ts:88`）；物理文件 `session[.vN].jsonl[.zstd]`，默认 checksummed Zstandard 帧，`compression:'none'` 时明文（`session-persistence-jsonl/src/index.ts:88,96,98,236,237`）。
- 磁盘布局：`<root>/--<normalized-cwd>--/<encoded-id>/session[.vN].jsonl[.zstd]`，session id 单射转义为安全路径段。
- 写入：header+首批 temp-write→fsync→原子 publish（无覆盖，POSIX 用 `link()` + 目录 fsync，`:1097,1145,1229`），Windows 走原生 write-through 命名空间操作（`src/win32.ts:134,183`）；后续每批 append 后 fsync，失败回滚文件长度（`:1242,1286`）。
- 跨进程租约：POSIX 在 `session.lock` 上 `flock(2)`（inode校验+重试）；Windows 不创建 lock 文件，而持有由该路径派生的命名内核信号量（`session-persistence-jsonl/src/lease.ts:1-12,36,78-85`，`win32.ts:151-160`）。因此“无lock文件”不等于“无跨进程租约”。浏览器worker使用单进程替代路径，仍需各平台实测。
- torn tail：末行不完整丢弃；torn zstd 帧只取可解出的完整记录，写 handle 截断并耐久重写后才发新批。
- 迁移链：**相邻步强制** `to === from + 1`（`session-format/src/chain.ts:26,30`），构造期拒绝重复 from、重复 name、缺口、超出 current 的边（`:51-79`）。三个冻结迁移包：`v0-to-v1`（identity + legacy normalization）、`v1-to-v2`（assistant stream 嵌入 + 全量 seq 重映射）、`v2-to-v3`（header/agentPreset/system message 插入/PTC 词表/envelope 规范化）。
- catalog：`generated.ts:16,18` **build-static 静态列出** 4 个 codec + 3 条边，模块初始化即校验完整无缝链；运行期插件**无法补缺边**。
- 版本策略：`readHeader()` 不读事件即返回 `current|migration-required|unsupported|malformed`（`catalog.ts:45`）；`storedVersion > currentVersion` 返回 `unsupported`（**拒绝而非降级**，`:52-58`）。
- projection 缓存：`session-projection-cache` 持久化 checkpoint，冷读免加载全日志；`checkpointIdentity` 绑定 `formatVersion/createdAt/cwd/isSeeded/inheritedEventCount`（`src/spec.ts:44`）；三个强制写点（session 创建 / `turn/end` / `session/disposed`）+ count/interval 节流（`src/index.ts:300-340`）；每写 fail-soft。
- checkpoint 策略：`session-checkpoint-policy` 在 `llm/stream`（首 chunk 前）、`tools/execute`（仅顶层）、`agent/pre-step` 三处 flush。
- SQLite 角色：仅**派生**——`session-query-sqlite`（`SESSION_QUERY_SQLITE_SCHEMA_VERSION = 8`，FTS5，版本不符则 `resetDerivedSchema`）与 `storage-sqlite`（`SCHEMA_VERSION = 1`）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D10.1 | 权威介质 | SQLite 关系表 | append-only JSONL(+zstd) | **最根本的差异**，决定所有上层 |
| D10.2 | 迁移机制 | 手工 v2→v20，无统一常量，无顺序强制 | 相邻步强制（`from+1`）+ 每步独立包 + 静态 catalog | A 侧易漏步/乱序 |
| D10.3 | 版本语义 | `PRAGMA user_version` + `Session.version` 两套 | 单一 `SESSION_FORMAT_VERSION=3` | — |
| D10.4 | 未知/超前版本 | 未找到策略 | `unsupported` 拒绝，不降级；未知事件需 `ignorable` | — |
| D10.5 | 崩溃/撕裂 | SQLite 事务保证 | torn tail 丢弃 + zstd 帧部分解码 + 截断重写 | 两侧机制不同 |
| D10.6 | 并发写 | SQLite 锁 | POSIX flock / Windows 命名内核信号量 + 单写者契约 | 两侧都有写者互斥实现，需分别验证进程崩溃与竞争场景 |
| D10.7 | 冷读优化 | 无（直接查表） | projection cache + cold reads | — |
| D10.8 | 备份/保留 | 无 | 无删除/保留 API；`list()` 无分页无过滤（`session-persistence` 缺口） | 两侧都缺保留策略 |

## 领域 11 上下文组装与压缩

**A 侧** `[完整]`
- 预算：`ConfigContextLifecycle`（`main.rs:5256-5283`）：warning 80%、auto_compact 80%、history budget 70%、memory budget 8%、output reserve 15%、prompt safety 1024 token、图片按 512 token 估算（`:5285-5319`）。
- 压缩：把旧消息压成一条摘要并**作为 memory bead 回灌**（source `context:auto-compact:v2`，`:26937-27012`）。
- 组装期硬预算循环：先丢记忆 bead，再截断用户输入（`:27143-27184`）；历史/记忆各自按预算切分（`:27815-27817`）。
- system prompt：`build_agent_system_prompt_with_beads`（`:26798-26812`）拼 "You are a COOLZHU AGENT…" + 工具政策 + 记忆 beads；硬规则**内联**在 `AGENT_GUIDE_INLINE`（`:26814-26825`），注释明确说明"**AGENT.md 未被 include、运行时工作区也无该文件，仅按名引用等于不生效**"。
- **未发现 AGENTS.md/CLAUDE.md 自动注入机制**（全仓源码无相关读取代码）。
- 污染过滤：注入前剔除旧"工具不可用/失败"类 bead（`:26855-26884`）。
- 阶段 prompt：每 phase 单独组装（goal/phase/role/required skills/output artifacts/verification/retry 上下文，`:44360-44395`）；phase prompt 硬上限 48000 字符（`:44458`）。

**B 侧** `[完整]`
- system prompt：`ctx.systemPrompt` registry（`core/system-prompt/src/index.ts:405-471,559`），section 按 `order` 拼接，`complete: true` 的 section 可整体接管，`includeHarnessIdentity`/`personaPrefix` 可配（README:40-50）。
- 指令注入：`context/agent-instructions` 把 AGENTS.md/CLAUDE.md 作为 **durable `user/message`**（`<system-reminder>` 框架）注入，**不进 system prompt**；`maxBytes` 默认 65536，项目根由 `.git` 标记，**仅在 `read`/`write`/`edit` 触达更深目录后刷新**（README:32,86,106）。
- 压缩 5 包：`compaction`（seam + checkpoint）、`compaction-basic`（压力触发 `agent/pre-step`，`contextWindow*0.8` 默认、`retainRatio` 0.16、`modelPolicies[]` 按模型覆写、`auto` 开关；溢出触发 `agent/request-error` 的 `CONTEXT_WINDOW_EXCEEDED_CODE`，绕过阈值直接压，`maxOverflowRetries` 默认 1；手动 `/compact` 要求 idle agent + `runMaintenance`）、`compaction-tool-result-pruner`（`thresholdChars 8192/head 4096/tail 1024`，中段标记替换，**零模型调用**）、`compaction-image-offload`（超预算旧图**永久**替换为"附件名+只读路径"文本，不经 provider retry 预算）、`command-compact`。
- 压缩重建：**不删日志**，追加 log-only 事件 `compaction/start|summary|end` + **紧随其后的替换 `user/message`**（checkpoint marker `{kind:'plugin',plugin:'compact'}`，`compaction/checkpoint.ts:17-38`）；`shadowedRange` 是**surface 位置跨度而非 seq 区间**（`types.ts:107-117`）；摘要由单次 `ctx.llm.stream()` 生成、**复用原 system prompt/tools 以保 KV cache**（`compaction-basic/src/index.ts:223-243`）。
- token 估算：`CHARS_PER_TOKEN=4`、`BLOCK_OVERHEAD=4`（`token-meter/src/estimate.ts:13-21`）；`measure()` 优先复用 provider usage 作 anchor（`src/index.ts:146-180`）。降级链：未超阈→不压；超阈→先 model-free prune→仍超→摘要压缩（`compaction-basic/src/index.ts:301-322`）。
- 其他 context 插件：`file-reference`（`@file` 语法 + 候选 seam，本身零 FS 访问）、`file-reference-local`（按 agent 本地 workspace 排序、有界发现、工具活动后刷新、不跟随目录 symlink）、`session-reference`（`@label` → canonical URI，抓有界只读快照作为 durable **untrusted** 上下文，附固定禁令警告，渲染预算 64 KiB 下限）、`time-context`（opt-in，每 eligible step 注入当前时间 + 浏览器时区 + 距上条模型可见消息的 elapsed）、`tmux-context`（opt-in，仅 turn 首步且位置变化时注入）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D11.1 | AGENTS.md 注入 | **无**（注释自承引用不生效） | durable user/message + system-reminder 框架 + `.git` 定根 + 触达深目录才刷新 + maxBytes | A 侧需补 |
| D11.2 | 压缩触发 | 阈值（warning/auto_compact 80%） | 三触发：压力 / **provider 溢出报错兜底** / 手动 `/compact` | B 侧有溢出兜底 |
| D11.3 | 压缩结果处理 | 压成摘要 + 作为 memory bead 回灌 | 追加 log-only 事件 + 替换 user/message + checkpoint marker；**不删日志** | 可重建性不同 |
| D11.4 | 摘要请求 | 未指定 prompt 复用 | **复用原 system prompt/tools 保 KV cache** | B 侧显式优化 |
| D11.5 | 剪枝 | 无独立剪枝阶段（只有预算切分） | `tool-result-pruner` 零模型调用 head/tail 剪枝 + 图片永久卸载 | — |
| D11.6 | prompt 组装 | 函数拼接 + 内联硬规则 | registry + section order + `complete` 接管 + persona/prefix | — |
| D11.7 | token 估算 | 预算按 token 硬编码 | 启发式 + provider usage anchor | — |

## 领域 12 记忆

**A 侧** `[完整]`
- 模型：SQLite "beads" 分 **L0–L4** 层（`core-runtime/src/memory.rs:6-37`，kind→layer 映射 `:39-53`）；`MemoryBeadView` 含 `valid_until`/`last_accessed_at`/`access_count`（`:71-100`）。
- 表：`memory_beads`（`main.rs:39702`）、`memory_vectors`（v7，BLOB 向量按 `model_id` 区分，`:40607`）、`memory_access`（v8，召回时间/命中次数，`:40631`）、`memory_meta`（v9，`entity_key` + status superseded，`:40655`）、`memory_edges`（v10 相似边，`:40717`）、`memory_settings`（含 polluted 状态，`:39886`）、`memory_jobs`（extraction/edges/consolidation，`:39905`）。
- 写入：每轮对话后 `persist_auto_memory_beads`（`main.rs:15967,16446,16478,16689,17015,18267`）；决策 `evaluate_auto_memory_candidate`（`:27017-27063`）——**只沉淀有成功证据的内容**（含 pass/verified/通过/验证 等词，`:27070-27100`），`assistant-fallback` 与失败/调试类**永不沉淀**；写前去重/取代 `decide_memory_write`、`find_supersede_target`（`memory.rs:229-300`）。
- 检索：`query_memory_beads`（`memory.rs:461-527`）先关键词 AND 精确匹配，**无召回时回退**到词重叠 + `hash_embed` 余弦（阈值 lexical>0.5 或 semantic>0.30）。`hash_embed`/`cosine_similarity`/`BruteForceCosineIndex`（`semantic.rs:38-50,112,124`）是**离线词袋哈希**——注释自陈"hash 嵌入是词袋级 lexical，真正同义词级语义需接 /v1/embeddings"（`memory.rs:497-503`）。
- 真语义路径：`api::embed_texts`（POST `{base_url}/embeddings`，可接 Ollama bge-m3，`llm-adapter/src/embeddings.rs:33-70`），仅在语义记忆开启时调用（`main.rs:25483`），向量落盘复用（`:25497-25588`）。
- 衰减/过期：`effective_recall_score`、`is_memory_bead_expired`、`rule_based_valid_until`（`memory.rs:405-446`）。
- 隔离：`memory_beads` 主键含 `session_id` → 按 session 隔离；Goal 阶段有"任务级临时记忆，完成后清理"（`main.rs:36487-36496,36737`）。

**B 侧** `[未实现]`
- `find packages -iname '*memor*'` 仅命中测试辅助文件（如 `credentials/tests/memory.ts`、`storage-domain/tests/helpers/memory-backend.ts`）。**无 memory / recall / embedding / 向量库 package**。
- 无长期记忆写入路径、无检索、无衰减、无跨会话记忆。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D12.1 | 长期记忆 | L0–L4 beads + 取代链 + 衰减 + 污染过滤 + 只沉淀成功证据 | **无** | A 侧独有资产 |
| D12.2 | 向量检索 | `hash_embed`（词袋，fallback）+ `embed_texts`（真语义，opt-in） | 无 | — |
| D12.3 | 与压缩的关系 | 压缩产物**回灌为 memory bead** | 压缩产物是 log-only 事件 + 替换消息 | 语义耦合方式不同 |
| D12.4 | 作用域 | 按 session_id 隔离 + Goal 临时记忆 | — | — |

## 领域 13 文件系统、附件与大输出

**A 侧** `[部分]`
- 附件：`attachment_refs` 表 + `attachment_store_dir()`（`main.rs:40386,25101`）；删除时无引用 GC（`:38144-38230`）。
- **无大输出外置（spill）**；**无文件读取缓存**（无 mtime 级 read cache）。
- IDE 索引：`ide-index/` 符号索引产物（`main.rs:7487-7489`），仅 IDE 索引做 mtime 增量重解析（`:7518-7581`），与 agent 的文件读取无共享。
- 桌宠拖放：`/api/pet/drop-files` 把文件移入 workspace 附件目录，经 `/api/pet/pending-attachments` 队列交给前端（`main.rs:4107-4129`）。
- 文件预览：有（`index.html:154-164`）；聊天内无工具改动 diff 预览。

**B 侧** `[完整]`
- fs seam：`ctx.fs` 方法集 `resolve/processPath/fileUrl/contains/stat/lstat/readText/streamText/readBytes/readByteRange/listDir/writeText/editText`（`fs/fs/src/index.ts:116-271`）；事件 `fs/write-intent`、`fs/edit-intent`（waterfall 单槽决定）与同步 `fs/observed`（`:57-78`）；基类 `sandboxMode` 返回 undefined（`:86-111`）。
- 实现：`fs-local`（宿主）、`fs-sandbox`（覆写 sandboxMode + writeText/editText 先检查 target 再委托）、`fs-ssh`（`SshFileSystem extends FileSystem`）。
- 新鲜度策略：`fs-observation-policy` 用 `WeakMap<actor, Map<targetKey, FsObservation>>`；writeIntent 未观测→`createIfAbsent`、已观测 present→`replaceIfVersion`；editIntent 未观测→`FS_NOT_OBSERVED`、已观测 absent→`FS_NOT_FOUND`（`src/index.ts:65-88`）。
- 工具：`read(file_path,offset,limit)`、`read_image`（输出 image block + attachment）、`write`、`edit(old_string/new_string/replace_all)`、`glob`、`grep`（打包 ripgrep）、`str_replace_editor(view/create/str_replace/insert)`（独立兼容工具）。
- spill：`ctx.spillStore.saveText()` 只"存全文 + 返回 locator"，不持策略（`spill/spill/src/index.ts:8-55`）；`spill-local` 落 `<tmp>/dsh-spill-*/session-<hash>/`（`src/store.ts:16,78`）；`spill-policy` 是 `tools/post-execute` 变换器，**仅当配置 `maxInlineBytes` 才生效**，超限→预览 + locator，存储失败保留原文（`src/index.ts:61-71,105-179`）。`bash-local` 的 `maxOutputBytes=64000`/`maxSpillBytes=64MiB`。
- attachment：`ctx.attachments` 抽象；`attachment-local` 内容寻址 `sha256:<64hex>` → `<DSH_HOME>/attachments/v1/objects/<xx>/<digest>`，**hard-link 发布 + digest 校验去重 + 只读**（`src/store.ts:52-53,109-115`）；编码阶梯 85/75/60（alpha→WebP、opaque→JPEG）；模型请求变体按 `(ref,target)` 缓存于 `request-images/`（`src/request-image.ts:120,188-192`）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D13.1 | 大输出外置 | **无** | spill（policy 变换器 + 本地存储，可配） | A 侧缺，属易补项 |
| D13.2 | 附件寻址 | `attachment_refs` 表 + 目录 | 内容寻址 sha256 + hard-link 去重 + 只读 + 编码阶梯 + 请求变体缓存 | — |
| D13.3 | 读前新鲜度 | 无 | `fs-observation-policy`（read-before-edit 的 no-clobber） | — |
| D13.4 | 文件读取缓存 | 无 | 无（仅 attachment 变体缓存） | 两侧都无 |
| D13.5 | 文件服务 seam | 无抽象（直接 std::fs） | fs seam + local/sandbox/ssh 三实现 | B 侧可换后端 |
| D13.6 | Diff 预览 | IDE 内 diff；聊天内无 | `workspace-changes`（git turn 首尾快照 + 整文件捕获）→ `workspace/changes` 事件 → changed-files card + 右栏 changes-review tab | — |

---

# 第六部分 编排、知识与外部工具

## 领域 14 子 agent、编排、后台任务、定时与触发

**A 侧** `[完整，一处未接线]`
- 未接线：`coolzhu-orchestrator` 插件 = `VecDeque<Task>` + `HashMap<Agent>`，优先级 Critical/High/Normal/Low、`dispatch()` 按空闲 agent 能力匹配、失败重试 3 次、超时释放、`max_concurrent=5`（`.coolzhu/plugins/coolzhu-orchestrator/src/lib.rs:59-164,166-220`）；**无 DAG、无持久化、无状态机，运行时无任何调用方** `[未接线]`。
- 产品编排 = **Goal 子系统**（DAG）：
  - 模型：`goals`（max_iterations/current_iteration/status/plan_json）+ `goal_phases`（`assigned_role`/`status`/**`depends_on_json`**/`skills_json`/`output_artifacts_json`/**`verification_json`**，`main.rs:40478-40510`）；v13 增 `retry_count=0`/`max_retries=2`（`:39794-39810`）；v15 增 `requires_human_ack`/`human_ack`（`:39823-39836`）；`goal_role_configs`（session_id/role/commander/heartbeat_timeout/task_timeout，`:40538-40556`）。
  - 依赖判定：`dependencies_ready = phase.depends_on.iter().all(status == "completed")`（`:46705-46711`）。
  - 派发决策：`ready_to_dispatch`/`wait_dependency`/`blocked_retry_exhausted`/`blocked_missing_role_session`/`blocked_role_stuck`/`blocked_role_offline`/`awaiting_human_ack`（`:46700-46768`）；全局刹车：迭代预算耗尽不再派发（`:46689-46696`）。
  - 默认计划：线性 plan→implement→verify（`:33121-33145`）。
  - 隔离：每 role 是**独立 agent session**（`assigned_session_id`）；记忆 bead 按 session_id 隔离；每 phase 单独组装 prompt + 任务级 skill overlay + 临时记忆完成后清理（`:36487-36496,36737`）。
  - 跨 session 交接：`chat_handoffs`（from/to/intent/depth/status），v14 增结构化 `contract_json`（verdict/reason/evidence/retry_count）（`:40444-40463`、`:39815-39822`）。
  - 执行：`run_goal_loop_background`（`:15710-15838`）循环取 `next_runnable_goal_phase_id`（仅返回 running 态，`:43353-43358`）、查 `cancel: Arc<AtomicBool>`、遇 paused/cancelled/completed 退出，**阶段间可停**；每次 dispatch 前刷新 role 心跳（`:15752`）。
  - 进度：`GoalLoopStatusResponse{running,stop_requested,max_steps,completed_steps,stopped_reason}`（`:49986-49997`）；事件写 `goal_events` 与 `runtime_runs/runtime_run_events`。
- 定时：`ConfigScheduledTask` 支持 `once`/`interval`/`daily`/`weekly` + 星期 + 时区偏移 + `task_kind=poll|goal`（goal 型每次触发推进绑定 goal 一个阶段，完成后停止）（`main.rs:5983-6021`）；守护调度器 **30s tick**（`:8480-8498`）；防漂移对齐 `next_aligned_run_at`/`next_wallclock_fire`（`:8218-8222`）；执行时**临时授 full-access 并事后回收**（`:8500-8540`）。
- **无 webhook、无 cron 表达式**（`grep webhook|cron` 在 core-runtime/web-console 零命中）。
- Goal skill：3 个 `coolzhu-goal-*` 作为 baseline 无条件注入同一段 phase prompt（`main.rs:44400-44416`）：`session-chain`（锚定 workspace/房间/session/artifact 路径与 phase 交接）、`tool-execution`（工具 schema 纪律、Windows/PowerShell 规范、临时脚本放 `tmp/`、输出重定向 `tmp/logs/`、命令显式超时）、`model-reasoning`（scope 收敛、上下文近阈值时保留决策/路径/错误、重试前先诊断、三次等价失败即停并报阻断）。

**B 侧** `[完整]`
- subagent（10 包）：seam `ctx.subagents` 为命名 provider 注册表（重名抛 `DUPLICATE_PROVIDER`）；`SubagentCapabilities{agentOptions,outputSchema,depthLimit,toolFilter,persona}` 与 `inheritsParentContext`；provider = `spawn`（无 seed，`inheritsParentContext=false`）/`fork`（**用父的已完成 turn 作一次性 seed**，`completedTurnPrefix` 取到最后一个 `turn/end`，in-flight turn 排除）/`acp`/`claude-code`/`codex`/`dsh-sdk`；**并发 `maxActiveSubagents` 默认 8（仅限 continuable 池）、`maxDepth` 默认 1**；深度取 `max(session.header.delegationDepth, AgentOptions.subagentDepth)` 单调不可降。
- continuable 子：一个 durable Session + 至多一个 process-local activation，**inbox 是唯一 turn 队列**（`continuation.ts:7-10`）；`create`/`send`/`steer`/`interrupt(targetSessionId, authority)`，authority ∈ `user|ancestor`。
- 工具：`tool-subagent`（一 provider 一实例；`backgroundMode: one-shot|continuable`；结果 union `{kind:'background',jobId}|{kind:'continuable',subagentId}|{kind:'foreground',runId,output}`）；`tool-subagent-control` 的 `send_message`/`interrupt_agent`/`list_agents`（scope `children|descendants`）。
- jobs：`ctx.jobs` 抽象 `JobRegistry`；`start(spec)` 无 controller 服务该 owner 时拒绝；**授权栅栏 = owner session id**；终态 first-wins 单一 record（`completed|killed|failed`）；`jobs-local` 内存、`maxConcurrentJobsPerOwner` 默认 10、**不跨重启**；工具 `job_output`/`job_list`/`job_kill`（wait 默认 30s / 硬顶 10min）；完成通知 busy→下一步注入、idle→唤醒 turn；kill 或终态读标记 reported 抑制重复通知。
- schedule：三型 `after`/`at`/`every` 恰一（`at` 支持 ISO 带 offset 或本地时区，不存在的本地时间报错；`every` 以 anchor 对齐）；**持久化需 listener 显式 ack**；仅 session 本地消息，**不发外部通知**；不支持 cron/日历。
- webhook：`WebhookRuntime.register(rule)` + `dispatch(delivery): void`；rule = branded id + provider kind + `run(delivery, signal)`；内置唯一 action = 在 Web Workspace 建 root Session（失败回滚）；**process-local、同步扇出、无队列/重试/持久化**，`deliveryId` 只记录、重复投递会重跑；`webhook-github` 做 HMAC（**先 HMAC 校验再 JSON parse**）、返回 202 不等规则、secret 每请求解析（支持轮换）。
- workflow：`ctx.workflowEngine`；脚本 hook `agent(prompt,opts)`/`parallel()`/`pipeline()`/`phase(title)`/`log(msg)`；`WorkflowMeta{name,description,whenToUse?,phases?}`，`phase` 仅是进度词汇**不施加结构**；引擎 = PTC Node 子进程 + 调用方 Session 文件沙箱，拒绝非 TS PTC provider；`tool-workflow` 参数 `meta`/`script`/`args`，返回 `{runId,agentsStarted,result}`，**阻塞父 turn** 至全部 settle，取消/失败不回部分成功；`tool-ralph` 固定前台序列，每轮**新** child 仅收 objective + 轮次/上限 + 上轮 bounded handoff，schema `{status: continue|complete|blocked, summary, evidence, nextSteps, blocker}`，`maxRounds` 默认兼上限 256，终态 `complete|blocked|budget-limited`。
- goal：事件溯源 fold；`GoalRevision{id,revision}` CAS 身份，每次变更 revision+1；转移校验（edit 不改 phase、pause active→paused、resume 可恢复相且未耗尽等）；round 事件须匹配当前 revision 与 `roundsStarted+1`；`maxGoalRounds` 默认 256；activation（armed/disarmed）**process-local 不 durable**；`goal-round-driver` 仅 agent `idle` + phase active + armed 触发，多道栅栏（续轮前后重读最新 goal、校验 `source.round === roundsStarted+1`、异常路径统一 `disarm`）；`tool-goal` 的 `get_goal`/`create_goal`/`update_goal` 做**执行期授权**（拒绝非人类与 subagent authority；edit/pause/resume 需直属 top-level 人类请求；autonomous round 允许 complete/blocked；blocked 低于配置最小轮数被拒）。
- todo：`todo_write` 整表替换、session 所有、`todo/write` 事件持久；写入前校验（trim 非空、内容唯一、≤1 `in_progress` 除非部署允许并行）。
- plan：每 agent 登录态 `plan/mode {active}`（log-only）；部署自有 guidance 注入每请求；`exit_plan_mode` 提交计划给用户审查；unit state v3 折 `command/*` 与 `plan/mode`，用户选择 pending 至下一步 `agent/pre-step` 才 append；**不限制任何工具**。
- Agent Teams（`packages/experimental/*` 5 包）`[实验性]`：Team domain façade = roster（`maxMembers`）+ durable mailbox（目标本地队列、ack、冷恢复前先读日志）+ task board（blockers/readiness/advisory 写域与重叠告警）+ journal + projection；会话 agent 为 Lead；9 个工具 `spawn_teammate`/`send_message`/`list_agents`/`wait_agent`/`interrupt_agent`/`team_task_create|list|get|update` + `team:policy` 提示段；`agent-team-profile` 插入 Team domain/tools 并**禁用**普通 subagent 委派；**shipped profile 均不启用**。
- 其他：`command-goal`（`/goal` 系列在 UI command plane 执行，结果不入模型请求）；`hooks` 桥（见 D2 与领域 2）；`acp`（见领域 20）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D14.1 | 编排范式 | **静态 DAG**（`depends_on` + `verification_json` + `requires_human_ack` + role 心跳/超时） | **无 DAG**；subagent 单次委派 + workflow **脚本化 fan-out**（模型写 JS，运行时决定并行度） | 两侧范式正交，可共存 |
| D14.2 | 阶段级人工验收 | 有（v15 `requires_human_ack`/`human_ack`） | 无（只有 approval） | A 侧独有 |
| D14.3 | 子 agent 隔离 | 每 role 独立 session + 每 phase prompt + 任务级 skill overlay + 临时记忆清理 | provider 化：`spawn`(无 seed)/`fork`(父已完成 turn 作 seed)/acp/cc/codex/sdk | B 侧 provider 面更宽；A 侧"临时记忆"概念独有 |
| D14.4 | 并发/深度上限 | `max_concurrent=5` 位于**未接线**的 orchestrator；真正跑的 loop 无显式上限 | `maxActiveSubagents=8` + `maxDepth=1` 显式配置 | A 侧缺闸 |
| D14.5 | 后台任务 | 无独立 job 概念（Goal phase 内含） | `ctx.jobs` + owner session 授权栅栏 + 终态通知（不等轮询）+ `maxConcurrentJobsPerOwner=10` | — |
| D14.6 | 定时语义 | `once/interval/daily/weekly` + 星期 + 时区 + 防漂移对齐 | `after/at/every`（`every` ≥5 分钟），无日历语义，**不支持 cron** | A 侧日历语义更全；两侧都无 cron |
| D14.7 | 定时任务权限 | 执行时**临时授 full-access 事后回收** | 无对应机制（jobs 绑定 session 作栅栏） | A 侧有降权窗口风险 |
| D14.8 | 外部触发 | **无 webhook** | webhook（process-local、fire-and-forget、无队列/重试/去重）+ GitHub HMAC adapter | B 侧独有 |
| D14.9 | workflow 脚本 | 无 | 模型写 JS：`agent/parallel/pipeline/phase/log`，PTC Node 沙箱，阻塞父 turn | B 侧独有 |
| D14.10 | 团队协作 | 无（有"多会话接力群发"`relay_timeout_ms`） | Agent Teams（roster + mailbox + task board）`[实验性]` | — |
| D14.11 | 跨 agent 交接 | `chat_handoffs` + `contract_json`（verdict/reason/evidence/retry_count） | continuable inbox + `send_message`/`steer`/`interrupt` | A 侧交接契约更结构化；B 侧控制面更细 |
| D14.12 | 未接线资产 | orchestrator 插件完整但无调用方 | 无同类 | A 侧需接线或移除 |

## 领域 15 SKILL

**A 侧** `[完整]`
- 结构：`skills/<name>/SKILL.md`，YAML frontmatter 只有 `name`/`description`（`skills/coolzhu-goal-session-chain/SKILL.md:1-4`），body 为指引文本。
- 发现：**两套独立实现**——CLI 走 `.codex/skills`、`.claw/skills`（含旧 `commands/`）+ `$CODEX_HOME` + home 逐级向上（`command-router/src/lib.rs:1314-1370`），按名去重并标 `shadowed_by`（`:1485-1554`）；web-console 另有 `skill_source_roots()` 扫 `.coolzhu/skills`、`skills/`（`main.rs:10681-10706`），解析器 `:10756-10790`。
- 能力：**只读文本，不执行**；catalog 明确标 `executable_now: false`（`main.rs:10136-10209`）；加载即去 frontmatter + **截断到 6000 字符**拼进 phase prompt（`:44452-44485`，`strip_skill_frontmatter` `:44427`）。
- 与 plugin 的边界：plugin 有 manifest/hooks/子进程工具；skill 只是 markdown，无 manifest、无权限、无工具。
- 3 个 goal skill 作为 **baseline 无条件注入**（不通过 skill 工具按需加载）。

**B 侧** `[完整]`
- seam：`skill` 是 provider 注册表，`rank` **越小越优先**（`src/index.ts:78`），`RUNTIME_RANK=250`、`BUNDLED_SKILL_RANK=600`（`:25,28`），分层 项目 > runtime > user（`:432`），同层按注册序（`:312`）；重名被低优先 provider 忽略并告警（`:575`）；rank 非法即抛（`:727`）。调用控制 `{modelInvocable, userInvocable}`（`:48-56`）。
- provider：`skill-filesystem` 根 project/custom/user（`index.ts:52-59`），支持 `SKILL.md` 目录包与扁平 `<name>.md`；frontmatter 至少 `name`/`description`（`:112-113`）；文件名必须 `SKILL.md`（`:676,687`）；缺失则 warn + 忽略（`:807-817`）；**legacy key 直接抛错**（`rejectLegacyInvocationKey` `:1001-1014`，拒 `disableModelInvocation`/`modelInvocable`/`userInvocable` 旧名）。发现来源表：`.dsh/skills`、`.agents/skills`、`$DSH_HOME/skills`、`~/.agents/skills`、custom、bundled（`:245-268`）。
- 其他 provider：`skill-badge`（随包 "powered by dsh" 徽章 skill，shipped 组合中默认禁用）、`skill-office`（Word/PowerPoint/Excel 工作流，默认 bundled Python 环境）。
- 注入：`tool-skill` 首请求前注入 durable 目录（可 cap 描述长度），`skill` 工具载全文，`/name` 用户调用注入同一指令；目录变更追加完整替换，空目录可退役名字。
- 仓库内 skill：`.agents/skills/` 12 个，组织含 `references/`（如 `dsh-doc/references/` 5 个）与 `templates/`（6 个 package README 模板）；**无 `scripts/` 目录**。
- 与 Claude Code 兼容性：**路径与 frontmatter 格式兼容**（`.agents/skills/<name>/SKILL.md` + YAML），但字段集更窄；**未找到 `allowed-tools` 等 Claude Code 专属字段解析**。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D15.1 | 发现实现 | 2 套（CLI / web-console），仅 CLI 有 `shadowed_by` | 1 套 + rank 数值表（低值胜） | A 侧需合并 |
| D15.2 | 调用控制 | 无（仅 name/description） | `{modelInvocable, userInvocable}` + `/name` 用户调用 | A 侧缺 |
| D15.3 | frontmatter 演进 | 未见 legacy key 拒绝 | legacy key **抛错** | — |
| D15.4 | 注入方式 | 截断 6000 字符拼进 phase prompt | durable 目录注入 + `skill` 工具载全文 + 目录变更替换 | — |
| D15.5 | 无条件注入 | 3 个 goal skill 作 baseline 无条件注入 | 无（均按需） | A 侧有 |
| D15.6 | 附带资源 | 无 scripts/assets | `references/`、`templates/`；无 `scripts/` | 两侧都不完整 |
| D15.7 | office 能力 | 无 | `skill-office`（Word/PPT/Excel） | B 侧独有 |

## 领域 16 MCP 与外部工具

**A 侧** `[完整]`
- 实现规模：`core-runtime/src/mcp.rs`（300 行）、`mcp_client.rs`（234 行）、`mcp_stdio.rs`（2,060 行）、`oauth.rs`（585 行）。
- 4 种 transport：`McpClientTransport` enum = `McpStdioTransport` / `McpRemoteTransport` / `McpSdkTransport` / `McpManagedProxyTransport`（`mcp_client.rs:7-41`）。
- 认证：`McpClientAuth` enum（`mcp_client.rs:43`），`from_oauth(oauth: Option<McpOAuthConfig>)`（`:108-110`）。
- 命名：`normalize_name_for_mcp`（`mcp.rs:7`）、`mcp_tool_prefix(server_name)`（`:26`）、`mcp_tool_name(server_name, tool_name)`（`:31`）。
- 配置：`McpServerConfig` / `ScopedMcpServerConfig` + `mcp_server_signature`（`:65`）、`scoped_mcp_config_hash`（`:84`）；`McpClientBootstrap::from_scoped_config`（`mcp_client.rs:59`）。
- 代理：`unwrap_ccr_proxy_url`（`mcp.rs:40`）。
- 桥：`McpStdio` ↔ `tool-registry`（工具注册）、`compatibility-harness`（扫上游仓清单）。

**B 侧** `[完整]`
- `mcp-client`：**一个 plugin 实例连一个 server**，工具以 `mcp__<serverName>__<rawName>` 注册（`mcp-client/src/tools.ts:82`），`serverName` 限 `^[A-Za-z0-9_-]{1,32}$`（`src/index.ts:2-4,46`）；transport `stdio|streamable-http`；生命周期 effect-scoped；server instructions 进 system-prompt section 且有**字节上限**。
- `mcp-resources`：三个工具 `list_mcp_resources`、`list_mcp_resource_templates`、`read_mcp_resource`（`src/tools.ts:34,43,52`），blob 渲染为长度描述。
- 实验 provider：`experimental-browser-use-chrome-devtools-mcp`、`experimental-browser-use-playwright-mcp`（经 `browser-use-runtime.mountSessionMcp()` 起 stdio MCP）、`experimental-computer-use-cua-driver-mcp`（外部已装 Cua Driver，**沿用其原工具名**）、`experimental-browser-use-stagehand-native` / `experimental-computer-use-cua-driver-native`（npm 原生嵌入）。**全部 opt-in，无 shipped profile 默认启用**。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D16.1 | transport | 4 种（stdio/remote/sdk/managed-proxy） | 2 种（stdio/streamable-http） | A 侧多 SDK 与代理两种 |
| D16.2 | 认证 | OAuth（`oauth.rs` 585 行 + `credentials.json` 明文） | 无内置（依赖上层 credentials seam） | A 侧有实现、B 侧有抽象 |
| D16.3 | 工具命名 | `mcp_tool_name()` 实际生成 `mcp__<server>__<tool>`，先规范化名称（`mcp.rs:26-35`） | `mcp__<server>__<rawName>`，serverName受格式限制 | 前缀格式相同，需核对规范化、长度、碰撞与兼容alias |
| D16.4 | 资源能力 | 未见 resources 工具 | 3 个 resources 工具 | B 侧有 |
| D16.5 | 服务端隔离 | `ScopedMcpServerConfig` + config hash | 一实例一 server + effect 作用域 | — |

---

# 第七部分 界面、运营与工程化

## 领域 17 UI 宿主、客户端、页面与槽位

**A 侧** `[完整]`
- 技术栈：原生 JS，**无框架、无构建器**。`index.html` 1,478 行 + `src/app.js` **20,416 行 / 765 KB** + `src/styles.css` 17,809 行，`<script defer>` 直接引入（`index.html:12-17`）。
- 交付：**Rust 编译期内联** —— `include_bytes!("../index.html")`（`main.rs:25041`）、`include_str!("app.js")`（`:56686`）；改前端必须 `cargo build -p coolzhu-web-console`。
- 视图：**无路由、无组件系统、无视图注册表**。11 个窗口硬编码为 `<article class="workbench-window" data-window-id="...">`（chat/project/settings/clawbot/browser/terminal/tasks/memory/vision/history/usage），窗口集合靠 DOM 枚举（`app.js:17476`）。
- 注册机制：`CHAT_TOOL_WINDOW_META` 是**冻结常量**（`app.js:194-206`）+ 白名单 `CHAT_TOOL_WINDOW_IDS`；`openChatToolWindow` 未知 id 直接返回 false（`:10490-10496`）。
- 路由：唯一形式 = URL 参数 `?window=<id>`（`:17479-17484`）；点左轨 `data-window-target` 不是路由跳转，而是把 `<article>` DOM 节点搬进聊天右栏宿主（`:17516-17530,10540-10549`）。
- 布局：工作台为54px快捷轨+舞台；旧DOM保留左/中/右区与resizer，但当前 `chat_experience.css:2-4` 已隐藏左聊天导航、左resizer及其toggle，`:6-14` 将顶部环境区改为横排。0.2.14产品采用快捷轨与右扩展页、顶部环境下拉；不能仅按旧DOM判定当前仍显示三栏。
- 主题：`theme` 关键词命中 22 文件；`assets/bamboo-*.js` 等动效资源。
- i18n：**无**（grep i18n/locale 仅命中 `target/` 里 Rust 依赖的 icu）；纯中文硬编码。
- 扩展页面：**不存在**。`/api/plugins/install` 是硬编码桩（固定文案 + `installed:true`，`main.rs:18944-18960`），前端只显示"确认已加载"（`app.js:14933-14945`）；`/api/web/cards` 是硬编码 card→endpoint 审计清单（`main.rs:1669,1728`），**前端未调用**。
- 桌面：Tauri shell 包 web console（`WebviewUrl::External(http://127.0.0.1:8765)`，`tauri-shell/src-tauri/src/main.rs:344`），12 个 IPC 命令（`main.rs:202-215`），窗口 `console`+`pet`，capabilities 仅授权这两窗 + `remote.urls` 限 127.0.0.1/localhost:8765（`capabilities/default.json`）。
- 桌宠 `[完整]`：`pet-mini.html` fetch `assets/pet-theme.json`（`:364`）；主题 15 个 state，字段 `frame_pattern/frame_count/interval_ms/priority/min_duration_ms/auto_return_ms/message/bubble/frame_offsets/frame_scales`（`main.rs:88-103`）；优先级 idle|blink=1 < thinking=2 < working|carrying|juggling|sweeping=3 < notification|success|dragging=4 < attention|crowned=5 < warning=6；`event_map` 24 条映射（`chat.completed→success`、`chat.failed→warning`、`*reasoning*→thinking`、`permission*→attention`、`tool.timeout→warning` 等）；链路 `record_pet_event`（`:4009`）→ `pet_state_store` + broadcast + `dispatch_desktop_pet_action`；SSE `/api/pet/events`（`:4334`）+ `/api/pet/state` + `POST /api/pet/event` + `/api/pet/drop-files`。
- 独立桌面控制台 `[完整/未纳入发布链]`：`desktop-console` 是 eframe+egui 应用，依赖 api/computer-use/vision/runtime/diagnostics，**不经 web 层**；面板：左栏 session/agent，右栏 inner_vision/outer_vision/tool/test_lab，另有 config 面板（`app.rs:773-1366`）；自带桌面截图（`desktop_capture.rs:214`）与桌面锚点（`desktop_anchor.rs`）。**全仓无任何 crate/脚本引用它，`package-manifest.json` 也不构建它** → 仅 `cargo run -p coolzhu-desktop-console` 手工入口。

**B 侧** `[完整]`
- 分工：`apps/web` 只是 Vite 入口（`src/main.ts` 跑 `new AppWebEntry(el).run()`）；`packages/client/*` 55 个 UI 包（浏览器半边）；`packages/host/*` 8 个服务端包；Client→Host 走 `api/gateway` + `api/remotes`。
- 声明与装配：Client 包在 `package.json` 声明 `dsh.client{platform:'web',inject}` + `exports["./client"]`（`client/ui-tool/package.json:28-38`）；Host 扫描 Loader entries 合成 `window.__DSH_BOOT__`（`docs/subsystems/client-modules.md:5,77`）。
- 布局与槽位：三列 AppFrame，root slot 声明 `sidebar`(single)/`main`(keyed)/`rightbar`(single)（`client/ui-layout/src/client/index.ts:147-153`）；`main` 的 key `conversation` 由 `ui-conversation` 注册（`ui-conversation/src/client/apply.ts:413`），`plugins` 由 `ui-plugin-manager` 注册（`ui-plugin-manager/src/client/index.ts:83`）；shipped composition **不注册其他 global panel**（`ui-layout/README.md:36`）；Settings 是 overlay + `sidebar.settings` seat（`ui-settings-general/src/client/SettingsRoot.tsx:72`）。
- 右栏 tab 注册表：`ctx.sidebarRightTabs`（`ui-sidebar-right/README.md:79-91`）= guide、文本预览（`ui-sidebar-documentpreview`）、files、browser、plan、terminal、changes-review（`ui-deliverables:81`）。
- 基础机制包：`client-web`（boot + `PLATFORM_MODULES` 模块表）、`client-modules`（Host 扫 enabled entries 组 boot graph，在 `/plugins` 服务 bundle）、`client-connection`（`ctx.connection` + generation 生命周期）、`client-resources`（`dsh-resource://` 协议 + `useResource`）、`client-store`（React-free observable，同步 + rAF 发布、Immer、浅相等、可选 localStorage）、`client-hmr`（开发期刷新 client plugin）、`client-locale`、`client-ui-slots`（typed Slot + Component Factory + `SlotCore`，含 `StaleAuthorizationError`/`SlotOwnershipError`）、`client-ui-renderer`（Host 侧 `apply()` 无行为是browser-only职责设计（`src/index.ts:4`），不标stub；真正 `SlotRegistry` 在 `src/client/registry.ts:120`）。
- 具体 UI 包（55 个中的代表）：chat、conversation、commands、input-trigger、deliverables、dockkit（split-tree 布局引擎）、goal、jobs、layout、message-feedback、model-selection、open-in-app、permission-presets、plan、plugin-manager、primitives、reference、schedule、session、settings 系（general/models/plugins/plugin-inventory/unarchive-sessions）、sidebar 系（sidebar/browser/documentpreview/files/right/terminal）、skill、slots、subagent、theme、tool、trajectory、user-questions、workflow-run、workspace、attachment、agent-preset、approval、brand-official、directory-picker 系。
- 工具 UI 契约：Host 侧 `presentCall`/`presentResult` 返回 `card`-tagged 纯函数（`shell/tool-bash/src/index.ts:101,386`）；**Web 不用它们**，Client 插件把 wire tool name 注册进 keyed slot `tool.call.toolview`（`client/ui-tool/src/client/apply.ts:33-50`，内建 bash/read/read_image/write-edit/grep-glob/web/todo/question）；未注册者回退 generic card（`ui-tool/README.md:11`）。
- i18n：`ctx.locale.register(ns,{zh,en})` + `t` seat，缺 key 两语即**编译错**（`client/locale/README.md:28-30`）；门禁 `scripts/verify-client-ui-i18n.ts` 挂 `scripts/run-gates.ts:311`。
- 桌面：`apps/desktop` Electron 壳（独占 `$DSH_HOME/profiles/desktop`，进程级单实例锁，bundled Python/Node/pnpm runtime）+ `apps/desktop-host`（private，`loadProfileDirectory` + `runProfile({profile:'desktop', args:['--no-open','--port','19387']})`）。**无桌宠**。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D17.1 | 前端技术 | 原生 JS 单文件 20,416 行，无框架无构建器 | React + Vite，55 个包 | 硬差异 |
| D17.2 | 页面组织 | 11 窗口硬编码 DOM + 冻结白名单 | keyed slot / seat / `sidebarRightTabs` 注册表 | A 侧新增面板要改 3 处 |
| D17.3 | UI 扩展点 | **不存在**（`/api/web/cards` 硬编码清单说明意图存在） | Client 包 `dsh.client` 声明 + Host 扫合成 BOOT | A 侧最大缺口 |
| D17.4 | 资源交付 | Rust 编译期内联（改 CSS 需重编译 crate） | Vite 独立构建 + client-hmr 开发期热刷 | — |
| D17.5 | i18n | 无（纯中文硬编码） | typed dictionary + 缺 key 编译错 + `verify-client-ui-i18n` 门禁 | — |
| D17.6 | 工具呈现 | 散在 `app.js` | Host presenter 纯函数 + Client keyed slot 双轨 + generic 回退 | — |
| D17.7 | 桌面形态 | Tauri（console + pet 双窗）+ 独立 egui 控制台（未纳入发布） | Electron 壳 + desktop-host（private） | — |
| D17.8 | 桌面附加物 | **桌宠**（15 state + 24 事件映射 + SSE + 拖放） | 无 | A 侧独有 |
| D17.9 | 布局引擎 | CSS + 手写 resizer | `ui-dockkit` split-tree 引擎（可逆操作） | — |

## 领域 18 诊断、审计、统计、轨迹与遥测

**A 侧** `[完整 + 2 处 stub]`
- 日志/span：`diagnostics` crate —— `static LOGGER: OnceLock<Logger>`（`lib.rs:24`）；`LogEntry{timestamp_ms, level, app, module, event, message, trace_id, span_id, fields}`（`lib.rs:63-72`）；`LogLevel` 1–5（`:28-36`）；`init`/`log_path`/`set_gui_callback`/`error|warn|info|debug|trace|emit`（`:84,80,122,127-186`）。Span 用 `thread_local CONTEXT_STACK`（`span.rs:8`），`start_span`/`start_span_with_parent`/`SpanGuard::record|event`/`SpanClosed{trace_id,span_id,parent_id,name,module,duration_ms,attributes}`（`span.rs:146,177,46,52,133`）；`TraceId(u128)`/`SpanId(u64)` 进程内 `AtomicU64` 递增（`trace_id.rs:4-8,51`）。
- 输出：`COOLZHU_LOG_DIR` | `CLAW_LOG_DIR` | 默认 `{USERPROFILE|HOME|tmp}/.coolzhu/logs/{app}.jsonl`（`lib.rs:277-284`），JSONL 手工拼装（`output.rs:53-105`）；console 由 `COOLZHU_LOG_CONSOLE` 控制。**无轮转/无清理**（`FileOutput::new` 仅 create+append，`output.rs:22-27`；grep rotate/max_size 无命中）。Span 生产使用**仅 1 处**（`web-console/src/main.rs:27132` `session.context_build`）。**日志文件无读取端**（desktop UI 只显示路径 `app.rs:780-783`）。
- 自检 API：`/api/diagnostics/health`（`main.rs:18310` → `build_diagnostics_health` `:29235`）固定 checks：`web.state`、`web.bind`、`workspace`、`workspace.config`、`session.store`、`llm`、`vision`、`audio`、`tools`、`desktop_pet`、`webview2` + `aggregate_health_status`；`/api/diagnostics/functional`（`:18373`）：`functional.config`（real_llm/llm_tools/semantic_memory/computer_use + provider readiness 计数）、`functional.workspace`、`functional.session-store`、`functional.chat-room`、`functional.tools`、`functional.chat-room-capabilities`、`functional.stream-guard`（**硬编码 `"warn"`**）、`functional.browser`。
- `/api/diagnostics/stream`（`:18504`）**全部字段硬编码 `ok/true`，无实际探测** `[stub]`。
- 自检 UI：任务窗口右列"模块自检"（`index.html:1162-1242`：9 个模块行、修复建议、自更新、语音/调度诊断）；前端用 health + functional（`app.js:1688,1707`）。**无独立日志查看器**（无 `/api/logs` 路由、无日志窗口）。
- 审计：`ToolAuditEntry` JSONL（见 D9.8）；`computer_use_audit.jsonl`（`main.rs:22461`）；`clawbot_group_audit`（仅 append）；查询接口 `read_tool_audit_entries(limit)`（`main.rs:4524`）。
- 统计：`chat_usage_events` + `chat_message_timing`（`src/chat_insights.rs:45-54`），流式每请求只记一条（`:13-43`）；`/api/chat/rooms/{id}/insights`（`main.rs:1264`）返回 usage/timings/indices；UI 在 usage 窗口按会话显示 input/output/cache_read/cache_write（`chat_experience.js:248-266`）；逐轮耗时进消息 meta（`:237-247`）。成本仅"上下文成本估算：约 N 词元"（`app.js:14880`，**非金额**）。
- **遥测外发：无**（grep telemetry/analytics 在源码区无命中）。
- **轨迹页面：未找到**。`trajectory`/`timeline`/`轨迹` 只出现在 docs 且是"尚未实现"说明（`docs/issues/2026-09-19-qwen-computer-use-test.md:104,118` 明确 `computer_use_steps` 只是收尾摘要、"并非精确动作轨迹"）。近似替代：消息 `#序号+本轮耗时`、聊天状态区最近 30 次工具列表（`chat_experience.js:138-148`）、工具审计表。`/api/sessions/{id}/events` 存在（`main.rs:1057`）但**前端不消费**。

**B 侧** `[完整]`
- 不变量：`runtime-diagnostics/invariants` 的 `InvariantRegistry`（`enabled` 默认 true + `package_allowlist/package_blocklist` 正则；`register(packageName, installer)` 重名抛错；失败抛 `InvariantError` 并归属违规包）；各包 `./invariant` 只断言自有关系。**无 UI**。
- 统计 projection：`session-stats` → `SessionStatsProjection{turns, steps, llmMs, toolMs, **ttftMs**, ttftSteps, **decodeMs**, decodeTokens}`（`src/types.ts:23-40`）；语义：`turns` 仅计含闭合 step 的不同 turn；`steps` 计 `step/end`（含失败/取消）；`llmMs` 为 `step/start → assistant/message`；`toolMs` 按 callId 配对；`ttftMs` 为 `step/start → 首个非空 delta`；`decodeMs/decodeTokens` 限同时报 output tokens 的 step。字段名**刻意对齐 client window fold** 以便无此 unit 时整体回退。`definition key:'sessionStats', stateVersion:1`。
- 大纲 projection：`session-turn-outline` → `TurnOutlineEntry{turn, seq, prompt, response}`；fold state `{turns, draft}`，`turn/end` 才提交；wire 只投 `turns`，故 draft-only apply 保持数组 identity；`stateVersion: 2`。
- UI：composer 下两枚 pill（turn/step + 输出速度；总 token + cache hit），点开"会话统计"对话框（Token 用量/模型用时/工具用时/TTFT/TPS，`client/ui-chat/src/client/locale.ts:13-20`，注册 `apply.ts:181`）；另有 **ContextMeter 上下文占用环 + 分解面板**（`client/ui-conversation/src/client/skeleton/ContextMeter.tsx:1-5`）。
- 轨迹：`client/ui-trajectory` —— trajectory tab，turn 感知 ledger + 交互时间轴，含 **TTFT/解码分段、拖拽选区间、滚轮缩放、record inspector**（token usage、duration、Input/Output/Timing、图片），长历史尾部**分页虚拟渲染**（README:12,47-55）。实现为**纯 projection**，定义在 `src/client/trajectory-*-definition.ts`；组件 `TrajectoryTable/Timeline/Toolbar/Cell/GroupHeader`。
- 查询与导出：`session-query` 抽象 `SessionQueryEngine`（`searchSessions`/`searchEvents` 由 sqlite 提供；继承 `readSession`/`filterSessions`/`readTitle*`/`listEvents`/`filterEvents`/`readSurface`/`traceSession`/`traceEvent`/`readEvent`）；词汇 `SessionEventSurface = 'current'|'shadowed'|'log-only'`、`SessionAvailability = 'live'|'persisted'`；`Config.readWindowMax` 默认 50。`session-query-sqlite` 用 **FTS5**（`persisted_sessions` + `persisted_docs` 虚拟表 + TEMP `live_sessions`/`live_docs`），`maxLimit 100`、`snippetChars 240`。`session-log-export` 提供 `/export` 命令 + `/api/session.export` 流式 ZIP（附件落 `media/<attachmentId>.<ext>`）。模型侧 `tool-session-query` 5 工具 + workspace 授权（`authorizeTarget`/`authorizeSessionIds`/`authorizeDescendants`）。
- 遥测：`session-telemetry` 捕获 `SessionTelemetryRecord{channel:'ledger'|'ops', ...}`，attributes **刻意最小**（ledger 带 session.id/format_version/event.type/event.seq；ops 带 telemetry.op/session.id + agent-error 的 agent.id/turn/step/error.name）；每条记录先经 `session-telemetry/record` waterfall 做 redaction（fail-closed）；sharing 三态 `full|feedback-only|disabled`；`flush` **故意留未实现**由 SDK 自行 batching。`session-telemetry-otel` 仅 `FEEDBACK_ONLY`（默认）与 `DISABLED`，OTLP/HTTP，`DEFAULT_SHUTDOWN_TIMEOUT_MILLIS=3000`；**无 `full` 模式的 OTel 实现**。
- 反馈：`command-feedback` 的 `feedback/record` 是 log-only、**永不进入模型上下文或派生历史**；`message-feedback` 的 `feedback/message-put|delete` 随 session 持久化、**绝不进入模型历史或 telemetry**，用 Branded version 做乐观并发，`maxNoteBytes` 必填。
- 身份：`anonymous-user-id` 每 harness home 一个 id（`.anonymous-user-id`，`flag:'wx'` 独占写，EEXIST 时重读采纳竞争赢家），附 telemetry/feedback/DeepSeek 请求，使同一安装的记录可识别而不识别用户。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D18.1 | 轨迹可视化 | **无**（`/api/sessions/{id}/events` 存在但前端不消费） | `ui-trajectory`（projection + 时间轴 + inspector + 虚拟化） | A 侧数据已具，缺 projection 与 UI |
| D18.2 | 统计维度 | turns/工具计数/耗时/成功率/token（input/output/cache_read/write） | turns/steps/llmMs/toolMs/**ttftMs**/**decodeMs**/decodeTokens | B 侧分离 TTFT 与 decode |
| D18.3 | 上下文占用可视化 | 一句"约 N 词元" | ContextMeter 占用环 + 分解面板 | — |
| D18.4 | 日志 | JSONL + span，**无轮转无清理无读取 UI**；Span 仅 1 处使用 | 无独立日志系统（用 session event + telemetry） | 两者机制不同 |
| D18.5 | 自检 | health 11 项 + functional 8 项 + **UI 面板（9 模块行 + 修复建议）**；stream 端点 `[stub]` | `InvariantRegistry`，**无 UI** | A 侧产品化更好 |
| D18.6 | 审计 | `ToolAuditEntry`（放行+拒绝+affected_paths）+ CU 审计 + 群聊审计表 | 经 session event（approval 落在 turn 边界内） | A 侧结构更独立 |
| D18.7 | 遥测外发 | **无** | `session-telemetry` seam + OTel 后端（sharing 三态，仅 FEEDBACK_ONLY 有实现） | 合规取向不同 |
| D18.8 | 匿名身份 | 无 | `anonymous-user-id` per harness home | — |
| D18.9 | 反馈 | 未见机制 | 两套（command-feedback 会话级 / message-feedback 逐条），边界写死"不给模型/不进 telemetry" | — |
| D18.10 | 会话检索 | history 窗口 UI + 搜索（`chat_experience.js:267-305`） | FTS5 后端 + 5 个模型工具 + workspace 授权 + trace 谱系 | A 侧检索无模型面工具 |
| D18.11 | 导出 | **无** | `/export` + ZIP 流式（含附件） | — |

## 领域 19 连接器与设备能力

**A 侧** `[完整，多处 stub]`
- Computer Use（`computer-use-core`）：
  - 输入原语：`click_point`、`mouse_button_action_point`（LeftClick/RightClick/LeftRightChord/DoubleClick）、`mouse_button_up_point`、`mouse_button_down_point`、`move_mouse_relative`、`move_mouse_absolute`、`drag_path`/`drag_point`（线性插值 1..128 段）、`scroll_wheel`、`type_text`、`press_virtual_key`、`hold_virtual_key`、`send_virtual_key_combo`、`press_escape`（`input.rs`，1209 行）。
  - **无 Rust 侧 Win32 FFI**：每个动作拼 `-Command` PowerShell 脚本，`Add-Type` 内联 C# P/Invoke（`SetCursorPos`/`SendInput`/`interception.dll`）后 spawn `powershell.exe`（`input.rs:313,317,1069`）。后端选择 `CLAW_MOUSE_BACKEND=auto|sendinput|interception`（`:238-267`），auto 需 DLL 存在且 `interception_create_context` 成功，否则回退 SendInput；DLL 默认 `%USERPROFILE%\.claw\vendor\interception\...\x64\interception.dll`，device id 默认 11/1（`:269-305`）。
  - DPI/多显示器：旧输入原语由调用方提供物理像素；正式桥已有PMv2观察、绑定窗口原图和窗口/DPI校验。旧 `capture_desktop` 主屏截图另列，不代表正式CU只能截主屏；跨屏、负坐标和混合缩放仍需专项验收。
  - 死代码：`move_mouse_absolute`、`mouse_button_down_point` 仅定义处出现 `[死代码]`（`input.rs:86,90`）。
  - 受控笔画：`input_stroke.rs` + `input_stroke_native.cs`（101 行 C#）—— 只接受 window identity + 数值路径，无脚本执行入口；`validate_stroke` 限 2–256 点、≤5000ms、坐标 ⊂ 已观察窗口矩形、防溢出；取消靠独立哨兵文件，2s 未退出则 kill 并递归 `emergency_release`（只释放左键、不移动光标）；`capture_window_image` 抓绑定窗口真实像素（≤1600 万像素）。
  - 预算/熔断：`TurnComputerUseSupervisor` 的 `TaskIdempotencyKey`（session/turn/surface/objective/target/criteria/constraints 归一化拼接），`before_run` 命中 `terminal_cache` 直接返回缓存结果；失败累计达 `max_calls_per_turn` 置熔断 `"second-computer-use-failure"`；`RunBudgetGuard` 硬上限 `max_actions=12`、`max_replans=2`、`max_same_signature=2`、`max_no_progress_steps=2`、`timeout_ms=120_000`、`max_calls_per_turn=2`（`contracts.rs:194-203`）；越限返回 `blocked(deadline_exceeded|no_progress|budget_exhausted)`。
  - trait 面：`ComputerUsePlanner`(classify/next_action/verify)、`ComputerUseAdapter`(observe/act/verify)、`ComputerUseEventSink`、`ComputerUseClock`、`ComputerUseApprovalPolicy`（`controller.rs:11-90`）；`host_sensitive_semantic_category` 对 objective/action.target/args/目标节点文本做敏感语义分类（删除/支付/外发/安装/凭据），命中者不走通用审批（`:92`）。
  - 回归资产：`ResolutionCase`+`standard_resolution_cases`、`RelativeAnchor`+`anchor_to_physical_pixel`、`InteractionScenario`+`default_regression_scenarios`；CLI `bin/check.rs`。
  - 输入控制已有敏感语义、目标能力、输入长度、授权及取消释放约束；尚不能证明完整输入内容治理或OS隔离。字符串检索未命中某类deny-list不能推出完全不存在控制。
- Vision：
  - `vision-service`：两套 trait —— `VisionBackend`（`lib.rs:232`）实现 `ZhipuVisionBackend`（默认 `glm-4.6v-flash`，`:299`）与 `LocalOpenAiVisionBackend`（OpenAI 兼容 `/v1/chat/completions`，图片路径→base64 data URL，`:366,1422`）；`DetectionBackend`（`:237`）实现 `HttpDetectionBackend`（UI-DETR-1，`POST {base}/detect`，`:479`）。
  - 解析：`parse_grounding_result`（点/bbox/候选数组/嵌入 JSON，`:752`）、`parse_detection_elements`（bbox 键名与绝对/相对双兼容，`:768`）；坐标映射 `relative_point_to_pixel = x*(w-1)`（`:830`）、`relative_bbox_to_pixel`。
  - 双模式路由（实现在 web-console，非 crate）：`LocateRequest`（target=natural/system/uia + backends + cross_verify + region_hint）→ `run_grounding_router`（`main.rs:20034`）；pipeline 取 `config.vision.router.pipeline`，默认 `["uia","ocr_template","local_vlm","remote_vlm"]`（`:5617`）；逐个 attempt，首个 Ok 且未开 cross_verify 即 break（`:20124`）。LocalVlm 探活 + `probe_local_vlm_readiness` 后按 `sample_count`(1..5) 轮换三模板，用简化 DBSCAN `cluster_median(eps=48px, min=2)` 取最大簇质心（`:20869`），置信度由 `inferred_point_only_confidence` 推断，低于阈值时回退色块扫描并抬到 0.72。OcrTemplate **不是 OCR**：PowerShell 注入 C# 对截图做像素连通域扫描，阈值硬编码（`:20324-20379`），仅对含 green/gold/按钮语义目标触发，否则 `skipped("...OCR provider is not configured")`（`:20703`）。
  - 截图：`capture_desktop` 用 `powershell + System.Drawing.CopyFromScreen` 抓**主屏**到 `desktop-latest.png`（`:34334-34393`），仅 Windows。
  - 缺口：`VisionToolCapability::Describe`/`Ocr`/`RealtimePerception` = `Reserved`，返回 "model calls are disabled for this round"（`lib.rs:598,605,626`）`[stub]`；**无 OCR 引擎**（全仓无 tesseract/paddle/Windows.Media.Ocr）；**无元素缓存**；`local_backend::uncrop_point`/`anchor_region` 仅自测引用 `[未接线]`；`cross_verify_tolerance_px` 已配置但无消费者 `[死代码]`。
- UIA（`uia-resolver`）：`snapshot_foreground_window(limit)`（`lib.rs:86`→`windows_impl.rs:348`，物理坐标模式）；`resolve_query`（`:126`）按 process_id/window_name(contains)/element_name(contains)/automation_id(全等)/class_name/control_type 过滤 enabled∧可见∧尺寸>0，多命中→`ElementAmbiguous`；`resolve_system_control`（`:174`）**仅实现 `StartButton`**（写临时 `.ps1` 调 .NET `UIAutomationClient`，解析 `OK|id|class|name|off|ena|x|y|w|h`，confidence 固定 0.99/0）；其余 10 个 `SystemControlId` 返回 not yet implemented `[stub]`。无元素缓存、无 UIA 句柄复用、无 Invoke/Value pattern 调用（只读几何）。
- 浏览器桥 `[完整]`：三层——扩展 —(nativeMessaging：4 字节 LE 长度 + JSON，上限 1 MiB)— `browser_native_host.exe` —(WebSocket `ws://127.0.0.1:8765/api/computer-use/browser/native`)— web-console。握手校验 argv 中 `chrome-extension://akpgmkdkaofanikngahmfbhddpppicfi/` + `runtime/browser-bridge-nonce` 的 `Hello{nonce}` 比对。注册脚本写 `HKCU\...\NativeMessagingHosts\com.coolzhu.agent.browser_bridge`，扩展 ID 由 manifest `key` SHA256 推导并与 `extension-identity.json` 交叉校验。扩展侧 MV3，`permissions=[activeTab,scripting,nativeMessaging,tabs,storage]`，`host_permissions=["http://*/*","https://*/*"]`；content_script 生成候选元素表 `dom-N` + `MutationObserver` 维护 revision；可执行动作固定白名单（navigate/click/text_input/select/check/submit/drag/slider_drag/key_combination/scroll/history_back/forward），按键限 14 个；**无任意脚本执行、无截图能力**；service_worker 用 `chrome.storage.session` 持久化 owned tabs 并在重启后回收，非 owned tab 拒绝操作。Rust 侧 `BrowserNativeBridge` 实现 `ComputerUseAdapter`，broker 以 `request_id`+`reply_token` 配对，HTTP 回包兜底 `POST /api/computer-use/browser/response`。
- 微信（ClawBot）`[完整，真实微信侧不在本仓]`：`clawbot-sidecar` 独立进程，provider 由 `COOLZHU_CLAWBOT_PROVIDER_KIND=mock|http` 选择，http 时必须给 `COOLZHU_CLAWBOT_PROVIDER_URL`；`HttpClawbotProvider` 调 provider 的 `/login/refresh`/`/login/logout`/`/updates`（45s 长轮询）/`/send_text`/`/send_file` —— **自定义 HTTP+JSON 轮询协议，不是 hook/注入/官方 API**。web-console 侧 `POST /api/channels/clawbot/inbound` → 落 `clawbot_inbox`（`UNIQUE(account_id,external_msg_id)` 幂等，返回 Accepted/Duplicate/Conflict）；`guard_action` 只接受 `source=weixin_user ∧ hop_count=0`，其余 `rejected_recursive`；群事件 BotAdded→`AwaitingConfirmation`、BotRemoved→`Removed` + `detach_group_transaction` 清绑定/成员权限/待发消息。身份绑定 `ClawbotConversationBinding{account_id,peer_id,chat_room_id,default_session_id,target_agent_ids,workspace_id,allowlisted,enabled}`；群 vs 私聊：群需 `group_id+member_id` 双身份 + `@bot` 别名匹配；权限 = `WechatMemberGrant`（preset None/ChatMember/Operator + **9 个能力位** chat/status/tools.read/tools.write/files.read/files.write/tasks.control/approvals.resolve/bindings.admin）。表：`clawbot_login_snapshot`、`clawbot_inbox`（含 request_state/request_id/parent_request_id/operation_fingerprint 迁移列）、`clawbot_outbox`（attempts/next_attempt_at_ms 重试）、`clawbot_group_member_grants`、`clawbot_contacts`、`clawbot_operation_administrators`（每 account 一个运维管理员，先到先得）、`clawbot_groups`（lifecycle_state）、`clawbot_group_audit`（仅 append，**无 TTL/清理/容量上限**）、`clawbot_denial_notices`（通知限流）。
- 实时语音 `[完整]`：`realtime_voice_capture.js` + `/api/realtime/session/*` + `/api/realtime/session/events`（EventSource）；audio/stt/speech/tts 关键词命中 43/12/13 文件。
- 终端：经 `/api/tools/runtime-execute` 跑 PowerShell（`app.js:10196-10201`），**无持久 PTY seam**。

**B 侧** `[完整，provider 多为实验性]`
- computer-use / browser-use：两者都只提供 **exclusive named provider 注册服务**——`ctx.browserUse`/`ctx.computerUse`，`register(name)` 第二次注册直接抛错（`browser-use/src/index.ts:35-45`）。**base 包内无实际 provider**。
- 实验 provider（5 个，全部 opt-in、无 shipped profile 接线）：`experimental-computer-use-cua-driver-mcp`（外部已装 Cua Driver，走 MCP，沿用其原工具名）、`experimental-computer-use-cua-driver-native`（`import('@trycua/cua-driver')` 原生嵌入并注册其工具）、`experimental-browser-use-chrome-devtools-mcp`、`experimental-browser-use-playwright-mcp`（经 `browser-use-runtime.mountSessionMcp()` 起 stdio MCP，工具前缀 `mcp__<name>__`）、`experimental-browser-use-stagehand-native`（本地 Stagehand npm + Worker，工具名 `stagehand_<method>`）；`experimental-browser-use-runtime` 是库（无 ctx key），负责 Session 级浏览器与 MCP 资源复用/串行。
- 终端：`terminal` seam（owner=Agent 精确围栏，错误码 `FOREIGN_SESSION`/`OWNER_NOT_LIVE`/`NO_BACKEND`/`SEND_ACTIVE`/`SERVICE_DISPOSING` 等）+ `terminal-bash`（backend `shell`，`@xterm/headless` scrollback 0，行 sanitizer 是唯一输出投影，PS1+PROMPT_COMMAND 私有 marker 判 ready，沙箱模式取 `ctx.sandboxPolicy.resolve({session})`）+ `tool-terminal`（6 工具 `terminal_open`/`terminal_send`/`terminal_read`/`terminal_signal`/`terminal_close`/`terminal_list`）。**会话不跨进程重启**、无全屏 TUI/按键序列/resize。另有 `api-terminal-controller`（GUI 用户终端，10 个 Remote，含 bounded screen recovery）。
- LSP：`ctx.lsp` 仅四操作 `goToDefinition`/`findReferences`/`goToImplementation`/`hover`，**无 JSON-RPC 逃生口**；`lsp-stdio` 按 `servers.<id>` 的 command/扩展名映射注册 provider，经 `ctx.fs` 读源、`ctx.subprocess` 起 server；`tool-lsp` 独占模型名 `lsp`。
- Web：`ctx.web` 两个注册表（provider id `exa`/`perplexity`/`deepseek` + http fetch）；选择语义：配置 id 不可用→报错，未配置且多个可用→`WEB_PROVIDER_AMBIGUOUS`；工具 `web_search(queries[1..N])`、`web_fetch(url)`；fetch 强制公共地址并限同源重定向跳数。
- SSH：`ssh` 共享 OpenSSH 连接 + **versioned POSIX remote helper**（ControlMaster socket、`ControlPersist=no`、`BatchMode=yes`；hello 的 `hash` 必须等于配置 `helperHash`(sha256)；`SSH_PROTOCOL_VERSION=1`；RPC 方法族 `heartbeat/close/executable/fs.*/process.*/terminal.*/sandbox`）；四个消费者复用同一连接：`fs-ssh`、`sandbox-ssh`、`subprocess-ssh`、terminal。
- 文档转换：`document/office-to-pdf` 基于 LibreOffice kit，队列支持 foreground/background 优先级、source key+version+ext 别名表、digest 级 in-flight 合并、内容寻址缓存（`maxCachedEntries=8`、`maxCachedBytes=128MiB`、`maxSourceEntries=64`）与 generation 失效，18 个 Config 字段。
- 无 vision 定位体系、无微信、无语音。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D19.1 | 桌面输入 | 自研（PowerShell + 内联 C# P/Invoke + SendInput/interception），含 4 级预算/熔断/幂等键/敏感语义分类 | 无自研；经 exclusive provider 接外部实现（Cua Driver MCP/native）`[实验性]` | 两侧恰好互补：A 有实现、B 有 provider 抽象 |
| D19.2 | 浏览器控制 | 自研 MV3 扩展 + native messaging + WebSocket + 动作白名单 | 经 provider 接 MCP（chrome-devtools/playwright/stagehand）`[实验性]`，无自研扩展 | 同上互补 |
| D19.3 | 视觉定位 | 完整体系：UIA + 本地/远端 VLM + 颜色块扫描 + DBSCAN 聚类 + 双模式路由 | **无** | A 侧独有 |
| D19.4 | 微信连接 | 完整（provider 协议 + inbox/outbox + 群权限 9 能力位 + 审计 + 幂等） | **无** | A 侧独有 |
| D19.5 | 实时语音 | 有（capture + realtime session + events） | **无** | A 侧独有 |
| D19.6 | 终端 | 无持久 PTY（走 PowerShell HTTP） | PTY seam + 6 工具 + xterm headless + owner 围栏 + 右栏 tab 可重连 | B 侧独有 |
| D19.7 | SSH | **无** | 共享连接 helper + 4 消费方 | B 侧独有 |
| D19.8 | LSP | `language-service` 有客户端但**无生产构造点** `[未接线]`；web-console 用另一套正则符号索引 | `ctx.lsp` + `lsp-stdio` + `tool-lsp` | 两侧实现不同且 A 侧未接线 |
| D19.9 | Web 搜索/抓取 | WebFetch/WebSearch 在19个静态工具spec中；模型最终可见集合动态生成 | `ctx.web` 3 个 provider + 歧义检测 | — |
| D19.10 | Office 文档 | 无 | `office-to-pdf`（LibreOffice，队列/缓存/gen 失效）+ `skill-office` | B 侧独有 |
| D19.11 | DPI/多显示器 | 已有线程PMv2物理坐标观察与恢复，0.2.14修复150%缩放错配；正式桌面桥按绑定窗口取原图；旧主屏截图入口另列，多屏拓扑/跨屏移动需专项验证 | 取决于选用provider，不能由注册服务推断 | 分开验收DPI、图像缩放、窗口移动与多屏，不能笼统写不处理 |

## 领域 20 SDK 与自动化入口

**A 侧** `[部分]`
- CLI（`command-line`，8,676 行）：手写 argv parser（**未用 clap**），`parse_args_with_default_model`（`src/main.rs:245`）逐 token 匹配；flags `--model/--model=`、`--output-format text|json`、`--permission-mode read-only|workspace-write|danger-full-access`、`--dangerously-skip-permissions`、`-p`、`--print`、`--allowedTools/--allowed-tools`、`-V/--version`；子命令 `dump-manifests`/`bootstrap-plan`/`agents`/`skills`/`system-prompt`/`login`/`logout`/`init`/`prompt`，默认落 `CliAction::Repl`。
- REPL：rustyline 多行编辑 + 历史 + Tab 补全（`src/input.rs`，1,195 行）；`handle_repl_command`（`main.rs:1397`）穷尽 match **25 个 slash 变体**；`branch`/`worktree`/`commit-push-pr` 只打印 `render_mode_unavailable`（`:1512-1526`）。
- 运行时：`LiveCli` 持有 `ConversationRuntime<DefaultRuntimeClient, CliToolExecutor>`（`main.rs:1198-1204`）；`run_turn`（`:1329`）→ `runtime.run_turn(input, Some(&mut CliPermissionPrompter))`（`:3208`）；**流式是阻塞式** `ApiClient::stream() -> Vec<AssistantEvent>`（`conversation.rs:39`），**非增量**。`--resume SESSION.json <cmd...>` 走 `run_resume_command`（`:958`）。
- 死代码：`args.rs`（104 行，clap `Cli/Command` 含 `Ndjson`）**未 `mod args` 且 Cargo.toml 无 clap** `[死代码]`；`app.rs`（402 行，`CliApp`/`ConversationClient` 第二套 REPL 实现）**未 `mod app`** `[死代码]`。
- `agent-server`：axum 会话托管（442 行单文件）——`AppState{sessions: Arc<RwLock<HashMap>>, next_session_id: AtomicU64}`，`BROADCAST_CAPACITY=64`；路由 `POST/GET /sessions`、`GET /sessions/{id}`、`GET /sessions/{id}/events`(SSE)、`POST /sessions/{id}/message`。**无任何 crate 依赖它**（仅 `Cargo.toml:59` 声明 `workspace.dependencies.server`；唯一使用是 `tests/module_linkage_smoke.rs:31`）→ **无消费者、无持久化、无认证、不依赖 llm-adapter** `[stub/未接线]`。
- **无对外 SDK、无 ACP、无 JSON-RPC server**。

**B 侧** `[完整]`
- SDK 协议：**newline-delimited JSON-RPC 2.0 on stdio**，方法集仅 **3 请求 + 4 通知**——client→server `initialize`/`session/prompt`/`shutdown`；server→client `session.event`/`session.status`/`subagent.started`/`subagent.finished`；`serverInfo.name` 冻结为 `deepseek-harness-sdk-runtime`。无 handler 回 `-32601`、handler 异常 `-32603`。**已知缺口**：server→client request 双方均未实现 `[未接线]`。
- TS SDK：两层 `DeepSeekHarness`/`HarnessSession` over `HarnessClient`；**进程生命周期归 client**（`dshBin` 缺省解析同版本 `@deepseek-ai/dsh`，以 `--profile` + 有序 `--patch` 启动；teardown 阶梯 stdin EOF→SIGTERM→SIGKILL）；`run()` 从 prompt 的 durable inbox receipt 走到下一个 whole-agent `idle`。**无 wire cancel、无 per-prompt result**。
- server：`HarnessSdkJsonRpcServer`，`initialize` 等 Loader 树 settle 后回复；每个 `sessionId` 一个 agent；subagent 完成仅当 lifecycle `local` 才转发；`shutdown` 后 dispose 根 context 并 exit 0；`maxTokensAsSuccess` 唯一配置项。
- Python SDK：`python/sdk/src/deepseek_harness/{api,client,models,errors}.py`，含 `next_request/respond` 供未来审批流；协议**镜像而非 import** TS 类型（改一端须同改另一端）；`python/sdk-runtime` 是平台 wheel，装 `dsh` console command + `deepseek_harness_runtime`，要求非空 `DSH_HOME`，**绝不回退 `~/.dsh`**。测试：`uv run --project python/sdk pytest`。
- ACP：`acp` service —— **automation-only ACP v1 stdio server**，方法是 `initialize`/`authenticate`/`session.new|list|resume|close|setConfigOption|prompt` + notification `session.cancel`；`AcpSession` 每 session 一个 handle、prompt slot、update chain、memoized close；多 session 并发、per-session 更新串行化。与 SDK 差异：ACP 是**标准协议**（走 `@agentclientprotocol/sdk` 的 ndJsonStream），SDK 是私有方法名；ACP 多 `session/list|resume|close|setConfigOption|cancel`，SDK 多 `shutdown`；ACP 有 permission prompt 与 MCP attach，SDK 无。**刻意省略的 wire 数据**：raw provider delta、retry attempt、DSH presentation card、plan/title/todo/terminal/elicitation、`session/load`、delete、fork、additional directories、SSE/ACP-transport MCP。
- 入口 profile：`headless`（一次性 core Agent/Session runner）、`sdk`、`sdk-minimal`（不继承 base）、`acp`、`web`、`desktop`。
- 双 SDK 同步机制：`scripts/snapshots/python-sdk-single-exe/` 快照 + TS/Python SDK 都在同一 PR 更新 loop 投影。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D20.1 | 对外协议 | 无 SDK；`agent-server` 是未接线的 axum 内存会话服务 `[stub]` | 私有 SDK（stdio JSON-RPC，3 请求 + 4 通知）+ 标准 ACP v1 | A 侧缺自动化入口 |
| D20.2 | 多语言 SDK | 无 | TS + Python（协议镜像而非 import，同 PR 同步） | — |
| D20.3 | CLI 运行时 | **唯一使用 `ConversationRuntime` 的生产端**；流式阻塞非增量 | 所有 profile 共用 agent-loop；流式是 waterfall | A 侧 CLI 与 web 行为可分歧 |
| D20.4 | CLI 解析 | 手写 argv（未用 clap）；`args.rs` 是 clap 版死代码 | `parseDshArgs` + `--profile` 分派 + patch overlay | — |
| D20.5 | slash 命令 | 25 个变体（含未实现的 branch/worktree/commit-push-pr） | `commands` 注册表 + `/` 菜单 | — |
| D20.6 | headless 形态 | 无（CLI 近似） | `headless` bundle 专门一次性 runner | — |

## 领域 21 测试与质量门禁

**A 侧** `[部分]`
- 根测试：`tests/module_linkage_smoke.rs`（42 行，4 个测试，均为链接性 + 单点断言：computer-use 锚点 `anchor_to_physical_pixel` 期望 `(86,173)`、vision `parse_relative_point`、`server::app` 可构造、`runtime::Session` 消息类型）。根 `src/lib.rs` 5 行纯注释，**仅为 tests/ 存在**。
- 单元测试数（`#[test]`+`#[tokio::test]`）：web-console 976、core-runtime 182、llm-adapter 96、CLI 87、tool-registry 43、computer-use 42、vision 36、plugin-system 28、command-router 20、diagnostics 20、clawbot-sidecar 19、desktop-console 16、uia-resolver 5、compat-harness 3、agent-server 2；**0 个**：`windows-process-guard`、`tauri-shell`、`language-service`。
- 契约测试与 mock：`llm-adapter/tests/`（4 文件 1,073 行）用 `tokio::net::TcpListener` 起本地假 HTTP server 并捕获请求（`CapturedRequest`）→ **有离线 mock 模型服务**；`web-console/tests/` 的 `chat-tool-status.test.cjs`(138)/`ui-control-contracts.cjs`(226) 用 `node:test`+`vm`+最小 DOM stub 直接执行真实 `src/chat_experience.js`；`browser-extension/tests/browser_extension_contract.test.mjs`；脚本级 7 个 `scripts/test-clawbot-*.mjs`（mock ilink provider + 本地端口）+ `test-package-safety/manifest/project-delivery/powershell-script-compat`。
- 缺口：**无 snapshot/recorded-session 框架**；`.github/` 只有 `ISSUE_TEMPLATE/`，**无 CI workflow**，脚本全靠手工执行；`docs/testing-standard.md` 要求的 `tests/manual-visual-confirmation.md` **不存在**；`docs/testing/` 实为 release-0.2.12/13/14 的证据与契约文档（1,503 行），非可执行套件。`docs/repository-structure.md` 描述的 `tests/README.md`、`docs/README.md`、`docs/migration-notes.md` **均不存在**。

**B 侧** `[完整]`
- test-support 7 包：`agent-loop-testkit`（真实 Agent + 生产 AgentLoop driver + 可替换 Inbox）、`client-runtime`（`SlotTestRuntime.create()`、jsdom、真实 web bundle roster + endpoint 级 Remote mock，未 stub 即显式失败）、`llm-mock-server`（OpenAI 兼容 HTTP/SSE 可编排故障：reset/stall/坏 chunk/429/5xx/工具调用）、`llm-replay`（**无 key**，从录制 Session JSONL 重放；每个 parent/subagent session 按首呼序取脚本；`replay.override.json` 表达 durable 无法重建的 pre-chunk 失败/取消/挂起/注入重试）、`loader-smoke`（从真实 bin + `cordis.yml` 在临时目录启动 fixture）、`remote-mock`（`mock.remote.<ns>.<method>`，同一函数服务直调与真实 Connection；流需显式声明；`assertNoUnmatched()` 收尾）、`session-snapshot`（keyless snapshot 支撑库：闭合 manifest v1、身份脱敏、标准化、workspace 比较、fixture guard、**headless/SDK/ACP/Web 四协议 adapter**）。
- snapshot 框架：`snapshots/` 顶层 `session/`（约 40+ 场景）、`acp/`、`sdk/`、`web/`；引擎在 `test-support/session-snapshot/src/suite.ts`，mode `replay|record|refresh`（由 `$DSH_SNAPSHOT` 推导），`recorded` 标记决定 `test:snapshot:record` 是否用真 API 重录（record 模式跳过 authored 场景）；**重放不匹配即失败**；预期输出 owner-local。
- benchmarks：7 场景（`agent-continuation`、`conversation-fold`、`session-open`、`long-session-browser`、`active-stream-reconnect`、`terminal-io`）+ `support/built-worker` + `calibration`。
- 门禁：聚合器 `scripts/run-gates.ts` 的 `gatesForMode`，模式含 `ci-primary`/`ci-static`/`ci-lint-contracts-ready`/`ci-coverage`/`ci-bench`/`ci-snapshot`/`ci-artifacts`/`ci-consumers`/`ci-windows-blocking|complete|observational`/`doc-sync`/`doc-quick`/`hygiene`/`check-all`/`node-compat`，带并发与 fail-fast。代表性静态门禁（~14 个）：`verify-runtime-closure`、`verify-cordis-config`、`verify-client-domain-graph`、`verify-module-graph`、`verify-default-product-isolation`、`verify-application-entrypoints`、`verify-package-dependencies`、`verify-dsh-package-licenses`、`verify-package-invariants`、`verify-optional-dependency-imports`、`verify-client-packages`、`verify-client-ui-i18n`、`verify-no-bare-dispatcher`、`verify-node-next-types`。
- 覆盖策略：`pnpm run test:coverage` 是 CI 覆盖门禁（**per-file 100%** on `packages/*/*/src`）。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D21.1 | 集成测试形态 | 单一 linkage smoke（4 断言） | loader-smoke + agent-loop-testkit + 四协议 snapshot | — |
| D21.2 | 录制重放 | 无 | `llm-replay` + `session-snapshot`（keyless replay，`replay.override.json`） | A 侧缺 |
| D21.3 | mock 设施 | 本地假 HTTP server（llm-adapter）+ node:test 前端契约 | `llm-mock-server`（可编排故障）+ `remote-mock` | B 侧故障注入面更宽 |
| D21.4 | CI | **无 CI workflow**，脚本手工跑 | 多模式 `run-gates` + CI 平台矩阵 | A 侧缺 |
| D21.5 | 覆盖门禁 | 无 | per-file 100% | — |
| D21.6 | 前端测试 | `node:test`+`vm`+DOM stub 执行真实 JS | jsdom slot bench + 真实 bundle roster | 都测真实前端 |
| D21.7 | 性能基准 | 5 文件命中 benchmark 关键词 | 7 个场景 + calibration + built-worker | — |

## 领域 22 打包、分发与运行形态

**A 侧** `[完整]`
- `package.ps1` 只接受 `all`，转发 `scripts/package-all.ps1`（`package.json` 暴露 `package:all`）。流程：读 `config/package-manifest.json` → 校验路径不越 workspace → **擦除并重建 `package/` staging** → 逐 artifact `cargo build --offline --target-dir modules/<mod>/target`（**各模块独立 target**）→ 比 source/target SHA-256 + mtime，**仅变化才复制**，旧文件备份 `tmp/package-backups/<name>/` 保留 `backup_keep=10` → resources 经 `Assert-PackageChildPath` 防越界 → 调 `package-safety.ps1` → 写 `package/package-report.json`（id/source/target/copied/sha256）。
- 隐私扫描：`scripts/package-safety.ps1` 的 `blockedPathPattern` 拦 `.coolzhu/.git/.claude/.superpowers/__pycache__/backups/tmp/logs/sessions/web-sessions/coolzhu.toml/*.sqlite3|db/*.pem|key|pfx|p12/credentials|secrets|token-cache`；对 `textExtensions` 白名单**逐行**扫 credential/bearer/url-credential/private-key 四类正则（含 test/dummy/example 豁免）。
- MSI：`scripts/build-msi.ps1`（272 行）用 WiX 5.0.2（`tmp/tools/wix/wix.exe` 或本地 dotnet 跑 `wix.dll`），源 `installer/Product.wxs` + app icon，输出 `dist/CoolzhuAgent-<version>.msi`，注入 `git rev-parse HEAD`/target/version 元数据与 `installer-report.json`。
- 源码交付：`scripts/project-delivery.ps1`（428 行）白名单 inventory → `Test-DeliveryExcludedPath` 按扩展名/目录排除（含 `.zip/.exe/.dll/.gguf/.onnx`）、跳过 >50MB → per-file SHA-256 + git status → 写 `SOURCE-MANIFEST.json`（`{generated_at, workspace, file_count, total_bytes, files:[{relative_path, size, sha256, repository, git_status}]}`，BOM UTF-8，983 文件/136.8MB）与 `SOURCE-EXCLUSIONS.md`（固定 4 段文案）→ `ZipFile::CreateFromDirectory` 打包后 `ExtractToDirectory` **往返校验**（逐条比对 sha256 且复查排除路径）→ `SHA256SUMS.txt` + obsidian-vault 投影。
- 缺口：`project-delivery.ps1` 默认 `-CurationMap = docs/obsidian-curation-map.json` **不存在**，非 `-InventoryOnly` 时直接 throw；仓库根提交的 `SOURCE-MANIFEST.json` 为 2026-07-28 生成，**与源码不同步**。
- 10 artifacts：web-console、browser-native-host、clawbot-sidecar、cli、computer-use-check、vision-smoke、latest-desktop-vision、tauri-shell、WebView2Loader.dll、COOLZHU-AGENT.exe；12 resources（tools/models/assets/browser-extension/local-vlm/uidetr/tauri-ui/docs），其中 `gui-web.stt-models` 源 `web-console/models` **目录不存在**（`optional`，静默跳过）。

**B 侧** `[完整]`
- 分发：npm/pnpm 包（`@deepseek-ai/dsh` 及 291 个 `@deepseek-ai/dsh-*`）；6 个 bundle 是分发单位（`base`/`web-app`/`headless`/`sdk-app`/`sdk-minimal`/`acp-app`）；`OPTIONAL_BUNDLES` 标记 shipped-but-off 不可移除 bundle。
- 安装与运行：`npx @deepseek-ai/dsh web`（README）；`dsh plugin --profile <name> add <package>` 经 pnpm 装插件；`bundled pnpm + Electron Node mode`（Desktop）；`packages/bundle/sdk-minimal` 自含完整树。
- native：`native/system` = `@deepseek-ai/node-addon-system` —— `landlock-run`（导出 launcherPath/probe/grantArgs，Linux 静态可执行，需 enforcing kernel，否则 probe 不可用）与 `flock`（`tryLockExclusive(fd)` 非阻塞，EAGAIN/EWOULDBLOCK）；平台包 darwin/linux × x64/arm64，**安装不编译原生代码**，Windows 用既有实现。
- Python：`python/sdk-runtime` 平台 wheel，装 `dsh` console command + `deepseek_harness_runtime`，要求非空 `DSH_HOME`；`runtime/node/` 载体是 dev-only。
- 桌面：`apps/desktop` Electron 壳（bundled Python/Node/pnpm runtime，Office skills 默认注册）+ `apps/desktop-host`（private）。
- 未见采用 MSI/WiX；**已有 Electron 安装包工程**：Windows NSIS、macOS dmg/zip，并有打包与安装器测试脚本（`apps/desktop/package.json:10,22-34`、`scripts/electron-builder-config.mjs:130,184,190`）。本次未重新构建或安装这些DSH制品，代码存在不等于本机发布验证通过。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D22.1 | 分发单位 | MSI 安装包 + `package/` staging | npm 包 + bundle/profile | 硬差异 |
| D22.2 | 构建产物 | 10 artifacts + 12 resources，独立 target，SHA256 增量 | tsc `lib/` + tsdown bundle；native 平台包不编译 | — |
| D22.3 | 隐私/安全扫描 | `package-safety.ps1` 逐行正则扫 4 类凭证 | 未找到同类脚本（靠 `.gitignore` + hygiene 门禁） | A 侧更显式 |
| D22.4 | 源码交付清单 | `SOURCE-MANIFEST.json` + ZIP 往返校验（**内容已过期**） | 无（git 即交付） | — |
| D22.5 | 原生代码 | Rust 全量原生 | 仅 `landlock-run` + `flock` 两个 addon，安装不编译 | — |
| D22.6 | 插件安装 | 拷贝到 install_root + installed.json | pnpm 安装 + profile patch | — |

## 领域 23 文档体系与自我描述

**A 侧** `[完整，有落差]`
- Agent 规范：三份 —— `AGENTS.md`(5,684 B) 与 `CLAUDE.md`(5,690 B) 内容相同仅首行对象不同（**无 symlink，双份维护**）；`AGENT.md`(4,667 B) 是 Agent Tool Calling Guide（Hard Rules / Working Approach / Common Tool Demos / Bad Patterns / Goal Artifact Checklist，`antml:` 工具调用协议），属使用约定非产品实现。
- 开发规范：`docs/development-standard.md`（127 行 12 节）—— config-first 禁业务环境变量、模块间只走已文档化接口、GUI 不得实现 core-runtime/vision/computer-use 业务、`cargo fmt/check/test -p <pkg> --offline`、模块独立 target + manifest 声明 + SHA256 比对 + 保留 10 次备份、用户可见功能须真实前端验收、一因一回归测试、禁硬编码端口/路径/模型名/开关、高风险修改先备份、启动自检 `tmp/logs/package-selfcheck-last.json`、err 日志最低字段 `event/module/level/err_kind/message/code_site/trace_id`、本地模型容量与 Provider 边界、用户可见身份统一 `coolzhu-model`。
- 接口契约：`docs/interface-contracts.md` 要求跨模块接口变更先改 `INTERFACE.md`、字段删除先 deprecated；**9 个模块均有 `INTERFACE.md`**。
- 文档规模：`docs/` 含 architecture-review、jade-bamboo-layout、release 变更与测试报告、UI 改进审阅、command-line、three-column-layout-plan、design-assets、github-issue-workflow、github-public-docs.json、github-public-index.md、packaging-and-device-migration、plans、repository-structure、superpowers、testing、testing-standard、user-guide、work-logs。
- **已知落差**：`docs/repository-structure.md` 描述的 `tests/README.md`、`tests/manual-visual-confirmation.md`、`docs/README.md`、`docs/migration-notes.md` **均不存在**；README 声称的"路径影响分析""输入安全控制"与代码不符（见 D5.5 / D9.10）；`docs/coolzhu-three-column-layout-and-agent-backend-plan-2026-08-24.md` 计划中的 `queue`/`steer`、`session_checkpoints`、`client_message_id`、审批持久化**未落地**（代码中无 `client_message_id`、无 checkpoint API）；`docs/2026-09-19-ui-improvement-review.md` 自述"第五优先级审阅稿，未实施整体美化"，含 UI-01~UI-20 问题。

**B 侧** `[完整]`
- Agent 规范：单份 `AGENTS.md`（+ `CLAUDE.md` **symlink**）+ `packages/AGENTS.md`（子规范）。
- 文档：`docs/` 543 个文件 + **website/ VitePress 投影**（`pnpm run website:build` 兼作死链检查）；**双语**（`.zh.md` + `.i18n.yaml` 配对 + `verify-*` 门禁 + `word budgets` 门禁 `verify-doc-budgets`）；`docs/AGENTS.md` 是文档规范；`docs/architecture.md`、`docs/glossary.md`、`docs/defensive-patterns.md`、`docs/testing.md`、`docs/development.md`、`docs/cordis-primer.md`、`docs/rescope.md`、`docs/cookbook/`（含 `adding-a-tool.md`、`reviewing-persistence-type-changes.md`）、`docs/subsystems/client-modules.md`、`docs/tool-execution-pipeline.md`（生成图）。
- 决策记录：`.agents/notes/`（implemented/ 分类 + archive policy：archived notes 冻结，不可编辑或当权威）；`.agents/skills/` 12 个 skill（含 `dsh-doc`、`dsh-code-review`、`dsh-prose-standard` 等）。
- 门禁整合：`doc-sync` / `doc-quick` 模式；`dsh-translate-docs` skill 仅可显式调用。
- 规范强度：AGENTS.md 内即是强约束（注册即 effect、waterfall 必须 next、Model-visible ⟺ logged、禁止硬编码 tunable、品牌 id、Source plane vs artifact plane 等），且多条挂到**执行的门禁**上。

**差异点**
| # | 维度 | A 侧 | B 侧 | 整合含义 |
|---|---|---|---|---|
| D23.1 | Agent 规范文件 | 3 份，AGENTS/CLAUDE 双份维护（无 symlink） | 1 份 + symlink + 子规范 | — |
| D23.2 | 双语 | 无（纯中文） | zh/en 配对 + i18n 门禁 + word budget 门禁 + VitePress 投影 | — |
| D23.3 | 接口契约 | 每模块 `INTERFACE.md`（9 个）+ 变更流程 | JSDoc + 生成文档 + `verify-export-jsdoc` 门禁 | 机制不同 |
| D23.4 | 决策记录 | work-logs / plans / issues（按时间） | `.agents/notes/`（按 implemented/ 分类 + 归档冻结政策） | — |
| D23.5 | 文档与实现一致性 | **多处落差**（见上） | 门禁驱动（doc-sync 等） | A 侧需一次性对齐 |

---

# 第八部分 命名与语义冲突清单

统一整合时最容易出错的地方：两侧同名但语义不同的概念。

| 术语 | A 侧 coolzhu 含义 | B 侧 dsh 含义 | 冲突级别 |
|---|---|---|---|
| **plugin** | manifest + **进程外子进程**，可任意语言 | **同进程** Cordis 模块，必须 JS/TS | 高 —— 不是同一抽象 |
| **skill** | `SKILL.md` 只读文本，截断 6000 字符进 phase prompt | `SKILL.md` + rank + invocation 控制 + 工具载全文 | 中 —— 格式兼容、能力不同 |
| **profile** | `PermissionProfile`（3 档**权限**：WorkspaceAuto/OutsideApproval/FullAccess） | 启动**组合**（web/headless/sdk/acp/desktop） | **高 —— 同词反义** |
| **session** | SQLite `sessions` 表 + 消息表 | append-only 事件日志 + `SessionEventMap` | 高 —— 存储模型不同 |
| **sandbox** | 未见统一进程沙箱；每工具requiredPermission在CLI有消费者，Web动态工具回退需审计；manifest权限标签不等同OS隔离 | 进程级 confinement（4 平台后端）+ fail-closed | 高 |
| **hook** | shell 脚本，exit 0/2 约定，2 类事件 | Claude Code/Codex 兼容桥，仅 command handler，7 类事件 | 中 —— 同名不同协议 |
| **tool** | 静态 `ToolSpec` 数组 + 三档暴露策略 | `ctx.tools.register()` effect + waterfall 流水线 | 中 |
| **MCP** | 4 transport + OAuth，`ScopedMcpServerConfig` | 2 transport，一实例一 server，`mcp__<s>__<t>` | 中 —— 命名规则不同 |
| **provider** | `ProviderKind` enum（模型厂商） | 通用 provider 模式（llm/sandbox/web/skill/browserUse/computerUse 皆有） | 中 —— 抽象层级不同 |
| **workspace** | 无独立实体（配置根 + `paths.data_dir`） | `ctx.workspaceRegistry` 命名有序实体 + 归档 | 中 |
| **command** | `command-router` slash 命令（与模型工具无关） | `ctx.commands` 注册表（UI command plane） | 低 —— 语义接近 |
| **guard** | 无 | `ctx.guards` monotonic guard（只能 deny/abstain） | 低 |
| **compact / 压缩** | auto-compact 摘要回灌为 bead | 三触发 + 追加式 checkpoint + 剪枝 + 图片卸载 | 中 |
| **agent-server** | axum 会话托管（**未接线**） | 无同名概念（用 profile + SDK） | 低 |
| **permission mode** | 5 档 enum + 3 档 profile + 3 档字符串 | sandbox mode × approval policy + preset | 高 |
| **memory** | L0–L4 beads + 向量 | 无 | 低 —— A 侧独有 |
| **plan** | 无 plan 模式 | `plan-mode`（协作状态，非权限档） | 中 —— 若整合易误认为权限档 |
| **task** | Goal phase / `ConfigScheduledTask` | `ctx.jobs` job / Agent Teams `team_task_*` | 中 —— 三个"task" |
| **team / roster / mailbox** | 无 | Agent Teams（roster + mailbox + task board）`[实验性]` | 低 |
| **audit** | `tool-audit.jsonl` + CU + 群聊（文件与表） | session event（`approval/asked`+`decided`） | 中 |

---

# 第九部分 统一整合的关键决策点

按依赖顺序排列。上游决策会约束下游，建议自上而下逐个拍板。

### 决策 1：权威状态模型（P5）— 最上游
- **选项需要分两层**：先确定单一权威写者、事务边界、事件语义与可重建投影，再选择SQLite或JSONL等物理介质。推荐先保留A的SQLite单权威，在同一事务中追加事件/outbox；JSONL可作导出/审计副本。SQLite也能承载append-only事件；没有冲突/提交协议的“热SQLite、冷日志双权威”不列为可直接实施方案。
- **影响面**：会话持久化、投影/统计/轨迹、压缩可重建性、迁移机制、崩溃恢复、跨进程并发、查询实现——即领域 10/11/12/18
- **A 侧改造量**：若要接受事件日志权威，需把 `sessions`/`session_messages` 的写路径改为 append-only 事件 + 投影；`Session.version` 与 `PRAGMA user_version` 双版本要归一
- **B 侧改造量**：若接受 SQLite 权威，需替换 `session-persistence-jsonl` 与 `session-format*` 全家，并重做 `Model-visible ⟺ logged` 断言（该断言依赖 `deriveMessages()`）

### 决策 2：扩展机制（P2）— 决定插件生态能否合并
- **选项**：① 统一为同进程 service 容器（B 侧）；② 统一为进程外 manifest（A 侧）；③ 双层（核心同进程 + 第三方进程外）
- **约束**：A 侧是 Rust，B 侧是 TS。若统一进程内，则必须选定单一宿主语言；若统一进程外，则 B 侧的 55 个 client UI 包与 service seam 无法直接沿用
- **观察**：A 的manifest权限标签、每工具requiredPermission与OS隔离必须分开审计：CLI消费每工具权限，Web未知/动态工具解析存在待验回退路径。B 的同进程VM扩展明确非containment，但平台进程沙箱另有实现，不能合并成同一结论。先统一实际执行闸门，再决定扩展宿主。

### 决策 3：前端形态（P6）
- **选项**：① 采用 B 侧（React + Vite + 55 包 + keyed slot）；② 采用 A 侧（原生 JS 内联）；③ A 侧保留原生 JS 但引入 slot 注册表与独立构建
- **影响面**：领域 17 全部、以及 UI 扩展点（D17.3）能否存在
- **最小可行**：不换框架也可先做"注册表化"（把 `CHAT_TOOL_WINDOW_META` 冻结常量改成注册表 + 白名单查表）并让前端资源脱离编译期内联

### 决策 4：权限模型映射（D9.1/D9.2）
- 需产出映射表：A 侧 `PermissionMode`(5) / `PermissionProfile`(3) / chat room 字符串(3) ↔ B 侧 `sandbox mode`(3) × `approval policy`(2) + preset
- 必须分开定义debug/release与用户显式配置：A当前debug缺省开发完全访问、release缺省关闭该开关；B的read-only+ask需按目标profile验证。不得用一个默认档覆盖全部入口。
- 需决定 `Protected 规则`（A 侧有）与 `WIDER_MODES 严格更宽表`（B 侧有）如何共存
- 需决定授权粒度：A 侧会话级 TTL 缓存 vs B 侧 allowed-once

### 决策 5：进程沙箱的采纳范围（D9.7）
- A 侧**无进程沙箱**，B 侧有 4 平台后端且 fail-closed
- 若采纳，Windows 侧可参考 B 侧 `sandbox-windows-acl`（ACL restricted token）而非同进程 vm
- 注意 B 侧 `sandbox-local` 的 fail-closed 语义：**confinement 不可用时不返回原始 argv**

### 决策 6：配置归一层（D7.1）
- 必须先选定权威文件与层级（A 侧候选 `coolzhu.toml`；B 侧 `settings.yaml`）
- 需决定默认值归属：引入显式 `resolve(request): Spec` 还是保留 `unwrap_or_else` 链
- 需决定会话级覆盖写哪（A 侧现在写回配置文件，B 侧写 session log）

### 决策 7：迁移机制（D10.2）
- 建议直接采纳"相邻步强制 + 每步独立单元 + 静态清单"三件套，无论权威介质是哪个
- 需决定超前版本策略（B 侧是 `unsupported` 拒绝不降级）

### 决策 8：编排范式（D14.1）
- A 侧静态 DAG（`depends_on` + `verification` + `human_ack`）与 B 侧脚本化 fan-out（workflow）**范式正交**，可共存而非二选一
- 若共存需定：phase 与 workflow agent 的关系、`maxDepth`/`maxActive` 闸门放在哪、`requires_human_ack` 用哪个审批通道

### 决策 9：记忆体系归属（D12.1）
- A 侧 beads（L0–L4 + 取代 + 衰减 + 污染过滤 + 只沉淀成功证据）是 B 侧完全没有的资产
- 需决定：记忆写在权威日志内（可重建）还是独立存储（A 侧现状是 SQLite 表）
- 需决定压缩产物与记忆的关系（A 侧是回灌为 bead，B 侧是替换消息）

### 决策 10：MCP 层归一（D16.1/D16.3）
- transport 集合取并集（A 侧 4 种含 SDK 与 managed-proxy）还是交集
- 工具命名前缀两侧都是 `mcp__<server>__<tool>`；需决策的是 server/tool 名规范化、长度与碰撞处理，以及旧会话工具名兼容映射，不能仅比较 A 的函数名与 B 的输出字符串。
- 认证：A 侧 `oauth.rs` 有实现但明文存储，B 侧有 `authorization` seam 但无 shipped flow

### 决策 11：工具暴露与模型面协议（D5.4）
- 是否引入 PTC（模型写程序）形态；若引入需定脚本运行时（Node / Python / 其它）
- A 侧"三档暴露策略 + 权限 allowlist"与 B 侧"sandbox/approval 决定"如何对齐

### 决策 12：CLI/SDK/自动化入口（D20.1）
- A 侧无对外 SDK；B 侧有私有 SDK + 标准 ACP
- 若采纳 ACP 作为统一自动化入口，需定哪些内部数据上 wire（B 侧刻意省略了 presentation 数据）

---

# 第十部分 缺口与未完成项汇总（两侧）

统一整合时容易被忽略的部分。以下为静态审计结论及明确边界，非逐项产品验收；本轮纠错与新增专项见文首和文末。

### A 侧 coolzhu
| 项 | 状态 | 证据 |
|---|---|---|
| 产品主路径不使用 `ConversationRuntime` | 两套主循环并存 | `grep -c ConversationRuntime web-console/src/main.rs` = 0 |
| 9 个内置插件 | `[未接线]` 注册 0 扩展点 | 全部 `plugin.json` 无 `tools`/`hooks` |
| 3 个插件目录（agents-md-updater/claude-compat/opencode-sync） | `[stub]` 只有 manifest，未列入 workspace | `Cargo.toml` members 清单 |
| `coolzhu-orchestrator` | `[未接线]` 完整实现但无调用方 | 仅 Cargo.toml/manifest 引用 |
| `coolzhu-marketplace` | `[stub]` 内存 HashMap，`install()` 只计数 | `lib.rs:6-69` |
| web-console `api_plugins_install` | `[stub]` 硬编码文案，自述 P2 接入 | `main.rs:18942-18960` |
| `agent-server` | `[未接线]` 无消费者、无持久化、无认证 | 仅 tests 使用 |
| `language-service` | `[未接线]` 无生产构造点，测试需 python3 | `LspManager::new` 仅 `#[cfg(test)]` |
| `desktop-console` (egui) | `[未接线]` 未纳入发布/启动链 | 全仓无引用 |
| `vision-service` 的 Describe/Ocr/RealtimePerception | `[stub]` 返回 "model calls are disabled" | `lib.rs:598,605,626` |
| OCR | **不存在**（OcrTemplate 是像素连通域扫描） | `main.rs:20324-20379` |
| `uia-resolver` 系统控件 | 仅 `StartButton` 实现，其余 10 个 `[stub]` | `lib.rs:192-279` |
| Computer Use 输入控制 | 已有敏感语义、目标/长度、授权和真实输入边界；完整覆盖待验 | `controller.rs:92,201,541`、`computer_use_desktop_bridge.rs:162`；见03专项 |
| 路径影响分析 | **不存在**（README 声称有）；只有静态入参抽取 | `path_effect.rs` |
| `cross_verify_tolerance_px` | `[死代码]` 配置字段无消费者 | `main.rs:5600` |
| `local_backend::uncrop_point`/`anchor_region` | `[未接线]` 仅自测引用 | `local_backend.rs:106,126` |
| `move_mouse_absolute`/`mouse_button_down_point` | `[死代码]` | `input.rs:86,90` |
| CLI `args.rs`（clap 版） | `[死代码]` 未 `mod args`，Cargo.toml 无 clap | 104 行 |
| CLI `app.rs` | `[死代码]` 未 `mod app` | 402 行 |
| `api_system_self_update_plan` | `[stub]` 只返回静态 plan 文本，无 promote/rollback | `main.rs:8915` |
| `/api/diagnostics/stream` | `[stub]` 全部字段硬编码 ok/true | `main.rs:18504` |
| 日志轮转/清理 | **不存在** | `output.rs:22-27` |
| 日志读取 UI | **不存在** | 仅显示路径 `app.rs:780-783` |
| 轨迹页面 | **不存在** | 仅 docs 提到 |
| 会话导出 | **不存在** | 无 export 路由 |
| snapshot/checkpoint | **不存在** | 无 checkpoint API |
| CI workflow | **不存在** | `.github/` 只有 ISSUE_TEMPLATE |
| snapshot/recorded-session 框架 | **不存在** | — |
| i18n | **不存在** | — |
| 遥测外发 | **不存在** | — |
| webhook | **不存在** | grep 0 命中 |
| cron 表达式 | **不存在** | — |
| `project-delivery.ps1` 默认 curation map | **不存在**，非 InventoryOnly 时 throw | `:230-231` |
| `SOURCE-MANIFEST.json` | 2026-07-28 生成，与源码不同步 | — |
| `gui-web.stt-models` resource | 源目录不存在（optional 静默跳过） | `package-manifest.json` |
| 凭据加密 | **不存在**；OAuth 明文 JSON | `oauth.rs:258,276` |
| `PluginPermission` 强制点 | **不存在** | 仅定义/解析 |
| `clawbot_group_audit` | 无 TTL/清理/容量上限 | `clawbot_gateway.rs:478` |
| 文档声称但缺失的文件 | `tests/README.md`、`tests/manual-visual-confirmation.md`、`docs/README.md`、`docs/migration-notes.md` | `docs/repository-structure.md` |
| 计划未落地 | `queue`/`steer`、`session_checkpoints`、`client_message_id`、审批持久化 | `docs/coolzhu-three-column-layout-...:2026-08-24` |

### B 侧 dsh
| 项 | 状态 | 证据 |
|---|---|---|
| 长期记忆 | **不存在**（无 memory/recall/embedding package） | `find packages -iname '*memor*'` 仅测试辅助 |
| 成本计费 | **不存在**（`token-meter` 明确声明非计费输入） | `src/projection.ts:20-29` |
| 插件市场 | **不存在** | — |
| 凭据加密 | **不存在**（仅 0600 文件权限 + 结构化 redaction） | `credentials-local` |
| `authorization` shipped flow | **不存在**（无内置 OAuth） | seam 有、实现无 |
| `redactSecrets` union/transform 缺口 | `[部分]` secret 原样返回 + TODO | `settings/src/redact.ts:61-63` |
| `session-telemetry` 的 `full` 模式 | 仅 OTel FEEDBACK_ONLY 有实现 | `session-telemetry-otel/src/index.ts:46,48` |
| `SessionTelemetryBackend.flush` | 刻意留未实现 | `session-telemetry/src/index.ts:111,114` |
| sdk-protocol server→client request | `[未接线]` 双方均未实现 | `sdk/protocol/README.md:301` |
| SDK cancel / per-prompt result | `[未接线]` | `sdk-client` |
| `typert` 生成子集限制 | `[部分]` 通配 export、跨 face namespace re-export、泛型/computed Zod 根、跨 face schema 运行期 import 未支持 | `generator/README.md:180-184` |
| `typert-loader` | `[部分]` 仅发现 host `./typert` | README:248 |
| `client-ui-renderer` host `apply()` | 仅浏览器renderer的预期空宿主入口，不是功能缺口 | `src/index.ts:1-4`及client实现 |
| gateway SRC mode | `[实验性]` 无 strict descriptor 时的源码回退 | `api/gateway/README` |
| `agent-presets` superseded generation 回收 | TODO | `src/index.ts:787` |
| `workspace.create(path, title?)` 的 title | 失去最后一个生产调用者 | `workspace/src/index.ts:152` |
| `session-log-deepseek` | 两处 TODO（直调/stale 结果未定义；2xx 崩溃窗口重复重放无轻量 checkpoint） | `src/index.ts:162,190` |
| `hooks-codex` 的 `stop_hook_active` | 硬编码 false + TODO | `src/index.ts:260` |
| Claude Code hooks 兼容度 | 30 事件中 **23 个不支持** | `hooks-claude-code/README.md:174` |
| `tool-bash` 部署权限策略归属 | TODO（应属 `tools/pre-execute`） | `src/index.ts:6`、`src/background.ts:19` |
| `bash-local` 持久 cwd/PTY | TODO `XXX(stateful-shell)` | `src/index.ts:176` |
| `shell-env.list()` 不含内建 | TODO | `src/index.ts:176-177` |
| `terminal-bash` 三处 TODO | 会话不跨进程重启、无全屏 TUI/按键序列/resize | `src/index.ts:111`、`session.ts:249,500` |
| `webhook` | 无队列/重试/去重/完成状态；重复投递会重跑规则 | `webhook/README.md:12,17` |
| `session-persistence` | 无删除/保留 API；`list()` 无分页无过滤 | README 缺口段 |
| `session-format` | 最终事件数组与 seq 重映射 O(event count)；只支持相邻整数版本 | README 缺口段 |
| `session-format-catalog` | 仅首方 build 清单，外部迁移所有权/分发不支持；运行期插件无法补缺边 | `generated.ts:16,18` |
| `session-query-sqlite` `openAt:'never'` | 搜索报 `SESSION_QUERY_SEARCH_DISABLED` | `src/index.ts:92-118` |
| `browser-use`/`computer-use` base 包 | 无实际 provider（仅注册服务）；5 个实验 provider 无 shipped profile 接线 | `experimental/README.md:32-38` |
| `experimental/*` agent-team 5 包 | `[实验性]` 公开但无稳定性承诺、shipped profile 默认关闭 | — |
| `experimental-ptc-runtime-python` | `[实验性]` 无文件沙箱、无 shipped profile 启用 | — |
| Windows session 写者互斥 | 有实现；无 lock 文件，改用命名内核信号量；本次未运行跨进程竞争测试 | `session-persistence-jsonl/src/lease.ts:1-12,78-85`、`win32.ts:151-160` |
| credentials POSIX 权限检查 | Windows 跳过（ACL 不可表达） | `credentials-local/src/index.ts:121,138` |
| `ui-trajectory` | `traceSession` 只有 API，**无 UI 入口**（fork 谱系页不存在） | `session-query` |
| 系统级通知 | 无（schedule 明确不做 push/邮件） | `schedule/README.md:12` |
| 命令面板/全局快捷键 | **不存在** | 仅 `/` 命令菜单 + composer `+` |
| 无 `dsh-plugin` topic 约定实现 | — | — |

---

# 附：配套文件

- `02-coolzhu-dsh-功能全量清单.md` — 两侧**完整模块/包清单**（A 侧 28 个 workspace 成员含 9 插件 crate + tauri-shell 独立项目 + 3 skill；B 侧 291 包），逐项含职责、架构位置、对外接口、完成度标记。用于核对"是否有遗漏"。


---

# 2026-09-21 独立审查补充

# 架构差异原文复核记录

审查日期：2026-09-21。只读审查，未修改产品代码、用户运行数据或桌面原文。对象为 `C:/Users/zhupu/Desktop/01-coolzhu-dsh-架构差异与整合决策.md`，已分段通读全部 962 行；逐项证据校验聚焦会改变执行方向的结论，不代表执行了两个产品的全部能力测试。

## 审查口径与来源

下文 A 根目录为 `C:/Users/zhupu/Desktop/coolzhuagent`；B 根目录为 `C:/Users/zhupu/Desktop/dsh`。路径 `A/...:行号`、`B/...:行号` 指本次读取的本地文件。原文行号均为修改前版本。

B 目录真实存在，可读源码及文档，但不是 Git 仓库，无法从本地确认提交、远程来源或是否最新。`B/package.json:2-9` 声明版本 `0.1.6-alpha.2`、license `MIT`、Node `^22.19.0 || >=24.0.0`、pnpm `11.7.0`。`B/LICENSE:1-12` 为 MIT 文本，包含 DeepSeek 2026 版权及保留版权/许可文字的条件。这仅是本地许可声明事实，不能替代所有依赖、资产、外部 Cua Driver 的独立许可/版本审查。

关键文件 SHA-256：

| 文件 | SHA-256 |
|---|---|
| A `modules/gui-web/packages/web-console/src/main.rs` | `0a2d2a41c1d24113ae83a27842d25e3b20bf1298b4492a5ee67a7ac6aedcdf15` |
| A `modules/llm-adapter/packages/llm-adapter/src/request_parameters.rs` | `9cc27ce4e95e377f90bc318a8e50415dd206eef1ed282588d45b9f04fdda7cba` |
| B `package.json` | `a9cc164c4d922d74571e09c04be4e0f85a318685d410bb1c56072258a19a6f1d` |
| B `pnpm-lock.yaml` | `72523ca50bd34e3e21d07c82e1cedfdb91b17e1bcd83006c645e8dc6e0786fe0` |
| B `packages/session/session-persistence-jsonl/src/lease.ts` | `377f24b1446382d703d5f1346f44e15a25368a0e7967e5bd41db700a919d7ad8` |
| B `apps/desktop/scripts/electron-builder-config.mjs` | `b0b56ad40de62b7ee85837047a131aadf3c05379756d3f869717f5e31a1bc4a0` |

## 需要优先修正的事实

| 编号/影响 | 原文位置与问题 | 核实结果与证据 | 对方案的影响 |
|---|---|---|---|
| AR-01 高 | 125 行 D3.5 把 A 崩溃恢复写成“无（SQLite事务）” | A `main.rs:796` 启动调用 `recover_incomplete_runtime_runs`；`:41804-41824` 明确一笔 Immediate 事务处理 runtime orphan、Goal claim、Goal状态与事件，提交后广播；`:68787` 有幂等恢复测试。 | 改为“已有 run/Goal 启动恢复，缺工具副作用结果的完备重建契约”。不应重做为全新零基础恢复模块，更不能恢复时自动重放有副作用动作。 |
| AR-02 高 | 196/214 行将请求参数概括为 3 字段，max_tokens仅静态表 | A `request_parameters.rs:6-12` 还有 reasoning_mode，`:15-39` 有Qwen3.8校验，`:42-114` 显式原生编码；A `main.rs:5154-5183` 会话参数含协议/endpoint/预算/图片/工具等；`:27719-27733` 显式会话输出覆盖优先。 | 统一配置页已上线，下一阶段是归一解析责任、能力校验和迁移，并非再次建设同一页面。Provider枚举仍存在，但手工custom连接无需给每个模型新增厂商枚举。 |
| AR-03 高 | 250/266 行“API key全从env”；B“只存引用” | A `main.rs:29913-29941` 支持会话内字面值或文件引用，前端 `model_settings.js:186` 可保存api_key_ref且空值不覆盖；B credentials-local `src/index.ts:268-284,443-463` 将refs映射值和records写入YAML。 | 两侧都有本地凭据值，不能把B称作仅引用、不含密钥；迁移要保留实际来源/不回显/清空语义，不能假定env是唯一真源。 |
| AR-04 高 | 259 行 B 凭据优先级漏 provider-managed store | B `packages/credentials/credentials-local/src/index.ts:2-24`：继承环境 > `.credentials.yaml` > 调用目录.env > home.env；设置页保存值高于.env。 | 统一配置时必须显式显示来源及只读覆盖，避免UI保存成功却被旧.env覆盖。 |
| AR-05 高 | 328/345/951 行把“Windows无lock文件”推为缺跨进程租约 | B `packages/session/session-persistence-jsonl/src/lease.ts:1-12,36,78-85` 使用Win32命名内核信号量；`win32.ts:151-160` 调CreateSemaphoreW/WaitForSingleObject；无文件不等于无锁。 | 删除“为DSH补Windows租约”的虚假需求。跨进程竞争/进程死亡恢复仍应作为本机验证项，存在实现不等于实测通过。 |
| AR-06 高 | 303 行“Protected规则优先于profile” | A `core-runtime/src/permission_gate.rs:241-268` FullAccess先短路；Protected仅优先于普通workspace自动放行。 | 不能在权限映射中把现有FullAccess描述成仍会被Protected阻断。若计划改变该语义，必须显式列为行为变更。 |
| AR-07 中 | 300/827 行只写A默认WorkspaceAuto | A `main.rs:6332-6334` debug默认dev_open_permissions=true，release默认false；已有显式配置优先。 | 开发测试与正式默认须分别定义，保留用户已授权“调试默认完全访问”，不把debug行为误带入发布默认。 |
| AR-08 中 | 77 行正确区分PluginPermission，但818/786行扩大为requiredPermission完全无强制 | A `tool-registry/src/lib.rs:208-229` 将插件required_permission纳入permission_specs；CLI `main.rs:4242,4259`消费。A web `main.rs:2141-2146` 只查MVP且未知工具默认ReadOnly，路径需要专项核查。 | 应分清manifest权限标签、每工具最低权限、OS隔离三层；不能全称“从不生效”。优先统一所有入口的权限解析，检查未知/动态工具最低权限回退。 |
| AR-09 高 | 529/789/856 行MCP命名规则不同 | A `core-runtime/src/mcp.rs:26-35` 本就生成`mcp__<server>__<tool>`。 | 决策改为server/rawName规范化、长度、碰撞和兼容alias；不是选择两个不同前缀体系。 |
| AR-10 高 | 664 行“DPI/多显示器不处理” | A `uia-resolver/src/windows_impl.rs:27-52` 有线程PMv2及Drop恢复，`:413-442`测试；0.2.14报告已记录150%缩放失配修复。主屏捕获/多屏拓扑缺口仍独立存在。 | 不得推倒已修DPI；下一阶段测100/125/150/200%、跨屏/窗口移动，分别定义坐标空间和图像变换。 |
| AR-11 中 | 545 行把旧DOM三栏当当前可见布局 | A `chat_experience.css:2-4`隐藏左聊天导航/左resizer；`:6-14`顶部横排；0.2.14报告确认快捷轨+右扩展、顶部下拉。 | 布局目标已经部分实现，前端拆分需保持玉石色控件、竹林图层、对比度和下拉行为，不能依据旧DOM恢复三栏。 |
| AR-12 高 | 737 行 B“无MSI/无安装包/无WiX” | B `apps/desktop/package.json:10,22-34` 有installer测试及win/mac打包；`scripts/electron-builder-config.mjs:130,184,190`配置dmg/zip、NSIS、安装配置。 | 可说未见MSI/WiX，不能说没有安装包。是否采用Electron与是否有分发工程是两件事；保持现有MSI可行。 |
| AR-13 中 | 558/932 行空Host apply标成stub缺口 | B `client/ui-renderer/src/index.ts:1-4`明确browser-only，无host行为是设计；原文自身也指出真正registry在client。 | 把它归为职责分层的空宿主入口，不应排进“补完功能”工单。 |
| AR-14 中 | 243 行配置并发“未见写锁” | A `main.rs:6675` mutate_workspace_config；`:6692` 全局Mutex<WorkspaceConfig>。 | 已有进程内序列化，尚需核实原子落盘和多进程写者锁；不能声称没有任何锁。 |
| AR-15 高 | 810-813行权威介质与事件溯源语义绑定，双权威热冷列为平级选项 | B `core/agent-loop/src/invariant.ts:18-58`检查请求与durable推导/请求头一致，无JSONL物理格式要求；A已SQLite事件表和事务恢复。 | 可先在SQLite事务中保留一个权威事件/状态写点与outbox，逻辑事件可追加且可重放；JSONL可做导出/审计副本。禁止无冲突规则的SQLite+日志双写双权威。 |
| AR-16 中 | 690 行称CLI是ConversationRuntime唯一生产端 | A `tool-registry/src/lib.rs:2199-2208` 也构造子Agent runtime；原文106行本身已写两端。 | 要把CLI、子Agent和web全部列入统一回归矩阵，不能只迁移两个表面入口。 |
| AR-17 中 | 699/701行静态#[test]数量与报告实际执行数混为完成度 | A `docs/testing/release-0.2.14-agent-fixes-report.md` 总结1175 pass、0fail、1ignored，web988、adapter119，含集成/脚本等。源码计数不是运行测试总数。 | 记录测试命令、基线、skip原因与实际报告；不能把代码计数当本次全部重新跑过，更不能据此宣称Paint成功。 |
| AR-18 中 | 628/888行全称“无输入安全控制” | A computer-use `controller.rs:92-109,201,541-569` 有host敏感语义分类、风险审批；正式笔画有窗口/坐标/取消/释放约束；但无通用输入内容治理或OS沙箱。 | 改为精确能力范围，避免用字符串grep结果证明安全控制完全不存在。 |

此外：原文标题承诺“不做优劣排序”，但“更严格”“更独立完整”“产品化更好”“后续90%”等是评价或无分母推断。建议去掉比例、改成具体机制与验收边界。`[完整]`宜拆为“代码存在 / 产品接线 / 本机验证 / 发布验证”四列；实验性provider不能因基础包可注册即标端到端完整。

## 原文可保留的核心认识

1. A产品web主循环与CLI/子Agent runtime确实分离；B主循环/服务容器契约值得借鉴。不能直接切到现有CLI runtime，因为其流接口返回Vec而非真正增量，现有web才是已发布行为基准。
2. A原生JS/CSS与业务巨型main.rs存在维护压力；B keyed slot/类型契约/模块生命周期可作为结构参考。React/Vite不是必须一轮采纳的前提，先按边界拆模块/注册面板可以渐进完成。
3. A长期记忆、Goal DAG、UIA/原生输入/浏览器扩展、桌宠是保留资产。B已安装同进程插件不是安全容器，实验Cua Driver provider不是桌面绘画能力已验证。
4. A自动AGENTS.md注入在本次源代码检索中未见；有内联AGENT.md规则字符串，这不是目录级指令发现。补实现要明确信任级、目录作用域、编码/大小/符号链接和取消，不只是把全文塞进system。
5. B可借鉴的可执行契约包括迁移相邻步清单、请求重建不变量、frozen request、单调用身份/生命周期、录制重放、provider/slot注册生命周期。需给A编写自己的适配与回归，不能把291包直接当替换库集。

## 推荐整合路线及门禁

推荐保留Rust/Tauri/现有Windows MSI与SQLite权威，从当前web产品行为提取共享服务。DSH先借鉴契约；需要复用其运行模块时采用明确、可停止的进程适配试点，避免并行两个不互知的Agent主循环。没有代码证据要求先改用Electron、JSONL物理日志或整包导入DSH。

| 阶段 | 交付与依赖 | 完成门禁 | 失败/回退 |
|---|---|---|---|
| S0 基线与来源 | 锁A源码/安装包/用户数据快照，锁B版本+源码/lockfile哈希及来源；更正两份原文并生成状态矩阵 | 文档断言能定位到代码/实测；许可证/依赖清单可审计；公开材料无密钥 | 不引入未锁依赖，不改用户配置 |
| S1 运行契约 | 定义workspace/room/session/turn/step/request/tool/action ID、终态first-wins、取消、错误/timeout、usage来源；新增脱敏fixture | stream/nonstream/CLI/subAgent共享不变量测试；不能把input_sent当目标success，未知副作用不自动重放 | 保留现web入口，新模块可切回原投影 |
| S2 状态与恢复 | 在SQLite权威事务内追加事件/outbox；广播后投影有序重放，幂等客户端；版本相邻迁移、forward-version拒绝 | 故障注入：commit前/后、广播前/后崩溃；无丢事件/重复副作用；旧库可迁移且可备份恢复 | 暂停新增写者，恢复备份；JSONL只有导出/审计角色 |
| S3 主循环抽离 | 先提取请求解析、权限、工具调度、历史投影，再提取turn runner；HTTP/SSE/CLI成为适配层 | 同fixture在流/非流/CLI/子Agent表现一致；原网页9项和工具9项回归；不会丢Qwen/图片/用量能力 | 分模块开关/接口兼容；不一次搬动87k行 |
| S4 配置/凭据 | resolve(request):Spec、协议能力表、运行配置与包清单分离；保留已存key/base_url和空值语义 | saved→resolved→wire矩阵；未知模型手工配置可用；多进程冲突不丢更新；图片原图/Agnes转述不串路 | 保留兼容读，迁移不静默重写endpoint或key |
| S5 CU专项 | 由独立CU差异/失败矩阵细化；原生动作、视觉反馈、任务验收分别计分 | 受控画布可验证落笔；Paint真实任务证据，stale重观察而非放宽身份检查；成本/延迟/预算有实测 | 有界重规划，保持硬取消与释放，未通过如实blocked |
| S6 UI及扩展 | 面板注册与资源独立构建，保留既有视觉和顶部下拉；轨迹/搜索投影、按需详细审计 | 低高度/缩放/键盘/对比度、消息顺序、迟到事件、跨房间测试；美化方案用户审阅后实施 | 老面板适配器可回退；不整轮复制55个UI包 |
| S7 可选DSH适配 | 仅在上述边界稳定后试点一种provider/工具或PTC；显式版本握手、取消/错误映射、调用授权 | 单写者/owner隔离、无孤儿进程、依赖/许可证/安装产物审核、失败可卸载 | 禁用适配恢复Rust执行；实验provider不能默认发布 |

## 提交给GPT6 Pro应明确追问的架构问题

- 是否赞成“SQLite单权威+事务outbox+可重放逻辑事件”，并指出哪些产品用例确实需要换物理日志？请区分必要架构变化与偏好。
- 共享主循环的最小切口、模块所有权、取消/审批/使用量怎样从巨型main.rs迁出，又保持当前多模态、Goal和接力行为？给依赖顺序与回滚条件。
- Paint已有原生输入与截图，仍出现工具栏误选、画布未识别、工具状态误假设、stale拒绝、20s规划超时、外层总结不可靠。哪些可用Agent侧通用能力改进，哪些必须标模型限制？哪些方案只是在放宽错误成功标准？
- 先采用哪些DSH机制可在1-2个小范围迭代内产生收益？列出明确不采纳项，不能默认迁入全部包、同进程第三方插件或alpha provider。

## 关于Paint与完成度的硬边界

以A `docs/testing/release-0.2.14-agent-fixes-report.md` 的R1-R6审计为依据：已有DPI/动作schema/原图/drag/点击flags/空总结等Agent修复；R4/R5有实际拖拽但白画布仍空，R6只有1次实际点击及两次stale拒绝；海绵宝宝端到端未通过。1175项自动化通过不覆盖这个失败目标。下一版方案可以重新研究通用Agent改进，不能把前次“暂不优化模型绘画能力”解释成所有剩余问题已被证明纯模型问题，也不能反向把所有剩余失败都断言Agent缺陷。

原生桌面窗口的安装后启动曾被自动审批阻断；后台/无窗口验证成功。它是前次验证限制，不是本次架构审查新增的产品缺陷或本轮再次复现。


## 本轮新增专项及后续计划

原生Computer Use、ZCode接口契约、Paint R1–R6、Agent改进候选与实验门槛，见[03专项](03-coolzhu-zcode-Computer-Use差异与Paint专项.md)。整体执行计划将结合GPT6 Pro实际审查回复单独形成，不能把本节候选阶段当作已经实施。
