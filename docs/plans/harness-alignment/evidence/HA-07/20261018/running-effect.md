# HA07-04 已开始动作后的撤销：真实效果保留

## 实现

- 一次性批准消费时，用 call_id 一次性 runtime 授权令牌，而不是写入 30 分钟 session grant。
- `execute_runtime_tool_blocking_with_executor` 只在当前 call_id 消费该令牌；目标启动后撤销不会回滚已发生效果。
- 审批消费前、pre-hook 后仍执行撤销世代/作用域检查；动作已开始后不再把效果伪装成未执行。

## 验证

`revocation_after_target_start_keeps_recorded_effect`：

- 通过审批执行真实 PowerShell：`Set-Content started; Start-Sleep 2; Set-Content finished; exit 0`。
- 等待 `running-started.txt` 出现后，调用 Full Access revoke。
- approval outcome `status=Ok`；`running-finished.txt` 存在。
- 证明已开始动作的真实效果保留，撤销没有把结果改写成未执行。
- 输出：`runs/final-barriers-02/running-effect.log`，1 passed，exit 0。

另外：`revocation_during_hook_invalidates_queued_session_requests`（1 passed）证明 hook 中撤销会拒绝 primary + session 队列 extra；`missing_or_expired_pending_approval_cannot_execute_after_restart` 证明清空/TTL 过期后 404。

## 回归/构建

- `runs/final-regression-04/receipt.json`：124 passed / 0 failed / 0 ignored，188.21 秒，source_changed_during_run=[]。
- `runs/build-05/output.log`：`cargo build -p coolzhu-web-console --offline` exit 0。

## 局限

- 已开始动作的取消/未知效果以 PowerShell 自然完成为样例；未覆盖长时间外部服务不可撤销效果。
- 测试为进程内审批 API + 真实 PowerShell，不是真实模型调用。
