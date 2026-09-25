# Qwen 实际文件任务测试发现（2026-09-19）

状态：**0.2.13 真实测试发现已保留；0.2.14 修复已落地并通过定向回归**。下面的原始证据与“未修问题”描述对应 0.2.13；文末记录后续修复及验证边界。

## 本轮已修范围与新增发现的区别

本轮多模态配置已接入普通、流式与接力模型入口：`parameters.supports_multimodal` 三态、默认视觉会话转述、原生协议传图、附件/预算失败不静默丢图、真实分会话 usage。实际 qwen 原图识别和默认 Agnes 转述后识别均已验收；新增 7 项多模态测试通过，Web 当次全量 960 项通过。详见 [多模态测试契约](../testing/release-0.2.13/multimodal-test-contract.md)。

以下是在真实 SVG 文件创作任务中发现的另一个问题链：**工具输入 JSON 与思考内容没有分开；保存的内部消息又被无类型地投影为普通历史文本。** 它不等同于多模态配置失效，也不能因为已有测试通过就视为已修复。

## 证据来源与审查范围

- `tmp/2026-09-19-qwen/evidence/svg-r1-events.json`：首轮完整 SSE 记录，约 694 KB，3,863 个事件。
- `svg-r1-insights.json`、`svg-r2-events.json`、`svg-r2-insights.json`、`svg-r3-events.json`、`svg-r3-insights.json`：同目录，核对后续历史数量、上下文和真实累计用量。
- 只读提取摘要：`tmp/2026-09-19-qwen/svg-r1-readonly-audit.json`、`svg-r1-history-audit.json`。这些文件保留在本机测试目录；本文仅写计数和结构，不复制原始思考、完整工具参数、用户图片或凭证。
- 源码审查：`main.rs` 的流式 handler、`tool_messages_from_summary`、上下文装配和持久化入口；`chat_experience.js` 的 `streamEvent/intercept/toolStatus`；`chat_insights.rs` 的可见消息过滤。

本审查没有读取 runtime 配置密钥，没有新增真实模型请求，也没有通过浏览器重新观察此轮瞬时 UI。因此下面工具状态栏的条数与显示顺序属于依据实际事件和确定性前端代码的推导，不冒充新的浏览器截图验收。

## 发现 1：工具确实执行，但事件语义和状态栏聚合不完整

### 已确认事实

首轮 SSE 类型计数：started 1、message 5、message_start 2、message_delta 3,852、message_replace 1、message_done 1、done 1。独立 `tool-call` 消息为 0，但 `tool-summary` 为 2、`tool-result` 为 2。

| 时点（相对本轮开始） | 实际事件 |
| --- | --- |
| 118.288 秒 | `reasoning` delta 文本为 `模型请求工具：write_file {}`，表示首轮模型已提出工具调用 |
| 170.704 / 170.710 秒 | write_file 的 tool-summary / tool-result，状态 completed，route 为 runtime-executed |
| 175.977 / 175.984 秒 | read_file 的 tool-summary / tool-result，同样 completed/runtime-executed |
| 185.434 秒 | done，status=completed |

`tool_messages_from_summary` 当前主动构造的是 `kind=tool-summary` 和 `kind=tool-result`，只是前一条的 ID 以 `tool-call-` 开头。因此 **不能把 tool-call 消息数为 0 判成模型没有使用工具**。本轮 write/read 均有真实执行反馈；insights 累计 3 次模型请求也与首轮请求、工具反馈和最终回复链相符。

### 当前 UI 为什么仍能显示结果

`app.js::handleChatStreamEvent` 优先交 `CoolzhuChatExperience.streamEvent`；消息渲染再由 `intercept` 接管内部 kind。工具摘要和结果不会作为聊天正文卡片显示，而会进入 `chat-tool-results`。

首个 `模型请求工具：` 前缀也由 `streamEvent` 转成工具状态，所以独立 tool-call 消息缺失不会让工具结果完全消失。

### 未修问题

1. 首轮 write_file 的 requested 状态用 reasoning 消息 ID 保存，完成摘要和结果又用两个不同 ID 保存。read_file 的反馈轮并不向这条聊天 SSE 流发送同样的 requested 前缀，完成时才出现两条记录。
2. `toolStatus` 按 `message.id` 更新 Map，并直接把 Map 的大小显示成“工具活动 · N 条”，没有按真正的 tool_use_id 聚合。根据该轮事件，最终会累积 **5 条活动记录，而真实工具执行为 2 次**；其中有一条仍是旧的请求提示。
3. 后端还发 `action_step` 到独立 realtime session 事件流，但其前端处理更新的是 realtimeSessionStatus，不等于完整更新该聊天工具活动列表。不能据此认为聊天列表已具有统一 requested/running/completed/failed 生命周期。

建议：引入有稳定 tool_use_id/call_id 的专用聊天工具状态事件；首轮流式与后续反馈轮共用它，按调用 ID 更新同一条状态，完成摘要/结果作为该调用的详情。保留内部执行证据，但 UI 的“调用次数”和“活动事件条数”必须明确区分。

后续验收：write/read 两个调用都显示开始和终态；最终调用计数为 2；失败/取消保持对应终态；工具状态不产生聊天正文消息；无需从 reasoning 字符串前缀猜测工具协议。

## 发现 2：流式工具参数被混入思考，造成展示泄漏和同轮重复输入

优先级建议：P1。

### 实际字符证据

首轮共有 3,822 个 reasoning delta，合并为 **33,864 个字符**：

- 工具请求标记之前：20,918 字符。
- 工具请求标记本身：21 字符。
- 标记之后：12,925 字符。这一整段可成功解析为 JSON，键为 `path`、`content`；其中 HTML 文件 `content` 为 **11,612 字符**。

这里不需要靠“看起来像代码”判断污染：标记后的全部文本恰好是模型 write_file 的完整结构化输入。

### 源码原因

在流式 handler 的 `ContentBlockDelta::InputJsonDelta` 分支，`partial_json` 一方面正确追加到 `model_tool_calls` 的 arguments，另一方面又追加到 `reasoning_message.content`，并以 `kind=reasoning` 发给前端。

`chat_experience.js` 只把以 `模型请求工具：` 开头的那一个 delta 转进工具状态。后面的参数 JSON delta 没有这个前缀，继续进入实时思考区，最多显示其末尾 6,000 字符。因而参数代码会作为“思考”出现，虽然后续正式答复到来会清空该区域。

随后进入同轮工具反馈时，后端把整个 `reasoning_message.content` 构造成 `InputContentBlock::Thinking`，同时又把已经解析的完整 arguments 构造成 `ToolUse`。因此这 12,925 字符工具 JSON 在**同一轮请求中被重复携带**：一次伪装成 thinking/reasoning_content，一次作为正确的 tool arguments。

### 影响与改动边界

- 工具参数进入思考展示和推理字段，违反前端“工具由独立状态显示”的语义。
- 生成较长文件时，完整代码会在思考与工具输入间重复，增加请求体和上下文消耗。
- 这与思考本身较长是两件事：原模型确实先输出了约 2.1 万字符思考，不应把全部 3.4 万字符都归因于 JSON 混入。

建议只在真实 ThinkingDelta 分支维护思考缓冲。InputJsonDelta 只进入结构化工具参数缓冲；UI 通过专用工具状态显示生成/执行进度，不展示未完成参数全文。反馈请求保留合法的原生 thinking、ToolUse、ToolResult，不用整个展示缓冲重建协议字段。

后续验收：用含唯一长标记的 write_file 参数做流式 mock；标记在 ToolUse.arguments 中出现一次，在 thinking/reasoning_content 和实时思考 DOM 中均不出现；真实 ThinkingDelta 保留；第二个工具反馈轮也满足同样条件。

## 发现 3：思考与工具内部记录被跨用户轮当普通 assistant Text 重放

优先级建议：P1。

### 与实际 history loaded=7 的对应关系

根据 R1 事件和保存逻辑，首轮形成以下 7 条聊天室记录：

| 记录 | 字符数 |
| --- | ---: |
| 用户原任务 | 254 |
| write_file 摘要 | 372 |
| write_file 结果摘要记录 | 1,315 |
| read_file 摘要 | 369 |
| read_file 结果摘要记录 | 1,314 |
| 合并思考（含工具输入 JSON） | 33,864 |
| 最终答复（含本地上下文页脚） | 990 |
| 合计 | 38,478 |

R2 最终页脚实际显示 `history source: chat_room.messages; history loaded: 7`，与这 7 条吻合；`history truncated: false`。约 88% 的上述持久化字符来自合并思考。后续装配会剥离答复中的 Context usage 页脚，但不会因此去掉思考与工具记录。

### 源码原因

1. 流式结束时把非空 `reasoning_message` 与正式回复一起加入 `messages`，工具摘要/结果此前也已加入同一集合；它们随 `persist_chat_dispatch` 进入聊天室持久化。
2. `prepare_chat_dispatch` 为下一轮复制整个 `room.messages`。
3. `build_context_assembly_with_roster` 的历史筛选处理时间 floor、Goal 临时消息、空文本、过时工具拒绝话术和预算，但没有一般性排除 reasoning/tool-summary/tool-result。
4. `input_message_from_persisted` 不按 kind 恢复结构化内容，而将全部消息构造成单个 `InputContentBlock::Text`。这些内部记录原 role 多为 assistant，因此成为普通 assistant 文本历史，而非原生 reasoning_content 或配对的 ToolUse/ToolResult。

前端隐藏思考/工具卡片、搜索与消息索引排除内部消息，**都没有改变这个模型上下文选择链**。本轮 insights 仅有 2 条可见索引，但后续模型历史加载了 7 条，这是不同投影，不代表索引统计错误。

### 上下文与用量证据的解释

- R1：本地初始装配估计 1,195 token，history loaded 0；本轮最大记录输入为 24,839 token。
- R2：本地初始装配估计 11,044 token，history loaded 7；本轮最大记录输入为 48,076 token。
- R3：本地初始装配估计 15,480 token，history loaded 28；本轮最大记录输入为 38,880 token。
- 累计模型请求从 R1 的 3 次变为 R2 的 13 次、R3 的 20 次。R2 新增 10 次真实调用，不能把全部用量增加只归因于一次历史重放；工具反馈轮数、实际读取内容与记忆变化也会影响输入。

这些数字证明内部文本已进入并扩大下一轮装配，但不是对“如果修复可节省多少 token”的精确测量。要量化节省，需要同输入、同工具反馈、同模型参数的修复前后对照。

### 不是“完整 read_file 结果无限重放”

本轮两个 `tool-result` 已是摘要：`tool_messages_from_summary` 对结果正文执行 1,200 字符 compact，再加工具元数据，实际总长约 1,315。工具当轮进入模型的结果还有 `truncate_tool_result_for_context`：超过 8,000 字符时保留前 6,000 字符并说明截断。

因此准确结论是：**工具摘要和思考会以普通文本跨轮重放；完整写文件输入借道 thinking 保存和重放。** 不能把这个证据写成“所有完整 read_file 输出均无界保存到下一轮”。

### 推荐修复边界

- 将用户可见对话、结构化工具执行记录、模型原生思考、持久审计四种用途分清；保存审计不应自动意味着加入下一轮 prompt。
- 跨用户轮默认保留原任务和最终答复，对工具仅保留必要、去重的事实摘要与产物引用；原始工具参数/长输出按需检索，不把整段内部 reasoning 当 assistant 正文。
- 需要保留原生 thinking 的模型应以协议支持的结构回放，并满足消息配对与 provider 规则，不能直接混入普通 content。不要为缩小体积简单删掉当前工具循环所必需的真实 ToolUse/ToolResult。
- 修改历史筛选时同时处理已存旧记录，避免只修新消息而旧的巨型混合 reasoning 继续被反复读入；保留原始审计证据，不必删除用户历史。
- 提供可检验的 history selection evidence：明确哪些记录因“内部思考/工具审计”排除，而非只报告笼统 token 数。

后续验收：两轮真实结构化工具 mock，第一轮写入唯一长参数标记并读取结果，第二轮仅问一个普通问题；验证第二轮 assistant Text 不含思考/原始参数/重复结果记录，必要事实与产物路径仍在；统计可见消息数、实际模型消息数、选中/排除 ID、请求 token 均一致。再运行长文件、多工具、低上下文、接力与多种原生协议回归，确认修复未破坏工具闭环。

## 本次处理结论

实际 write/read 链路成功，不能因独立 tool-call 消息为零否定工具执行。0.2.13 审查登记的问题是流式参数混入思考、同轮重复携带、跨轮内部历史膨胀，以及工具状态列表缺少按调用 ID 聚合。0.2.14 的后续处理如下。

## 0.2.14 修复与可复测契约

| 问题编号 | 改动入口 | 现在的行为 |
| --- | --- | --- |
| QWEN-REAL-001 工具状态重复与缺失 | `main.rs::api_chat_send_stream`、`chat_tool_history::{call_id,status,dispatch_status}`、`chat_experience.js::{toolStatus,streamEvent,finishTools}` | 专用 `tool_status` SSE 包含稳定 `call_id`、原始 `tool_use_id`、工具名称与状态；首轮和后续响应都发 requested/running/终态。同一调用的旧摘要和结果不重复计数，同名不同调用分别展示。反馈轮把响应轮次加入身份，避免 provider 重用 call ID 覆盖旧调用。权限预览显示“未执行”；中止或缺失结果不伪报完成。终态状态只给执行事实和可解析文件路径，不携带 content/oldString/newString/originalFile 等代码字段；旧审计显示也执行同样保护。 |
| QWEN-REAL-002 参数混入思考 | `main.rs` 的 `InputJsonDelta` 分支、`reasoning_text` | JSON 分片只组装工具参数，真实 ThinkingDelta 才进入思考；工具请求经专用状态事件展示。当前工具闭环继续保留原生 Thinking、ToolUse 和配对 ToolResult。 |
| QWEN-REAL-003 跨轮审计与自动记忆污染 | `chat_tool_history::{project,project_dto,project_memory}`、上下文装配、自动记忆提取与压缩 | 默认排除 reasoning/tool-call/tool-result 等内部记录；工具摘要只保留执行状态和可识别产物路径。过滤发生在历史预算与滚动压缩之前；原存储不删除。自动提取不再把 reasoning 晋升成 L2 decision，工具只沉淀事实投影。 |

定向命令：

```powershell
cargo test -p coolzhu-web-console --offline chat_tool_
node --test modules/gui-web/packages/web-console/tests/chat-tool-status.test.cjs
```

Rust 7 项通过，源码在 `src/chat_tool_history.rs::tests`：

- `chat_tool_stream_two_calls_and_next_user_turn_keep_protocol_and_trim_internal_history`：真实本地 HTTP mock，首轮流式 write_file（参数含一万字符唯一标记）、反馈轮 read_file、第三次最终回复，再发第二用户轮的第四次请求。检查真实文件、两次调用各三阶段、原生工具 ID 配对、真实思考保留、JSON 不入思考、下一轮无旧思考/长参数/原始审计且保留文件事实；下一轮 messages JSON 小于 6,000 字节。
- `chat_tool_history_filters_audit_before_budget_and_compaction_without_mutating_store`：26 万字符内部审计不挤占 200 token 历史预算，不污染自动压缩；正常用户、产物事实和最终答复保留。
- `chat_tool_history_preserves_outcome_and_artifact_without_raw_arguments_or_results`：别名、原始用户文本、未知摘要和完整产物字段边界。
- `chat_tool_memory_stops_legacy_audit_recall_but_keeps_manual_and_final_reply_memories`：旧自动思考/旧无类型压缩不自动召回，手工 decision 和正常答复经验保留，原数据不变。
- `chat_tool_status_correlates_requested_running_and_terminal_without_parameters`：状态 ID、不同用户轮/反馈轮分离及权限预览语义。
- `chat_tool_planner_request_preserves_planner_contract_and_session_reasoning`：内部规划请求保留独立系统契约和会话思考参数，不附加聊天禁工具话术。
- `chat_tool_cancellation_checker_keeps_actual_token_after_scope_cleanup`：真实取消 token request 传给工具检查器，外层清理映射后闭包仍看到取消。

前端 6 项通过，源码在 `tests/chat-tool-status.test.cjs`，使用真实前端模块公共事件入口及最小 DOM：同名工具两调用聚合、临时思考的清理、中止/缺终态不伪完成、旧摘要/结果和权限预览兼容、结构化工具结果中的代码字段不渲染、超过 30 次显示窗口时总调用数仍准确。日志在 `tmp/2026-09-19-agent-fixes/test-chat-tools.log`、`test-chat-tool-status-js.log`。

兼容边界：旧 `chat-room:auto-extract` 中的 decision 由原 reasoning 分支产生，停止自动召回；旧工具自动记忆尝试只提取事实。旧 `context:auto-compact` 没有逐消息 kind 或 origin ID，无法可靠拆分，仍保留存储与查看，但不再自动注入。新安全压缩使用 `context:auto-compact:v2`；手工来源、正常用户最终答复提取的经验不被批量删除。旧图像、工具参数或审计原文也未删除。

完整 Web 测试已通过 981 项（main 972、lib 8、native host 1），日志为 `tmp/2026-09-19-agent-fixes/test-web-all.log`。最后仅前端计数窗口调整另经上述 6 项 Node 行为测试；最终内嵌资源由发布构建验证。

这些定向结果是可重复的合成 HTTP/真实文件操作和前端行为测试，不是新的真实 Qwen/Agnes/Paint 能力评估，也不是 token 节省比例测量。完整后端、真实模型、桌面操作和安装包验收以 0.2.14 交付报告为准。
