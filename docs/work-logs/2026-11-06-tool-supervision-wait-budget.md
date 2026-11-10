# 2026-11-06 会话工具阻断与会话超限（反馈轮）分析与修复

## 现象（用户截图）

rea-video 视频分解会话（magpie/devin/swe-2）：
1. 模型 `PowerShell run_in_background` 起后台任务（pid 115760）；
2. 调 `Sleep` 等待 → 「Sleep 被停止且未确认收尾」；
3. 「当前会话工具暂时被阻断，无法继续读取后台输出」；
4. 用户追问原因，模型自析"等待/轮询链路失败 + 任务不吐日志 → 误判全局阻断"；
5. 次轮继续轮询，8 分钟后命中 `tool_feedback_round_limit`（max=40, pending=1），本轮任务未完成。

## 根因链路

**「会话工具被阻断」= supervision 的 unsettled 会话门（设计行为，被 Sleep 误触发）**

- `runtime_tool_supervision.rs:56-60`：同 workspace+session 只要有 worker 标 `unsettled`（已请求停止但未确认退出），后续工具调用一律拒绝："该会话仍有已请求停止但未确认退出的工具"。本意是防未确认副作用被后续调用覆盖。
- 触发链：`Sleep` schema 只有 `duration_ms`，`tool_timeout_ms_for` 无对应分支 → 落默认 `default_timeout_ms=30s`。模型睡 >30s → supervisor 超时 `control.cancel()`；但 `execute_sleep` 是 `std::thread::sleep` 整段睡，**不感知 ExecutionControl**，睡到自然醒才退出。阻断窗口 = 剩余睡眠时长（模型睡 120s → 后续 ~90s 全拒绝）。
- 阻断期间 TaskOutput 也被拒 → "无法继续读取后台输出"。

**「会话超限」= 轮询式等待烧反馈轮**

- TaskOutput 无等待模式，模型只能 TaskOutput+Sleep 轮询；rea-video 几分钟不吐日志 → 每轮一个反馈轮 → 默认 `max_feedback_rounds=40` 烧光 → `tool_feedback_watchdog_message` 终止。

## 修复（4 处，最小改动）

1. **`tool-registry/src/lib.rs` `execute_sleep` 分片可中断**：50ms 切片循环检查 `runtime::managed_process::current_execution_control().interruption()`，被打断立即返回（`SleepOutput.interrupted`）。worker 取消后 ~50ms 内确认退出，不再进入 unsettled。
2. **`TaskOutput` 加 `wait_ms`**（schema/input/实现）：有界等待到「进程退出 / 日志新增字节 / 预算耗尽 / 被取消」任一发生即返回；250ms 探测粒度、上限 300s。一次调用替代 N 次轮询。输出加 `interrupted`。
3. **`main.rs` `tool_timeout_ms_for`**：`Sleep` → `duration_ms + 5s` slack；`TaskOutput` → `wait_ms + 5s` slack（均受既有 600s 钳位）。等预算匹配工具声明的等待语义，不再被 30s 默认拦腰杀。
4. **`runtime_tool_supervision.rs`**：`WorkerScope` 记 `tool_name`，拒绝文案带上阻塞中的工具名（模型/用户可辨识是 Sleep 还是别的在收尾）。

Sleep/TaskOutput 描述同步更新（告知模型：等待后台任务优先用 `wait_ms`，别用 Sleep+轮询）。

## 验证

- `cargo build -p coolzhu-tool-registry --offline` / `-p coolzhu-web-console --offline`：通过。
- 新增 4 测试全过：
  - `sleep_exits_promptly_when_execution_control_cancelled`：已取消 control 下 60s 睡 <2s 返回 interrupted；
  - `task_output_wait_returns_at_budget_when_task_stays_alive`：存活 pid + 日志不增 → 等满 500ms 预算返回 running；
  - `task_output_wait_returns_immediately_when_task_not_running`：死 pid → 首探测即返回；
  - `wait_tools_timeout_follows_declared_duration`（main.rs）：Sleep/TaskOutput.wait 预算按声明发放、缺省回默认、超上限钳 600s。
- 回归 7/7：`web_search_tool_timeout_follows_declared_budget`、`tool_feedback_watchdog_*`、`dispatch_hook_wire_cancelled_turn_*` 等。

## 不改动项

- `max_feedback_rounds` 默认 40 / clamp 64 不动（设计上限，有 wait_ms 后等效等待时长≈40×300s，够用）。
- unsettled 会话门保留：真实阻断窗口已从「剩余睡眠时长」缩到 ≈分片粒度；门本身是防未确认副作用的安全设计。
- WebFetch/REPL 等其他同步工具未同样分片化（无相同触发场景，Sleep 是模型轮询的标准等待动作）。
