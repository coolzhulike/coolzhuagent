# Goal 阶段工程固定真实验收

2026-09-27，`tmp/s25_goal_workspace_pin_e2e.py` 在独立工作区、随机本机端口启动 Web 与假模型，并创建合成的 commander、implementer、聊天室和一个 Goal 阶段。所用 Web 可执行文件 SHA-256 为 `dfd39612cd2d689e6cf4e87de9b0c421682d1be33137d854a4ce8f5d2479d8ce`；没有请求用户 8765 服务或云模型。

测试在实际 implementer 模型请求进入假模型后暂停响应，此时 Goal 阶段已经接纳并正在执行。对 `/api/workspace` 发送切换请求返回 **409**，读回仍是原工作区。放行假模型后，阶段执行 HTTP 返回 **200**；再次发送相同切换请求返回 **200**，工作区读回已变更。结果见 [脱敏 JSON](s25-goal-workspace-pin-result.json)，原始隔离日志在 `tmp/s25-goal-pin-eeaaaa6a06/`。

这项验收针对 Goal `run_goal_phase_once` 生命周期的工程固定；阶段内子 Agent 的用量归属另由宿主定向测试及最终 CLI/轨迹回归核对，不借此单例宣称全部闭环。
