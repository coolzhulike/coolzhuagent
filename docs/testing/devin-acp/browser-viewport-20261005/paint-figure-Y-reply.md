先确认 coolzhu-agent 当前声明的工具。按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id。继续等待同一 job_id。CU-CANDIDATE-20261005-Y 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=desktop`，`target={application: mspaint.exe}`，`max_actions=6`
- 回执 `running`（job_id `f818edea…a3345`），对同一 job_id 调用 `computer_use_wait` 多次后到达终态；未重复调用 perform

**终态回执**
- `status: blocked`，`stage: supervisor`，`goal_achieved: false`
- 错误：`budget_exhausted` — "computer-use action budget exhausted"，`retryable: false`，`retry_owner: none`
- `attempts: 6`，`steps_completed: 6`——6 步动作预算全部用满后熔断
- terminal_reason 原文：「最新图像尚未提供所有目标达成的证据」，`verification_is_last_observation: true`

**证据链（回执原文节选）**
- 已投递并确认释放的真实 drag 笔划（节选内可见 3 笔）：
  - `native_stroke:uia-eee5940e25c5adae:points=5:duration_ms=2500:released`（前图 `c1d66774…` → 后图 `1d138562…`，`image_changed: true`）
  - `native_stroke:uia-eee5940e25c5adae:points=13:duration_ms=800:released`（→ `210f45bf…`，`image_changed: true`）
  - `native_stroke:window-canvas:64307c8:points=9:duration_ms=800:released`
- 视觉核验逐代结果：
  - generation=1（run_start 基线）：`geometry_comparable=true`
  - generation=2：`image_changed=true: criteria_met=1/3`
  - generation=3：`image_changed=true: criteria_met=1/3`——仅一项标准获确认
- 坐标映射：canvas_rect `511,364,1536,779`，UIA 快照 `hwnd-64307c8`；观察窗口 `105056200`（uia_win32_no_synthetic_input）

**结论**
- 6 步预算用满，多笔真实笔划投递并释放、画布图像逐步有变化，但三项标准在终态仅 `1/3`——矩形身体、双眼+笑嘴、底边下两条分开腿脚未全部获确认，`goal_achieved: false`
- 按约束"失败取消或未知立即停止不重试不补发"，本轮不再发起第二次调用；不凭释放或 image_changed 报完成，不把旧人物或两个小菱形当完成