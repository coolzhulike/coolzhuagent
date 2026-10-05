先确认 coolzhu-agent 当前声明的工具，然后按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。BU-CANDIDATE-LOADING-20261005-zoom-form 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:49944/form.html}`，`max_actions=6`
- 首轮回执 `running`，对同一 job_id 调用 `computer_use_wait` 2 次到达终态；未重复调用 perform

**终态回执**
- `status: blocked`，`stage: execution`，`goal_achieved: false`
- 错误：`native_browser_target_outside_viewport` — "节点预检拒绝，未发送输入"，`retryable: false`，`retry_owner: none`
- `attempts: 4`，`steps_completed: 3`，supervisor `action_count: 4`，`replan_count: 0`，`no_progress_count: 0`，`circuit_open: false`
- CU 预算已接线（cu_budget 600s），非超时

**证据链（回执原文）**
- 三次输入动作投递记录：`native-input:8f99e14e…`、`3f688875…`、`b124db0c…`（推测为点击姓名框、text 输入 SWE2-074、滚动）
- 第 4 次动作节点预检被拒：`input_delivery: not_sent`，`effect: not_observed`，`goal_verdict: not_checked` —— 目标节点（应为视口外的确认复选框/提交按钮）不在当前视口，输入未发送
- 每步前后均有 `native-ax` / `native-observation` 观察对，页面观察在 3 步间确有变化（`72cf1ed5…` → `8e01ab19…` → `c866aa62…` → `0982a21f…` → `26d3b5ad…`）

**结论**
- 三次输入已真实投递，但第 4 步因目标节点在视口外被预检拒绝未发送，阻塞于执行阶段；`最新页面显示FORM-PASSED-074` 未获确认，`0/1`
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用；不凭输入释放报完成