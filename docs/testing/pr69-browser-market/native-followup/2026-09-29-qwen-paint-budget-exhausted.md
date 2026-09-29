# 2026-09-29 Qwen Paint：0.2.27 安装版 computer-use 预算耗尽证据

- 测试对象：安装版 0.2.27；目标 `qwen(qwen3.8-flash)`；聊天室 `验收-20260927`（`room-1790469689319`）。
- 前置事实：安装前后同根数据比对已由主会话核实为 59 文件、90,440,400 bytes、manifest hash unchanged；本报告不重复采样，也没有重启或改动数据。
- 请求前缀：`NATIVE-PAINT-20260929-0227-A`。
- 请求约束：只允许 Qwen 自身的 `computer_use` 在现有 Paint 空白画布画约 100px 黑色水平线；禁止终端、代码、脚本、保存文件和操作其他窗口。

## 完整时间线（Asia/Shanghai）

1. `10:18:23.929`：用户消息 `msg-1790648303801-user` 写入，发送对象为 `qwen(qwen3.8-flash)`。
2. `10:18:24.083–10:18:29.438`：顶层 Qwen chat 请求（`request-1790648304075-14844-1:1`）已 dispatched 且 completed；usage 为 input 6,360 / output 207 tokens。
3. `10:18:29.474`：`computer_use_perform` tool call 创建；`10:18:29.513` 建立 CU run，deadline 为 `10:20:29.513`。
4. 同一 CU call 的 verification：usage dispatch 为 `10:18:35.405`，request id 为 `request-1790648315400-14844-2:1`；planner diagnostics 的 provider 时间为 `10:18:35.399–10:18:43.504`，provider response id 为 `chatcmpl-d4beadfe-152f-92bc-9bd0-e8ac5d3e1cdb`；usage finished 为 `10:18:43.503`，status `completed`，input 18,746 / output 329。
5. 同一 CU call 的第 1 次 `computer_use_planning`：usage dispatch 为 `10:18:43.536`，request id 为 `request-1790648323530-14844-3:1`；planner diagnostics 的 provider 时间为 `10:18:43.529–10:19:01.846`，provider response id 为 `chatcmpl-4317d766-ffd9-937e-b7be-646bdc29f226`；usage finished 为 `10:19:01.845`，status `completed`，input 19,605 / output 1,405。此前报告中的 `10:18:54.845` 没有对应本 run 的字段，属于错误抄录，已更正。当前数据库没有该 request 的独立 controller-completed 字段；紧接的 step 0 开始时间 `10:19:01.851` 只代表 step 时间，不能冒充 controller completion。
6. `10:19:01.851–10:19:03.677`：持久化 step 0 记录为 `failed / stale_observation`，`input_delivery=not_sent`；没有鼠标输入。
7. 同一 CU call 的第 2 次 `computer_use_planning`：usage dispatch 为 `10:19:05.567`，request id 为 `request-1790648345562-14844-4:1`；usage 在 `10:19:26.005` 才标记 `completed`，input 19,671 / output 1,485，cache_read 16,128。该 request 没有对应的 planner diagnostics provider 时间行；controller run 已在 `10:19:25.583` 进入终态，因此这里仅记录 usage 的网络 completed 时间，不能写成 controller 已接受的完成时间。
8. `10:19:25.634–10:19:31.550`：内部 `tool_feedback` 请求（`request-1790648365627-14844-5:1`）completed，input 7,955 / output 296，cache_read 6,144。
9. `10:19:31.561`：工具结果与 Qwen 最终回复写入；UI 显示本轮 `#13` 结果、随后 `#14 · 本轮 1 分 7 秒`。
10. `10:19:31.664`：顶层 chat runtime `run-chat-0d63cb986d7f6952b0c8497496ce50dd8a48ced87ce9138d` 进入 `completed`，无仍运行任务。

## 真实工具身份与终态

- `computer_use_runs.call_id`：`cu-session-1779459149988-000000000000000218d9a9252d8bc2c8-tool-92f9615dcfb569130de6ca07030f3f4b14d65ad5a3b2f6c0ee812669db7b66f9`
- `computer_use_runs.provider_tool_call_id` / `tool_calls.tool_call_id`：`tool-92f9615dcfb569130de6ca07030f3f4b14d65ad5a3b2f6c0ee812669db7b66f9`
- `tool_calls.provider_tool_call_id`：`call_44f9bd429b549cebd154af7`
- 聊天消息中的工具结果 id：`tool-call-001eb788eca8d9a8`
- 工具状态：`blocked`；阶段：`planning`；错误码：`budget_exhausted`。
- 原始错误：`computer_use_planning cannot start: remaining computer-use budget 0 ms is below the 500 ms minimum; no model request was sent`。
- 用户可见结果：`steps_completed=0`、未发出鼠标输入、目标未完成；不可重试（`retryable=false`）。
- 持久化 run：`state=blocked`、`state_version=13`、顶层 `action_count=0`、`goal_achieved=false`。
- 持久化 step：唯一 step 0 为 `failed / stale_observation`、`input_delivery=not_sent`；这不是成功动作，也没有改变画布。
- 终态中的 supervisor 诊断另记录 `action_count=1 / replan_count=1`，但 terminal result 的 `steps_completed=0`，且没有任何物理输入；本报告按用户可见终态和物理输入事实判定为 0 步失败。

原始回执中的英文 “no model request was sent” 必须按原文保留，不能单独据此断言网络侧没有发起内部规划请求。数据库同时记录了顶层 Qwen、verification、两次 planning 及 tool_feedback 请求；其中第二次 `computer_use_planning` usage 从 `10:19:05.567` 开始，网络记录到 `10:19:26.005` 才 completed，而 controller 的 CU run 已在 `10:19:25.583` 进入终态。Sol 复核的解释是：第二次 planning 约 20 秒固定等待后被 controller 丢弃/误报 `budget_exhausted`，其迟到的 completed 响应不能当作 controller 已接受的规划结果；root deadline 为 `10:20:29.513`，终态时理论上仍余约 64 秒。故本报告把“原始 controller 错误回执”“网络 completed 的迟到响应”和“controller 丢弃/误报诊断”分开记录，不把该英文片段改写成顶层 Qwen 请求未发送的结论。

## 窗口与截图操作边界

- 从 `10:18:23.929` 提交到 `10:19:31.664` runtime completed 期间，测试者没有调用 `sky.activate_window`、click、type 或键鼠输入；只做了安装版控制台的 `get_window_state(..., include_screenshot=false)` 只读观察和数据库只读核对。
- CU 内部证据出现 `observation_window_selection:Some(34605722):uia_win32_no_synthetic_input`，这是被测 computer-use 自己选择 Paint 窗口的记录，不是测试者切换窗口；与 step 0 `stale_observation` 一并保留，不能归因于测试者鼠标干扰。
- runtime completed 后，测试者先通过 Sky 读取 Paint 状态并保存 B-18，没有显式激活 Paint；之后第一次控制台截图 B-19 被前景 ChatGPT 窗口遮挡，已排除为废证据。最后才在 runtime 完成后显式 `sky.activate_window` 激活控制台并取得无遮挡 B-20。因此 B-20 的激活动作不在模型执行期间，不影响本轮 tool 结果。

## 原始截图

- [B-18-0.2.27-paint-after-qwen-budget-exhausted.jpg](B-18-0.2.27-paint-after-qwen-budget-exhausted.jpg)：Paint 结果，画布空白，SHA-256 `f1f21f9b6ec139fd45c013171aa9c5e744ac31aee7e09ca06f0964f804d30f72`。
- [B-20-0.2.27-console-final-unobstructed-budget-exhausted.jpg](B-20-0.2.27-console-final-unobstructed-budget-exhausted.jpg)：激活控制台后的 fresh 终态截图，显示 `blocked / planning / budget_exhausted / steps 0 / no model request was sent`，SHA-256 `c9a5be8298c55dcfaf0aeee8833101a5d28bbfd9cef7018eaa3447c42221363a`。
- B-19 为 ChatGPT 遮挡的截图，明确排除，不作为验收证据。

结论：安装版 0.2.27 的本轮真实 Qwen Paint 验收未完成；在 computer-use 规划预算门禁处终止，未画线、未发送物理输入、未进行海绵宝宝扩展测试。
