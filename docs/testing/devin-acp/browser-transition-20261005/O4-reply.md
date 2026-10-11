我先确认 coolzhu-agent 当前声明的工具，然后按正式六项提交一次 `computer_use_perform`。BU-TRANSITION-20261005-O4 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:52930/popup-source.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`no_progress_count: 0`，`replan_count: 0`
- 单击投递回执：`native-input:6ee49aee24f0202918b79616a1527679`

**页面事实（新鲜观察，generation 2，`freshness_confirmed: true`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:52930/popup-target.html`（window.open 产生的新页面在内置浏览器中显示，viewport 446×578）
- 页面标题：「新窗口目标」
- 页面节点：`heading: "POPUP-TARGET-20261005"`（index 1）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`（URL 精确匹配 popup-target.html + 页面显示 POPUP-TARGET-20261005），`ungrounded_positive_count: 0`
- 仅来源页一次单击，无 navigate、无滚动、无输入，未向目标页发送任何输入，无重试、无补发

验收通过：一次单击后 window.open 链接在内置浏览器中打开 popup-target.html 并显示 POPUP-TARGET-20261005，由最新页面真实事实核验，非以释放代替达成。