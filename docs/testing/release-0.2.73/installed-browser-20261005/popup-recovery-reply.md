我先确认 coolzhu-agent 当前声明的工具。按正式六项提交一次 `computer_use_perform`。BU-INSTALLED-073-POPUP-RECOVERY 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:50764/explicit.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait；未重复提交 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`replan_count: 0`，`no_progress_count: 0`
- 单击投递回执：`native-input:b705f9464467916ae98c3747ac6b706c`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `2699db8c…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:50764/explicit.html` —— 标准 1 达成（reason: url）
- 页面标题：「显式导航目标」
- 节点 index 6：`StaticText: "EXPLICIT-PASSED-073"`（name_chars=19）—— 标准 2 达成（reason: node_text）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`，`ungrounded_positive_count: 0`
- 仅一次单击，无 navigate、无输入、无滚动，未操作慢弹窗，无重试、无补发

验收通过：在当前显式导航目标页单击确认后，页面 URL 保持 `explicit.html` 并显示 EXPLICIT-PASSED-073，由最新页面真实节点核验，非以输入释放代替达成。