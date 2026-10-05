先确认 coolzhu-agent 声明的工具。按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。继续等待同一 job_id。BU-INSTALLED-073-PHASE2-form3 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:64415/form.html}`，`max_actions=6`
- 首轮回执 `running`，对同一 job_id 调用 `computer_use_wait` 2 次到达终态；未重复调用 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 5`，`steps_completed: 5`，supervisor `action_count: 5`，`no_progress_count: 0`，`replan_count: 0`
- 五次输入动作投递回执：`native-input:958150ac…`、`9eb19251…`、`3348f452…`、`6995f85f…`、`770c0651…`（点击姓名框、text 输入、滚动、勾选复选框、点击提交）

**页面事实（新鲜观察 generation 6，`freshness_confirmed: true`，终验观察 `b97357b0…`）**
- 最新页面 URL：`http://127.0.0.1:64415/form.html`，标题「FORM-072」
- 节点 index 10：`StaticText: "FORM-PASSED-072"`（name_chars=15）—— 标准 1 达成（reason: node_text）
- viewport：`page_x 0.0，page_y 406.0`——含一次 scroll，符合约束允许的 scroll

**终态**
- `criteria_met: 1/1`（最新页面显示 FORM-PASSED-072），`ungrounded_positive_count: 0`
- 无 navigate、无 Enter、无额外文本，无重试、无补发

验收通过：填写 SWE2-072、滚动勾选确认、提交本地表单后页面显示 FORM-PASSED-072，由最新页面真实节点核验，非以输入释放代替达成。