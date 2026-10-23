# HA-00 当前基线与调用链缺口图

状态：进行中；本文是静态源码核对结果，不是功能验收，也不替代运行证据。
生成时间：2026-10-18（America/Los_Angeles）。
源码基线：`docs/plans/harness-alignment/baseline/HA-00-20261018/`（当前工作区源码；HEAD `8a13551b037f5f673188802f2c9128984245fab8`）。

## 1. 基线身份

| 项 | 值 |
|---|---|
| 当前 `main.rs` | 3,652,979 B；SHA-256 `3d1aa95e28d0eb839279e4d2381ddd20e6a1f160ea6fb9193b08e7ca7e930a4d` |
| 0.2.142 final-staged-source 的 `main.rs` | 与当前一致（同一 SHA-256） |
| 规划指纹 `baseline.json` 的 `main.rs` | 3,652,853 B；SHA-256 `90ab33494c3923ea7bdf52b586ea97314a2805f888fdc869494a4d5872b27377` |
| Pro 审核引用的 `main.rs` | 3,652,545 B；SHA-256 `817f…`（见 `baseline.json` 的 `changed_from_reviewed_files`） |
| 工作树 | 300+ 项既有改动；本基线只收录 Harness 相关源码/测试/配置，不收录运行态、DB、日志、构建产物 |
| 基线包 | 72 个 changed text files（patch 639,732 B，后经 manifest 记录）+ 29 个 untracked source files；恢复校验 101/101 通过（canonical git blob 比较；CRLF/LF 归一化） |

> 结论：规划指纹不是可恢复内容快照，且当前源码比规划时多 126 B。HA-00 已把当前工作区冻结为新的实施基线；旧 `baseline.json` 保留为历史观测，不覆盖。

## 2. 四条后端路径与订阅入口

| 路径 | 入口 | 当前支持 | 当前缺口 |
|---|---|---|---|
| Web 普通 HTTP | `POST /api/chat/send`（`main.rs:1968`）→ `api_chat_send`（约 `:18654`）→ `prepare_chat_dispatch` → `MessageRequest`（`:33623`） | 工具循环、用量、Goal/普通聊天；非流式 | `MessageRequest` 没有实际请求身份/最终工具集合/参数摘要；响应无法按“发送时集合”解释 |
| Web SSE | `POST /api/chat/send/stream`（`:1969`）→ `api_chat_send_stream`（`:18994`）→ `started` 事件（`:19030`） | 流式、工具循环、取消 | `started` 只带 turn/run/room，没有请求绑定/工具集合/模型参数/首响应归属；首响应与后续工具集合可能漂移 |
| CLI 原生 conversation | `modules/cli/packages/command-line/src/main.rs` 的 `run_turn`/`run_turn_with_output`（`:1354`、`:1387`）→ `runtime.run_turn` | 同步执行、三种 permission mode、slash commands | 共享提示/工具/恢复接口变更需回归；没有 SSE；不能把 Web 通过推广为 CLI 通过 |
| Devin ACP | `stream_agent_model` 的 `devin_acp` 分支（`main.rs:32007`）→ `devin_acp/chat.rs` | 独立身份/审批/收尾；文本/图片/CU；无 token usage 账本 | 与 Web 的请求绑定/审批/验证语义不是同一层；`devin_acp_attempts` 无 token/时间戳；不能把 HTTP 结论推广到 ACP |
| OpenAI/ChatGPT 订阅（可选） | `openai_subscription.rs`（Codex app-server，文件头 `:1`） | 文本会话；账号/目录来源官方进程 | 没有原生工具/Goal/图片执行；不能作为 15 组 Harness 工具对照的默认可比路径 |

## 3. HA-01～HA-10 现状、缺口与动作

| 工作包 | 当前实现位置 | 现状/缺口 | 建议动作 |
|---|---|---|---|
| HA-01 提示词最小化 | `AGENT_GUIDE_INLINE`（`:28326-28335`）；`agent_tool_usage_instruction`（`:28352-28357`）；`build_context_assembly_with_roster`（`:28592`）；Goal prompt（`:46979`） | 通用规则已有单一来源；但 `agent_tool_usage_instruction` **无条件拼接 CU 约束**（`:28354`），无 CU 能力时仍出现；`AGENT_GUIDE_INLINE` 含“Plan first / call reads in parallel”等绝对条款；Goal prompt 另有“Before calling tools…”独立指导，存在重复/强制搜索风险 | 修复：能力化注入、去重、Goal 只附阶段差异；补 HA01-01/02 的最终 wire 断言 |
| HA-02 活跃任务连续性 | `chat_insights::record_source_messages`（`chat_insights.rs:216`）；`chat.source_messages` 事件；`chat_tool_history` 的压缩/摘要投影 | 已有 source message 归属、取消/完成消息、压缩摘要；**没有“活跃任务实体”**（原始人类输入、可信补充、完成证据、取消状态、任务 revision 的统一记录）；摘要可能把旧任务带回 | 修复：在现有任务/运行记录上补任务级引用与状态，不新建第二套权限 |
| HA-03 工具能力配置 | `messages_have_tool_intent`（`:32854`）；`select_tools_for_request`（`:33635`，`!has_tool_intent` 直接返回 None）；`text_has_tool_action_intent` | 仍以动作关键词作为首轮工具暴露硬门；没有显式“能力配置”对象；小窗口裁剪存在 | 修复：显式能力配置 + 基础工具/发现入口；关键词只做提示，不做硬门；保留显式禁用 |
| HA-04 实际请求绑定 | `MessageRequest`（`:33623`）；`ChatStreamStarted`（`:19030`）；`ToolDispatchService`/`ModelToolIdentity`（`tool_invocation_identity.rs:7`） | 没有把实际发送的工具集合、模型参数、请求身份写入响应/事件；`ModelToolIdentity` 只覆盖工具派发来源，不覆盖 SSE 首响应；没有 attempt/父关联的完整绑定 | 修复：在最终 wire 点冻结请求快照并绑定响应；SSE 首响应携带绑定；补普通/SSE 两入口对照 |
| HA-05 L1 与等待 | L1 监督（`:27954-28136`）：相同签名连续重复→纠偏提示；`max_tool_feedback_rounds` | 没有识别“运行中/等待/可恢复”状态；相同参数一律提醒换方法；`tool_repeat_count >= 1` 即纠偏；没有等待上限/退避/长轮询分类 | 修复：区分等待/恢复/终态；只在无进展时纠偏；补有界等待与取消 |
| HA-06 恢复与效果 | `call_agent_model_text_recovery`（`:28036`）；`ToolDispatchService::dispatch`（`tool_dispatch_service.rs:27`）；`ToolDispatchSettlement`（`:84`）；`ModelToolIdentity` | 已有身份、收尾、部分恢复；但恢复条件主要看文本/工具暴露/CU 终态；`model_tool_calls_executed` 仍是布尔；没有统一的“效果已发生/回执丢失/post-hook 失败”分类 | 复用身份/收尾；修复恢复状态与效果身份；补断流/回执丢失/post-hook 用例 |
| HA-07 审批复用与撤销 | `session_grants`（`:3835`）；`pending_approvals`（`:4162`）；`enqueue_pending_approval_custom`（`:4221`） | 会话授权 key 只有 `(workspace, session, tool)`，30min TTL；pending 以 `call_id` 为 key，没有逻辑调用合并、room/version/目标变更校验；没有“同一批准只问一次”的 call 级共享 | 修复：逻辑调用级共享/在途合并；hook/房间/版本/目标变化使批准失效；撤销竞态四阶段测试 |
| HA-08 Web 并发 | `dispatch_model_tool_calls_parallel`（`:27692`）；`Semaphore`（`:27716`） | 所有 tool_requests 都进入并行 spawn，仅受 max_concurrency 限制；没有区分安全读/写/未知工具/资源冲突；`dispatch_model_tool_calls_parallel_preserves_order` 只测返回顺序 | 修复：安全读并行、写/未知有序、资源资格互斥、取消/撤销 |
| HA-09 验证状态语义 | `goal_phase_command_failures`（`:48139`）；`run_goal_phase_command_gate`（`:48182`）；`command_failures`（`:18118`） | 返回 `Vec<String>`：无 verification、非 Command、空 commands、gate disabled、goal/phase 读取失败都返回空 → 与“通过”不可区分；FilesExist 与 Command 未类型化；验证执行者不唯一 | 修复：类型化 `NotApplicable/Skipped/Failed/Unable/Passed`；唯一执行者；补故障注入 |
| HA-10 证据复用 | 当前无任务内验证证据复用；`task_evidence.rs` 只是 CU 截图读取（`:1-74`） | 同一任务无变化会重复执行验证；没有证据签名/输入哈希/复用边界；相关修改不会失效 | 新增：任务内证据记录/复用/失效；补 HA10-01～05 |

## 4. 审批合并粒度（HA00-02 要求确认）

当前事实：

- 会话授权：`session_grant_key(workspace_id, session_id, tool_name)`（`:3840-3847`），TTL 30min（`:3815`）；`record_session_grant` 只在 `scope=session` 时写入。
- 待审批：`pending_approvals` 以 `call_id` 为 key（`:4253`），5min TTL（`:3817`）；没有按 `(workspace, room, session, tool, 参数摘要)` 合并。
- 全量访问：`FULL_ACCESS_GRANT_TOOL = "*"`，1 年 TTL（`:3818-3821`）；与房间权限/版本的关系需要继续核对。
- 结论：**当前“合并”只到会话工具级；不是逻辑调用级，也没有在途审批合并。** HA-07 不是简单复用现有缓存，需要补 call 级身份与适用性。

## 5. 现有测试入口与缺口

| 验收领域 | 已找到的入口 | 尚不能证明 |
|---|---|---|
| HA-06 hook 超时/取消、效果保留 | `dispatch_hook_budget_tests.rs::interrupted_hook_case`（`:45`）；`dispatch_hook_budget_post_deadline_preserves_target_effect`（`:99`）；`dispatch_hook_budget_post_cancel_yields_and_preserves_effect`（`:103`） | 断流后重送不产生第二次效果；回执丢失/重送路径 |
| 身份拒绝/收尾 | `tool_dispatch_settlement.rs::refused_registration_never_reaches_file_write_or_changes_original_owner`（`:231`）；`tool_invocation_identity.rs::provider_id_is_correlated_without_colliding_across_actual_requests`（`:35`） | 所有外部服务的恰好一次；父子/attempt 关联的完整链 |
| hook 实链路 | `dispatch_hook_config_tests.rs::startup_configuration_reaches_real_dispatch`（`:25`）；`main.rs::hook_dispatch_fixture`（`:76346`）/`dispatch_hook_override_guard`（`:76301`） | 真实审批/撤销；部分测试启用了测试全开放权限 |
| HA-08 并行 | `main.rs::dispatch_model_tool_calls_parallel_preserves_order`（`:77335`） | 执行区间重叠、资源冲突、未知工具默认独占 |
| HA-09 命令门 | `main.rs::goal_command_gate_detects_exit_code`（`:61557`）；`goal_condition_validation_accepts_goal_command_gate_shape`（`:64535`）；`phase.last_evidence` command_failures 断言（`:69694`） | 无验证/空命令/读取失败的类型化区分；唯一执行者 |
| HA-10 证据复用 | 未找到任务内验证证据复用测试 | 全部 |

## 6. 隔离与运行入口

- `main.rs:1326` 启动时调用 `app_launcher::instance_restart::stop_owned_instance`；该函数会核对同用户、`coolzhu-web-console.exe`、重启凭据，并向旧实例发 `POST /api/system/restart`（`instance_restart.rs:24-57`）。**启动第二个进程不能假定不影响日常实例。**
- 隔离入口：`[web].bind_addr`（`main.rs:1323`）、`COOLZHU_RUNTIME_DIR`（`:3525-3540`）、`COOLZHU_WEB_SESSION_DB`（`:37796`）、`COOLZHU_WEB_SESSION_STORE`（`:37781`）、`COOLZHU_USER_STATE_ROOT`/`COOLZHU_USER_STATE_DIR`（`workspace_projects.rs:40-47`）、`COOLZHU_INPUT_SAFETY_STATE_ROOT`（`input_safety_store.rs:39`）。
- HA-00 计划中的隔离参数：独立 `bind_addr`（当前选择 `127.0.0.1:59299`，启动前重新探测）、独立 runtime/workspace、独立 session DB/store、独立 user state、独立 log；不替换安装版，不向日常 8765 发写请求。实际启动前还需核对 `COOLZHU_LOG_DIR` 是否被当前版本识别（静态检索未在 web-console 中发现该变量）。

## 7. 当前阻断/未确认

1. **测试编译导入错误已修，测试二进制仍受内存限制**：最初 `cargo test -p coolzhu-web-console ...` 在 `main.rs:79996`/`tool_dispatch_audit.rs:2` 报 `ToolOutcome`/`ToolOutcomeStatus`/`ToolInvocationExecutor` 未解析；已做两处纯测试/导入修复（`tool_dispatch_audit.rs` 改为直接 `runtime::{...}`；测试函数改为 `use runtime::ToolInvocationExecutor`），导入错误消失。再次编译时 rustc 在 test 目标链接/代码生成阶段报 `memory allocation of 21233680 bytes failed` + `STATUS_STACK_BUFFER_OVERRUN`（exit 0xc0000409），属于当前 Windows 提交内存限制；`cargo check -p coolzhu-web-console --offline` 的生产构建已通过（exit 0，1m07s）。确定性基线改用较小的 `coolzhu-core-runtime` 测试补一次真实回执。
2. **规划指纹不可 diff**：`90ab…` 的内容没有保留；当前只能以 0.2.142 源码为实施基线，不能逐行重放规划时差异。HA00-01 的“内容变化”以哈希/版本和当前符号为准，具体语义差异需 Pro 原包或旧快照。
3. **模型额度/身份未确认**：HA-11 的 15 组配对未开始；当前只用静态/确定性测试。正式比较前必须固定 provider/model/effort 与费用授权。
4. **隔离未实测**：当前只确认了入口和参数，尚未启动隔离实例；启动前需重新探测端口、确认 `stop_owned_instance` 不会命中日常实例。
5. **基线更新**：上述两处导入修复后已重新冻结基线（73 changed text / 29 untracked / 102 files verified，patch 660,858 B）；修复前基线副本保留在 `baseline/HA-00-20261018-pre-test-import-fix/`。

## 8. 下一步

1. 等 `cargo check` 结果：若生产构建通过，定位测试导入问题并按最小改动修复（只改测试/导入，不改业务语义），再重跑确定性基线。
2. 若生产构建也失败，先把失败作为基线阻塞记录，不改业务代码，向用户报告需要修复的编译错误。
3. 完成后补齐 `test-map`、`isolation` 的运行证据，更新 `state.json` 的 HA-00 条目。
