BU-CANDIDATE-PHASE2-natural-static 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/source.html}`，`max_actions=1`
- 直接返回终态（非 running），无需 wait；未重复提交 perform

**终态回执**
- `status: blocked`，`stage: supervisor`，`goal_achieved: false`
- 错误：`budget_exhausted` — "computer-use action budget exhausted"，`retryable: false`，`circuit_open: true`
- `attempts: 1`，`steps_completed: 1`，单击投递回执 `native-input:2dce7665a8d97b12740a9c8fec7e04bc`
- `criteria_met: 2/3`，`ungrounded_positive_count: 1`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `b3f78b39…`）**
- 最新页面 URL：`http://127.0.0.1:51866/target.html` —— 标准 1 达成（reason: url）
- 标题 `TARGET-072`，节点 index 0（RootWebArea，name_chars=10）—— 标准 2 达成（reason: title）
- 输入计数：**标准 3 未获确认**（`grounded: false`，reason: `non_adjacent_nodes`）

**关于 StaticText 节点要求（如实报告，不编造）**
- 本轮验收按要求仅选 StaticText 节点：index 4 `「目标页输入事件：」`（name_chars=8）与 index 8 `「0」`（name_chars=1）——两个 StaticText 原文节点
- 但两节点**不相邻**：span 内 index 5 为空 generic（blank），index 6–7 为 InlineTextBox 重复片段（各 name_chars=10、8），宿主判 `non_adjacent_nodes`，grounding 不成立
- 完整原文「目标页输入事件：0」在 StaticText 层被拆为不相邻的 index 4 与 index 8；中间隔着 blank generic 节点（5）和 InlineTextBox 副本（6、7）——这些中间节点已如实列出，不编造

**结论**
- 一次单击真实投递，页面跳转到 target.html 且标题 TARGET-072，但第三项因仅选 StaticText 时节点不相邻未判达成；预算随后耗尽熔断，`goal_achieved: false`
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用；不凭点击释放报完成