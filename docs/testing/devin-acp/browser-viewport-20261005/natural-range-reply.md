先确认 coolzhu-agent 声明的工具，然后提交一次 `computer_use_perform`。BU-CANDIDATE-PHASE2-natural-range 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/source.html}`，`max_actions=1`
- 直接返回终态（非 running），无需 wait；未重复提交 perform

**终态回执**
- `status: blocked`，`stage: supervisor`，`goal_achieved: false`
- 错误：`budget_exhausted` — "computer-use action budget exhausted"，`retryable: false`，`circuit_open: true`
- `attempts: 1`，`steps_completed: 1`，单击投递回执 `native-input:ffbca9137aa98239dc036596ddd28816`
- `criteria_met: 2/3`，`ungrounded_positive_count: 1`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，observation `8d9cd460…` / 终验 `db10ccf8…`）**
- 最新页面 URL：`http://127.0.0.1:51866/target.html` —— **标准 1 达成**（reason: url）
- 页面标题 `TARGET-072`，heading 节点 index 1 —— **标准 2 达成**（reason: title）
- 输入计数：**标准 3 未获确认**（`grounded: false`，reason: `non_adjacent_nodes`，model_met=true 但 grounding 失败）

**关于输入计数节点范围（如实报告，不编造）**
- 宿主回执中，`StaticText` 证据被拆为两个不相邻节点：index 4 `「目标页输入事件：」`（name_chars=8）与 index 8 `「0」`（name_chars=1）
- 回执给出的 `observed_page.node_count=10`，但 `nodes` 摘录只列出 index 1、4、8 三个节点；**index 5、6、7 等中间节点（含可能的空结构节点或 InlineTextBox 副本）未在回执中枚举，我无法如实提供其索引内容，也不予编造**
- 最新页面中不存在单节点完整原文「目标页输入事件：0」，这正是宿主判 `non_adjacent_nodes` 的原因

**结论**
- 一次单击真实投递并跳转成功，但第三项因节点不相邻未被判达成，预算随后耗尽熔断，`goal_achieved: false`
- 按约束"失败立即停止，不重试不补发"，本轮不再发起第二次调用