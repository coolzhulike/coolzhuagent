我先确认 coolzhu-agent 当前声明的工具，然后按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。BU-INSTALLED-072-form 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/form.html}`，`max_actions=6`
- 首轮回执 `running`，对同一 job_id 调用 `computer_use_wait` 1 次到达终态；未重复调用 perform

**终态回执**
- `status: blocked`，`stage: execution`，`goal_achieved: false`
- 错误：`native_browser_target_outside_viewport` — "节点预检拒绝，未发送输入"，`retryable: false`
- `attempts: 3`，`steps_completed: 2`，supervisor `action_count: 3`，`no_progress_count: 0`，`replan_count: 0`，`circuit_open: false`

**证据链（回执原文）**
- 两次输入动作真实投递：`native-input:3227cf5e5e9b1d27fc1a511d9a03ed21`、`native-input:fb68a74565847d231bb616e099b10bd6`（页面观察在各步间有变化，`476d115b…` → `ff2471d3…` → `a4820a4d…`）
- 第 3 个动作被节点预检拒绝：目标节点在视口外，`input_delivery: not_sent`，`effect: not_observed`，`goal_verdict: not_checked`——规划器未先滚动到该节点而是直接尝试交互，预检拦截
- 最终页面观察 `native-observation:d5525e59…` 存在，但回执未包含页面文本，「FORM-PASSED-072」未获确认

**结论**
- 前两步（可能为填写姓名、勾选或滚动尝试）有部分进展，第 3 步因目标在视口外被预检拒绝且不可重试，任务阻塞
- 验收标准 `criteria_met: 0/1`——页面是否显示 FORM-PASSED-072 未经核验
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用；不凭输入投递报完成