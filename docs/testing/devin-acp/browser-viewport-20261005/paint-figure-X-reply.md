先确认 coolzhu-agent 当前声明的工具。按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id 至终态。继续等待同一 job_id。CU-CANDIDATE-20261005-X 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=desktop`，`target={application: mspaint.exe}`，`max_actions=6`
- 回执 `running`（job_id `10c492ea…86de5`），对同一 job_id 调用 `computer_use_wait` 4 次后到达终态；未重复调用 perform

**终态回执**
- `status: blocked`，`stage: observation`，`goal_achieved: false`
- 错误：`target_ambiguous` — "UIA target is ambiguous"，`retryable: false`，`retry_owner: model`
- `attempts: 4`，`steps_completed: 4`（6 步预算中执行 4 步后阻塞于观察阶段）

**证据链（回执原文节选）**
- 三笔真实 drag 笔划均投递并释放（第 4 次 attempt 在观察/预检阶段阻塞）：
  - `native_stroke:uia-eee5940e25c5adae:points=5:duration_ms=1200:released`（前图 `fb1d5c19…` → 后图 `25fa4f8e…`，`image_changed: true`）
  - `native_stroke:uia-eee5940e25c5adae:points=13:duration_ms=800:released`（→ `c2610dfb…`，`image_changed: true`）
  - `native_stroke:uia-eee5940e25c5adae:points=14:duration_ms=800:released`
- 视觉核验逐代结果：
  - generation=1（run_start 基线）：`geometry_comparable=true`
  - generation=2：`image_changed=true: criteria_met=1/3`
  - generation=3：`image_changed=true: criteria_met=1/3` —— 仅一项标准获确认，双眼+笑嘴、底边下两条腿脚未达
- 坐标映射采用 UIA 画布 rect（canvas_rect=511,364,1536,779），UIA 快照 `hwnd-64307c8`

**结论**
- 3 笔真实笔划投递并释放、画布图像逐步有变化，但三项标准终态仅 `1/3`，且第 4 步因 UIA 目标歧义（`target_ambiguous`）阻塞，`goal_achieved: false`
- 按约束"失败取消或未知立即停止不重试不补发"，本轮不再发起第二次调用；不凭释放或 image_changed 报完成，不把旧人物或两个小菱形当完成