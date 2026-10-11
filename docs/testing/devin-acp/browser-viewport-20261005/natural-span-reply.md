先确认 coolzhu-agent 声明的工具。按正式六项提交一次 `computer_use_perform`。BU-CANDIDATE-PHASE2-natural-span 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/source.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，`replan_count: 0`，`no_progress_count: 0`
- 单击投递回执：`native-input:55531a9e460ccd4c72f5ee6dd01c1e48`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`，终验观察 `1807a2b7…`）**
- 最新页面 URL：`http://127.0.0.1:51866/target.html` —— 标准 1 达成（reason: url）
- 节点 index 1：`heading "TARGET-072"`（name_chars=10）—— 标准 2 达成
- 输入计数按相邻文本段判定 `grounded: true`，reason: `adjacent_text`，节点范围 **index 7 → 8**：
  - index 7：`InlineTextBox`「目标页输入事件：」（name_chars=8，`inline_has_static_text: true`）
  - index 8：`StaticText`「0」（name_chars=1）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**如实说明**
- 完整原文「目标页输入事件：0」在页面上由相邻两节点（7、8）拼接呈现，非单一节点；宿主本轮以 `adjacent_text` 跨度方式判定成立，这与上轮 non_adjacent_nodes 判负不同

**终态**
- `criteria_met: 3/3`，`ungrounded_positive_count: 0`
- 仅一次单击，无 navigate、无输入、无滚动，目标页未再操作，无重试、无补发

验收通过：单击后自然跳转到 target.html 显示 TARGET-072，「目标页输入事件：0」经相邻节点 7–8 逐字核验，非以输入释放代替达成。