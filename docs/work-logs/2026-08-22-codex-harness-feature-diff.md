# Codex open-source harness 与 coolzhu agent 功能差异分析

日期：2026-08-22（Asia/Shanghai）
分析对象：OpenAI Codex open-source harness 与当前 `coolzhu agent` 工程
分析方式：源码静态盘点、接口/目录/测试数量对照；未把任一工程源码直接复制到另一工程。

## 1. 基线与来源

### 1.1 官方定义

官方资料将 Codex harness 描述为支撑 Codex App、CLI、IDE Extension 等体验的可复用 agent execution layer：它负责收集上下文、维护会话、流式执行、调用工具、执行沙箱与审批策略，并跨轮次继续工作。官方还说明 App Server 通过文档化客户端协议暴露 thread、turn、事件和审批请求。

- [Codex as a platform: build on the open agent harness](https://learn.chatgpt.com/blog/codex-as-a-platform)
- [Codex Open Source 组件清单](https://learn.chatgpt.com/docs/open-source)
- [Codex App Server 协议文档](https://learn.chatgpt.com/docs/app-server)
- [Codex CLI 文档](https://learn.chatgpt.com/docs/codex/cli)

因此，本次把官方列出的 `openai/codex` 仓库作为“Codex harness”代码基线，而不是把某个第三方包装器当作 harness。

### 1.2 本地副本

| 项目 | 本地路径 | Git 基线 |
| --- | --- | --- |
| Codex harness | `C:\Users\coolzhu\Desktop\codex-harness-src-20260822` | `51ebf5b1842d44a8e2c955e8b5cd2a589d41e71e`，2026-08-21 17:45:56 UTC，`main` |
| coolzhu agent | `C:\Users\coolzhu\Desktop\coolzhu-agnt-src` | `73cee4d3c526d9b55254ed5e19bf3acaeaa94d7a`，当前分支 `codex/full-selftest-fixes-20260813` |

Codex 副本使用浅克隆 `git clone --depth 1 https://github.com/openai/codex.git`，源码目录约 81.5 MB，远程为 `https://github.com/openai/codex.git`，克隆副本工作区干净。当前工程原有的 4 个未提交前端/后端文件保持不变。

### 1.3 规模信号（用于定位维护成本，不是质量评分）

| 静态指标 | Codex harness | coolzhu agent |
| --- | ---: | ---: |
| `rg --files` 文件数 | 6,307 | 964 |
| Rust 文件数 | 3,267 | 117 |
| Markdown 文件数 | 155 | 41 |
| `#[test]`/异步测试属性行（近似） | 16,171 | 1,481 |
| 根 Cargo workspace 包数 | `codex-rs` workspace 结构复杂，根目录没有 Cargo.toml | 28 |

这些数字说明 Codex 更像一个长期演进的运行时平台；coolzhu agent 的业务覆盖面更集中，但一部分协议、状态和 UI 逻辑仍聚集在大文件中。

## 2. 两个工程的架构盘点

### 2.1 Codex harness 的关键层

| 责任 | 主要源码位置 | 观察到的能力 |
| --- | --- | --- |
| agent core / thread runtime | `codex-rs/core/src/codex_thread.rs`、`codex-rs/core/src/session/` | 线程级配置快照、turn start/resume/recover/steer、取消、输入队列、背景终端、MCP 生命周期、上下文窗口和自动压缩 |
| 客户端协议 | `codex-rs/app-server-protocol/src/rpc.rs`、`protocol/common.rs`、`protocol/v2/` | JSON-RPC request/response/notification；强类型 request/response；thread、turn、workspace roots、审批、工具、文件系统和诊断 API；带 serialization scope 的并发约束 |
| app-server | `codex-rs/app-server/src/message_processor.rs`、`request_processors/`、`bespoke_event_handling.rs`、`transport.rs` | 把 core 事件映射为稳定的服务器通知；处理审批回调、turn 生命周期、工具输出、计划/差异/用量、WebSocket/stdio/远程连接 |
| history / persistence | `codex-rs/thread-store/`、`codex-rs/rollout/`、`codex-rs/state/`、`codex-rs/history/` | thread metadata、追加式 rollout、SQLite 索引/迁移、分页历史、搜索、归档、删除、恢复、fork、rollback/revert、队列和 goal 状态 |
| command / sandbox policy | `codex-rs/execpolicy/`、`codex-rs/sandboxing/`、`codex-rs/windows-sandbox-rs/` | 命令前缀策略、审批 reviewer、权限 profile、文件/网络边界、Windows 原生 sandbox（elevated/unelevated）、违规审计 |
| tools / MCP / skills | `codex-rs/tools/`、`codex-rs/codex-mcp/`、`codex-rs/skills/`、`codex-rs/plugin/` | 工具 spec/definition/search/discovery、dynamic tools、MCP resource/tool/event stream、技能解析与选择、插件清单和安装 |
| CLI / headless | `codex-rs/cli/`、`codex-rs/exec/` | 交互 CLI、`codex exec` 非交互任务、结构化 JSONL 事件、resume/fork、CI 使用、doctor/诊断 |
| 测试与可观测性 | `codex-rs/app-server/tests/suite/v2/`、`core/src/*_tests.rs`、`rollout/src/*_tests.rs`、`otel/` | 协议 fixtures、快照、app-server 端到端套件、sandbox/权限测试、迁移/恢复测试、OpenTelemetry 和结构化日志 |

### 2.2 coolzhu agent 的关键层

| 责任 | 主要源码位置 | 现状 |
| --- | --- | --- |
| 会话/对话运行时 | `modules/core-runtime/packages/core-runtime/src/conversation.rs`、`session.rs` | 已有 `ConversationRuntime`、工具循环、assistant event、session JSON 序列化和 token/compact 辅助；接口适合继续演进为 thread/turn runtime |
| 基础 agent server | `modules/core-runtime/packages/agent-server/src/lib.rs` | Axum REST + SSE；session 目前由进程内 `HashMap` 持有，端点主要是 create/list/get/message/events，事件只有 snapshot/message，尚未成为完整 turn/app-server 协议 |
| Web 主控制台 | `modules/gui-web/packages/web-console/src/main.rs` | 约 2.64 MB 的单一 Rust 文件，集中包含路由、SQLite、会话、聊天室、目标链、权限、工具、浏览器、Computer Use、音频和模型状态；前端为单文件 `app.js`/`styles.css` |
| workspace / room 权限 | `web-console/src/main.rs` 的 `api_set_workspace`、`reload_workspace_scope`、`chat_room_permission_*` | 已有 workspace 切换、workspace 级 session store 重载、聊天室权限 profile（`workspace-write`/`full-access`）、SQLite 审计；当前 UI 与后端状态仍通过大量 ad hoc handler/DTO 连接 |
| 工具注册 | `modules/tooling/packages/tool-registry/src/lib.rs` | 已有 builtin tool spec、执行器、插件工具、ToolSearch、Agent、Skill、MCP/WebSearch/PowerShell 等，权限由 core runtime gate 再评估；单文件约 181 KB |
| 权限与沙箱 | `core-runtime/src/permission_gate.rs`、`permissions.rs`、`sandbox.rs`、`bash.rs` | 已有纯函数权限闸门、workspace/protected path 判断、session grant、Linux sandbox 命令构造；Windows 原生强制隔离与命令策略还没有形成 Codex 那样独立的执行层 |
| Computer Use / browser / vision | `modules/computer-use/`、`modules/vision/`、`web-console/src/browser_bridge*.rs`、`computer_use_*.rs` | 这是 coolzhu agent 的明显差异化优势：本地视觉定位、UIA、坐标映射、输入安全、browser bridge、GLM-5.2/agnes 等本地或兼容模型接入 |
| 桌面与渠道 | `modules/gui-desktop/`、`clawbot`/微信相关模块 | 有桌宠、Tauri/桌面 shell、微信私聊/群聊、聊天室、实时语音和目标链；这些是 Codex harness 不应直接替换的产品层 |
| 测试 | 各 crate 内联单测、`tests/module_linkage_smoke.rs`、web-console 端到端/静态断言 | 已有不少单测和 smoke test，但跨进程协议、恢复/并发、权限矩阵和真实工具事件链的合同测试数量明显少于 Codex |

## 3. 功能差异矩阵

| 维度 | Codex harness 优点 | coolzhu agent 已有能力 | 主要差距 | 纳入判断 |
| --- | --- | --- | --- | --- |
| Agent loop | thread/turn 分离；可 start、resume、recover、steer、interrupt；有 active turn 和 input queue | `ConversationRuntime::run_turn` 已有基本模型→工具→结果循环；目标链另有 phase loop | 基础 server 没有 turn ID、取消、恢复、steer、队列的统一契约；网页端各流转可能重复实现 | **P0 纳入生命周期模型** |
| 协议 | 类型安全 JSON-RPC；v1/v2；显式 notification；schema/TS 导出；请求 serialization scope | Web console 使用许多 REST/SSE endpoint，MCP/LSP 自己有 JSON-RPC；事件总线分散 | 没有一套覆盖 web、CLI、桌面、渠道的公共 Agent Protocol；DTO 容易在 handler 和 JS 间漂移 | **P0 纳入协议边界** |
| 事件 | item started/updated/completed、turn diff/plan/usage/error、审批和工具事件均带 thread/turn 关联 | 有 tool/goal/pet/realtime broadcast、SSE、审计日志 | 事件类型和顺序没有统一序列号/关联 ID；恢复或断线重放能力弱 | **P0 纳入统一 AgentEvent** |
| workspace | thread start/resume/fork/turn/settings 都能传 `cwd` 和 runtime workspace roots；设置更新有明确作用域 | `active_workspace_path`、workspace reload、聊天室 workspace 设置已实现 | workspace 更新与当前会话/权限/工具 catalog 的生效边界需统一；当前逻辑集中在大文件 | **P0 纳入原子作用域快照** |
| 权限 | permission profile、approval policy、approvals reviewer、一次/会话/线程范围审批，事件化回调 | `PermissionGateReport`、room profile、session grant、审计已具备良好基础 | 审批等待、超时暂停、批准后重试、跨端审批回调还没有成为 runtime 原语 | **P0 复用现有闸门，补状态机** |
| 沙箱 | 独立 `execpolicy` + `sandboxing` + Windows sandbox；文件/网络拒绝可测试 | 有 Linux sandbox 解析和安全输入；Windows 侧主要依赖应用/工具层 | Windows native ACL/firewall/elevated runner、命令前缀策略、违规恢复需独立模块 | **P1，先做 Windows 命令执行隔离** |
| 历史/恢复 | rollout append log + SQLite state；分页读取、搜索、archive/delete/fork/revert/rollback、迁移 | web console 有 JSON/SQLite session/chat room/goal 数据，runtime 有 session save/load | 不同 store 的 source of truth 不统一；跨进程恢复与断线重放缺少统一实现 | **P0/P1，先统一 thread history schema** |
| 上下文 | context manager、模型 context window、auto compact、remote compact、保留摘要/usage | `compact.rs` 已有摘要、token 估算、关键文件/待办推断 | compact 触发、事件和恢复与 turn runtime 未完全绑定；模型 provider 差异未抽象到协议层 | **P1，保留现有摘要算法并接入生命周期** |
| 工具发现 | tool spec/definition/discovery/search、dynamic tools、MCP catalog/resource/event stream、apps | registry 已有多工具、插件、ToolSearch、Agent/Skill、MCP | registry 很强但执行入口被 web-console、runtime、插件分别包装；工具输出/错误 schema 不完全统一 | **P0/P1，先统一 ToolCall/ToolResult** |
| CLI/headless | `codex exec`、JSONL、resume/fork、CI/脚本友好、doctor | 有 command-line crate 和诊断命令 | CLI 与 web/桌面共用的事件/权限/恢复协议不完整 | **P1，增加非交互 JSONL adapter** |
| MCP/插件/技能 | 协议和生命周期在 app-server/core 有明确边界，技能与插件可发现/安装/测试 | 已有 plugin-system、MCP SSE/WS/stdio、skills 目录 | 缺少公共 capability catalog 和版本/权限/热更新事件 | **P1，吸收 catalog/动态工具契约** |
| Computer/Browser Use | Codex harness 提供协议和环境/审批层，可由宿主挂接能力 | coolzhu agent 已有本地视觉、UIA、浏览器 bridge、输入安全，场景更丰富 | 需要把这些能力注册为稳定的 tool/event contract，而不是耦合在 web-console 路由 | **保留 coolzhu 实现，接入 P0 协议** |
| 可观测性 | OpenTelemetry、diagnostics、record/replay、丰富事件和状态查询 | diagnostics crate、tool audit、goal/tool SSE 已存在 | 缺少按 thread/turn/call 的完整 trace 和可回放测试数据 | **P1，补最小 trace/span 与录放** |
| 测试工程 | app-server v2 suite、schema fixtures、snapshot、sandbox/迁移/恢复测试密集 | 组件单测覆盖不少，但跨模块链路与协议合同测试偏少 | 新功能容易只在 UI 或单元层通过，运行时边界可能回归 | **P0 建合同测试；P1 引入 fixtures/record-replay** |
| 产品层 | 通用、可嵌入、OpenAI provider/CLI/TUI 体验成熟 | GLM-5.2/agnes、中文 Windows、聊天室、桌宠、微信、goal 编排、本地 Computer Use 更贴近目标设备 | 不能把 Codex UI/认证/Responses API 假设直接带入 | **只吸收 harness 层，不替换产品层** |

## 4. 最值得纳入的设计优点

### 4.1 P0：建立“Thread → Turn → Item → Event”公共契约

建议在 `core-runtime` 增加稳定 DTO（名称可按项目风格调整）：

- `ThreadId`：聊天室/会话的持久身份；包含 workspace、模型/接收者等元数据。
- `TurnId`：一次用户输入或恢复操作；明确 `started/running/waiting_approval/completed/failed/interrupted`。
- `ItemId`：消息、工具调用、工具结果、文件差异、计划步骤、审批请求等可独立更新的项目。
- `AgentEvent`：所有前端/CLI/桌面/渠道都消费同一套事件，至少带 `seq`、`thread_id`、`turn_id`、`item_id`、时间戳和可回放字段。

实现策略是先把现有 tool/goal/pet/realtime SSE 事件适配到 `AgentEvent`，不改变现有 UI；再让 REST/SSE 和未来 WebSocket/CLI adapter 都由同一个 runtime stream 产生。这样聊天室工作目录或权限变更时，可以明确广播 `ThreadSettingsUpdated`，而不是依赖多个 handler 重新读取全局状态。

### 4.2 P0：把审批做成可等待、可恢复的 runtime 状态机

当前 `PermissionGateReport` 已经是很好的纯函数边界，应保留。需要补上 Codex harness 的生命周期语义：

1. gate 返回 `allow/deny/require_approval/require_confirmation`；
2. runtime 持久化 pending approval（含 workspace、room、session、tool、path、命令摘要）；
3. 通过 Web UI/CLI/桌面/渠道任一端回复后，以 `approval_id` 唤醒原 turn；
4. 审批等待期间暂停超时计时，断线可重新订阅；
5. 审批作用域显式区分 once/turn/session/workspace，workspace 或 room 切换时失效规则可测试。

这会直接改善当前聊天室权限、工具调用和 Computer Use 的一致性。

### 4.3 P0/P1：统一 workspace 生效快照

当前 `reload_workspace_scope` 已同步更新 workspace、配置和 session store，是可复用的基础。建议把它提升为不可变 `WorkspaceRuntimeSnapshot`，至少包含：

- canonical workspace root 与允许的附加 roots；
- 当前配置层/权限 profile；
- 模型/provider/vision model；
- tool catalog 版本；
- active session/thread；
- browser/computer-use 能力和安全策略。

每个 turn 捕获一个快照；设置变更只影响后续 turn，并通过事件通知 UI。这样能避免长时间运行的模型调用在中途意外切换工作目录或权限。

### 4.4 P1：把历史拆成追加式 rollout + 可查询索引

当前工程的 session JSON/SQLite、聊天室消息、goal 事件各自有持久化逻辑。可借鉴 Codex 的分层：

- 追加式 JSONL/rollout 保存完整事件和工具结果，便于故障恢复、审计和录放；
- SQLite 只保存 thread/turn/item 元数据、索引和分页游标；
- 读取、搜索、归档、删除、fork、rollback/revert 走统一 `ThreadStore`；
- web-console 继续提供现有聊天室视图，只改为消费 projection。

先做兼容读取，再逐步让新会话写入新 schema，避免破坏现有 `.coolzhu/web-sessions.*` 数据。

### 4.5 P1：工具协议和动态目录

当前 registry 已覆盖很多用户能力，但执行入口和错误形态仍分散。建议借鉴 Codex `tools`/`codex-mcp` 的分层：

- `ToolDefinition`：名称、版本、输入 JSON Schema、危险等级、路径效果、所需权限、是否支持 dry-run；
- `ToolCall`：调用来源（LLM/WebUI/CLI/MCP/Plugin）、call ID、thread/turn/workspace；
- `ToolResult`：状态、输出、错误、耗时、sandbox/permission 证据；
- `CapabilityCatalog`：按 workspace/room/profile 动态返回可见工具，支持 ToolSearch 和热更新事件。

这一步优先覆盖 `bash/PowerShell`、文件读写、browser、Computer Use 和 MCP；已有 tool registry 实现继续作为 executor，不做重写。

### 4.6 P1：最小化 record/replay 与合同测试

建议为 `agent-server`、web-console runtime 和 CLI 增加共享 fixtures：

- `thread/start → turn/start → tool approval → tool result → turn/completed`；
- workspace 切换后权限/工具目录/后端 cwd 的一致性；
- turn interrupt/resume/steer；
- SSE 断线重连与 seq 补发；
- Computer Use dry-run、坐标映射失败、无 showUI 模型时的明确 skip；
- Windows GBK/UTF-8 shell 输出。

每条 fixture 同时喂给 Rust 单测、HTTP/SSE 集成测试和前端状态 reducer，能显著降低当前“后端通过但 UI 状态漂移”的风险。

## 5. 不建议直接移植的部分

1. **OpenAI provider/auth/Responses API 细节**：当前工程必须继续支持 GLM-5.2、agnes 和 OpenAI-compatible provider；应只吸收 provider-neutral 的 turn/tool/event 抽象。
2. **Codex TUI/桌面产品 UI**：coolzhu agent 已有聊天室、桌宠、Web 控制台、微信渠道和本地视觉流程，直接替换会损失产品差异化。
3. **完整 Windows sandbox runner 的直接复制**：Codex 的 elevated helper、ACL、防火墙和构建链需要逐项评估当前安装包、权限和杀毒软件兼容性；先以隔离的 `windows-exec-policy` crate 做最小能力。
4. **整套大仓库构建系统**：Codex 同时维护 Cargo/Bazel/脚本；当前项目先沿用 Cargo workspace，只有在协议 schema 或大规模测试需要时再引入生成步骤。
5. **源代码整段复制**：Codex 仓库为 Apache-2.0，当前项目为 MIT。若未来直接移植代码，必须保留 Apache-2.0 LICENSE/NOTICE、版权和专利条款，并做依赖/第三方许可审查；设计复刻或独立实现更稳妥。

## 6. 分阶段落地路线

### 阶段 A（P0，先做稳定性）

- 在 `core-runtime` 定义 Thread/Turn/Item/Event DTO 和版本号。
- 新增 `AgentEventStream`，把现有 tool/goal/chat/realtime 事件适配进去。
- 为每次模型/工具调用补 `thread_id/turn_id/call_id/seq`。
- 统一 Web UI、CLI 和 agent-server 的事件序列化；保留现有 REST/SSE URL 作为兼容层。
- 把 pending approval 变成可持久化状态，支持批准/拒绝/过期/断线恢复。

### 阶段 B（P0/P1，运行时边界）

- 引入 `ThreadRuntime`，将现有 `ConversationRuntime` 包装为 start/resume/interrupt/steer/queue API。
- 把 workspace、room permission、model/provider、tool catalog 聚合为每个 turn 的 immutable snapshot。
- 将聊天室工作目录和权限变更映射为 `thread/settings/update` 类事件；后端只让后续 turn 使用新快照。

### 阶段 C（P1，持久化和 CLI）

- 新增兼容的 rollout/event log 和统一 ThreadStore projection。
- 实现 thread list/read/search/archive/delete/fork/rollback 的最小集合。
- 提供 `coolzhu-agent exec --jsonl` 或等价 headless adapter，复用同一事件协议。

### 阶段 D（P1/P2，安全和质量）

- 独立 Windows command policy 与 sandbox 状态检查；将 showUI/computer-use 能力声明为 capability。
- 增加 protocol schema fixtures、record/replay、跨 crate 集成测试、诊断/trace。
- 评估是否需要 WebSocket app-server；在 SSE + JSONL 已稳定后再增加。

## 7. 结论

Codex harness 最值得借鉴的不是某个工具或界面，而是“可嵌入的运行时边界”：稳定协议、显式 thread/turn 生命周期、事件化审批、workspace/权限快照、可恢复历史和高密度合同测试。coolzhu agent 已经拥有更丰富的本地模型、中文 Windows、聊天室、桌宠、微信、浏览器和 Computer Use 能力；合理路线是保留这些产品能力，把它们接到一个更接近 Codex harness 的公共 runtime/protocol 上。

建议下一步先立一个独立 P0 PR：只增加 `Thread/Turn/AgentEvent` 契约、审批状态机和现有 SSE/聊天室适配，不迁移 Codex provider 或 UI。完成后再以 workspace 实时切换、工具审批恢复和 CLI JSONL 三条链路做回归验收。

## 8. 专项差异审视与优先级补齐计划（2026-08-22）

本节在上一节的整体差异基础上，单独审视模型思考过程、上下文/记忆、Computer Use 和流式语音。结论以“协议是否存在、运行时是否真正接线、UI/持久化是否可见、设备/模型依赖是否满足”四个层次区分，避免把只声明了类型或路由误判为完整功能。

### 8.1 结论总览

| 专项 | Codex harness / 官方协议侧 | coolzhu agent 当前实现 | 实际差异 | 优先级 |
| --- | --- | --- | --- | --- |
| 模型会话思考过程显示 | App Server 有 `item/reasoning/summaryTextDelta`、`summaryPartAdded`，在模型支持时还有 `item/reasoning/textDelta`；每条事件带 thread/turn/item 关联，可持续更新和回放（[App Server 事件](https://learn.chatgpt.com/docs/app-server)） | `llm-adapter` 能解析 `Thinking`/`ThinkingDelta`；web-console 直连流式路径能生成“推理卡片”，但 `core-runtime::AssistantEvent` 和 `tool-registry` 会丢弃 thinking，前端 `message_done` 后移除 reasoning 卡片，历史也没有稳定的 reasoning item | 有“临时 UI 展示”，没有公共运行时事件和完成态可回放；不同调用路径的行为不一致 | **P0** |
| 会话上下文与记忆加载 | Thread/turn/item 是持久化边界，可读取/分页 turns、恢复和压缩；线程有独立 `memory_mode`，并有阶段化 memory job/consolidation（[Conversation state](https://developers.openai.com/api/docs/guides/conversation-state)、[Compaction](https://developers.openai.com/api/docs/guides/compaction)） | SQLite 会加载 session messages 和 memory beads；`ContextAssembly` 会按 token、TTL、衰减、语义/关键词检索组装上下文，超过阈值会摘要并写自动 bead | 本地记忆能力并不弱，但模型请求实际以聊天室消息为主要历史，session messages 与 room history 的权威关系不够清楚；没有统一的 turn/item 事件账本、分页读取和 canonical compaction item | **P0（边界）/P1（记忆作业）** |
| Computer Use | 官方 CUA 是“取截图→模型返回动作→宿主执行→回传新截图”的通用闭环，动作包含 click、drag、scroll、keypress、type、wait、screenshot，并要求对删除、安装、外发等风险动作确认（[Computer Use guide](https://developers.openai.com/api/docs/guides/tools-computer-use)）；开源 harness 主要提供 policy/connector/host 边界，并不等于内置 Windows 桌面执行器 | 有实际 Windows UIA 桌面 adapter、DOM browser bridge、DPI/窗口代数校验、stale observation、权限审批、证据和重试；planner 面向 `uia-*`/`dom-*` 语义目标，桌面动作集合比 browser 小；视觉坐标 grounding 依赖本地 ShowUI/视觉服务 | 当前本地执行和安全状态机更具体，但不是通用截图 CUA 协议；桌面拖拽等动作仍有覆盖差异，ShowUI 不可用时不能伪报成功 | **P0（能力真实性）/P1（协议与动作覆盖）** |
| 流式语音会话 | Codex 已有实验性的 thread-scoped realtime v1/v2/v3：WebSocket/WebRTC、append audio/text/speech、stop/list voices，并把 transcript/audio/SDP/error/closed 作为通知（开源 `app-server-protocol/v2/realtime.rs`；[Realtime guide](https://developers.openai.com/api/docs/guides/realtime)） | 有本地 PCM 采集、provider-native streaming STT（当前代码含 Aliyun Paraformer WebSocket）、文本 agent、SSE `assistant_text`/`assistant_done`、chunked TTS 和 barge-in 状态；`full_streaming` 需 STT/AEC/provider/TTS 全部 gate ready，否则显式降级；voice monitor 标为 `dry-run` | 当前是可观测的 guarded/chained pipeline，不是原生 Realtime 音频模型会话；GLM-5.2/agnes 在现有代码中作为文本/视觉 agent 使用，未证明具备可直接复用的 Realtime 音频端点 | **P0（状态诚实）/P1（链路完善）/P2（原生协议）** |

### 8.2 专项一：模型会话的思考过程内容显示

#### 已核实的现状

1. `modules/llm-adapter/packages/llm-adapter/src/types.rs` 已有 `OutputContentBlock::Thinking`、`RedactedThinking` 和 `ContentBlockDelta::ThinkingDelta`；OpenAI-compatible provider 会从 `reasoning_content` 解析 thinking。这说明 provider 层具备接收能力。
2. `modules/core-runtime/packages/core-runtime/src/conversation.rs` 的 `AssistantEvent` 只有 `TextDelta`、`ToolUse`、`Usage`、`MessageStop`。`modules/tooling/packages/tool-registry/src/lib.rs` 在 `ThinkingDelta`/`SignatureDelta` 分支直接丢弃，并在 `push_output_block` 中忽略 `Thinking`/`RedactedThinking`。因此走通用 ConversationRuntime/tool-registry 的会话不能保留思考内容。
3. `modules/gui-web/packages/web-console/src/main.rs` 的直连流式路径会把 thinking 和工具参数追加到 `kind: "reasoning"` 的临时卡片；`modules/gui-web/packages/web-console/src/app.js` 在 `message_done` 后调用 `hideReasoningForAssistant`，而完成态渲染函数也排除了 reasoning。结果是：流式过程中可见，模型完成后被删除，刷新或历史重放时不可作为稳定条目查看。
4. 当前实现把“模型原始 reasoning、工具调用过程、可展示的 reasoning summary”混在一张卡片中；Codex 协议把 summary、raw reasoning（仅在支持时）和工具/计划 item 分开，且都有 item ID 和完成/增量事件。前者会造成敏感内容暴露和跨 provider 行为不一致，后者更适合审计和折叠展示。

#### 补齐方案（P0）

- 在 `core-runtime` 增加 `ReasoningSummaryDelta`、`ReasoningPartAdded`、可选 `ReasoningTextDelta` 三类 typed event；保留 `redacted`/`summary`/`raw` 的来源和可见性，不把签名或加密 reasoning 当成普通文本。
- `tool-registry`、web-console、CLI、桌面入口统一转换为同一 `AgentEvent`，每个 reasoning item 带 `thread_id/turn_id/item_id/seq`，并在 `completed` 时写入可回放事件或 projection。
- 前端改成“完成后保留、默认折叠”的 reasoning summary 卡片；工具调用单独渲染为 tool-call/tool-result，原始 reasoning 只有在 provider 明确支持且用户权限允许时显示。删除 `message_done` 后无条件移除 reasoning 的行为，改成归档或折叠。
- 在 SSE/JSONL 合同测试中覆盖：无 reasoning、summary-only、raw reasoning、redacted reasoning、断线重连、工具调用紧跟 reasoning、消息完成后刷新仍可见。

验收标准：同一模型请求从 web、CLI 和 ConversationRuntime 三条路径都产生相同的 reasoning item 序列；刷新会话后 summary 可见且顺序不变；不支持 reasoning 的 provider 不产生空卡片；敏感/加密内容不会被错误降级成普通可见文本。

### 8.3 专项二：会话上下文管理与记忆管理加载

#### 已核实的现状与差异

当前 web-console 的实现包含较多可复用基础：

- workspace store 启动/切换时从 SQLite 加载 session、messages、memory beads；`/api/sessions/{id}/context-preview` 可查看组装结果。
- `select_context_memory_beads` 支持语义 ID、关键词/hash fallback、TTL/过期和 superseded 过滤、衰减重排及 memory token budget；`build_context_assembly` 还会按模型 context window、输出预留、历史预算和图片估算裁剪。
- `compact.rs` 能生成摘要并保留最近消息；`apply_context_lifecycle_after_turn` 会在阈值触发后写入 `context:auto-compact` 的 L2 conversation bead、更新 `context_reset_at` 并清理临时目标记忆。

差异集中在运行时边界而非“有没有记忆”本身：

- 当前模型请求的 `context_history` 主要来自聊天室消息；源码注释已明确“写 session messages 不会进下一次模型请求”。session messages 可以持久化并加载，但并不天然等价于下一轮模型上下文，session/room 两个 store 的权威关系必须明确。
- 当前压缩是本地可读 summary + 自动 bead；Codex 的 compaction 是上下文窗口中的 canonical item，并支持独立 compact、previous response/状态链和恢复。两者不能把摘要文本直接当作等价协议。
- 当前没有一套公共的不可变 turn/item 账本来记录“本轮实际使用的历史、memory revision、权限、工作目录、模型、压缩前后边界”。因此 workspace/权限实时切换、断线恢复、同一会话多端读取时，可能出现 UI 看见的历史与模型实际收到的上下文不一致。
- Codex 的状态层把 `memory_mode`、stage-1 extraction、global consolidation job 和 watermark 作为独立持久化对象；当前 beads 主要嵌在 session/room 生命周期，检索很实用，但还没有同等级别的后台作业、全局合并和可观测 job 状态。

#### 补齐方案

**P0：先统一本轮上下文的权威边界。**

1. 定义 `ContextRuntimeSnapshot`：`thread/room/session_id`、canonical workspace、model/provider、permission profile、tool catalog revision、memory revision、context reset floor、历史/记忆 item IDs。
2. 每个 turn 开始时固定 snapshot；workspace、接收者、权限或模型的改动只影响后续 turn，并发出 settings-updated 事件。把 snapshot 中的 item IDs 和实际送给 provider 的消息摘要写入 turn record。
3. 让聊天室 history 与 session messages 通过一个兼容 projection 读取，短期保留旧表，但明确“模型上下文 source of truth”只有一个。`ContextAssembly`、上下文预览和恢复接口均读取同一 projection。
4. 将 compaction、memory injection、context reset 写成 typed items/events，而不是只写字符串 footer/bead；保留当前摘要算法作为 provider-neutral fallback。

**P1：补齐记忆生命周期和历史读取。**

- 为 thread 增加 `memory_mode`（enabled/disabled/polluted 等）和可查询的 extraction/consolidation job 状态；先将现有 beads 作为 stage-1 输出适配器，再增加跨 session 的 global consolidation。
- 增加 turns/items 分页读取、resume、fork、rollback/revert 的最小集合；读取时能区分完整事件、summary projection 和未加载的历史段。
- 记录 memory selection 证据（候选、过滤原因、token 占用、最终注入顺序），让“为什么加载了这条记忆”可诊断、可回放。

验收标准：同一 turn 的 context-preview、provider 请求日志和恢复后的下一轮输入能对上同一 snapshot/sequence；session/room 切换不丢上下文；压缩后不会重复注入旧历史；memory disabled 时不注入 beads，job 失败有可重试状态而不是静默丢失。

### 8.4 专项三：Computer Use 功能差异

#### 已核实的现状与差异

- 当前 `modules/computer-use` 已定义 observe/classify/plan/policy/approval/execute/verify 的状态机，带 action/replan/no-progress/time budget、risk class、evidence、stale observation 和 circuit breaker。web-console 的 Browser adapter 使用 DOM bridge revision，Desktop adapter 校验 window/process/rect/DPI/WebView2 overlay；这些是比“只调用一次截图动作”更完整的本地安全执行基础。
- 当前模型-facing planner 使用 `dom-*`/`uia-*` 语义 target，不允许任务请求直接携带裸坐标；Browser 支持 navigate/click/text/select/check/submit/scroll/history/drag/slider/tabs，Desktop 主要是 click/double_click/text_input/scroll/key combo。Desktop 拖拽、滑块、标签页等动作覆盖不对称。
- 当前视觉 grounding 通过 `vision-service` 的 ShowUI-2B 或兼容云端模型做点位/bbox；ShowUI 服务不可用时只能走 DOM/UIA 或 dry-run，不能把 capability 标为 ready。用户已经允许本地 showUI 不支持时跳过该项测试。
- 官方 CUA 的模型协议则是截图动作闭环，动作可含 raw coordinate click/drag/move/scroll/key/type/wait/screenshot，宿主需每次动作后取得新截图并对高风险动作向用户确认。Codex 开源 harness 本身主要提供 requirements/policy/connector 边界，当前 clone 中没有一个等价的 Windows UIA 执行器。因此这里是“协议形态差异”，不是简单的功能多少比较。

#### 补齐方案

**P0：能力探测和结果真实性。**

- 启动/测试时分别探测 ShowUI、vision backend、Browser DOM bridge、Desktop UIA、DPI mapping 和真实输入权限；把 `available / reserved / skipped / blocked / dry_run` 作为互斥状态返回。
- 记录 `capability_probe_id`、截图/DOM/UIA evidence、原因和版本；ShowUI 失败时测试结果标为 `skipped_showui_unavailable`，不伪造 click 成功。
- 让模型和 UI 看到同一 capability catalog；当前 dry-run、execute gate、审批和终态统一映射到 AgentEvent。

**P1：协议适配和动作覆盖。**

- 增加 provider-neutral `ComputerAction`（semantic target + optional screenshot coordinate）和 `ComputerObservation`；提供 screenshot-CUA → UIA/DOM semantic adapter，动作后强制回传新 observation，保留坐标/DPI 映射证据。
- 补齐 Desktop drag/slider/multiple-window 等必要动作，或在 capability catalog 中明确声明不支持；Browser 与 Desktop 复用同一 policy/approval/audit 逻辑。
- 在隔离浏览器/临时桌面 profile 中增加外部网页不可信内容、安装/删除/凭证/外发/支付等确认测试；不要把页面指令当作用户授权。

验收标准：每个成功动作都有前后 observation 和可验证 success criteria；窗口/DPI/DOM revision 变化会阻止 stale action；无 ShowUI 时功能被准确标记 skip/block；高风险动作在执行前停在审批态，拒绝后 turn 可恢复而非丢失。

### 8.5 专项四：流式语音会话功能

#### 已核实的现状与差异

当前工程已经不是完全占位：

- `realtime_voice_stream.rs` 校验 16 kHz mono PCM16、帧序列和 session 绑定，并实现 provider-native streaming STT 的 WebSocket 连接、partial/final transcript 计数和证据。
- web-console 有 `/api/realtime/session/status`、`/events`、start/stop/barge-in、`assistant_text`/`assistant_done`/`tts_chunk` 事件；TTS chunk 会持久化并由前端播放。`app.js` 也有 realtime 状态和自动播报路径。
- `main.rs` 明确把 full streaming 的 ready 条件设为 true streaming STT、far-end reference AEC、realtime provider adapter、streaming TTS 全部满足；任一 gate 不满足会记录 downgrade reason 并切换 guarded mode。`audio.rs` 的 `voice_monitor_mode` 仍为 `dry-run`，并明确提示真实唤醒链路后置。

与 Codex Realtime 的差异：

- Codex 是 thread-scoped realtime session，有版本、voice、output modality、initial items、start/end instructions、append audio/text/speech、stop/list voices，以及 WebSocket/WebRTC transport；输入/输出 transcript delta/done、audio delta、SDP、error/closed 都是协议通知。
- 当前是“音频采集/流式 STT → 普通文本模型 turn → 分段 TTS”的 chained/guarded pipeline，没有统一的 realtime session protocol、WebRTC negotiation 或 provider-native audio response event。它可以是稳定的本地方案，但不能标成与 Codex 原生 Realtime 等价。
- 已配置的 GLM-5.2 和 agnes 可继续用于 chained pipeline 的文本/视觉推理；当前源码未看到它们作为 Realtime 音频模型被直接接入，不能据此推断有 speech-to-speech 能力。

#### 补齐方案

**P0：先把模式和健康状态说清楚。**

- 把 `guarded`、`chained_streaming`、`native_realtime`、`dry_run`、`unavailable` 固化为 mutually exclusive mode；status、日志、UI 和测试报告必须同时显示 requested/active mode 及 downgrade reason。
- 启动时探测麦克风、16 kHz PCM、STT partial/final、文本模型流、TTS chunk、播放设备、AEC/barge-in；任一缺失只降级，不伪报 full streaming。
- `voice_monitor` 在真正唤醒链路接通前保持 dry-run，并在 API/前端显示“仅状态桥接”。

**P1：完成 provider-neutral chained streaming（适配 GLM-5.2/agnes）。**

- 统一事件：`audio_in_delta`、`input_transcript_delta/done`、`turn_started`、`assistant_text_delta/done`、`output_audio_delta`、`barge_in`、`turn_completed/error`，每条带 session/thread/turn/item/sequence。
- 把最终 transcript、模型上下文 snapshot、工具调用和 TTS 播放结果落到同一 turn projection；支持中断后取消未播放音频、重新开始 turn 和断线重连。
- 保留现有 Aliyun/本地 STT/TTS adapter，但将 provider-specific payload 隔离；用假音频/录制 fixture 做不依赖外部服务的回放测试。

**P2：在 provider 确实提供 Realtime API 时再增加原生 adapter。**

- 参考 Codex `ThreadRealtimeStartParams` 的 session/start/stop/append 形态，实现 WebSocket；若浏览器端需要低延迟，再实现 WebRTC SDP/ephemeral credential 流程。
- 先完成事件映射和 transcript/audio 回放，再开放 native_realtime；GLM-5.2/agnes 没有对应接口时继续使用 P1 chained path，不引入伪兼容层。

验收标准：半双工、chained streaming、native realtime 三种模式的状态和日志可区分；语音输入能看到 partial/final transcript，文本和音频增量严格按 sequence 重放；barge-in 能停止当前 TTS 且不污染下一轮上下文；没有 STT/AEC/TTS/Realtime provider 时测试结果明确为 skip/degraded。

### 8.6 按优先级的执行计划与交付物

| 优先级 | 交付批次 | 主要改动 | 完成判定 |
| --- | --- | --- | --- |
| **P0-1** | 公共事件与思考内容 | `Thread/Turn/Item/AgentEvent`、reasoning summary/raw/redacted 事件；SSE/JSONL/前端统一 reducer；推理卡片完成后折叠保留 | 三条调用路径事件序列一致；刷新可回放；不支持 reasoning 不显示空卡片 |
| **P0-2** | ContextRuntimeSnapshot | 固定每轮 workspace/model/permission/tool/memory/context reset；统一 room/session projection；记录实际 provider 输入边界 | context-preview、请求日志、恢复输入能按 snapshot 对齐；设置变更只影响后续 turn |
| **P0-3** | 能力真实性与合同测试 | ShowUI/vision/UIA/DOM/audio/STT/TTS/AEC probes；skip/block/degraded 状态；高风险审批和断线重连 fixtures | 无本地 ShowUI 时 Computer Use 明确 skip；full streaming gate 未满时明确降级；无假成功 |
| **P1-1** | 记忆与历史生命周期 | memory_mode、extraction/consolidation jobs、compaction typed item、turn/item 分页、resume/fork/rollback | memory 选择可解释、可重试；压缩和恢复不重复/丢失历史 |
| **P1-2** | Computer Use 协议适配 | semantic + screenshot action schema、动作后 observation、Desktop drag/slider/tabs、统一 audit/approval | Browser/Desktop 能力目录准确；stale/DPI/风险动作测试通过 |
| **P1-3** | Chained streaming voice | 统一 audio/transcript/text/TTS/barge-in 事件和持久化；GLM-5.2/agnes 继续走文本 agent | 录制 fixture 可回放；断线、中断、TTS 取消、上下文关联通过 |
| **P2-1** | 原生 Realtime adapter（条件项） | WebSocket/WebRTC、session/append/stop/list voices、SDP 和 audio/transcript event mapping | 只有 provider capability probe 通过才启用；否则稳定落回 chained |
| **P2-2** | 隔离与规模化 | 临时浏览器/桌面 profile、全局 memory consolidation、trace/record-replay 性能优化 | 安全隔离和多会话并发有可重复基准；不影响现有中文 Windows 安装包 |

### 8.7 本轮验证与后续验收命令

本轮只补充分析报告，没有改动功能源码，因此未重新编译；仓库原有四个 web-console 源文件的未提交修改保持不变。实现 P0 后建议按以下顺序验收：

```powershell
cargo build -p coolzhu-web-console --offline
cargo test  -p coolzhu-web-console --offline
cargo check -p coolzhu-tool-registry --offline
cargo test  --test module_linkage_smoke --offline
```

运行时还需在已配置 GLM-5.2/agnes 的设备上记录以下接口和 SSE：

```text
GET  /api/sessions/{id}/context-preview
GET  /api/sessions/{id}/beads
GET  /api/realtime/session/status
GET  /api/realtime/session/events
GET  /api/showui/service
GET  /api/computer-use/browser/health
POST /api/realtime/session/start
POST /api/realtime/session/stop
```

每次测试报告至少保存：thread/room/session/turn/item/seq、requested/active capability、context snapshot 摘要、reasoning visibility、ShowUI/音频 gate、downgrade/skip 原因和对应日志路径。这样后续提交 issue/PR 时能区分协议缺陷、设备不支持和单纯 UI 显示问题。

### 8.8 本节结论

四项中，当前最需要优先修复的不是重新引入某个 Codex UI，而是把已有能力接到一个可回放的公共运行时边界：先让 reasoning、工具、上下文、审批、Computer Use 和语音都拥有同一套 thread/turn/item/sequence；再让 UI、CLI 和桌面分别做 projection。保留现有中文 Windows、GLM-5.2/agnes、UIA/DOM、安全闸门和本地音频差异化能力，同时只在 provider capability probe 通过时打开原生截图 CUA 或 Realtime，能避免测试结果虚假和后续维护分叉。

## 9. 本轮执行结果（2026-08-22）

本轮已按 P0-1、P0-2、P0-3 的最小可交付范围落地并保留兼容路径：

1. `core-runtime` 新增公共 `ThreadId`、`TurnId`、`ItemId`、`AgentEvent`、`ContextSnapshot` 契约。`ConversationRuntime::run_turn` 现在为每轮生成稳定 turn id、上下文 revision 和事件序列；reasoning summary、工具请求、消息完成和 turn 完成均进入 `TurnSummary.events`。redacted thinking 仍不会混入可见 reasoning 文本。
2. Web Console 的 context preview 增加 `context_snapshot_id`、`memory_revision`、实际注入的 `memory_bead_ids` 和 `history_floor_millis`，记忆窗口会显示这些值，便于确认加载边界和识别过期预览。
3. `/api/computer-use/capabilities` 新增只读能力探测：分别返回 ShowUI、Desktop、Browser 的 `available/status/reason` 和动作清单。未检测到本地 ShowUI 时只把视觉 grounding 标记为 `skipped`，并明确说明 dry-run/坐标映射不依赖 ShowUI；未连接浏览器扩展时 DOM 能力同样保持 `skipped`。
4. Realtime SSE 保留历史 `kind`，同时在 payload 中加入 `schema_version=coolzhu.realtime.v1` 和 canonical `event_type`（transcript、reasoning、text、audio），为后续统一 reducer 和 JSONL/录制回放提供兼容入口。

本轮本地验证记录：

- `node --check modules/gui-web/packages/web-console/src/app.js`：通过。
- `cargo metadata --no-deps --offline --format-version 1`：通过。
- `git diff --check`：通过。
- `rustfmt --edition 2021` 对本轮修改的 Rust 文件：通过；全仓 `cargo fmt --check` 仍被其他未改文件的既有格式差异阻塞。
- `cargo test -p coolzhu-core-runtime --offline`、`cargo check -p coolzhu-web-console --offline`：已启动依赖编译，但本机 MSVC 工具链缺少 `link.exe`，在依赖 build script 阶段停止，未进入 crate 测试；这属于设备工具链阻塞，不是测试断言失败。

因此，运行时 GLM-5.2/agnes、Browser extension/native host、ShowUI 本地模型和麦克风/TTS 的真机回放仍需在相应服务可用时执行；本轮代码不会把缺少这些设备依赖误报为成功。
