先确认 coolzhu-agent 当前声明的工具。按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。BU-CANDIDATE-LOADING-20261005-loading-recover 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:49944/slow.html}`，`max_actions=2`
- 首轮回执 `running`（job_id `66f847d5…ced86`），对同一 job_id 调用 `computer_use_wait` 到达终态；未重复调用 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 2`，`steps_completed: 2`，supervisor `action_count: 2`，`replan_count: 0`，`no_progress_count: 0`
- 两次动作投递回执：`native-input:86614862…`（导航到 explicit.html）、`native-input:16f3fe43…`（单击确认新页面），每步前后均有 `native-ax` / `native-observation` 观察对

**页面事实（新鲜观察 generation 3，`freshness_confirmed: true`，终验观察 `6413f99d…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:49944/explicit.html` —— 标准 1 达成（reason: url）
- 页面标题：「模型接管目标-074」
- 节点 index 8：`StaticText: "EXPLICIT-PASSED-074"`（name_chars=19）—— 标准 2 达成（reason: node_text）
- viewport：`page_x 0.0，page_y 0.0`（无滚动）

**终态**
- `criteria_met: 2/2`，`ungrounded_positive_count: 0`
- 共两步动作：模型侧导航 → 单击确认；无输入文本、无滚动，无重试、无补发

验收通过：在 slow.html 加载未结束时，由模型自己的导航接管到 `explicit.html`，加载后单击确认，最新页面显示 EXPLICIT-PASSED-074，由最新页面真实节点核验，非以动作释放代替达成。