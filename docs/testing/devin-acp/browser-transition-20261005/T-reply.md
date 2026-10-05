我先确认 coolzhu-agent 声明的工具，然后按正式六项提交一次 `computer_use_perform`。回执为 running，等待同一 job_id 直到终态。CU-PAINT-20261005-T 结果：**未通过**，依据本轮宿主真实终态回执报告。

**调用**
- `computer_use_perform`（coolzhu-agent），1 次，`surface=desktop`，`target={application: mspaint.exe}`，`max_actions=2`
- 首轮回执 `running`，对同一 job_id 调用 `computer_use_wait` 第 2 次到达终态；未重复调用 perform

**终态回执**
- `status: failed`，`stage: verification`，`goal_achieved: false`
- 错误：`planner_backend_unavailable` — "planner request task ended without a response"，`retryable: true`，`retry_owner: system`
- `attempts: 2`，`steps_completed: 2`，supervisor `action_count: 2`，`no_progress_count: 1`，`circuit_open: false`

**证据链（回执原文）**
- 两笔真实 drag 笔划均投递并释放：
  - `native_stroke:window-canvas:64307c8:points=5:duration_ms=800:released`（第一笔）
  - `native_stroke:window-canvas:64307c8:points=5:duration_ms=800:released`（第二笔）
- 每笔前后均有真实原图对比（sha256 `40b03533…` → `4fa07056…` → `7eac4cb6…`），两次 `image_changed: true`——画布确实产生了两处新笔迹
- 视觉核验：`visual_verification generation=2: image_changed=true: criteria_met=0/2`——两项验收标准（第一个闭合菱形清楚可见、最新原图同时可见两个分离闭合菱形）均未获确认
- 最终失败发生在 verification 阶段：规划器请求任务无响应结束（后端不可用），而非动作未投递

**结论**
- 两笔五点闭合 drag 真实执行并释放、画布有真实新笔迹，但宿主视觉核验判 `criteria_met=0/2`，且终局验收因规划器后端无响应而失败
- `retry_owner: system` 属系统侧可重试错误，但按约束"失败取消或未知立即停止不补发不重试"，本轮不再发起第二次调用；不凭释放或 image_changed 报完成