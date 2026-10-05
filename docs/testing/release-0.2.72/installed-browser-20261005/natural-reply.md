我先确认 coolzhu-agent 当前声明的工具。按正式六项提交一次 `computer_use_perform`。BU-INSTALLED-072-natural 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/source.html}`，`max_actions=1`
- 直接返回终态（非 running），无需 wait

**终态回执**
- `status: blocked`，`stage: supervisor`，`goal_achieved: false`
- 错误：`budget_exhausted` — "computer-use action budget exhausted"，`retryable: false`，`circuit_open: true`
- `attempts: 1`，`steps_completed: 1`，`action_count: 1`，`replan_count: 0`，`no_progress_count: 0`

**证据链**
- 一次输入动作投递记录：`native-input:89d2f1ed17ad95756f7b6b366d56af2a`
- 前后观察对：`native-observation:7931613c…`（同一代重复）→ `08607d25…`（点击后）→ `33f11439…`（最终观察）
- 回执未包含观察到的页面文本——最终 URL 和 TARGET-072 显示状态均未给出，验收标准无任何一项获宿主确认

**结论**
- 一次点击已真实投递，但随后预算耗尽熔断，未能进入对最终页面的成功核验；三项标准（URL 为 target.html、显示 TARGET-072、目标页输入事件为 0）均 `0/3` 未确认
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用；不凭点击释放报完成