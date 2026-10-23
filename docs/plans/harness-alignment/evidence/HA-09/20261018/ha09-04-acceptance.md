# HA09-04 可信诊断回执单消费者：验收记录

状态：**通过**。所有命令均由真实 PowerShell 进程启动；命令启动次数以目标文件实际行数计数，回执 ID 来自工具进程/Goal 进程，不采用模型正文或命令字符串推断。

## 实现

- `core-runtime` 的 `BashCommandOutput` 增加 `executionReceipt`：真实 shell、command、cwd、timeout、起止时间、exit code、interrupted。
- `tool-registry` 的 PowerShell 前景执行在 `execute_shell_command` 中填充该回执；后台执行不产生可复用回执。
- `RegistryExecutor::execute` 从工具 JSON 读取宿主回执，按 `call_id` 暂存。
- `goal_verification` 的 Command 消费端按当前 tool messages 逐条取出回执；只有 command、规范化 cwd、实际 shell、exit 0、非中断、timeout 不弱于合同才复用。缺失/不匹配/旧记录才由 runtime 执行，并只执行未满足的命令。
- `Goal Command` 与 PowerShell 工具共用 `resolve_powershell_shell`（pwsh 优先、powershell 回退），避免解释器不一致。
- repair 阶段持久化 `repair_result.command_verification`；verifier 阶段从同阶段结构化回执复用，不把模型文字当回执。
- 旧 `Vec<String>`、空失败、legacy 记录仍不能升级为 Command passed。

## 实际验证

### 1. 有可信回执：runtime 0 次启动

`runs/diagnostic-01/`：

- 工具真实执行一次 `Add-Content ... reused.txt`，产生 `executionReceipt`。
- Goal gate 使用同一 command/cwd/shell/timeout 的回执，结果 `state=passed`，receipt ID 与工具回执相同。
- `reused.txt` 实际 1 行（若 runtime 再执行会是 2 行）。
- 无回执 fallback：runtime 启动一次，receipt ID 为 `goal-command-*`，文件 1 行。
- command 不匹配：工具回执被拒绝，runtime 启动一次，receipt ID 为新的 `goal-command-*`，文件 1 行。
- 证据：`ha09-diagnostic-receipts.json`、`output.log`。

### 2. repair_result 回执复用

`runs/repair-01/`：

- 先由 `goal_command::run_receipt` 真实执行一次，得到带 shell 的持久回执。
- 将结构化 `repair_result.command_verification.receipts` 写入 phase 的 `last_evidence_json`。
- verifier gate 无当前 tool messages，仍从 persisted repair 回执复用；`repair-once.txt` 保持 1 行，receipt ID 不变。
- 证据：`ha09-repair-receipt.json`、`output.log`。

### 3. 真实 phase finalizer 最终状态

`runs/phase-diagnostic-01/`：

- 真实工具回执 -> `run_goal_phase_command_gate_with_receipts` -> token-aware finalizer。
- phase 最终 `status=completed`、`last_verdict=pass`；`last_evidence.command_verification.receipts[0].id` 等于工具原始回执 ID。
- 目标文件 1 行，finalizer 未再次启动命令。
- 证据：`ha09-phase-diagnostic.json`、`output.log`。

### 4. 回归

`runs/ha09-04-regression-02/`：

- `ha09_*` 5 项通过（含 diagnostic、repair、cancel/timeout、contract faults、phase consumption）。
- `goal_command::tests` 2 项通过。
- `goal_verification::tests` 2 项通过。
- 日志：`ha09-tests.log`、`goal-command.log`、`goal-verification.log`。

生产构建：`tmp/ha09-04-build-02.log`，`cargo build -p coolzhu-core-runtime -p coolzhu-tool-registry -p coolzhu-web-console --offline` exit 0。

## 局限

- 测试使用真实 PowerShell 和真实 Goal gate/finalizer 夹具，但没有真实模型生成 tool call；模型侧诊断由测试直接调用同一 registry 出口模拟。真实模型集成仍由 HA-11 检查。
- 未覆盖 pwsh 与 powershell 同时存在时的跨解释器复用；当前规则要求实际 shell 完全相同，不匹配即拒绝复用。
- repair 复用只接受带 shell 的结构化回执；旧记录没有 shell 时仍重新执行，这是有意的保守行为。
- HA10 尚未实施，输入变化失效/有限复用不在本条宣称。
