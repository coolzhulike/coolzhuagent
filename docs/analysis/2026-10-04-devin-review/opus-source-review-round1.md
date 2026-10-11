Cargo.toml 已真实读取（sha256 与用户给出值一致）。`modules/*` 匹配的是目录，返回 0 个文件，所以改用文件级 glob 列模块。已确认模块共 9 个：core-runtime、llm-adapter、tooling、vision、computer-use、gui-web、gui-desktop、cli、diagnostics（共 18 个 crate）。接下来定位各审查主题。Read protocol.rs model parsing and session.rs configure flow.Brace glob not supported here; use single file.# coolzhuagent 只读源码审查报告（Devin Opus 5.5 High）

**结论：** 这次没有确认出 P0。确认了 8 个 P1，另有 1 个疑似 P1 待证实，外加若干 P2。主要拖累普通操作的是这几处：换模前置校验、CLI 每轮哈希核对、提示词里写了不存在的工具、`stale_observation` 一次运行只能恢复一次、Paint 的预算和画布、遗留运行不按资源区分就阻断。以下五条保护都应保留：未确认释放即停止、旧 owner/epoch 拒绝、unknown 不自动重发、真实换模确认、ACP 权限请求一律回 cancelled。

## 0. 调用事实
- 一共调了约 64 次：1 次 `mcp_list_tools`，约 63 次 coolzhu-agent 的 `read_file/glob_search/grep_search`，宿主回执全部是 `status:ok`。没有用原生工具，没有执行命令，没有写文件，没有联网，没有用子 Agent。
- `repository/Cargo.toml` 读到的 sha256 是 9cc59f26…536d，和您给的一致。
- `glob modules/*` 返回 0 个文件，因为它只匹配到目录。改用 `modules/*/packages/*/Cargo.toml` 列出 18 个 crate，分属 9 个模块：core-runtime、llm-adapter、tooling、vision、computer-use、gui-web、gui-desktop、cli、diagnostics。
- 宿主把部分长输出截断，原文存进 `.coolzhu/tool-results`。我只回读过 1 个溢出文件（bridge.rs 后半段），其余截断段都只看了节选。
- `main.rs` 有 90,619 行（3.57MB），只按定位读了 3 段。

## A. 整体推进方案
1. **先把 Devin 接入稳住（P1-1、P1-2、P1-3）。** 修好之后，"每轮新 ACP session + 全历史快照"才算稳定。
   - 现在全历史拼进一条提示，上限 768KB（`devin_acp/chat.rs` 约 350-362 行），超了就直接拒绝。
   - 需要补"摘要 + 最近 N 轮"的压缩办法，否则聊天室一长就用不了。
2. **上下文、记忆、技能、插件：Devin 侧目前只开放 3 个只读工具。**
   - `bridge.rs:14-22`：`INITIAL_TOOLS` 和 `REVIEW_TOOLS` 两份清单。
   - `chat.rs:132-141`：`review_enabled` 只接受 `REVIEW_TOOLS`。
   - `chat.rs` 约 258-262 行：只允许 chat-send，不支持 Goal、接力和子 Agent。
   - 仓库里还有第二套 Devin 接入：`devin_plugin/mod.rs:6-61`，走云端 API，`api_key` 存在工程内的 `.coolzhu/devin-plugin.json`。
   - 建议定一套主路径：ACP 用于本地会话，云插件只作可选工具。两套共用模型目录和凭据来源。
3. **Browser Use。** 原生面板协议的设计是对的，有预检票据和执行实例绑定，执行阶段不中途取消，默认按 `MayHaveBeenSent` 记。建议的接法：
   - Devin 只读验收通过后，桥上只为 `surface=browser` 开放 `computer_use_perform`。
   - 依赖 P1-4、P1-5 先修好。
4. **Paint。** 依赖 P1-4、P1-6、P1-7，见下文。
5. **正式 UI。** `app.js`、tauri-shell 我只在 grep 里命中过，没有审查，结论待定。

## B. 问题清单

**P1-1 换模前置校验过严（首轮失败的根因）**
- 证据：`protocol.rs` 约 95-125 行的 `model_config` 要求 `currentValue` 必须在 options 里（114 行附近，报"生效模型不在返回的选项中"）。`set_model_params`（约 127-136 行）发换模请求前就调用它。初始回执在 `session.rs` 约 431 行落盘，之后照样失败。
- 最小改法：发换模请求那一步改用宽松解析，只核对四点：唯一的 model 类选项、select 类型、有 id、请求值在 choices 里。`confirmed_model` 保持严格（生效值等于请求值，且在 options 里）。最终比较没有放宽。
- 验收：
  - 用一个"初始 currentValue 不在 options"的夹具，应能发出 `set_config_option`，selected 回执一致即通过。
  - 生效值不一致时仍然失败。
  - 用真实 CLI 跑 721 个 ID，差异为 0。

**P1-2 每轮全量哈希、版本、模型目录核对**
- 证据：
  - `chat.rs:19-20` 固定了 `CLI_VERSION` 和 `CLI_SHA256`。
  - 约 305-345 行：每轮读入最大 512MB 的二进制做 sha256，再跑 `--version`（5 秒超时），再跑 `models list`（30 秒超时）。
  - CLI 一自动更新，整条接入就停了。
- 最小改法：
  - 进程内按 (路径, 长度, mtime, 文件 ID) 缓存哈希结果。
  - 模型目录按 TTL 缓存，失败时作废。
  - 版本不一致时进入"待验收版本"状态，由用户显式启用，不再硬性阻断。
  - 真实生效模型仍以 ACP 换模回执为准。
- 风险：缓存期间二进制被替换（TOCTOU）。用文件身份加 mtime 能缓解。

**P1-3 提示词里有不存在的工具**
- 证据：
  - `main.rs:28668-28673`：`agent_tool_usage_instruction` 无条件拼入整段 computer-use 约束和 `write_file/edit_file/bash` 指引。
  - `chat.rs` 约 355-356 行：把这段系统提示原样传给 Devin。
  - 本轮我实际收到的提示里就有 `computer_use_perform` 和 `write_file`，但实际只有 3 个只读工具。
- 最小改法：按本轮真实挂载的工具清单裁剪各段指引。
- 验收：Devin 审查会话的提示快照里，不出现未挂载工具的名字。

**P1-4 `stale_observation` 一次运行只能恢复一次**
- 证据：`controller.rs:482` 的 `stale_recovered` 在整次运行内有效，757-774 行置为 true 后从不复位。多步任务里第二次出现零输入的 stale 也会走 Execution 终态。这就是"stale_observation 零输入拒绝"，和"三笔画歪 / 预算不足"是两个不同的问题。
- 最小改法：某个动作成功之后复位，或者改成计数，由 `guard.record_replan`（`max_replans`）兜底。回执条件不变。
- 验收：
  - 两次零输入 stale：两次都重新观察。
  - 输入可能已发出后的 stale：进终态。
  - 超出重规划预算：返回预算错误码。

**P1-5（待确认）没有回执的错误被当作"未发送"**
- 证据：`contracts.rs:251-257` 在 `receipt` 为 `None` 时返回 false，于是允许重新观察和重规划。
- 最小改法：只有带明确 `NotSent`/`NotNeeded` 回执的错误才允许重新观察。预检阶段由适配器统一附上 `pre_input_receipt`（`native_browser_adapter` 已经这样做）。
- 待核：桌面适配器是否在所有分支都附了回执。我没有读到这部分。

**P1-6 计算机操作预算对 Paint 不够**
- 证据：
  - `main.rs:7255-7286`：默认最多 12 个动作，`max_calls_per_turn` 被硬性限制在 1-2，`no_progress` 不超过 3。
  - `input_stroke.rs:68-70`：每个动作只能是一笔，2-256 个点，不超过 5000ms。
  - 一只简化海绵宝宝需要 15-30 笔，外加验图，在这些上限内画不完。
- 最小改法：只在"已授权 + desktop + mspaint"的场景启用绘图预算档，例如 40 个动作、4 次调用、300 秒。也可以新增"多笔批量"动作，每笔单独确认按下和释放。
- 验收（真实 Paint）：单轮至少 20 笔且每笔都是 Released，左键没有卡住，验图预算有回执，窗口保持最大化。

**P1-7 Paint 画布没有语义识别**
- 证据：`computer_use_desktop_bridge.rs:104-127` 的兜底 `window-canvas` 是整个可见 client 区，包含工具栏，注释明确要求模型"按原图找"。这是"画歪"的直接原因。
- 最小改法：先用 UIA 找画布元素，找不到再用图像检测 client 区内最大的白色矩形，作为 `canvas_rect` 下发。
- 验收：真实截图叠加识别框，笔画落在框内。四边泛光提示另行实测。

**P1-8 遗留运行不按资源区分就阻断开放**
- 证据：`input_safety_opening.rs` 约 129-141 行，`legacy_unconverged_runs` 统计整个会话库，只排除了已人工放行的，没有按 `resource_scope` 过滤。约 143-153 行据此拒绝开放。结果是某个旧运行会挡住所有浏览器和 Paint 资源。
- 最小改法：按 scope 过滤。同一 scope 的未收敛运行仍然阻断。
- 验收：资源 A 上的遗留运行不影响 B，同一资源仍被阻断。

**P1-9 审批完成状态**
- 证据：
  - `main.rs:35354-35359`：只有 `dsh_` 前缀的工具会记为 `awaiting_approval`，其他需审批的结果被记成 completed 或 failed。
  - 后续状态流转（`approval_running` 到终态）只在 `dsh_web.rs:550/568` 有实现。
  - `bridge.rs` 约 105-160 行的冻结门：清掉 `user_authorized`，grants 在 capture 时就冻结了，所以本轮中途的审批过不去（返回 DryRunOnly）。现在只读模式碰不到这条路径，开放写工具时会立刻暴露。
- 最小改法：
  - 审批结果统一落 `awaiting_approval`。
  - 审批记录绑定 call_id、参数摘要和 owner/epoch。
  - 冻结门只认这条具体的审批，不放宽到其他调用。

**P2**
- `tool_dispatch_settlement.rs:43-47, 61-67`：预算过期或 Drop 时，执行前就被拒绝的调用也记成 `*_outcome_unknown`，会误触发人工复核。建议在执行器边界之前先结算为 `not_executed`。
- `native_browser_adapter.rs:117-119`：`NotDispatched` 已证明零投递，却设了 `RetryOwner::None`。建议允许在重规划预算内由模型重新观察。
- `computer_use_turn_scope.rs:12-56`：原生浏览器靠关键词识别；显式请求要求整体完全相等。建议只比较关键语义字段。
- `chat.rs:159-175`：全局只要存在 Devin MCP、Hooks 或插件就整体拒绝启动。建议改用隔离配置根，并在提示里列出具体文件。
- `bridge.rs:71-100`（`live`）和 `session.rs`（每条通知查一次 binding）：每次调用都访问 SQLite。owner/claim 校验必须保留，可以按 epoch 缓存，变更时作废。
- `main.rs` 有 9 万行，应该拆分，这本身就是审查盲区。

**应保留的保护**
- `protocol.rs` 约 156-169 行：权限请求一律回 cancelled。
- `session.rs` 约 490-512 行：提示只发一次，失败记 unknown。
- 浏览器执行阶段不中途取消，`ReleaseUnknown` 返回 blocked 并交给用户。
- `bridge.rs` 的 owner/claim 和工程身份校验。
- `ResourceSafetyState` 默认 Unknown，fail-closed。
- Devin 进程用 Job 对象 `terminate_and_wait` 回收（`process.rs:120-127`）。

**依赖顺序：** P1-1/2/3 → Devin 只读验收 → P1-5/4 → 开放 Browser → P1-8/6/7 → Paint → P1-9 → 开放写工具。

## 覆盖清单
**实际读过的段落：**

| 文件 | 行 |
|---|---|
| `Cargo.toml` | 全文 |
| `devin_acp/protocol.rs` | 61-170 |
| `devin_acp/session.rs` | 361-470（被截断）、479-588 |
| `devin_acp/bridge.rs` | 1-约250（含溢出文件） |
| `devin_acp/chat.rs` | 121-215、250-369 |
| `controller.rs` | 701-810 |
| `contracts.rs` | 281-340 |
| `input_stroke.rs` | 44-118 |
| `native-browser-protocol/lib.rs` | 191-260 |
| `native_browser_input.rs` | 2-31、60-约125 |
| `native_browser_adapter.rs` | 106-145 |
| `computer_use_turn_scope.rs` | 2-101 |
| `input_safety.rs` | 46-120、176-225 |
| `input_safety_opening.rs` | 41-160 |
| `tool_dispatch_settlement.rs` | 2-77 |
| `main.rs` | 7201-7305、28665-28676、35341-35375 |
| `devin_plugin/mod.rs` | 2-61 |

**只做过 grep 定位、没有阅读：** `memory.rs`、`plugin_runtime.rs`、`dsh_web.rs`、`computer_use_desktop_bridge.rs`、`cleanup.rs`、`process.rs`、`auth.rs`、`transport.rs`、`discovery.rs`、`journal.rs`、`computer_use_executor.rs`、`input_permit_store.rs`、`native_recovery_store.rs`、`app.js`。

**没有审查：**
- vision 模块（vision-service、uia-resolver）
- llm-adapter
- tooling 下 4 个包（tool-registry、plugin-system、command-router、compatibility-harness）
- cli、diagnostics
- gui-desktop（desktop-console、tauri-shell）
- gui-web 下的 clawbot-sidecar、windows-process-guard
- core-runtime 下的 language-service、agent-server
- `packages/app-launcher`、`.coolzhu/plugins/*`
- 前端 JS/CSS

这次没能做到每个生产模块逐一评估，以上部分需要另开一轮。

## 尚未实测
以上全部是静态阅读，没有运行任何测试。以下各项都还没有真实验证：
- Browser：点击、输入、滚动、导航，以及 pointerdown/up 区间内关闭或替换的竞争。
- Paint：完整简化海绵宝宝、多笔规划、画布识别、验图预算、保持最大化、四边泛光。
- 正式安装版：插件统一编辑器、图标和无调试信息、方案 B Q 版舞剑动画。
- P1-5 和 P1-9 的实际触发路径。
