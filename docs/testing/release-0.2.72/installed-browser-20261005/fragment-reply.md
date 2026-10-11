按正式六项提交一次 `computer_use_perform`。BU-INSTALLED-072-fragment 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/fragment.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`no_progress_count: 0`，`replan_count: 0`
- 单击投递回执：`native-input:420150f38cfc7ff492935961081e1bf0`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:51866/fragment.html#destination`（锚点已就位）
- 页面标题：「FRAGMENT-SOURCE-072」（同文档锚点跳转）
- 页面节点：`heading: "FRAGMENT-TARGET-072"`（index 3）及 `StaticText: "FRAGMENT-TARGET-072"`（index 6）
- viewport：`page_y 312.67`——浏览器因锚点自然滚动到目标段落，非主动滚动操作

**终态**
- `criteria_met: 2/2`（URL 精确匹配 `fragment.html#destination` + 页面显示 FRAGMENT-TARGET-072），`ungrounded_positive_count: 0`
- 仅一次单击，无 navigate、无输入、无主动滚动，无重试、无补发

验收通过：一次单击后网页跳转到 `#destination` 锚点并显示 FRAGMENT-TARGET-072，由最新页面真实事实核验，非以输入释放代替达成。