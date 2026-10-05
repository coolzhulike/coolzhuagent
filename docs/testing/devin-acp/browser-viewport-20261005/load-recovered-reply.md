先确认 coolzhu-agent 声明的工具。按正式六项提交一次 `computer_use_perform`。BU-LOAD-20261005-recovered 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:53227/ready.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`no_progress_count: 0`，`replan_count: 0`
- 单击投递回执：`native-input:7a42ffb5adf9315e2f0011ba99a19eae`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `ee8024e0…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:53227/done.html` —— 标准 1 达成（reason: url）
- 页面标题：「DONE-20261005」
- 节点 index 2：`StaticText: "DONE-20261005"`（name_chars=13）—— 标准 2 达成（reason: node_text）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`，`ungrounded_positive_count: 0`
- 仅一次单击，无 navigate、无输入、无滚动，无重试、无补发

验收通过：一次单击后网页自然跳转到 done.html 并显示 DONE-20261005，由最新页面真实事实核验，非以动作释放代替达成。