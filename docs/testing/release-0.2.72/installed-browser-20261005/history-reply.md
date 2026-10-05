按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。BU-INSTALLED-072-history 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/history.html}`，`max_actions=3`
- 首轮回执 `running`（job_id `117ca92d…65f109`），对同一 job_id 调用 `computer_use_wait` 到达终态；未重复调用 perform

**终态回执**
- `status: blocked`，`stage: supervisor`，`goal_achieved: false`
- 错误：`budget_exhausted` — "computer-use action budget exhausted"，`retryable: false`，`circuit_open: true`
- `attempts: 3`，`steps_completed: 3`，`replan_count: 0`，`no_progress_count: 0`
- CU 预算已接线（cu_budget 600s），非超时——3 步动作预算用满后熔断

**证据链（回执原文）**
- 三次真实输入动作投递记录：
  - `native-input:b549b84253fedfb63a521c5013eaac07`
  - `native-input:2d9d26f8286aed4e3263e4924ce03156`
  - `native-input:c07586266536784b0dd6c38f3db49802`
- 每步前后均有 `native-ax` / `native-observation` 观察对（`d9127156…`、`6770c591…`、`179e9e2b…`、`db9be2d5…`、`a85d54f8…` 等），页面观察在各步间确有变化
- 回执未包含最终页面文本——最终 URL、阶段显示、操作顺序三项均未给出

**结论**
- 三次 click（进入第二阶段 → 历史后退 → 历史前进）均真实投递，但动作预算在第 3 步用满后即熔断，未获得对最终页面的成功核验；三项标准（URL 为 `history.html?stage=2`、显示「阶段：第二阶段」、显示完整操作顺序）均 `0/3` 未确认
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用；不凭输入释放报完成