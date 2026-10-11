我先确认 coolzhu-agent 当前声明的工具，然后按正式六项提交一次 `computer_use_perform`。BU-INSTALLED-072-cross 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/cross.html}`，`max_actions=1`
- 直接返回终态（非 running），无需 wait

**终态回执**
- `status: blocked`，`stage: supervisor`，`goal_achieved: false`
- 错误：`budget_exhausted` — "computer-use action budget exhausted"，`retryable: false`，`circuit_open: true`
- `attempts: 1`，`steps_completed: 1`，`action_count: 1`，`replan_count: 0`，`no_progress_count: 0`
- CU 预算已接线（cu_budget 600s），非超时——1 步动作预算用满后熔断

**证据链（回执原文）**
- 一次输入动作投递记录：`native-input:5fa0e9ac3b9e9c6955653d3d2a6bceda`
- 前后观察对：`native-observation:b423853a…`（点击前）→ `bb26aca4…`（点击后）→ `9a9197ab…`（最终观察）
- 回执未包含观察到的页面文本——最终 URL、TARGET-072 显示、目标页输入事件三项均未给出

**结论**
- 一次点击已真实投递，但动作预算在第 1 步用满后即熔断，未获得对最终页面的成功核验；三项标准（URL 为 `http://localhost:51866/target.html`、显示 TARGET-072、目标页输入事件为 0）均 `0/3` 未确认
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用；不凭点击释放报完成