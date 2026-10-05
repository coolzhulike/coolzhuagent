我先确认 coolzhu-agent 声明的工具，然后按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。CU-PAINT-20261005-U 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=desktop`，`target={application: mspaint.exe}`，`max_actions=2`
- 首轮回执 `running`，对同一 job_id 调用 `computer_use_wait` 2 次到达终态；未重复调用 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 2`，`steps_completed: 2`，supervisor `action_count: 2`，`no_progress_count: 0`，`replan_count: 0`

**两笔真实 drag 回执**
- 笔 1：`native_stroke:window-canvas:64307c8:points=5:duration_ms=800:released` —— 前图 `4af4273a…` → 后图 `ed8acf0e…`，`image_changed: true`
- 笔 2：`native_stroke:uia-eee5940e25c5adae:points=5:duration_ms=800:released` —— 前图 `ed8acf0e…` → 后图 `e998cfd7…`，`image_changed: true`

**视觉核验逐代结果**
- generation=1（run_start 基线）：`image_changed=false`
- generation=2：`image_changed=true: criteria_met=1/2`（第一个闭合菱形确认、在白画布留白内）
- generation=3：`image_changed=true: criteria_met=2/2`（最新原图同时可见两个新增且分离的小闭合菱形）

**终态**
- `criteria_met: 2/2`，notes：「最新原图逐项确认目标已达成」
- 两笔各 5 点闭合、约 800ms，位于画布底部留白；未擦除、未保存、未改设置，无重试、无补发

验收通过：本轮新增两个分开的闭合菱形，由前后真实原图对照核验，非以释放或 image_changed 代替。