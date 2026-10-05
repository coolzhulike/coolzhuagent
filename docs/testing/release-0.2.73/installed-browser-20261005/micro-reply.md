先确认 coolzhu-agent 声明的工具。按正式六项提交一次 `computer_use_perform`。BU-INSTALLED-073-MICRO 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:59803/source.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait；未重复提交 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`replan_count: 0`，`no_progress_count: 0`
- 单击投递回执：`native-input:ecd38dd394d42e7a2bd4c43603243615`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `02e0104548…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:59803/target.html` —— 标准 1 达成（reason: url）
- 页面标题：「按下导航目标」
- 节点 index 1：`heading "DOWN-NAV-TARGET-073"`（name_chars=19）—— 标准 2 达成（reason: node_text）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`，`ungrounded_positive_count: 0`
- 仅一次单击，无 navigate、无输入、无滚动，目标页未再操作，无重试、无补发

**关于微区间竞争**：本轮验收仅依据实际页面事实——单击后导航到 target.html 并显示 DOWN-NAV-TARGET-073；回执未单独给出 pointerdown 事件级判定，故仅如实报告导航达成，不据此宣称竞争覆盖。

验收通过：由最新页面真实节点核验，非以输入释放代替达成。