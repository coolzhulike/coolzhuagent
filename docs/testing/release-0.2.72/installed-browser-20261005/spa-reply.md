BU-INSTALLED-072-spa 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/spa.html}`，`max_actions=1`
- 直接返回终态，未进入 running，无需 wait

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`no_progress_count: 0`，`replan_count: 0`
- 单击投递回执：`native-input:32d841730ed3e497eb5b8c6e795e5e72`

**页面事实（新鲜观察 generation 2，`freshness_confirmed: true`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:51866/spa-target.html`（同文档 SPA 跳转，地址已变化）
- 页面标题：「SPA-SOURCE-072」
- 页面节点：`heading: "SPA-TARGET-072"`（index 1）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`（URL 精确匹配 spa-target.html + 页面显示 SPA-TARGET-072），`ungrounded_positive_count: 0`
- 仅来源页一次单击，无 navigate、无输入、无滚动，无重试、无补发

验收通过：一次单击后 SPA 同文档跳转到 spa-target.html 并显示 SPA-TARGET-072，由最新页面真实事实核验，非以输入释放代替达成。