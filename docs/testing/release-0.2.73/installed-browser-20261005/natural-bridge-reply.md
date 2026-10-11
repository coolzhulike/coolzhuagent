按正式六项提交一次 `computer_use_perform`。BU-INSTALLED-073-PHASE2-natural-bridge 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:64415/source.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait；未重复提交 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`replan_count: 0`，`no_progress_count: 0`
- 单击投递回执：`native-input:ac282321d0cc3cc33e84627ec6db32c6`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `08ebe10d…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:64415/target.html` —— 标准 1 达成（reason: url）
- 节点 index 3：`StaticText: "TARGET-072"`（name_chars=10）—— 标准 2 达成（reason: node_text）
- 输入计数：完整原文「目标页输入事件：0」由两个 **StaticText** 原文节点拼接，reason: `bridged_text`，`node_indices: [4, 8]`：
  - index 4：StaticText「目标页输入事件：」（name_chars=8）
  - index 8：StaticText「0」（name_chars=1）
- span 中间节点已如实列出：index 5（空 generic，blank）、index 6–7（InlineTextBox 重复片段，name_chars=10、8，未选入 grounding）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 3/3`，`ungrounded_positive_count: 0`
- 仅一次单击，无 navigate、无输入、无滚动，目标页未再操作，无重试、无补发

验收通过：单击后自然跳转到 `http://127.0.0.1:64415/target.html` 显示 TARGET-072，「目标页输入事件：0」经 StaticText 节点 4 与 8 桥接核验（中间 InlineTextBox 副本未计入），非以输入释放代替达成。