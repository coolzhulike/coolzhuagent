BU-CANDIDATE-VIEWPORT-form 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/form.html}`，`max_actions=6`
- 首轮回执 `running`（job_id `be878686…e432`），对同一 job_id 调用 `computer_use_wait` 2 次到达终态；未重复调用 perform

**终态回执**
- `status: blocked`，`stage: execution`，`goal_achieved: false`
- 错误：`native_browser_target_outside_viewport` — "节点预检拒绝，未发送输入"，`retryable: false`，`retry_owner: none`
- `attempts: 4`，`steps_completed: 3`，supervisor `action_count: 4`，`replan_count: 0`，`no_progress_count: 0`，`circuit_open: false`

**证据链（回执原文）**
- 三次输入动作投递记录：`native-input:61c5da7d678fb72e388947417cf0b2b0`、`369f62b1a64f4f30839654e8f220b640`、`6315cbfcfc3cfcf938ea080c29edfc6d`
- 第 4 次动作在节点预检被拒绝：`input_delivery: not_sent`，`effect: not_observed`，`goal_verdict: not_checked`——目标节点（应为视口外的确认复选框/提交按钮）不在当前视口，预检拒绝发送
- 每步前后均有 `native-ax` / `native-observation` 观察对，页面观察在 3 步间确有变化（`e87e8ac7…` → `1a3e9a56…` → `1a12f20b…` → `d0c51018…` → `585d2a90…`）

**结论**
- 三次输入（推测为点击输入框、文本输入 SWE2-072、滚动）已真实投递，但第 4 步因目标节点在视口外被预检拒绝、未发送，阻塞于执行阶段；`FORM-PASSED-072` 显示未获确认，`0/1`
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用；不凭输入释放报完成