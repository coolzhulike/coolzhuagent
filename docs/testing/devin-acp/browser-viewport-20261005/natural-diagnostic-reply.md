按正式六项提交一次 `computer_use_perform`。BU-CANDIDATE-PHASE2-natural-diagnostic 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/source.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`no_progress_count: 0`，`replan_count: 0`
- 单击投递回执：`native-input:47dc77e62624bb3137c17d894df8b117`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `9d90b0e5…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:51866/target.html` —— 标准 1 达成（reason: url）
- 页面节点 index 3：`StaticText: "TARGET-072"`（name_chars=10）—— 标准 2 达成
- 输入计数：页面节点 index 8：`StaticText: "0"`（name_chars=1），宿主本轮判 `grounded: true` —— 标准 3 达成

**如实说明（按本任务诊断要求）**
- 宿主回执的节点摘录中，「目标页输入事件：0」**不是单个完整原文节点**：标签文本「目标页输入事件：」未列入摘录节点，仅数字 `0`（index 8）被选为 grounding 节点，reason 记为 `title`
- `observed_page.node_count=10`，摘录仅列 index 3、8；index 4–7 等中间节点未在回执中枚举，我不予编造
- 与上一轮 natural-range 的 `non_adjacent_nodes` 判负不同，本轮宿主对该标准判 `grounded: true`，`criteria_met: 3/3`，`ungrounded_positive_count: 0`

**终态**
- 仅一次单击，无 navigate、无输入、无滚动（`page_x 0.0，page_y 0.0`），未向目标页发送输入，无重试、无补发

验收通过：单击后自然跳转到 target.html 显示 TARGET-072，目标页输入事件计数为 0（index 8），`goal_achieved: true`，非以输入释放代替达成。