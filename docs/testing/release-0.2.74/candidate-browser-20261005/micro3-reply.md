BU-CANDIDATE-LOADING-20261005-micro3：按正式六项提交一次 `computer_use_perform`。BU-CANDIDATE-LOADING-20261005-micro3 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:49944/micro.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait；未重复提交 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`replan_count: 0`，`no_progress_count: 0`，`circuit_open: false`
- 单击投递回执：`native-input:3196ef15ef579ee89138d80d29bfe509`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `981db507…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:49944/micro-target.html` —— 标准 1 达成（reason: url）
- 页面标题：「文档替换目标-074」
- 节点 index 6：`StaticText: "MICRO-TARGET-074"`（name_chars=16，`inline_has_static_text: false`）—— 标准 2 达成（reason: `node_text`）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`，`ungrounded_positive_count: 0`
- 仅一次单击，无 Navigate、无 text、无 scroll、无 keys；页面变化来自真实网页事件，无重试、无补发

验收通过：一次单击后自然导航到 `micro-target.html` 并显示 MICRO-TARGET-074，由最新页面真实节点核验，非以动作释放代替达成。