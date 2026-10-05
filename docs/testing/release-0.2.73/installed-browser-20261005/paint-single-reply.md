先确认 coolzhu-agent 当前声明的工具。CU-INSTALLED-073-SINGLE 完成，依据本轮宿主真实终态回执报告：

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=desktop`，`target={application: mspaint.exe}`，`max_actions=1`
- 首轮回执 `running`（job_id `2c75e675…`），对同一 job_id 调用 `computer_use_wait` 1 次到达终态；未重复调用 perform

**执行**
- `status: succeeded`，`stage: terminal`，`goal_achieved: true`
- `attempts: 1`，`steps_completed: 1`，supervisor `action_count: 1`，`replan_count: 0`，`no_progress_count: 0`
- 真实 drag 笔划投递回执：`native_stroke:uia-eee5940e25c5adae:points=5:duration_ms=1200:released` —— 五点一笔约 1200ms，符合约束

**证据链（回执原文）**
- 前图 `10f3d458…` → 后图 `e6e9e6f1…`，`image_changed: true`
- 坐标映射：`frame_binding: screen_rect=0,61,2560,1152: canvas_rect=511,364,1536,779: container_inside_image=true`（按 UIA 画布 rect 定位，非全图比例）
- UIA 快照 `hwnd-64307c8`（elements 175 → 176），观察窗口 `105056200`（uia_win32_no_synthetic_input）
- 视觉核验：
  - generation=1（run_start 基线）：`geometry_comparable=true`
  - generation=2：`image_changed=true: criteria_met=2/2`

**终态**
- `criteria_met: 2/2`，notes：「最新原图逐项确认目标已达成」——本轮新增短 W 折线、完全在白色画布内且与旧图分离
- 未擦除、未保存、未改设置，无重试、无补发

验收通过：一笔五点 W 折线新增于白画布留白内，由前后真实原图对照核验，非以释放或 image_changed 代替达成。按任务限定，本任务仅核执行链路与运行泛光，不作为完整人物验收。