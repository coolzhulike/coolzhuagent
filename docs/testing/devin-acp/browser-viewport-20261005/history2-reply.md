先确认 coolzhu-agent 当前声明的工具。按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id 至终态。BU-CANDIDATE-PHASE2-history2 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:51866/history.html}`，`max_actions=3`
- 首轮回执 `running`，对同一 job_id 调用 `computer_use_wait` 2 次到达终态；未重复调用 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 3`，`steps_completed: 3`，supervisor `action_count: 3`，`no_progress_count: 0`，`replan_count: 0`
- 三次单击投递回执：`native-input:8aa4fb74…`（进入第二阶段）、`49f9e167…`（历史后退）、`4f51fe59…`（历史前进）

**页面事实（新鲜观察 generation 4，`freshness_confirmed: true`，终验观察 `64e49b68…`）**
- 最新页面 URL 逐字引用：`http://127.0.0.1:51866/history.html?stage=2` —— 标准 1 达成（reason: url）
- 节点 index 8：`StaticText: "阶段：第二阶段"` —— 标准 2 达成（reason: node_text）
- 节点 index 9：`StaticText: "操作顺序：起点→进入第二阶段→历史后退→历史前进"` —— 标准 3 达成（reason: node_text）
- 页面标题 `HISTORY-072`，viewport `page_y 0.0`（无滚动）

**终态**
- `criteria_met: 3/3`，`ungrounded_positive_count: 0`
- 仅三次单击，无 navigate、无输入、无主动滚动，无重试、无补发

验收通过：三次单击依次进入第二阶段、后退、前进，最终回到 `history.html?stage=2` 并显示「阶段：第二阶段」及完整操作顺序，由最新页面真实节点核验，非以输入释放代替达成。