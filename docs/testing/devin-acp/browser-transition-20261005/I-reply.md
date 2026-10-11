我先确认 coolzhu-agent 声明的工具，然后提交一次 `computer_use_perform`。BU-TRANSITION-20261005-I 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:52616/source.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`no_progress_count: 0`，`replan_count: 0`
- 点击投递回执：`native-input:710acc8fc49379a2eb00f2c61164af9a`

**页面事实（新鲜观察，generation 2，`freshness_confirmed: true`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:52616/target.html`
- 页面标题：「在途导航验收目标」
- 页面节点：`StaticText: "目标标记 TARGET-NAV-20261005"`（index 6）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`（URL 精确匹配 + 页面显示 TARGET-NAV-20261005），`ungrounded_positive_count: 0`
- 仅一次单击，无主动导航、无滚动、无输入文字，未向目标页发送任何输入，无重试、无补发

验收通过：来源页一次点击后网页自然跳转到 target.html 并显示 TARGET-NAV-20261005，由最新页面真实事实核验，非以释放代替达成。