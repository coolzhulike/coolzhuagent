先确认 coolzhu-agent 当前声明的工具。按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。继续等待同一 job_id。继续等待同一 job_id。BU-INSTALLED-074-20261005-zoom-form2 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=browser`，`target={url: http://127.0.0.1:53414/form.html}`，`max_actions=6`
- 首轮回执 `running`（job_id `466036bb…51dee`），对同一 job_id 调用 `computer_use_wait` 3 次到达终态；未重复调用 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 5`，`steps_completed: 5`，supervisor `action_count: 5`，`replan_count: 0`，`no_progress_count: 0`，`circuit_open: false`
- 五次输入动作投递回执：`native-input:b94e61f8…`、`ed4bc019…`、`5868c97e…`、`d7ee7c7d…`、`1397433f…`（点击姓名框、text 输入、滚动至控件 in_viewport、勾选确认、点击提交），每步前后均有 `native-ax` / `native-observation` 观察对

**页面事实（新鲜观察 generation 6，`freshness_confirmed: true`，终验观察 `2003a8a3…`）**
- 最新页面 URL：`http://127.0.0.1:53414/form.html`，标题「缩放表单-074」
- 节点 index 12：`StaticText: "FORM-PASSED-074"`（name_chars=15，附 index 18 InlineTextBox 副本）—— 标准 1 达成（reason: `node_text`）
- viewport：`page_x 0.0，page_y 796.27`——滚动后到达目标控件区，符合约束

**终态**
- `criteria_met: 1/1`（最新页面显示 FORM-PASSED-074），`ungrounded_positive_count: 0`
- 无 navigate、无 Enter、无重复输入姓名，无重试、无补发

验收通过：填写 SWE2-074、持续滚动至确认与提交控件可见后勾选并提交，最新页面显示 FORM-PASSED-074，由最新页面真实节点核验，非以动作释放代替达成。