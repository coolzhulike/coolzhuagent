# RPR-11c：预算链的源码核对与准入设计

- 工单：RPR-11c（本轮**只做设计与实测清单，不改源码**）
- 日期：2026-09-24
- 结论口径：每条结论标注 `已确认源码 <file>:<line>` 或 `未验证`。**未读到的内容一律写"未验证"，不写成已确认。**
- 本轮**未发起任何真实模型调用**，未启动本地模型，未打任何 LLM / 视觉接口。所有"实测数字"均来自历史报告并已注明出处。

## 0. 源码指纹与行号漂移警告

本轮取证期间 `main.rs` **被并发修改**（同一仓库有其它 agent 在工作）：

- `modules/gui-web/packages/web-console/src/main.rs`：mtime `2026-09-24T01:43:02+08:00`，size `3,405,151`，md5 `a0ee6c53d2d27d981300656983df5b88`
- 同一轮内 `computer_use_executor.rs` 与 `computer-use-core/src/contracts.rs` 也在 `2026-09-24T01:42` 被改动

因此 `main.rs` 的行号在本文档写完后**可能继续漂移**（本轮已经漂移过一次，见下方"历史材料与当前源码不符"第 6 条）。
本文档内所有 `main.rs` 行号均为上表中 md5 对应版本，并额外给了**稳定锚点**（函数名/代码片段），复核时请以锚点为准。

其它被引用文件的指纹（md5 前 8 位 / mtime）：

| 文件 | md5(8) | mtime |
| --- | --- | --- |
| `modules/gui-web/packages/web-console/src/computer_use_adapters.rs` | `3c8b8e99` | 2026-09-22T07:37 |
| `modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs` | `d9781b4f` | 2026-09-22T07:37 |
| `modules/gui-web/packages/web-console/src/browser_bridge.rs` | `3f79baac` | 2026-09-22T07:37 |
| `modules/gui-web/packages/web-console/src/computer_use_planner.rs` | `9aec0643` | 2026-09-21T07:10 |
| `modules/gui-web/packages/web-console/src/multimodal_input.rs` | `158115f8` | 2026-09-21T07:10 |
| `modules/gui-web/packages/web-console/src/computer_use_executor.rs` | `3c869f79` | 2026-09-24T01:42 |
| `modules/computer-use/packages/computer-use-core/src/controller.rs` | `91c8404a` | 2026-09-22T06:17 |
| `modules/computer-use/packages/computer-use-core/src/supervisor.rs` | `0aee718d` | 2026-09-22T06:13 |
| `modules/computer-use/packages/computer-use-core/src/contracts.rs` | `a953445c` | 2026-09-24T01:42 |
| `modules/computer-use/packages/computer-use-core/src/input_stroke.rs` | `c76d0fcf` | 2026-09-22T05:48 |
| `modules/core-runtime/packages/core-runtime/src/run_contract.rs` | `3e08e71c` | 2026-09-22T00:49 |
| `modules/llm-adapter/packages/llm-adapter/src/providers/openai_compat.rs` | `55303b12` | 2026-09-22T23:41 |
| `modules/llm-adapter/packages/llm-adapter/src/providers/claw_provider.rs` | `be49b171` | 2026-09-22T23:41 |
| `modules/vision/packages/vision-service/src/lib.rs` | `5525bd1e` | 2026-09-16T00:15 |

---

## 1. 实际生效的预算层级

### 1.1 结论速览

实际生效的预算链**不是**"根 deadline 逐层收紧"，而是**彼此独立、取 min 都不一定取到的若干固定上限**，其中：

- **外层用户运行的根 deadline：源码中不存在。** `RunBudget` 契约结构体已定义但**未接线**（见 1.2）。
- **单次模型请求的硬上限 = 600 秒**，来自 provider 的 HTTP 客户端，计时从请求发出开始（见 1.3）。
- **另一条"600 秒上下限"来自工具执行策略**，`clamp(100, 600_000)`，作用于**配置值而非剩余时间**（见 1.4）——这是工单线索里那个 `clamp(100, 600_000)` 的真身，**它不是模型请求上限**。
- **CU 子运行有独立 120 秒任务预算**，由 config 决定，与任何父 deadline **无推导关系**（见 1.5）。
- **planner / verifier 内部模型请求有独立的 20 秒外层 tokio 上限**，这个 20 秒**既不看剩余父预算、也不看 CU 剩余预算**（见 1.6）。
- **本地模型"加载→就绪"上限 = 1s~120s（默认 60s）**；**"排空→卸载"无任何上限**（见 1.7）。
- **取消收尾有一个固定 2 秒宽限，该宽限不被 any remaining 收紧**（见 1.8）。

### 1.2 外层根 deadline / 接纳与终止时刻 —— **未实装**

**已确认源码**：契约结构体存在：

- `modules/core-runtime/packages/core-runtime/src/run_contract.rs:201-216`
  ```rust
  /// 预算是一次 run 的上限快照；执行器应在每个安全点比较 `deadline_unix_ms`。
  pub struct RunBudget {
      pub deadline_unix_ms: u64,
      pub max_actions: u32,
      pub max_replans: u32,
      pub max_request_attempts: u32,
  }
  impl RunBudget { pub const fn is_expired_at(self, now_unix_ms: u64) -> bool { ... } }
  ```
  该文件模块头自述（`run_contract.rs:1-4`）："**此模块只定义跨入口共享的事实口径，不拥有数据库、调度循环或桌面输入**"。

**已确认源码（关键）**：`RunBudget` 在**全部生产路径中没有任何构造点**。

- 仓库内引用 `RunBudget` / `run_contract` 的文件仅：`core-runtime/{run_contract.rs, recovery.rs, lib.rs, late_facts.rs, action_injection.rs}` 与 `computer-use-core/{lib.rs, controller.rs, supervisor.rs}`。
- `run_contract.rs:285`、`recovery.rs:105-106` 两处构造**都在 `mod tests` 内**。
- **`modules/gui-web/packages/web-console/src/` 下对 `RunBudget` / `run_contract` / `deadline_unix_ms` 的引用数为 0**（对 `modules/gui-web/packages/web-console/src/` 与 `packages/` 直接 grep，无命中）。

**已确认源码**：聊天轮的终态枚举里没有时间型终态：

- `main.rs:185-199` `enum ChatTurnStatus { Running, InterruptRequested, Interrupted, Completed, Failed }`，无 `TimedOut` / `DeadlineExceeded`。
- 对 `turn_deadline` / `run_deadline` / `TURN_TIMEOUT` / `CHAT_TURN_TIMEOUT` / `max_turn_duration` / `wall_clock` 在 `web-console/src/` 下 grep **无命中**。

**结论**：`外层用户运行的根 deadline / 接纳与终止时刻` 在**当前源码中不存在**。用户运行的终止只能来自：用户取消（cancellation token，`main.rs:375-379` `tool_turn_cancellation_checker`）、正常完成、或某一层子请求失败。**"接纳时刻"没有任何实装**——没有任何地方在 run 开始时计算并冻结一个 deadline。

### 1.3 单次模型请求：配置上限 vs 硬上限

**硬上限 = 600 秒，来自 reqwest client**：

- `modules/llm-adapter/packages/llm-adapter/src/providers/openai_compat.rs:200-209`
  ```rust
  /// 构造带超时的 HTTP 客户端：connect_timeout 让不可达端点快速失败，
  /// timeout 给单次请求（含 reasoning_effort=max 深思考）一个 10 分钟上界，避免上游挂起导致
  /// `send_message().await` 无限期阻塞、拖垮整个会话回合并使 web-console 长时间无响应（平台并发缺陷 #6 根因之一）。
  fn build_http_client() -> reqwest::Client {
      reqwest::Client::builder()
          .connect_timeout(Duration::from_secs(20))
          .timeout(Duration::from_secs(600))
          .build()
          .unwrap_or_else(|_| reqwest::Client::new())
  }
  ```
- `modules/llm-adapter/packages/llm-adapter/src/providers/claw_provider.rs:124-129`：同一形态的 `fn build_http_client()`，`.connect_timeout(20s)` + `.timeout(600s)`。

**关键：`build_http_client()` 同时供 `send_message` 与 `stream_message` 使用**（`openai_compat.rs:234`、`claw_provider.rs:136/151` 都把它装进 provider 的 `http` 字段），所以**流式主聊天路径（`main.rs:30892-30894` `.stream_message(&request)`）也共享同一条 600s 硬上限**。

**计时起点**：reqwest 的 `ClientBuilder::timeout` 是**整个请求的 deadline**，从请求被发出（含连接建立）起算到响应体读完；**包含预填充（prefill）阶段**。→ 这就是历史报告里"整次调用超过 600 秒被截断并自动重试"的那条 600 秒。

**作用范围（已确认源码，逐个列出主要模型调用点）**：

| 调用点 | 外层 tokio 超时 | 生效硬上限 |
| --- | --- | --- |
| 流式主聊天 `main.rs:30892-30894`（`.stream_message(&request)`） | **无** | 600s（与 `send_message` 共用 `build_http_client()`） |
| 聊天主循环 `main.rs:26734`（`.send_message(&request).await?`） | **无** | 600s |
| 工具轮多轮 `main.rs:17814`（`await_chat_turn(cancellation, client.send_message(&request))`） | **无**（`await_chat_turn` 只在取消时返回，`main.rs:540-551` 无超时） | 600s |
| 单图理解 `main.rs:26420` | **无** | 600s |
| 恢复重答 `main.rs:33835` | **无** | 600s |
| 光标位置视觉校验 `main.rs:28839`（`verify_cursor_on_target`，`main.rs:28754` 起） | **无** | 600s（**外加** `sleep(300ms)` + 一次桌面截图，`main.rs:28765`、`28772`） |
| 附件图片描述 `multimodal_input.rs:92-94` | **120s** `tokio::time::timeout` | min(120s, 600s) = **120s** |
| CU planner / CU verifier `computer_use_planner.rs:492-495` | **20s** `tokio::time::timeout` | min(20s, 600s) = **20s** |

**注意**：`main.rs:28839` 的 `verify_cursor_on_target` 是一条**藏在输入路径里的视觉模型请求**，被 `main.rs:22418` 与 `main.rs:23858` 调用；它不经过 CU 控制器、不花 CU 预算、也不进 `1+2n` 计数（详见第 4 节）。

### 1.4 工单线索中的 `clamp(100, 600_000)` 定位 —— **它是工具执行超时，不是模型请求上限**

**已确认源码**：

- `main.rs:6651`（稳定锚点：`fn tool_execution_policy()` 内）
  ```rust
  default_timeout_ms: execution.default_timeout_ms.clamp(100, 600_000),
  ```
  这是**工具执行策略**的配置钳位：[100ms, 600s]，来源 `workspace_config().tool.execution.default_timeout_ms`，默认 `30_000`（`main.rs:6563-6565` `fn default_tool_timeout_ms`）。
- `main.rs:6682`（稳定锚点：`fn tool_timeout_ms_for` 末行）
  ```rust
  requested.clamp(1, 600_000)
  ```
  这是**单次工具调用超时**，同样是配置/入参钳位，不是模型请求。

**裁决相关结论**：`clamp(100, 600_000)` 的"下限回扩"在**当前调用形态下不发生**，因为它的入参是**配置值**（`default_timeout_ms`）或**模型给的 timeout 字段**，不是"剩余预算"。也就是说：现存的 `clamp(100, …)` **不是**"把不足 100ms 的剩余时间扩成 100ms"的现场；真正的问题现场在 CU 桥接层（第 2 节），而那一层**用的不是 clamp 而是 `min`**。

**但这条仍是风险**：只要将来有人把 `remaining` 传进 `tool_execution_policy()` / `tool_timeout_ms_for()`，`clamp(100, …)` 立刻变成回扩。产品裁决的要求（剩余不足即拒绝）应当**明确禁止** `remaining` 流经任何带下限的 `clamp`。

### 1.5 CU 子运行的独立任务预算，及其与根 deadline 的关系

**已确认源码**：

- `RunBudgetGuard`：`modules/computer-use/packages/computer-use-core/src/supervisor.rs:173-207`
  ```rust
  pub struct RunBudgetGuard { budgets: ComputerUseBudgets, started_at_ms: u64, ... }
  pub fn new(budgets: ComputerUseBudgets, started_at_ms: u64) -> Self { ... }
  /// 单调 deadline 的剩余预算（毫秒，饱和到 0）。
  #[must_use]
  pub fn remaining_ms(&self, now_ms: u64) -> u64 {
      self.budgets.timeout_ms.saturating_sub(now_ms.saturating_sub(self.started_at_ms))
  }
  ```
- `before_action`：`supervisor.rs:209-239`，按序检查 **deadline**（`now - started >= timeout_ms` → `deadline_exceeded`）、`max_no_progress_steps`（`no_progress`）、`max_actions`（`budget_exhausted`）、`max_same_signature`（`no_progress`）。
- `record_replan`：`supervisor.rs:249-255`，`replan_count >= max_replans` → `budget_exhausted`。
- `after_verification`：`supervisor.rs:241-247`，`visible_progress` 为真则清零 `no_progress_count`，否则 +1。
- 预算默认值：`contracts.rs:214-225`
  ```rust
  max_actions: 12, max_replans: 2, max_same_signature: 2,
  max_no_progress_steps: 2, timeout_ms: 120_000, max_calls_per_turn: 2,
  ```
  → **历史材料的"120 秒 CU 任务预算"在当前源码中仍然生效**（默认值）。

**计时起点**：`controller.rs:378-379`
```rust
let started_at_ms = self.clock.now_ms();
let mut guard = RunBudgetGuard::new(self.budgets, started_at_ms);
```
即 **CU 子运行自己的起点**，不是父 run 的起点。

**预算来源**：config 而非父 deadline：

- `main.rs:5610-5621`（`impl ConfigComputerUse { fn budgets(&self)`）：`timeout_ms: self.controller.timeout_seconds.clamp(5, 300) * 1_000`
- 默认 `timeout_seconds = 120`：`main.rs:5602-5604`（`fn default_computer_use_timeout_seconds`）
- 构造点：`computer_use_executor.rs:1144` `ComputerUseExecutor::new(&planner, &adapters, &store, config.budgets())`
- 契约测试固定了默认值：`computer_use_store.rs:806-819`（`budgets.max_actions == 12`、`budgets.timeout_ms == 120_000`）

**与根 deadline 的关系 —— 无**：

- 入口签名 `computer_use_executor.rs:1092-1096`
  ```rust
  pub(crate) async fn execute_with_current_runtime(
      input: &JsonValue,
      identity: &ToolCallIdentity,
      chat_room_id: Option<&str>,
  ) -> ComputerUseResult
  ```
  **没有任何 deadline / remaining / budget 入参**。
- 调用点 `main.rs:33709-33711` 也不传任何时间预算，且**不套工具超时**（该分支在 `main.rs:33695-33712` 直接 return，早于运行时工具路径）。
- 因此：**CU 的 120s 与父 run 的剩余时间完全无关**。父 run 只剩 10 秒时，CU 仍会拿到完整 120 秒预算。

**`remaining: Duration` 逐阶段传递（本轮新增）的范围 —— 只覆盖 adapter，不覆盖 planner**：

- `ComputerUseAdapter` trait **有** `remaining`：`controller.rs:40-51`（`observe` / `act` / `verify` 各带 `remaining: std::time::Duration`），并有明确注释"实现方**必须**用它收紧自己的子请求超时上限（§2.3「子请求上限不超过剩余预算」），不得用固定常量硬顶"。
- `ComputerUsePlanner` trait **没有** `remaining`：`controller.rs:11-35`（`classify(request, observation)`、`next_action(request, observation, step)`、`verify(request, before, after, verification)`）。
- controller 每个阶段都重新取 `remaining` 再交给 adapter：`controller.rs:386`、`458`、`623`、`641`、`693`、`712`。
- 但交给 planner 的调用**不带 remaining**：`controller.rs:502-506`（`next_action`）、`controller.rs:716-721`（`verify`）、`controller.rs:405`（`classify`）。

→ **这是第 1 节最重要的结构性缺口**：真正发起模型请求的 planner/verifier **拿不到** CU 剩余预算；而拿到 remaining 的 adapter 只做屏幕截图与桌面/浏览器输入。

### 1.6 planner / vision / verifier 的上限与剩余父预算

**已确认源码（逐个核对，历史数字不可直接采信）**：

| 角色 | 常量与数值 | 位置 | 是否受剩余父预算约束 |
| --- | --- | --- | --- |
| CU 规划（planning） | `PLANNER_TIMEOUT = 20s`，`tokio::time::timeout(PLANNER_TIMEOUT, client.send_message(&request))`，超时报 `"planner timed out after 20 seconds"` | `computer_use_planner.rs:12`、`:493-494` | **否**（固定常量，planner trait 无 remaining） |
| CU 验收（verification） | 同上（`verify_visual` 复用 `request_model`） | `computer_use_planner.rs:583-584` 经 `:476-507` | **否** |
| CU 视觉描述（planner 模型不支持图片时的**额外**请求） | 同上 20s | `computer_use_planner.rs:546-547` | **否** |
| CU planner 请求的 max_tokens | `4096` | `computer_use_planner.rs:489` | — |
| CU planner 响应体上限 | `MAX_PLANNER_RESPONSE_BYTES = 8 * 1024` | `computer_use_planner.rs:13` | — |
| CU observation 上限 | `MAX_OBSERVATION_CHARS = 64 * 1024` | `computer_use_planner.rs:14` | — |
| 附件图片描述（vision） | `120s`，超时报"默认视觉 Agent 描述图片超时…" | `multimodal_input.rs:92-94` | **否** |
| vision-service 本地 VLM 默认超时 | `DEFAULT_LOCAL_VISION_TIMEOUT_SECONDS = 180`（实际 `timeout_seconds.max(5)`） | `vision-service/src/lib.rs:22`、`:452-453`、`:538-539` | **否** |
| vision locate 默认 | `default_timeout_ms() = 15_000` | `vision-service/src/locate.rs:122`（`assert_eq!(req.timeout_ms, 15_000)` 见 `:235`） | **否** |
| 光标位置视觉校验 | 无外层超时 → 600s | `main.rs:28754`、`28839` | **否** |
| CU 适配器截图 | `CAPTURE_TIMEOUT = 10s`，`clamp_stage_timeout(remaining, CAPTURE_TIMEOUT)` | `computer_use_desktop_bridge.rs:432`、`:440` | **是** |
| CU 浏览器桥往返 | `BRIDGE_TIMEOUT = 10s`，`clamp_stage_timeout(remaining, BRIDGE_TIMEOUT)` | `browser_bridge.rs:27`、`:107` | **是（但见 :595 例外）** |

**核实结论（对历史材料）**：

- `20 秒规划上限` → **仍然生效**（`computer_use_planner.rs:12`）。
- `20 秒视觉请求上限` → **不成立/表述有误**：CU 路径里的视觉请求确实复用 planner 的 20s（`:546`、`:583`），但**附件图片描述是 120s**（`multimodal_input.rs:93`），**vision-service 内部默认是 180s**（`vision-service/src/lib.rs:22`），**locate 默认是 15s**（`locate.rs:122`），**光标校验无上限（600s）**（`main.rs:28839`）。"视觉请求 = 20 秒"只在"CU planner/verifier 经 `request_model`"这一条路径上成立。
- `120 秒 CU 任务预算` → **仍然生效**，且是默认值（`contracts.rs:221`、`main.rs:5602-5604`），但**可被 config 改到 [5s, 300s]**（`main.rs:5620`）。

### 1.7 本地模型切换（排空 → 卸载 → 加载 → 就绪）的耗时上限

**已确认源码**：

- 切换入口：`main.rs:3910`（稳定锚点：`fn switch_local_models(mode: &str)`），分支 `"chat"` / `"vision"` / `"off"`（`main.rs:3911-3920+`）。
- **释放（卸载）**：`main.rs:3885-3904`（`fn release_local_model_roles`，该函数起于 `main.rs:3885`）→ `main.rs:3497-3505`（`fn stop_managed_local_service`）→ `main.rs:3522+`（`fn terminate_process_tree`，`#[cfg(windows)]`）
  ```rust
  match std::process::Command::new("taskkill")
      .args(["/PID", &pid.to_string(), "/T", "/F"])
      .output()
  ```
  **`taskkill` 是阻塞调用且没有任何超时**（没有 `tokio::time::timeout`，没有 `wait_timeout`）。
  → **"卸载"阶段无耗时上限（未设界）。**
- **排空（drain，等待在途请求结束）**：在 `release_local_model_roles` / `stop_managed_local_service` / `switch_local_models` 中**未发现任何等待在途请求的步骤**——直接 `taskkill /T /F` 强杀进程树。对 `drain` / `排空` 在 `main.rs` grep 只有 `messages.drain(..)`、`collected.drain(..)` 这类集合操作（`main.rs:48047`、`4658`），与请求排空无关。
  → **"排空"阶段在当前源码中不存在（未实装）。**
- **加载 + 就绪**：`main.rs:3754-3777`（`fn start_local_gemma`，该函数起于 `main.rs:3716`）
  ```rust
  let deadline = Instant::now() + Duration::from_millis(runtime.startup_timeout_ms);   // :3754
  while Instant::now() < deadline {
      if local_gemma_ready(runtime.port) { return (true, format!("... ready ...")); } // :3756
      std::thread::sleep(Duration::from_millis(500));                                 // :3769
  }
  (false, format!("{LOCAL_MODEL_BRAND} 启动超时（PID {pid}，请检查日志 {}）", log_path.display()))  // :3774
  ```
- 上限值：`main.rs:3289-3311`（`fn normalized_local_chat_runtime_config`），钳位在 `main.rs:3309`
  ```rust
  startup_timeout_ms: startup_timeout_ms.clamp(1_000, 120_000),   // :3309
  ```
  默认值 `60_000`：`main.rs:6120-6122`（`fn default_local_chat_startup_timeout_ms`）与 `main.rs:6092-6093`（`local_chat_startup_timeout_ms` 字段，config 绑定见 `main.rs:6451`）。
- 轮询粒度 500ms（`main.rs:3769`）→ 最坏多等半个轮询周期。

**小结**：切换总耗时的**已界部分是"加载→就绪" = [1s, 120s]（默认 60s）**；**"卸载"无界**（`taskkill` 阻塞无超时）；**"排空"不存在**。由于切换发生在**发起模型请求之前**，它目前的耗时**不落在任何模型请求预算内**，但会**吃掉用户感知的整段时间**——这正是 §6.6 准入门禁需要覆盖的一环。

### 1.8 取消收尾的释放宽限

**已确认源码**：`modules/computer-use/packages/computer-use-core/src/input_stroke.rs:151-176`（`fn run_helper` 的等待循环）
```rust
let started = Instant::now();
let mut cancellation_at = None;
...
    if cancellation_at.is_none() && (cancelled() || started.elapsed() >= timeout) {
        // 独立哨兵文件只承载取消信号，不接收任何用户代码或命令。
        let _ = std::fs::write(&cancel_file, b"cancel");
        cancellation_at = Some(Instant::now());
    }
    if cancellation_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(2)) {
        let _ = child.kill();
        forced_kill = true;
        break child.wait().map_err(|error| error.to_string());
    }
    std::thread::sleep(Duration::from_millis(15));
```

- **释放宽限 = 固定 2 秒**（`input_stroke.rs:170`，`Duration::from_secs(2)`），写入取消哨兵文件后最多再等 2s 才强杀。
- **这 2 秒不被 `timeout` 或任何 remaining 收紧**：`timeout` 只触发"写取消哨兵"，宽限是**在这之上**追加的固定量。
- 相关固定值：
  - `input_stroke.rs:249-251`：`emergency_release()` 用 `Duration::from_secs(6)` 且 `cancelled = &|| false`（永不取消）——即"补发释放"本身可再耗 6s。
  - `input_stroke.rs:70`：`stroke` 路径的 `timeout` 由调用方给（桌面板 `computer_use_desktop_bridge.rs:278` 给 `clamp_stage_timeout(remaining, Duration::from_secs(10))`）。
- **代价（对第 2 节很关键）**：一次输入调用的**实际最坏 wall clock = clamp 后的 timeout + 2s 宽限（+ 可能的 6s 补发）**。当 `remaining` 已经很小（甚至 0）时，`clamp_stage_timeout` 会把 timeout 压到 0，但 **2s 宽限照付**，所以**仍然会超过剩余的零预算**。

### 1.9 1.x 汇总表（实际生效的上限）

| 层级 | 数值 | 位置 | 是否受上层剩余约束 |
| --- | --- | --- | --- |
| 外层用户运行根 deadline | **不存在** | `run_contract.rs:201-216` 未接线；`main.rs:185-199` 无时间型终态 | — |
| 单次模型请求硬上限 | **600s** | `openai_compat.rs:206`、`claw_provider.rs:127` | 否 |
| 工具执行配置钳位（工单线索的那条） | **[100ms, 600s]**，默认 30s | `main.rs:6651`、`main.rs:6682`、`main.rs:6563` | 否（入参是配置值） |
| 接力群发超时 | [5s, 600s]，默认 120s | `main.rs:8965`、`main.rs:16375` | 否 |
| CU 任务预算 | **120s**（[5s,300s] 可配） | `contracts.rs:221`、`main.rs:5620`、`computer_use_store.rs:818` | **否**（无父 deadline） |
| CU 规划/验收内部请求 | **20s** | `computer_use_planner.rs:12`、`:493` | **否**（planner 无 remaining） |
| CU 适配器截图 | **10s**，`min(remaining, 10s)` | `computer_use_desktop_bridge.rs:432`、`:440` | 是 |
| CU 输入（点击/输入/滚动/按键） | **8s**，`min(remaining, 8s)` | `computer_use_desktop_bridge.rs:14`、`:189/192/211/212/231/232/246` | 是（但 `remaining=0` 时仍付 2s 宽限） |
| CU 拖拽笔画 | **10s**，`min(remaining, 10s)` | `computer_use_desktop_bridge.rs:278` | 是 |
| 浏览器桥往返 | **10s**，`min(remaining, 10s)` | `browser_bridge.rs:27`、`:107` | 是（**tab 动作例外，见 :595**） |
| 取消收尾释放宽限 | **固定 2s** | `input_stroke.rs:170` | **否** |
| 补发释放 | 固定 6s | `input_stroke.rs:251` | **否** |
| 本地模型加载→就绪 | **[1s, 120s]**，默认 60s | `main.rs:3309`、`main.rs:3754`、`main.rs:6120` | 否 |
| 本地模型卸载 | **无界**（`taskkill` 阻塞无超时） | `main.rs:3522+` | 否 |
| 本地模型排空 | **未实装** | — | — |
| 附件图描述 / vision-service / locate | 120s / 180s / 15s | `multimodal_input.rs:93`、`vision-service/lib.rs:22`、`locate.rs:122` | 否 |

---

## 2. `clamp(100, …)` 之类的"下限回扩"问题（本节最重要）

### 2.1 直接结论

**当前 CU 桥接层的 `clamp_stage_timeout` 不是回扩，而是 `min`；7 处输入调用 + 截图 + 拖拽都正确地取 min。** 
**但同一批调用点存在一个更隐蔽的问题：`remaining` 小于阶段最小值时，代码既不拒绝、也不回扩，而是"用退化超时照发"，随后付出不被 remaining 约束的固定 2 秒取消宽限。**

即：产品裁决禁止的"回扩"在当前现场**没有出现**；产品裁决要求的"直接拒绝"在当前现场**也没有实现**。当前行为是**第三种**：照发 + 固定宽限超支。这不是"没关系"，而是"用错的方式没出事、该有的守卫却缺失"。

### 2.2 回扩函数的定义（已确认源码）

`modules/gui-web/packages/web-console/src/computer_use_adapters.rs:156-165`
```rust
/// 子请求上限不得超过整体 deadline 的剩余预算（§2.3）。
///
/// 各阶段的固定上限只表示"最多愿意等多久"；真正可用的是本 run 剩余的 deadline。
/// 两者取小，避免 3 秒剩余时仍发起一个 8 秒上限的输入或 10 秒上限的桥往返。
pub(crate) fn clamp_stage_timeout(
    remaining: std::time::Duration,
    cap: std::time::Duration,
) -> std::time::Duration {
    remaining.min(cap)
}
```

→ 函数名含 `clamp`，**语义是 `min(remaining, cap)`，无下限**。`remaining = 0` 返回 `0`，不会回扩。

已有的单测**正向固定了这一点**（不是我推断的）：

`modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs:982-992`
```rust
assert_eq!(clamp_stage_timeout(Duration::from_secs(30), cap), cap);
assert_eq!(clamp_stage_timeout(Duration::from_secs(3), cap), /* 3s */);
assert_eq!(clamp_stage_timeout(Duration::ZERO, cap), Duration::ZERO);
```

### 2.3 逐个调用点：`remaining` 小于阶段最小值时的实际行为

**CU 桌面板（`computer_use_desktop_bridge.rs`）的 7 处输入调用** —— 全部为 `min`，**均无"拒绝"守卫**：

| # | 调用点（file:line） | 动作 | 阶段上限 | `remaining` 小于上限时的实际行为 | 会不会回扩 |
| --- | --- | --- | --- | --- | --- |
| 1 | `computer_use_desktop_bridge.rs:189` | `input::click_point(x, y, 1, ...)`（Click） | `INPUT_TIMEOUT = 8s`（`:14`） | 用 `min(remaining, 8s)` 发起；**不拒绝** | **否** |
| 2 | `computer_use_desktop_bridge.rs:192` | `input::click_point(x, y, 2, ...)`（DoubleClick） | 8s | 同上 | **否** |
| 3 | `computer_use_desktop_bridge.rs:211` | `input::click_point(x, y, 1, ...)`（TextInput 先聚焦） | 8s | 同上 | **否** |
| 4 | `computer_use_desktop_bridge.rs:212` | `input::type_text(text, ...)` | 8s | 同上 | **否** |
| 5 | `computer_use_desktop_bridge.rs:231` | `input::click_point(x, y, 1, ...)`（Scroll 先聚焦） | 8s | 同上 | **否** |
| 6 | `computer_use_desktop_bridge.rs:232` | `input::scroll_wheel(sign*amount*120, ...)` | 8s | 同上 | **否** |
| 7 | `computer_use_desktop_bridge.rs:246` | `input::send_virtual_key_combo(&virtual_keys, ...)` | 8s | 同上 | **否** |

**另两处（同文件，非"输入"但同属阶段上限）**：

| # | 调用点（file:line） | 动作 | 阶段上限 | 实际行为 | 会不会回扩 |
| --- | --- | --- | --- | --- | --- |
| 8 | `computer_use_desktop_bridge.rs:278` | `input::controlled_drag_path(..., clamp_stage_timeout(remaining, Duration::from_secs(10)), &*self.cancelled, ...)` | 10s | `min`；不拒绝 | **否** |
| 9 | `computer_use_desktop_bridge.rs:440`（经 `:94` 调用） | `input::capture_window_image(identity, clamp_stage_timeout(remaining, CAPTURE_TIMEOUT))` | `CAPTURE_TIMEOUT = 10s`（`:432`） | `min`；不拒绝 | **否** |

**浏览器桥（`browser_bridge.rs`）的 2 处 `BRIDGE_TIMEOUT`**：

| # | 调用点（file:line） | 动作 | 阶段上限 | 实际行为 | 会不会回扩 |
| --- | --- | --- | --- | --- | --- |
| 10 | `browser_bridge.rs:107`（`clamp_stage_timeout(remaining, BRIDGE_TIMEOUT)`，经 `:108` `wait_for_bridge_response`） | 桥往返等待 | `BRIDGE_TIMEOUT = 10s`（`:27`） | `min`；不拒绝；超时报 `browser_bridge_timeout` 且文案写明 `remaining budget capped` | **否** |
| 11 | `browser_bridge.rs:595`（`broker().request(BRIDGE_TIMEOUT, ...)`） | **tab 生命周期动作**（OpenTab/ActivateTab/CloseTab） | 10s | **完全没接 remaining**：用的是**裸 `BRIDGE_TIMEOUT`**，不受剩余预算约束 | **否（但更糟：绕过预算）** |

**第 11 条是真缺口（已确认源码，非推断）**：

- `browser_bridge.rs:484-497` `fn execute(..., remaining: std::time::Duration)` 在开头就把 tab 类动作转交出去：
  ```rust
  if matches!(action.kind, ComputerUseActionKind::OpenTab | ... | ComputerUseActionKind::CloseTab) {
      return self.execute_tab_action(action);   // ← remaining 在此被丢弃
  }
  ```
- 对比同一函数的非 tab 分支 `browser_bridge.rs:513-519`：`broker().request(remaining, ...)`，**正确使用 remaining**。
- 所以 `execute_tab_action`（`browser_bridge.rs:560` 起）拿不到 `remaining`，其 `:595` 只能写死 `BRIDGE_TIMEOUT`。

**结论表（工单要求的"逐个列出：调用点 / 剩余小于最小值时的实际行为（拒绝还是回扩）"）**：

- 11 个调用点中：**0 个回扩**（因为用的是 `min`，不是带下限的 `clamp`）；
- **0 个显式拒绝**（没有任何一处检查 `remaining < 最小可执行值` 后返回错误）；
- **11 个都是"退化超时照发"**；
- 其中 **1 个（`browser_bridge.rs:595`）连 min 都没做，直接绕过 remaining 用满 10s**。

### 2.4 "照发"的实际代价：剩余预算可被突破（已确认源码）

以 `remaining = 0` 为例（`clamp_stage_timeout` 返回 0）：

1. 输入调用进入 `input_stroke.rs:run_helper`；
2. `:165` `if cancellation_at.is_none() && (cancelled() || started.elapsed() >= timeout)` —— `timeout = 0`，条件立刻成立，写入取消哨兵文件；
3. `:170` `if cancellation_at.is_some_and(|at| at.elapsed() >= Duration::from_secs(2))` —— **仍要等满固定的 2 秒**才强杀；
4. 于是 `remaining = 0` 的一次输入调用，实际占用的 wall clock **≈ 2 秒**（外加进程 spawn/回收开销，`input_stroke.rs:123-138` 还有两条读线程 join）。

**即：`min` 保证了"不会等超过 remaining"，但 `run_helper` 的固定 2 秒宽限保证了"一定会超过 remaining"。** 这是"下限回扩"的等价后果（预算被突破一个固定量），只是机制不是 `clamp`。

### 2.5 对产品裁决的落地要求（设计，不实现）

1. **保留** `clamp_stage_timeout` 的 `min` 语义，并把名字改掉（如 `shrink_to_remaining`），以免后人误以为是带下限的 clamp。
2. 在**每个阶段发起前**加入**显式拒绝守卫**：`remaining < MIN_EXECUTABLE_STAGE`（建议每个阶段各自定义，如输入 ≥ 200ms、截图 ≥ 500ms、桥往返 ≥ 500ms，**具体数值属产品决定，本轮不给硬数字**）→ 返回 `ComputerUseError::blocked("insufficient_remaining_budget", ..., RetryOwner::None)`，**不得**扩大、不得照发。
3. **修 `browser_bridge.rs:595`**：把 `remaining` 传进 `execute_tab_action`（改签名 `fn execute_tab_action(&self, action: &ComputerUseAction, remaining: Duration)`），并在 `:595` 用 `clamp_stage_timeout(remaining, BRIDGE_TIMEOUT)`。
4. **修 `input_stroke.rs:170` 的宽限**：宽限应当是 `min(GRACE, remaining_after_timeout)`（或在 remaining 不足时把宽限按 remaining 缩短），否则"拒绝守卫"的意义会被固定 2 秒抵消。**注意**：这涉及 `computer-use-core` 的公开行为，属跨 crate 改动，需按 RPR 流程评估。
5. **明文禁止**：任何 `remaining` **不得**流入 `main.rs:6651` / `main.rs:6682` 这类带下限的 `clamp`。

---

## 3. `effective_call_budget` 的落点设计（只设计，不实现）

### 3.1 定义

```
effective_call_budget =
    min(
        本次配置上限,            // 该调用类型的 config 上限
        本调用实际经过的硬上限,   // 沿调用栈所有外层 timeout 的最小值
        父运行剩余时间,          // 父 run 的单调 deadline 余量
        CU 剩余时间,            // RunBudgetGuard::remaining_ms
        当前阶段上限             // 阶段常量（输入 8s / 截图 10s / 桥 10s / 规划 20s …）
    )
```

四个"较小者"中，最后一类（当前阶段上限）与"本调用实际经过的硬上限"**已经存在且可用**；前两类（父运行剩余时间、CU 剩余时间）**只有 CU 那一半可用，父运行那一半不存在**。

### 3.2 映射到现有符号

| 公式分量 | 现有符号（真实名称） | 现状 |
| --- | --- | --- |
| 本次配置上限 | `ToolExecutionPolicy::default_timeout_ms`（`main.rs:6558`，由 `tool_execution_policy()` `main.rs:6645+` 产出）；CU 侧为 `ComputerUseBudgets::timeout_ms`（`contracts.rs:210`） | **已有** |
| 本调用实际经过的硬上限 | provider 的 600s（`openai_compat.rs:206` / `claw_provider.rs:127`）；CU 内部请求的外层 20s（`computer_use_planner.rs:12`）；附件描述 120s（`multimodal_input.rs:93`） | **已有，但分散在 3 个 crate、互不知情** |
| 父运行剩余时间 | **无对应符号**（`RunBudget.deadline_unix_ms` 已定义但未接线，`run_contract.rs:204-209`） | **缺失 → 必须先接线** |
| CU 剩余时间 | `RunBudgetGuard::remaining_ms(now_ms)`（`supervisor.rs:203-207`）；`RunBudgetGuard::new(budgets, started_at_ms)`（`supervisor.rs:186`）；controller 内 `let remaining = Duration::from_millis(guard.remaining_ms(self.clock.now_ms()))`（`controller.rs:386/458/623/641/693/712`） | **已有** |
| 当前阶段上限 | `clamp_stage_timeout(remaining, cap)` 的 `cap`：`INPUT_TIMEOUT`（`computer_use_desktop_bridge.rs:14`）、`CAPTURE_TIMEOUT`（`:432`）、拖拽 10s（`:278`）、`BRIDGE_TIMEOUT`（`browser_bridge.rs:27`）、`PLANNER_TIMEOUT`（`computer_use_planner.rs:12`） | **已有** |

### 3.3 唯一计算位置

**建议唯一落点：`ComputerUseAdapter` 的每个 `observe` / `act` / `verify` 实现入口，以及 `ComputerUsePlanner` 的 `request_model`。**

理由（已确认源码支撑）：

1. `ComputerUseAdapter` 的 `observe/act/verify` **已经是** `remaining` 的唯一收口（`controller.rs:40-51` 的 trait 定义 + controller 6 个调用点的 re-read）。在这里算 `effective_call_budget` 不需要新增数据流。
2. `ComputerUsePlanner` **目前完全没有** `remaining`（`controller.rs:11-35`），而它才是真正发模型请求的地方。因此"唯一位置"需要先给 planner trait 加 `remaining`（等价于本轮给 adapter 加的那一步），再把 `effective_call_budget` 收口到 `CurrentSessionComputerUsePlanner::request_model`（`computer_use_planner.rs:476-508`）——**`plan`/`verify_visual`/视觉描述三条路径都必经此函数**（`:546`、`:554`、`:583`）。

**建议形态（伪代码，不实现）**：
```rust
// 唯一计算点（planner 侧，与 adapter 侧同名同义）
fn effective_call_budget(
    config_cap: Duration,      // 本次配置上限
    outer_hard_cap: Duration,  // 本调用实际经过的硬上限
    parent_remaining: Option<Duration>,  // 父运行剩余（接上 RunBudget 后为 Some）
    cu_remaining: Option<Duration>,      // RunBudgetGuard::remaining_ms
    stage_cap: Duration,       // 当前阶段上限
) -> Duration;                 // 返回 min(...)；调用方必须先判断是否 < MIN_EXECUTABLE
```

### 3.4 需要改成读 `effective_call_budget` 的调用方清单

**必须改（当前用的是固定常量或裸值，不受任何剩余约束）**：

| 调用方 | 位置 | 现状 |
| --- | --- | --- |
| `CurrentSessionComputerUsePlanner::request_model` | `computer_use_planner.rs:476-508`（`timeout(PLANNER_TIMEOUT, ...)` 在 `:493`） | 固定 20s，无 remaining |
| `plan` → 视觉描述请求 | `computer_use_planner.rs:546-547` | 经 `request_model`，固定 20s |
| `plan` → 规划请求 | `computer_use_planner.rs:554` | 经 `request_model`，固定 20s |
| `verify_visual` → 验收请求 | `computer_use_planner.rs:583-584` | 经 `request_model`，固定 20s |
| 工具轮模型调用 | `main.rs:17814`（`await_chat_turn`，`main.rs:540-551`） | **无任何超时** |
| 聊天主调用 | `main.rs:26734` | **无** |
| 流式主聊天 | `main.rs:30892-30894`（`.stream_message`） | **无** |
| 单图理解 | `main.rs:26420` | **无** |
| 恢复重答 | `main.rs:33835` | **无** |
| 光标位置视觉校验 | `main.rs:28839`（`verify_cursor_on_target`，`main.rs:28754`） | **无** |
| 附件图片描述 | `multimodal_input.rs:92-94` | 固定 120s |
| 浏览器 tab 动作 | `browser_bridge.rs:496` + `:595` | 裸 `BRIDGE_TIMEOUT`，绕过 remaining |
| `run_helper` 取消宽限 | `input_stroke.rs:170` | 固定 2s，不受 remaining |

**需要新增入参（数据流改造）**：

| 符号 | 位置 | 需要的改动 |
| --- | --- | --- |
| `ComputerUsePlanner::classify / next_action / verify` | `controller.rs:11-35` | 加 `remaining: std::time::Duration` |
| `controller.rs:405 / 502-506 / 716-721` | controller 里对 planner 的三处调用 | 传入当场 re-read 的 `remaining` |
| `execute_with_current_runtime` | `computer_use_executor.rs:1092-1096` | 加父运行 deadline/remaining 入参 |
| `ComputerUseExecutor::new` / `execute_in_room_with_policy` | `computer_use_executor.rs:433-446` / `:488-494` | 同上，透传 |
| 调用点 `main.rs:33709-33711` | CU 工具分发 | 传入父 run 的 remaining |
| `ConfigComputerUse::budgets()` | `main.rs:5610-5621` | **不要**改成算 remaining；它是"配置上限"分量，应保持纯配置语义 |

**关键设计约束（回应产品裁决）**：`effective_call_budget` 的返回值**必须**由一个独立的 `MIN_EXECUTABLE_*` 常量判定拒绝，**绝不能**被任何 `clamp(lower, upper)` 处理。建议把该拒绝逻辑与 `effective_call_budget` 放在**同一个函数**里返回 `Result<Duration, ComputerUseError>`，从类型上杜绝"拿到一个被回扩过的值"。

---

## 4. 12 个动作与 1+2n 请求的预算含义

### 4.1 n 个动作的请求数（已确认源码逐条核对）

CU run 的结构（`controller.rs`）：

1. 起始观测：`adapter.observe` —— `controller.rs:386-387`（**1 次适配器观测，不是模型请求**）
2. **初始验收**：`adapter.verify` + `planner.verify` —— `controller.rs:458-468`（**0 或 1 次模型请求**）
3. 进入 `loop`（`controller.rs:500`）：
   - **规划**：`planner.next_action` —— `controller.rs:502-506`
   - 执行：`guard.before_action`（`:608`）+ `adapter.act`（`:624`）
   - 执行后观测：`adapter.observe`（`:693-694`）
   - **验收**：`adapter.verify` + `planner.verify`（`:712-723`）
   - `guard.after_verification`（`:740`）

**每次"验收"的模型请求数**（`computer_use_planner.rs:564-589`）：
```rust
async fn verify_visual(&self, request, before, after, original) -> ... {
    if after.surface != ComputerUseSurface::Desktop { return Ok(original); }   // ← 0 次模型请求
    ...
    let (response, started_at) = self.request_model(&vision, &prompt, &images, ...).await?;  // ← 1 次
```
→ **Desktop 表面：每次验收 1 次模型请求；Browser 表面：0 次。**

**每次"规划"的模型请求数**（`computer_use_planner.rs:524-562`）：
```rust
let mut images = observation_image(observation).into_iter().collect::<Vec<_>>();
if observation.surface == ComputerUseSurface::Desktop && images.is_empty() { return Err(...); }
if !images.is_empty() && !crate::multimodal_input::supports_images(&agent, ...) {
    let vision = crate::multimodal_input::configured_vision_agent()...;
    let (response, started_at) = self.request_model(&vision, &vision_prompt, &images, ..., "computer_use_visual_description").await?;  // ← 额外 1 次
    ...
    images.clear();
}
let (response, started_at) = self.request_model(&agent, &prompt.to_string(), &images, PLANNER_SYSTEM_PROMPT, "computer_use_planning").await?;  // ← 1 次
```
→ **规划者模型支持图片：每次规划 1 次；规划者模型是纯文本（Desktop 场景，本地 llama.cpp 典型）：每次规划 2 次。**

**`classify`**（`computer_use_planner.rs:631-658`）：纯本地分支判断，**0 次模型请求**。

### 4.2 两种计数

| 场景 | 初始 | 每步规划 | 每步验收 | 合计（n 个动作） | n=12 |
| --- | --- | --- | --- | --- | --- |
| Desktop + **多模态** planner（工单的 `1+2n`） | 1 | 1 | 1 | **1 + 2n** | **25** |
| Desktop + **纯文本** planner（本地模型典型） | 1 | **2** | 1 | **1 + 3n** | **37** |
| Browser | 0 | 1 | 0 | **n** | **12** |

**工单的 `1+2n = 25` 成立于"Desktop + 多模态 planner"；本地文本模型的真实计数是 `1+3n`。**
（另需叠加第 1.3 节那条 `main.rs:28839` 光标校验视觉请求——它每次调用都不进这个计数，属**未计入的额外请求**。）

### 4.3 为什么"每次调用不超 600 秒且动作不超 12 即算匹配"是错的

按工单给的推理链复算（**这些是算式，不是我本轮实测**）：

- 12 个动作 ↔ 25 次内部模型请求（Desktop + 多模态 planner，§4.2）。
- 若每次内部请求都撞当前的 20 秒上限（`computer_use_planner.rs:12`）：`25 × 20s = 500s`。
- 500s **已经**接近 600s，且：
  - 换成文本 planner 的 `1+3n`：`37 × 20s = 740s` → **直接超过 600s**；
  - 每次请求还要叠加适配器观测/输入/截图（各 8~10s 上限，`computer_use_desktop_bridge.rs:14/432/278`）与 2s 取消宽限（`input_stroke.rs:170`）。
- 而 CU 的**任务预算只有 120s**（`contracts.rs:221`）。以 20s/请求算，120s 只够 **6 次**内部请求——**连 3 个动作（Desktop 多模态）都跑不完**。

因此"单次不超 600s + 动作不超 12"这个准入条件**内部自相矛盾**：它同时允许了 25~37 次内部请求、每次 20s、CU 总预算却只有 120s，三条上限无法共存。**"动作数 ≤ 12"和"单次调用 ≤ 600s"是两个各自成立的约束，但它们相乘之后的量级从未被任何一个预算覆盖。**

### 4.4 应当写成什么形式的准入语句（设计）

准入必须写成**时间预算与请求数联合式**，而不是两条独立上限。建议形态：

> **准入判据（必须同时成立）**
> 1. **预算充分性**：`n_max · (t_plan + t_verify) + t_initial + t_adapter(n_max) ≤ CU 任务预算`（`ComputerUseBudgets::timeout_ms`），其中 `t_plan`/`t_verify` 取**实测的 P95**，不是常量上限；
> 2. **单请求可行**：每次内部请求的 `effective_call_budget`（第 3 节）**≥ 该阶段的 `MIN_EXECUTABLE_*`**，否则**拒绝**（不得回扩、不得照发）；
> 3. **硬上限一致性**：`CU 任务预算 ≤ 父运行剩余时间`；若父运行无 deadline，则**必须先接线 `RunBudget.deadline_unix_ms`**（`run_contract.rs:204-209`），否则本条件不可判定；
> 4. **动作数换算**：把"动作数上限"改为**由预算反推**——`n_max = floor((CU 预算 − t_initial − t_adapter) / (t_plan + t_verify))`，而不是把 12 当独立常量（`contracts.rs:217` 的 `max_actions: 12` 应降级为**安全上限**，最终受限者是预算）；
> 5. **请求数显式声明**：准入语句里必须写明**按哪种 planner 计数**（`1+2n` 还是 `1+3n`），因为本地文本模型走 `1+3n`；
> 6. **排队/加载前置**：若本次调用会触发本地模型切换，`t_switch`（加载上限默认 60s，`main.rs:6120`）**必须计入**，或明确要求先完成切换再准入。

**一句话版本**：准入不是"每次都别超 600 秒、动作别超 12"，而是"**本次 CU 运行在扣除初始验收、适配器开销与实测 P95 规划/验收耗时后，仍能容纳承诺的动作数，且每个子请求都能拿到不小于阶段最小可执行值的有效预算；任一条不满足即拒绝运行**"。

---

## 5. 上下文容量与时间双检查

### 5.1 容量式 `P + O + M ≤ C`

**已确认源码**（公式在源码里就是这么实现的）：

- `main.rs:32507-32519`（稳定锚点：`fn output_reserve_tokens`）
  ```rust
  fn output_reserve_tokens(context_window: u32, model_max_output: u32) -> u32 {
      let context_window = context_window.max(1);
      let reserve_by_pct =
          context_window.saturating_mul(context_lifecycle_policy().output_reserve_percent) / 100;
      model_max_output.max(reserve_by_pct).min(context_window / 2).max(1)
  }
  ```
- `main.rs:27977-28008`（稳定锚点：`fn context_build_options_for_agent_with_floor_and_room`；预算体在 `:27988-28005`）
  ```rust
  let (model_context_raw, max_output) = effective_model_limit_for_agent(agent);   // :27988
  let model_context = model_context_raw.max(1);                                  // :27989
  if model_context > policy.model_budget_min_tokens {                            // :27990
      let output_reserve = output_reserve_tokens(model_context, max_output);      // :27994
      let usable_context = model_context                                        // :27995-27998
          .saturating_sub(output_reserve)
          .saturating_sub(policy.prompt_safety_tokens)
          .max(1);
      options.max_prompt_tokens = usable_context;                                // :27999
      options.image_token_estimate = policy.image_token_estimate;                // :28000
      options.history_token_budget = usable_context.saturating_mul(policy.history_budget_percent) / 100;   // :28001-28002
      options.memory_token_budget  = usable_context.saturating_mul(policy.memory_budget_percent)  / 100;   // :28003-28004
  }                                                                              // :28005
  ```
- 各分量默认值（`main.rs:5476-5494`）：
  | 常量 | 函数 | 值 |
  | --- | --- | --- |
  | `output_reserve_percent` | `default_context_output_reserve_percent`（`:5483`） | **15** |
  | `history_budget_percent` | `default_context_history_budget_percent`（`:5476`） | **70** |
  | `memory_budget_percent` | `default_context_memory_budget_percent`（`:5480`） | **8** |
  | `prompt_safety_tokens` | `default_context_prompt_safety_tokens`（`:5488`） | **1024** |
  | `image_token_estimate` | `default_context_image_token_estimate` | **512** |
  | `model_budget_min_tokens` | `default_context_model_budget_min_tokens` | **8000** |
  | 历史/记忆/输出预留/安全余量的钳位 | `main.rs`（`context_lifecycle_policy` 构造处） | `history 1..95`、`output_reserve 0..50`、`prompt_safety 0..16384`、`image 64..8192` |

**`P / O / M / C` 对应变量（已核实）**：

| 符号 | 含义 | 现有变量 |
| --- | --- | --- |
| **C** | 模型上下文窗口 | `model_context`（`main.rs:27989`，来自 `effective_model_limit_for_agent(agent).0`，见 `:27988`）；判定门 `model_context > policy.model_budget_min_tokens`（`:27990`，默认 8000） |
| **O** | 输出预留 | `output_reserve = output_reserve_tokens(model_context, max_output)`（`main.rs:27994`），即 **`max(max_output, C×15%)` 再 `min(C/2)`**（`main.rs:32507-32519`，`:32513-32516`）；请求体 `max_tokens` 的上界是 `request_max_tokens_for_limit`（`main.rs:32525-32531`） |
| **M** | 本轮 prompt/历史+记忆可占用的上限 | `options.max_prompt_tokens = usable_context`（`main.rs:27999`），其中 `usable_context = C − O − prompt_safety_tokens`（`:27995-27998`）；细分预算为 `history_token_budget = usable×70%`（`:28001-28002`）、`memory_token_budget = usable×8%`（`:28003-28004`） |
| **P** | 实际组装出的 prompt token 数 | 由 `build_context_assembly_with_roster` / `build_context_assembly` 产出；工具 schema 会挤占 prompt 空间，小窗口被裁剪：`SMALL_CONTEXT_TOOL_CUTOFF_TOKENS = 16_384`（`main.rs:32483`，`fn select_tools_for_request` 起于 `:32477`），≤16k 时只保留 12 个工具（`main.rs:32488-32496`） |
| **C 的即时校验（CU 专用）** | planner 必须能完整放下"当前观测 + 图片" | `computer_use_planner.rs:481-488`：`if actual_images != images || 最后一条消息不含 prompt → Err("planner context budget cannot preserve the complete current observation and images")` |

**所以容量式的现有实现就是**（`main.rs:27995-27998` + `:32513-32516`）：

```
P + O + M ≤ C
P ≤ usable_context = C − max(max_output, C×15%) − 1024        // M 的上限
M = usable_context（其中 history ≤ 70%·usable、memory ≤ 8%·usable）
O = max(max_output, C×15%)，且 ≤ C/2
```

**注意**：`O` 有 `min(C/2)` 上限（`main.rs:32516`）；`C ≤ 8000` 时整套预算**不生效**（`main.rs:27990` 的 `if` 直接跳过 → `ContextBuildOptions::default()`，见 `main.rs:27982` 与 `impl Default for ContextBuildOptions`（`main.rs:30500-30512`）：默认 `max_prompt_tokens: 8_000`、`history_token_budget: 3_000`、`memory_token_budget: 1_200`）。

**CU 侧额外的容量约束（已确认源码）**：
- `MAX_OBSERVATION_CHARS = 64 * 1024`（`computer_use_planner.rs:14`，`bounded_observation` 在 `:76`）
- `MAX_PLANNER_RESPONSE_BYTES = 8 * 1024`（`:13`）
- planner 请求 `max_tokens = 4096`（`:489`，再经 `agent_planner_message_request` 的 `min(agent_request_max_tokens(agent))`，`main.rs:32366-32382`）

### 5.2 时间式

```
t_queue_or_switch + t_prefill + t_generate + t_protocol_and_image_overhead + t_margin
    ≤ effective_call_budget
```

**每一项对应的现有符号 / 现状**：

| 分量 | 含义 | 现有对应 | 现状 |
| --- | --- | --- | --- |
| `t_queue_or_switch` | 排队 / 本地模型切换（排空+卸载+加载+就绪） | 加载→就绪 `startup_timeout_ms.clamp(1_000,120_000)`（`main.rs:3309`，默认 60s `main.rs:6120`）；卸载 `taskkill` **无界**（`main.rs:3522+`）；**排空未实装** | **不可完整估算**（卸载无界、排空缺失） |
| `t_prefill` | 预填充 | **源码中无 estimator**（对 `prefill` / `tokens_per_sec` / `prompt_tokens_per_second` 在 `web-console/src/`、`llm-adapter/src/` 无命中） | **未实装 → 门禁必须先用外部实测值** |
| `t_generate` | 生成 | 由 `max_tokens`（`main.rs:32525-32531`）与模型速度决定；源码无速度模型 | **未实装** |
| `t_protocol_and_image_overhead` | 协议与图像开销 | `image_token_estimate = 512`（容量侧有）；CU 观测图来自 `observation_image`（`computer_use_planner.rs:621`），截图 base64 编码 + `MAX_OBSERVATION_CHARS` 截断；`verify_visual` 最多带 **2 张图**（before+after，`:571-575`） | **容量侧有估计，时间侧无估计** |
| `t_margin` | 保守余量 | 容量侧对应 `prompt_safety_tokens = 1024`（`main.rs:5488`）；**时间侧无对应量** | **未实装** |
| `effective_call_budget` | 本次有效预算 | 第 3 节设计；当前不存在 | **待实现** |

### 5.3 历史实测证据（**来自历史报告，非本轮实测**）

来源：`docs/analysis/2026-09-21-integration-review/risk-decision-review.md:16`（原文）：

> V-5："关闭工具不能保证本地模型长任务稳定；不解决上下文溢出/生成超时/规划质量" — 与我在 bonsai 上的实测一致：真正的失败原因是 **64K 时 prompt ≈46k、prefill ≈90 tok/s、整轮 >600s 被切断并自动重试**，以及工具回路脱轨；与工具开关无因果

**按产品裁决要求，准确表述如下**（这段表述是本工单的既有裁决口径，非我的实测）：

- bonsai 本机、64K 上下文配置下，agent 实际组装的 prompt **约 46k token**；
- 单次调用中**预填充估算本身约 511 秒**（`46k / 90 tok/s ≈ 511s`）——**511 秒尚未超过 600 秒**；
- 被报告的失败是"**整次调用**超过 600 秒被截断并自动重试"，**不是预填充单独超限**；
- **`90 token/s` 是这台机器上该次预填充的观测值，不能当作生成速度，也不能外推到其它模型 / 图像请求 / 其它缓存状态**；
- 该数字**未包含**协议开销、图像编码、以及 CU 每步可能追加的额外请求（§4.2 的 `1+3n`、§1.3 的光标校验请求）。

**对本设计的意义**：`t_prefill` 在本地 64K + 46k prompt 的量级下已经吃掉 600s 硬上限的 **85%**，`effective_call_budget` 的 min 链在本地模型场景会**被 600s 硬上限或阶段上限（20s）完全支配**。因此 §6.6 门禁必须先取得**本机实测的 `t_prefill` / `t_generate`**，任何用"理论 tok/s"外推的做法都应被拒绝。

---

## 6. 真实请求上的待测清单（**只列命令，本轮不执行**）

**合规声明**：
- 本轮**未执行**以下任何一条。
- 标记 **【需用户授权】** 的条目**一律涉及真实模型调用**（本地或远端 LLM / 视觉接口），**必须由用户明确授权后**才可执行；未获授权前不得运行。
- 标记 **【只读】** 的条目不发起模型调用，仍建议在用户在场时执行以免干扰其在跑的任务。
- 每条测试都必须**先记录机器状态**（模型/上下文配置/是否有并发任务），因为 §5.3 的 90 tok/s **不可外推**。

**关于 §6.6**：工单要求"按产品裁决 §6.6 的六项准入门禁"，但**我未能在仓库内定位到 §6.6 的原文**（对 `docs/`、`tmp/` 全树 grep `6.6` / `准入门禁` / `六项` 均无匹配到门禁清单；`01-…决策.md:220` 的 `D6.6` 是"token 用量与计费"，与门禁无关）。因此下表按**本工单声明的六个技术面向**组织，并**逐条标注对应裁决要点**；**§6.6 的确切六项措辞属未验证**，实施前需与产品确认是否与下表一一对应。

### 门禁 1：单次调用的硬上限与下限拒绝（对应第 1.3 / 2.x 节）

| 项 | 内容 |
| --- | --- |
| 要测什么 | (a) 单次内部请求是否在 600s 处被 reqwest 截断；(b) `remaining < MIN_EXECUTABLE_STAGE` 时是否**拒绝**而非回扩/照发；(c) `remaining=0` 的一次输入调用实际 wall clock 是否为 ~2s（`input_stroke.rs:170`） |
| 怎么测 | (a) 用**远期代理/超长思考**场景观察 `openai_compat.rs:206` 的 600s 截断与自动重试；(b) 构造 `remaining` 极小值（需先在 §3.4 的接线完成后才有可观测入口）；(c) 直接对 `clamp_stage_timeout` + `run_helper` 做**离线单测**（不需要真实模型） |
| 前置条件 | (a) 可复现 600s 的模型与配置；(b) **需先完成第 3 节接线**；(c) 无 |
| 授权 | (a) **【需用户授权】**（真实模型调用）；(b) **【需用户授权】**（会触发真实桌面输入）；(c) **【只读】/纯离线**，可先做 |

### 门禁 2：CU 子运行预算与父运行剩余的一致性（对应第 1.5 节）

| 项 | 内容 |
| --- | --- |
| 要测什么 | 父 run 剩余 30s 时，CU 是否仍拿满 120s（当前预期**是**，因 `execute_with_current_runtime` 无 deadline 入参） |
| 怎么测 | 在一次长聊轮接近尾声时发起 `computer_use_perform`，记录 `guard.remaining_ms` 起点与父 run 已耗时；对照 `computer_use_store` 的 run 记录 |
| 前置条件 | 需要能在同一轮里观测父 run 时间与 CU run 记录（sqlite: `.coolzhu/web-sessions.sqlite3`） |
| 授权 | **【需用户授权】**（CU 会真实操作桌面） |

### 门禁 3：请求数换算 `1+2n` vs `1+3n`（对应第 4 节）

| 项 | 内容 |
| --- | --- |
| 要测什么 | 一次 12 动作 Desktop run 的实际内部请求数：是 25 还是 37 |
| 怎么测 | 读 `chat_insights` / `computer_use_store` 的 planner 诊断记录（`computer_use_planner.rs:510-522` `fn diagnostic`，按 `request_kind` = `computer_use_planning` / `computer_use_verification` / `computer_use_visual_description` 分类计数，见 `:549/560/587`）——**这是离线读库，不需要新发请求** |
| 前置条件 | 需要一份**已存在**的 12 动作 CU run 记录；若无，需用户授权跑一次 |
| 授权 | 读已有记录 **【只读】**；若无记录则 **【需用户授权】** |

### 门禁 4：上下文容量 `P + O + M ≤ C`（对应第 5.1 节）

| 项 | 内容 |
| --- | --- |
| 要测什么 | `P`（实际组装 prompt token）是否 ≤ `usable_context`；46k/64K 场景下 `usable_context = 65536 − max(max_output, 9830) − 1024` 的空间是否足够容纳观测图 + 工具 schema |
| 怎么测 | 纯计算 + 读 `assembly`：`context_build_options_for_agent` 返回值（`max_prompt_tokens` / `history_token_budget` / `memory_token_budget`）与 `build_context_assembly_with_roster` 的 token 估算对比；`computer_use_planner.rs:481-488` 的完整性校验会直接报错，可作为探针 |
| 前置条件 | 需要能读到 context options（可加临时日志，或经现有 API；**本轮不改源码**） |
| 授权 | **【只读】**（纯本地计算/读配置，无模型调用） |

### 门禁 5：时间式各分量（对应第 5.2 节）

| 项 | 内容 |
| --- | --- |
| 要测什么 | 本机 `t_prefill`、`t_generate`、`t_queue_or_switch`（含卸载与就绪）各自实测值；验证 §5.3 的 511s 预填充在**当前配置**下是否仍成立 |
| 怎么测 | (a) 从本地模型服务日志取 prefill 耗时与 token 数（`local_model_log_path()`，`main.rs:3733/3750`）；(b) 计次：切 chat → 只发一条固定 46k prompt 的请求，记录首 token 时间；(c) 量切换：`switch_local_models("vision")` 前后打点，覆盖 `taskkill` 与 `local_gemma_ready` 轮询 |
| 前置条件 | 本机 llama.cpp 可用、模型文件已在配置里；**必须记录机器/上下文配置**，因为 90 tok/s 不可外推 |
| 授权 | **【需用户授权】**（真实模型调用 + 会切换/吞掉显存） |

### 门禁 6：准入拒绝路径与收尾（对应第 2.5 / 4.4 节）

| 项 | 内容 |
| --- | --- |
| 要测什么 | (a) 预算不足时是否返回可读的拒绝（而非 600s 截断 / 静默照发）；(b) 取消后释放宽限是否 ≤ 2s，补发释放是否 ≤ 6s（`input_stroke.rs:170/251`）；(c) `browser_bridge.rs:595` 的 tab 动作是否真的绕过 remaining |
| 怎么测 | (a) 需先实现 §2.5 的拒绝守卫，再构造不足预算场景；(b) 触发一次取消（Escape / 中断），量测 helper 进程退出时刻；(c) 代码走查 + 一次 tab 动作的桥往返计时 |
| 前置条件 | (a) **依赖第 2/3 节实现，当前不可测**；(b) 需要真实桌面输入 |
| 授权 | (a) 当前不可测；**(b) 【需用户授权】**；(c) 走查 **【只读】**，计时 **【需用户授权】** |

---

## 7. 历史材料数字与当前源码不符之处（汇总）

| # | 历史说法 | 当前源码事实 | 位置 |
| --- | --- | --- | --- |
| 1 | "20 秒规划/视觉请求上限"（把 20s 当作规划与视觉的统一上限） | 规划/验收确实是 20s；但**附件图描述是 120s**、**vision-service 默认 180s**、**locate 默认 15s**、**光标校验无上限（600s）** | `computer_use_planner.rs:12`；`multimodal_input.rs:92-94`；`vision-service/src/lib.rs:22`；`locate.rs:122`；`main.rs:28839` |
| 2 | 线索指向"某处 `clamp(100, 600_000)` 是 600 秒**单次模型请求**上限" | 该 `clamp` 是**工具执行超时**（配置值钳位），**不是**模型请求上限；模型请求的 600s 硬上限来自 reqwest `.timeout(600s)` | `main.rs:6651`、`main.rs:6682` vs `openai_compat.rs:206`、`claw_provider.rs:127` |
| 3 | "12 动作 = 25 次请求（`1+2n`）"作为通用计数 | 仅对 **Desktop + 多模态 planner** 成立；**本地纯文本 planner 是 `1+3n = 37`**；Browser 表面是 `n = 12`（验收 0 次模型请求） | `computer_use_planner.rs:539-554`、`:566` |
| 4 | 隐含"根 deadline 存在并可作父预算" | **根 deadline 未实装**：`RunBudget` 仅契约与单测，web-console 零引用；`ChatTurnStatus` 无时间型终态 | `run_contract.rs:201-216`、`run_contract.rs:1-4`、`main.rs:185-199` |
| 5 | 隐含"CU 预算由父 run 推导/收紧" | CU 预算**纯来自 config**（默认 120s，可配 5~300s），入口 `execute_with_current_runtime` 无 deadline 入参 | `main.rs:5610-5621`、`computer_use_executor.rs:1092-1096`、`:1144` |
| 6 | `main.rs:6473` / `main.rs:6504`（本轮早前记录的 `clamp` 行号） | 取证期间 `main.rs` 被并发修改，同内容现位于 **`main.rs:6651` / `main.rs:6682`** | 见 §0 指纹 |
| 7 | 线索称"`computer_use_adapters.rs` 的 `clamp_stage_timeout(remaining, cap)`"存在回扩问题 | 该函数是 `remaining.min(cap)`，**不存在回扩**；回扩风险在 `main.rs:6651` 那类**带下限的 clamp**（但入参是配置值，当前不构成回扩）；真缺口是**无拒绝守卫**与 **`input_stroke.rs:170` 的固定 2s 宽限** | `computer_use_adapters.rs:160-165`、`input_stroke.rs:170` |
| 8 | 线索称"桌面桥 7 处输入调用与 `CAPTURE_TIMEOUT`" | 7 处输入确认为 `:189/192/211/212/231/232/246`，`INPUT_TIMEOUT = 8s`（线索未给该数值）；另有 `CAPTURE_TIMEOUT = 10s` 与拖拽 10s，共 **9 处** `clamp_stage_timeout` | `computer_use_desktop_bridge.rs:14/189/192/211/212/231/232/246/278/432/440` |
| 9 | 历史"整轮 >600s 被切断"的归属 | 产品裁决口径已修正为"**整次调用**超限"，且**预填充估算约 511s 本身未超 600s**；90 tok/s **不是生成速度**、**不可外推** | `risk-decision-review.md:16` |

---

## 8. 未验证清单

以下内容**本轮未读到源码或无法判定**，一律不作为已确认结论：

1. **产品裁决 §6.6 的六项准入门禁原文** —— 仓库内未定位（`docs/`、`tmp/` 全树 grep 无匹配）。第 6 节的门禁是我按本工单声明的六个技术面向组织的，**与 §6.6 的一一对应关系未验证**。
2. **`RunBudget` 是否在 `modules/` 之外的入口（如 `src/`、`packages/`、`tests/`、非 Rust 侧）被引用或构造** —— 我 grep 了 `modules/` 全树与 `web-console/src/`、`packages/`，未查 `src/`、`tests/`、`scripts/`、`dist/` 及 JS/TS 侧。
3. **`effective_model_limit_for_agent` 的完整实现**（`C` 与 `max_output` 的最终来源、会话级覆盖逻辑）—— 只读到调用点与返回签名，未读实现体。
4. **`build_context_assembly_with_roster` / `build_context_assembly` 内部如何用 `max_prompt_tokens` 裁剪历史** —— 未读实现；因此 `P` 的**实际**裁剪算法（超预算时丢什么）未验证。
5. **`select_tools_for_request` 被裁剪后工具 schema 的 token 占用** —— 已知 `SMALL_CONTEXT_TOOL_CUTOFF_TOKENS = 16_384` 与保留的 12 个工具名，但每个 schema 的实际 token 数未验证。
6. **`observation_image`（`computer_use_planner.rs:621`）产生的图像 token 量** —— 未验证它与 `image_token_estimate = 512` 的关系；CU 桌面截图 base64 的 token 成本未验证。
7. **`await_chat_turn` 之外是否还有其它上游超时包住 `main.rs:17814` 的工具轮** —— 只读了 `await_chat_turn` 本身（`main.rs:540-551`），未穷举调用栈。
8. **`main.rs:28839` `verify_cursor_on_target` 的调用频次与是否在 CU 路径内** —— 已确认调用点 `main.rs:22418`、`23858`，但这两个调用点所属的具体流程（是否属于 legacy 输入路径、是否与 CU 控制器并发）未读透。
9. **`taskkill` 是否存在系统级/父进程级的隐含超时** —— 源码无超时；OS 行为未验证。
10. **本地模型"排空"是否在别处（如 host 层、sidecar、JS 侧）实现** —— 只在 `main.rs` 与 `computer-use-core` 范围内确认缺失，未查 `clawbot_*`、`dist/`、`installer/`。
11. **`vision-service` 的 `timeout_seconds` 在 web-console 侧是否被显式覆盖** —— 只确认默认 180s 与 `.max(5)`，未追调用方是否传入更小值。
12. **`clamp_stage_timeout` 在 `browser_bridge.rs` 之外的实现方是否有第二份**（AGENTS.md 第 5 条提醒各 crate 独立）—— 已确认 `computer_use_adapters.rs` 是唯一定义、`browser_bridge.rs:18` 与 `computer_use_desktop_bridge.rs:12` 是导入；但未检查是否存在功能等价的内联 `min`。
13. **§5.3 的 511 秒与 90 tok/s 在本轮机器上是否仍成立** —— **未实测**（本轮禁止真实模型调用）；该数字仅来自历史报告。

---

## 9. 阻塞 RPR-11c 实施的缺口（若按第 3 节的落点推进）

1. **`ComputerUsePlanner` trait 缺 `remaining`**（`controller.rs:11-35`）。`effective_call_budget` 的核心作用对象是 planner 的模型请求，但 planner 拿不到 CU 剩余预算。**改 trait 是跨 `computer-use-core` 的公开 API 变更**，需先确认是否走 `ComputerUseAdapter` 同样的"加参数"路径（本轮已给 adapter 加过，风险面相同但影响所有 planner 实现，包括 `controller.rs:853/861/915` 的测试实现）。
2. **父运行剩余时间无符号可依**（`RunBudget` 未接线，`run_contract.rs:201-216`；`execute_with_current_runtime` 无入参，`computer_use_executor.rs:1092`）。`effective_call_budget` 的 `min` 链**少一项**，且准入判据第 3 条（`CU 预算 ≤ 父剩余`）**不可判定**。→ 需要先决定：是把 `RunBudget` 接进 web-console 的聊天轮，还是显式声明"本项目不存在根 deadline，故该项恒为 None"。
3. **`input_stroke.rs:170` 的固定 2s 宽限不被 remaining 约束**。只要这一条不改，任何"剩余不足即拒绝"的守卫都会被这 2s 抵消（`remaining=0` 仍可超支 ~2s）。该文件属 `computer-use-core`，改动会影响测试实现（`input_stroke.rs:298` 附近的用例）。
4. **`browser_bridge.rs:595` 的 tab 动作绕过 remaining**（`:484-497` 丢弃 remaining 后调用 `:595` 的裸 `BRIDGE_TIMEOUT`）。**这是可以在不引入跨 crate 变更的情况下先修的一处**，建议作为 RPR-11c 实施的第一步。
5. **`t_prefill` / `t_generate` 在源码中完全无 estimator**（第 5.2 节）。时间式无法在运行期自检，只能靠外部实测常量。→ 需要产品决定：是引入一个（保守的）估算常量，还是把"时间准入"完全建立在门禁 5 的实测数据上。
6. **§6.6 六项门禁的原文未取得**（第 8 节第 1 条）。若 §6.6 的六项与第 6 节的结构不一致，本工单第 6 节需重做。
7. **`main.rs` 正被并发修改**（§0）。任何以行号锚定的改动都应在动手前重新对齐 md5 与稳定锚点，否则会撞上他人未提交的改动。

---

## 10. 第 6 项交付：G1–G6 的原文、编号与本轮三条解释

本节是**文档交付**。它把裁决第 6 项的三条解释与六项门禁的编号固定下来；
**本节本身不表示已修改仓库其它部分**，也不把任何一项门禁标记为"已通过"。

### 10.1 六项门禁的原文与编号

本工单收到的裁决文本给出了下列编号与名称（逐字照录，未改写）：

| 编号 | 名称（原文） |
| --- | --- |
| **G1** | 请求链清单 |
| **G2** | 实际 wire 体量 |
| **G3** | 小范围能力校准 |
| **G4** | 最低闭环可行性 |
| **G5** | 超预算行为 |
| **G6** | 原有输入安全门禁 |

**未验证声明（已由集成负责人补齐）**：实施子代理当时只收到上表这六个"编号 + 名称"，
因此它**没有**重写或扩写细则——这个克制是正确的。以下原文由集成负责人从第二轮裁决
《源码复核后的决策补丁》第 6 节**逐字补录**，编号与上表一致；若产品侧文本与此不同，以产品侧为准。

#### 10.1.1 G1–G6 逐条原文（逐字照录，未改写）

| 编号 | 上一轮原文 |
| --- | --- |
| **G1 请求链清单** | **请求链清单。** 明确外层聊天、视觉转述、规划、验收分别调用哪个已配置模型，共几类请求，是否需要显存往返切换。 |
| **G2 实际 wire 体量** | **实际 wire 体量。** 记录每类请求的输入构成、输出上限、图片路线和实际生效 deadline。 |
| **G3 小范围能力校准** | **小范围能力校准。** 使用代表性的短请求测量预填充、生成和模型就绪耗时；少量样本只作为保守试运行依据，不能称为稳定 P95。 |
| **G4 最低闭环可行性** | **最低闭环可行性。**"确认工具状态→一条短线或矩形→验收"在该路线的预算内可以运行。 |
| **G5 超预算行为** | **超预算行为。** 使用假时钟和故障注入验证到期停止、无同请求自动重试、无新动作、未知请求不消失。 |
| **G6 原有输入安全门禁** | **原有输入安全门禁。** 所有权、回执、释放、动作去重、审批和观察有效性继续通过。 |

**适用范围（裁决同节明确）**：G1 与 G3 属于"连续 Paint 前"的准入准备，**不在本轮 RPR-11c
的实施范围内**（本轮实施的是裁决第 3、4、5 项）；G2/G4/G5/G6 与预算闭环直接相关。

### 10.2 本轮三条解释

以下三条是本轮对 G 系列的**解释口径**，与上表编号一一对应：

**解释一（对应 G2「实际 wire 体量」）**
根 deadline 未接线时，记录必须写 `root_deadline_state = not_wired`、`root_deadline = absent`。
此时**只验证真实存在的那三类约束**：CU 任务预算、当前阶段上限、以及本调用实际经过的
请求层硬上限；**不得**把不存在的父运行剩余时间写成一个伪造数值，**也不得**把整个聊天期限
默认为 120 秒或 600 秒。`not_wired` **不等于**"用户选择了无限时间"。

**解释二（对应 G5「超预算行为」）**
"超预算**无新动作**"里的"动作"指**新业务动作**（新的规划请求、新的点击、新的拖拽、
新的输入）。因此：

- 受限的安全收尾（停止旧 helper、必要释放、有限等待与收尾对账）**不被这条门禁误禁**；
- 但收尾**也不是**继续任务的例外：不得借收尾继续规划、重画或补做业务动作。

**解释三（对应 G4「最低闭环可行性」与 G6「原有输入安全门禁」）**
两者的推进顺序固定为三段，不得跳步：

1. **先离线安全验证**（纯本地、无真实模型调用、无真实桌面输入）：预算判定、串联扣减、
   零预算零请求、迟到事实、收尾窗口不与重复信号刷新等；
2. 然后建立**有安全底座的最小真实闭环**（G6 的原有输入安全门禁必须先在场：审批、
   敏感语义拦截、未确认释放互锁、`pre_input_receipt` 的"可证明未发送"）；
3. **只有第 2 段通过后**，才进入**连续 Paint**（长任务、连续笔画）验证。

### 10.3 与 §1–§9 的口径对齐

本节成立的前提是第 1 节的两条既有结论不变：

- **外层用户运行的根 deadline 在源码中不存在**（§1.2）。因此 G2 的"父运行剩余时间"
  这一项在本轮**不可判定**，只能标 `not_wired` + `absent`；
- **CU 子运行有独立任务预算**（§1.5，默认 120 秒、可配 5–300 秒），它是**唯一**
  真实存在的"任务级"时间约束。

**报告用语口径（裁决明确规定，必须逐字遵守）**：

- "**CU 级预算已闭环**"**不等于**"整个聊天已有根 deadline"。前者只说明：
  本 CU 运行内部的观察、规划、验收、重试、重规划与受限恢复共用同一个截止时间；
  后者需要宿主把根 deadline 接进 CU 接纳点（本轮**未做**，见 §11.4 接线清单）。
- 本轮不得把 CU 预算表述成整个聊天的 deadline。

---


## 11. 第二轮实施（裁决第 3、4、5 项）的实际落点

本节与 §1–§9 的性质不同：§1–§9 是**只读取证与设计**，本节记录**已经落在源码里的实现**。
所有条目都标注 `已确认源码 <file>:<line>`；行号取自本轮实施完成时的文件状态。

### 11.1 第 3 项：CU 截止时间的建立点

**已确认源码**：新增 `modules/computer-use/packages/computer-use-core/src/budget.rs`

- `CuDeadline`（`budget.rs:103`）、`CuDeadline::establish`（`budget.rs:116`）、
  `CuDeadline::establish_without_root`（`budget.rs:137`）：
  `cu_deadline_ms = min(接纳时刻 + CU 任务预算, 根 deadline)`；根 deadline 缺席时只取前半项。
- `RootDeadlineState`（`budget.rs`，`not_wired` / `wired`）与
  `RootDeadline`（`Absent` / `At { unix_ms }`）：**两者都由 `RootDeadline` 的取值推导**，
  因此记录里不可能出现"`state = wired` 但 `root_deadline = absent`"这种自相矛盾。
- `CuBudgetFacts`（`budget.rs:206`）与 `CuBudgetFacts::not_accepted`（`budget.rs:223`）：
  随运行记录保存的三个事实。未接纳的请求写 `cu_deadline_ms = None`，
  **不伪装成已经启动的 CU 任务**。

**已确认源码（接纳点，早于租约/切换/观察/规划）**：

- `modules/gui-web/packages/web-console/src/computer_use_executor.rs:695`
  ```rust
  let cu_deadline = CuDeadline::establish_without_root(&self.budgets, now_ms());
  ```
  该行的位置**早于**（源码顺序由测试
  `computer_use_executor::tests::cu_deadline_is_established_before_interlock_lease_and_observation` 固定）：
  1. 未确认释放互锁 `match release_interlock_decision(`；
  2. 租约等待 `windows_process_guard::ScopedInputOwnership::acquire(`；
  3. 适配器构建与随之发生的本地模型切换 `self.adapters.build(surface)`；
  4. 控制器入口 `ComputerUseRunContext {`（初始观察与规划都在它里面）。
- 同一个截止时间被显式交给控制器：`computer_use_executor.rs:851`
  `let mut controller = controller.with_cu_deadline(cu_deadline);`；
  控制器侧落点 `computer-use-core/src/controller.rs:367`（`with_cu_deadline`）与
  `controller.rs:956`（`run_facts`，只解析一次）。
- 落库的 `deadline_ms` 与它同源：`computer_use_executor.rs:896`（`create_run`），
  `created_at_ms = 接纳时刻`、`deadline_ms = CU 截止时间`。
- **接入后仍须注意**：`PendingRunGuard`（`computer_use_executor.rs:88` 的 `cu_budget` 字段）
  在 future 被丢弃时也会把已建立的预算事实写进终态，避免取消路径丢掉截止时间。

**当前有效调用预算**：`min(CU 剩余时间, 当前阶段上限, 实际模型请求上限)`。
`父运行剩余时间` 这一项**不存在**（§1.2），因此没有进入 `min` 链，也没有被伪造成数值。

### 11.2 第 4 项：planner 消费 remaining

**已确认源码**：

- `ComputerUsePlanner` 的三个方法全部接收 `remaining: std::time::Duration`
  （`computer-use-core/src/controller.rs:24` 起，三个方法定义在 `:32`/`:41`/`:56` 区段），
  类型与单位与 `ComputerUseAdapter` 一致；
  **`verify` 的默认实现已删除**，不存在"为了兼容而忽略 remaining"的实现。
- controller 在每个阶段重新读取剩余并交给 planner：
  `controller.rs:425`（初始观察，交 adapter）、`:447`（`classify`）、`:503`（初始验收，交 adapter）、
  `:511`（初始验收，交 planner）、`:553`（`next_action`）、`:677`（规划前的守卫判定）、
  `:703`（stale 恢复的重新观察）、`:758`（执行后观察）、`:778`（执行后验收）、
  `:785`（验收，交 planner）。
- 阶段门限与拒绝：`modules/gui-web/packages/web-console/src/computer_use_planner.rs:20`
  （`MIN_STAGE_BUDGET = 500ms`）、`:442`（`insufficient_budget`）、`:458`（`require_stage_budget`）。
  判定用的是 `clamp_stage_timeout`（`min`，无下限回扩），**不是**带下限的 `clamp`。
- 零预算即零请求：判定在**构建 context / 请求 / 客户端之前**
  （`computer_use_planner.rs:521` 起 `request_model`，`let budget = require_stage_budget(...)` 在
  `build_context_assembly_with_roster` 之前），因此预算不足时 HTTP 请求次数为 0。
- 串联请求连续扣减：`computer_use_planner.rs:593`（规划阶段入口判定）→ `:616`
  （`budget = budget.saturating_sub(started.elapsed())`）→ 同一个 `budget` 传给动作规划请求。
- 迟到事实：模型请求交给独立任务执行（`computer_use_planner.rs:724` `dispatch_model_request`），
  等待受预算约束（`:521` `request_model`）。
  **预算到期只结束等待、不取消底层请求、也不补发第二次请求**——补发等于在预算之外
  启动一个新的模型操作（正是"不得借收尾继续规划"禁止的行为）。
  响应回到任务时若等待方已放弃，则由 `LateUsageRecorder`（`computer_use_planner.rs:696`）
  只记账；动作永远不产生。反例断言见测试
  `late_model_result_is_recorded_as_a_fact_without_a_new_action`（到期后 HTTP 请求数仍为 1）。

### 11.3 第 5 项：两个期限分开

**已确认源码**：新增 `modules/computer-use/packages/computer-use-core/src/cleanup.rs`

| 策略值（**待验默认值，不是实测时延**） | 常量 | 位置 |
| --- | --- | --- |
| 协作退出等待 ≤ 2 秒 | `DEFAULT_COOPERATIVE_EXIT_GRACE_MS` | `cleanup.rs:31` |
| 自动收尾总窗口 ≤ 4 秒 | `DEFAULT_CLEANUP_WINDOW_MS` | `cleanup.rs:33` |
| 独立释放等待 ≤ 2 秒 | `DEFAULT_INDEPENDENT_RELEASE_WAIT_CAP_MS` | `cleanup.rs:35` |

- 唯一集中定义点：`CleanupPolicy::default()`（`cleanup.rs`）；输入安全/取消策略的单一入口是
  `input_stroke.rs:19`（`fn input_cleanup_policy`）。除这两处外**没有**第二份数值。
- 两个期限的载体：业务期限 = `CuDeadline`（§11.1）；收尾期限 = `CleanupDeadline`。
- 窗口共享：`cleanup.rs:167`（`cooperative_exit_grace_at` = `min(2s, 剩余)`）与
  `cleanup.rs:173`（`independent_release_wait_at` = `min(2s, 剩余)`），
  两者都从**同一个** `deadline_at` 起算，因此"终止 helper、等待静止、独立释放、同步对账"
  共同消耗这 4 秒，不会各自再领一份。
- **`cleanup_deadline` 不被重复信号刷新**：`cleanup.rs:127`（`CleanupDeadline::fixed`，
  `get_or_insert_with` 语义）；helper 侧的使用点 `input_stroke.rs:591`
  （只在 `cancellation_at.is_none()` 时执行一次）。
- 独立释放最多一次：`input_stroke.rs:632` 起（`if release_needed { ... }` 单块，
  窗口到期走 `independent_release_skipped_window_expired = true` 而不尝试），
  补发函数 `input_stroke.rs:776`（`fn emergency_release(timeout)`，不再写死 6 秒）。
- **不再承诺"释放一定成功"**：确认不了就写 `CleanupReleaseStatus::Unconfirmed`
  （`cleanup.rs` 的 `quarantines()`），并把"未知即隔离"记录到
  `CleanupReport::quarantined`；阻止后续输入的机制仍是既有的未确认释放互锁
  （`computer_use_executor.rs` 的 `release_interlock_decision`，见 §1.5）。
- 报告分开给出五项：`CleanupReport`（`cleanup.rs:237`，`assemble` 在 `:259`）
  分别记录 `business_deadline_ms` / `new_business_input_stopped_at_ms` /
  `cleanup_started_at_ms` / `cleanup_finished_at_ms` / `release` / `quarantined` /
  `exceeded_business_deadline`，helper 细节另存 `HelperCleanupFacts`（`cleanup.rs:213`）。
- 报告的产生点：`controller.rs:878`（`cleanup_report`）。helper 层事实经
  `ComputerUseError::with_cleanup`（`contracts.rs`）上抛，桌面桥在
  `computer_use_desktop_bridge.rs:337` 附加，读不到 helper 事实时由控制器按
  "阶段 + 回执"判定（**确认不了就写未确认**）。

**顺带修掉的一处预算泄漏**：`browser_bridge.rs:597` 的 tab 生命周期动作过去丢掉
`remaining`、直接使用裸 `BRIDGE_TIMEOUT`；现在改为
`clamp_stage_timeout(remaining, BRIDGE_TIMEOUT)`（自检等诊断路径仍用常量上限，
它们在 `execute_tab_action` 调用点显式传入）。

### 11.4 需要集成负责人接线的最小清单

**A. `main.rs` 侧（CU 工具分发点）**

1. **必须传什么**：CU 接纳点目前没有任何父 deadline 入参
   （`computer_use_executor.rs` 的 `execute_with_current_runtime` /
   `execute_in_room_with_full_access` 签名未改）。若将来接线，只需在
   `ComputerUseExecutor` 上增加一个"父 run 根 deadline"入参，并在接纳点把它传给
   `CuDeadline::establish(&self.budgets, now_ms(), RootDeadline::at(root_deadline_ms))`
   —— CU 侧已经支持 `RootDeadline::At{..}`，`min` 链会自动收紧。
2. **哪里建立截止时间**：**不要**在 `main.rs` 里算 CU 剩余时间；CU 截止时间由
   `computer_use_executor.rs:695` 在任务被接纳时建立（已早于租约/切换/观察/规划）。
   `main.rs` 只需提供根 deadline 这一个事实。
3. 若不接线：保持现状即可，记录会如实写 `not_wired` + `absent`，
   但**不得**把 CU 预算描述成整个聊天的 deadline（§10.3）。

**B. 存储层侧（运行记录的表达形式，本轮未改该文件）**

本轮**没有**修改 `computer_use_store.rs` 的列定义与 `API`。三个字段目前的表达路径是：

| 要表达的事实 | 现状 | 需要集成负责人决定 |
| --- | --- | --- |
| `cu_deadline`（实际截止时间） | **已有列**：`computer_use_runs.deadline_ms`，由 `create_run` 写入（`computer_use_executor.rs:896`），值 = 接纳时刻 + CU 任务预算 | 无需改动 |
| `root_deadline_state = not_wired` | 已随结果对象写入 `terminal_result_json`（`ComputerUseResult.cu_budget`，`#[serde(default)]`，旧记录读成 `None`） | 是否要提升为**独立列** |
| `root_deadline = absent` | 同上 | 同上 |

若要把后两项提升为独立列（便于 SQL 侧筛选），需要集成负责人在
`computer_use_store.rs` 的建表/迁移与 `finish(...)`/`create_run(...)` 中串行接线；
**本轮已把这些事实放进了结果对象**，因此列化只是可读性问题，不是表达缺失问题。

**C. 需要产品/集成负责人裁决的两点**

1. `MIN_STAGE_BUDGET = 500ms`（`computer_use_planner.rs:20`）是**自定义的当前实现值**，
   不是裁决给出的数值；它决定"剩余多少才允许开始一个阶段"。
2. §11.3 的三个收尾数值是**待验默认值**；门禁 5（§6 的"门禁 5"）取得本机实测时延后应重新校准。

### 11.5 本轮未做 / 未验证

1. **未接线根 deadline**：`RunBudget.deadline_unix_ms` 仍未进入 CU 接纳点（§11.4-A）。
   因此 `min` 链里"父运行剩余"这一项**不存在**，"CU 预算 ≤ 父剩余"这一条**仍不可判定**。
2. **`input.rs`（点击/输入/滚动/按键）路径未纳入有界收尾协议**：该路径用 `run_powershell`
   + 分离线程 + `recv_timeout`，超时后**子进程仍在运行**（无 kill、无静止确认），
   与本轮的 `input_stroke.rs` 有界收尾是两套机制。**未验证**其实际后果；
   按 §11.3 的口径，它的释放事实仍然只按"错误码 → may_have_been_sent + unknown"记录。
3. **未发起任何真实模型调用**：§11.3 的"迟到事实"用例用的是本地 mock HTTP 服务；
   真实的"600 秒截断"与预填充量级（§5.3 的 511 秒 / 90 tok/s）**仍未实测**。
4. **未做连续 Paint 端到端**：按 §10.2 的解释三，连续 Paint 必须在
   "离线安全验证 → 有安全底座的最小真实闭环"之后才进行；本轮只完成了第一段。
5. **`cleanup_deadline` 的四个时刻未做真实时延实测**：本轮只验证了"有界、共享、
   不被刷新、最多一次"这些**结构性**性质。
6. **`main.rs` 与 `modules/core-runtime` 本轮被并发修改**：实施期间的第一次
   `cargo build -p coolzhu-computer-use-core --offline` 因
   `coolzhu-core-runtime` 的并发编辑失败（15 个错误，全部位于 core-runtime，
   与本轮改动无关）；原样重跑后通过。详见 §12 验证记录。
7. **独立释放的 wall clock 有界但比"等待上限"更长**：`emergency_release(min(2s, 剩余))`
   自身仍是一次 helper 调用，若该 helper 不退出，它会再走一遍 helper 内部的协作退出宽限
   （同样 `min(2s, 剩余收尾时间)`）。因此最坏 wall clock 约 `2s + 2s`，
   **有界、不重试、不递归**（`mode=release` 不触发补发），但**未实测**其分布。
   `HelperCleanupFacts.independent_release_wait_ms` 记录的是**外部等待上限**，
   不是这整段时长，读记录时不要把它当成"独立释放的实际耗时"。

### 11.6 `min` 链的构成（避免误读为"已经有父预算"）

本轮实现里**实际参与 `min` 的只有三项**：

```
effective_call_budget = min(
    CU 剩余时间,          // CuDeadline::remaining_ms（真实存在）
    当前阶段上限,          // planner: PLANNER_TIMEOUT=20s；adapter: 输入 8s / 截图 10s / 桥 10s
    实际模型调用上限       // provider 的 600s 硬上限；恒大于 20s，因此不改变结果
)
```

- **父运行剩余时间这一项不存在**，因此没有进入 `min` 链（§1.2）。
  记录里对应写 `root_deadline_state = not_wired` + `root_deadline = absent`。
- 因此"**CU 级预算已闭环**"只表示：本 CU 运行内部的观察、规划、验收、重试、重规划与
  受限恢复共用同一个截止时间，且每个阶段发起子请求前都按剩余收紧。
  它**不等于**"整个聊天已有根 deadline"。

---

## 12. 本轮验证记录（原样记录，含并发编辑导致的伪故障）

### 12.1 按序执行的确切结果

| # | 命令 | 结果 |
| --- | --- | --- |
| 1 | `cargo build -p coolzhu-computer-use-core --offline` | **成功**（`Finished dev profile`，0 error） |
| 2 | `cargo test -p coolzhu-computer-use-core --offline` | **82 passed / 0 failed**（另两个 target：0 / 0）。工单基线 60，**+22** 全部来自本轮新增测试 |
| 3 | `cargo build -p coolzhu-web-console --offline` | **成功**（0 error；71 个 warning 全部来自并发修改中的 `main.rs`，本轮改动文件不再产生新 warning） |
| 4 | `cargo test -p coolzhu-web-console --offline`（**全量**） | `src/lib.rs` **8 passed / 0 failed**；`src/bin/browser_native_host.rs` **1 passed / 0 failed**；**`src/main.rs` 1037 passed / 0 failed**；doc-tests 0。**合计 1046 passed / 0 failed** |
| 5 | `cargo test --test module_linkage_smoke --offline` | **4 passed / 0 failed**（与基线 4 一致） |

**关于第 4 项基线的差异说明**：工单给的基线是 1028 通过 / 0 失败。
本轮实测 `src/main.rs` 为 **1037 通过 / 0 失败**（+9），口径如下：

- 其中 **+7 是本轮新增测试**（`computer_use_planner` 4 个 + `computer_use_executor` 3 个，
  名称见 §12.2）；
- 另外 **+2 无法归因于本轮改动**：实施期间 `main.rs` 与
  `modules/core-runtime/.../fact_store.rs` 被同一仓库的其它 agent 并发修改
  （本轮实测到 `main.rs` mtime `2026-09-24 08:18`、`fact_store.rs` mtime `08:22`，
  均晚于本轮开始时间）。**本轮不对这 2 个测试做归因断言**，
  但已确认它们不失败、且与本轮改动无关。

### 12.1.1 复跑记录：并发编辑落地后的第二次全量结果（如实记录）

上表的"1037 / 0 failed"是**本轮改动完成时**的结果。随后（约 10 分钟后）同一仓库的
其它 agent 把**迁移 v21** 落了进来（`main.rs` 的 `PRAGMA user_version` 期望值仍是 20），
因此**同一命令的复跑**结果变为：

- `cargo test -p coolzhu-web-console --offline`（复跑，全量）：
  `src/lib.rs` **8 / 0**、`src/bin/browser_native_host.rs` **1 / 0**、
  `src/main.rs` **1036 passed / 5 failed**（共 1041 个测试）、doc-tests 0。
- 这 **5 个失败全部位于 `main.rs` 的迁移/模式断言**，与本轮改动无关：
  `tests::chat_handoff_contract_roundtrips_and_defaults_none`（`user_version` 实测 21 ≠ 期望 20）、
  `tests::migration_v13_adds_goal_phase_routing_columns`、
  `tests::migration_v13_upgrades_v12_database_without_losing_rows`、
  `tests::migration_v20_adds_goal_phase_claim_columns_idempotently_without_losing_rows`、
  `tests::runtime_run_v19_schema_is_complete_repairable_and_idempotent`。
- **本轮改动范围内**的测试在同一复跑中**全部通过**：
  `cargo test -p coolzhu-web-console --offline computer_use` → **130 passed / 0 failed**；
  `cargo test -p coolzhu-computer-use-core --offline` → **82 passed / 0 failed**；
  `cargo test --test module_linkage_smoke --offline` → **4 passed / 0 failed**。

**结论口径**：第 12.1 节的 1037 / 0 failed 是本轮改动完成时的真实全量结果；
第 12.1.1 节的 5 个失败属于并发落地的迁移 v21 工作（`main.rs`），
**不是**本轮 CU 预算改动的回归。

### 12.2 并发编辑导致的伪故障（台账 §B-3 已知问题）

本轮的**第一次** `cargo build -p coolzhu-computer-use-core --offline` 失败，共 15 个
编译错误，**全部位于 `modules/core-runtime/packages/core-runtime/src/`**
（`run_contract.rs` 的 `Deserialize` 冲突、`fact_store.rs` 的
`String` vs `Option<String>` 类型不匹配等），与本轮改动无关。
按工单要求**原样重跑一次**后通过。此后 `core-runtime` 一直处于可编译状态，
因此第 12.1 节的全部结果都是在本轮改动上取得的真实结果。

### 12.3 本轮新增测试清单（共 29 个：core 22 + web-console 7）

**`coolzhu-computer-use-core`（22）**

| 文件名 | 测试名 | 对应的裁决验收项 |
| --- | --- | --- |
| `budget.rs` | `not_wired_root_is_recorded_as_absent_and_never_as_an_infinite_choice` | 第 3 项 1：not_wired ≠ 无限时间 |
| `budget.rs` | `absent_root_is_not_replaced_by_a_fabricated_parent_budget` | 第 3 项 5：不伪造父运行剩余 |
| `budget.rs` | `wired_root_deadline_shrinks_the_cu_deadline_to_the_smaller_value` | 第 3 项 1：接线后取 min |
| `budget.rs` | `not_accepted_requests_have_no_cu_deadline` | 第 3 项 4：未接纳不得伪装成已启动 |
| `budget.rs` | `budget_facts_round_trip_with_stable_snake_case` | 第 3 项 1：三个字段可序列化 |
| `cleanup.rs` | `policy_defaults_are_the_pending_verification_values` | 第 5 项 14：策略值（待验默认值） |
| `cleanup.rs` | `the_whole_cleanup_window_is_shared_and_never_reissued_per_stage` | 第 5 项 14：三者共用 4 秒 |
| `cleanup.rs` | `repeated_signals_never_refresh_a_fixed_cleanup_deadline` | 第 5 项 15：重复信号不刷新 |
| `cleanup.rs` | `finer_policy_makes_the_window_shared_between_all_cleanup_steps` | 第 5 项 14 |
| `cleanup.rs` | `unconfirmed_release_is_reported_as_unknown_and_quarantined` | 第 5 项 17：未知即隔离 |
| `cleanup.rs` | `helper_facts_and_run_report_are_serializable_and_separate` | 第 5 项 18：五项分开给出 |
| `controller.rs` | `planner_receives_the_remaining_of_the_established_cu_deadline` | 第 3 项 2 + 第 4 项 7 |
| `controller.rs` | `consecutive_planner_calls_see_strictly_decreasing_remaining` | 第 4 项 9：连续扣减 |
| `controller.rs` | `internal_recovery_does_not_reset_the_remaining_budget` | 第 4 项 10：恢复不重置时钟 |
| `controller.rs` | `unconfirmed_release_is_reported_as_quarantined_cleanup` | 第 5 项 17/18 |
| `controller.rs` | `cancellation_before_input_reports_cleanup_without_release_obligation` | 第 5 项 16/18 |
| `controller.rs` | `successful_run_records_budget_facts_without_a_cleanup_report` | 第 3 项 1 |
| `supervisor.rs` | `guard_uses_the_deadline_established_at_acceptance_not_its_own_construction` | 第 3 项 2：模型切换/租约耗时计入 |
| `supervisor.rs` | `internal_recovery_never_resets_the_deadline_clock` | 第 4 项 10 |
| `input_stroke.rs` | `cleanup_policy_is_defined_once_and_has_no_fixed_large_release_timeout` | 第 5 项 14：集中定义、不再写死 6 秒 |
| `input_stroke.rs` | `independent_release_wait_is_bounded_by_the_remaining_cleanup_window` | 第 5 项 14：独立释放 ≤ min(2s, 剩余) |
| `input_stroke.rs` | `cleanup_window_is_fixed_once_and_the_release_has_a_single_call_site` | 第 5 项 15：固定一次 + 最多一次 |

**`coolzhu-web-console`（7）**

| 文件名 | 测试名 | 对应的裁决验收项 |
| --- | --- | --- |
| `computer_use_planner.rs` | `zero_budget_returns_insufficient_budget_with_zero_model_requests` | 第 4 项 11：零预算即零请求 |
| `computer_use_planner.rs` | `chained_requests_share_one_budget_and_the_second_sees_the_remainder` | 第 4 项 9：第二请求拿到扣减后余量 |
| `computer_use_planner.rs` | `late_model_result_is_recorded_as_a_fact_without_a_new_action` | 第 4 项 12：迟到结果只记账不触发动作 |
| `computer_use_planner.rs` | `browser_verification_is_local_and_never_needs_a_model_request` | 第 4 项 11：0 请求路径不受影响 |
| `computer_use_executor.rs` | `cu_deadline_is_established_before_interlock_lease_and_observation` | 第 3 项 2：建立点早于租约/切换/观察/规划 |
| `computer_use_executor.rs` | `cu_deadline_covers_initial_observation_and_is_recorded_once` | 第 3 项 1/2：三个字段 + 耗时计入 |
| `computer_use_executor.rs` | `rejected_request_never_pretends_a_cu_task_was_started` | 第 3 项 4：未接纳不伪装已启动 |

### 12.4 本轮测试覆盖到的、由工单点名的验收项

| 工单验收项 | 覆盖测试 |
| --- | --- |
| 零预算 | `zero_budget_returns_insufficient_budget_with_zero_model_requests` |
| 请求前耗尽 | 同上（判定在构建请求之前，实测 HTTP 计数 0） |
| 两个串联请求共同消耗预算（证明第二个拿到扣减后余量） | `chained_requests_share_one_budget_and_the_second_sees_the_remainder` |
| 请求中取消 | 既有 `cancellation_during_*` 系列（`controller.rs`）+ `cancellation_before_input_reports_cleanup_without_release_obligation` |
| 迟到结果不执行新动作但可作迟到事实保存 | `late_model_result_is_recorded_as_a_fact_without_a_new_action` |
| CU 内连续两次请求得到递减预算 | `consecutive_planner_calls_see_strictly_decreasing_remaining` |
| 内部恢复不重置时钟 | `internal_recovery_does_not_reset_the_remaining_budget`、`internal_recovery_never_resets_the_deadline_clock` |
| 预算到期不启动新业务动作 | 既有 `deadline_expiry_before_first_action_performs_no_input` + 零预算用例 |
| 模型切换耗时计入预算 | `cu_deadline_covers_initial_observation_and_is_recorded_once`（观察侧延迟 250ms）、`planner_receives_the_remaining_of_the_established_cu_deadline` |
| cleanup_deadline 固定后不被重复信号刷新 | `repeated_signals_never_refresh_a_fixed_cleanup_deadline`、`cleanup_window_is_fixed_once_and_the_release_has_a_single_call_site` |
| 独立释放最多一次且等待 ≤ `min(2s, 剩余)` | `independent_release_wait_is_bounded_by_the_remaining_cleanup_window` |
| 收尾五项时刻/状态分别记录 | `helper_facts_and_run_report_are_serializable_and_separate`、`unconfirmed_release_is_reported_as_quarantined_cleanup` |

**"请求中取消"的说明**：这条在 core 侧由既有用例覆盖（取消到达即终态、零新输入），
本轮新增的是**取消后的收尾报告**（五项分开给出）。
**未覆盖**的是"取消与模型请求同时发生时的竞态"——该场景需要真实模型时延，**未验证**。
