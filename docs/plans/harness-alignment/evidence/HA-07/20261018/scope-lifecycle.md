# HA07-03 / HA07-05：范围失效与生命周期

## HA07-03

- PendingApprovalRecord 保存审批时规范化 `targets`（路径+访问类型）和 required permission；消费时重新计算。
- workspace 路径、WorkspaceConfig.configuration_revision、approval_epoch、required permission、targets 任一不匹配即 409，不执行。
- 测试 `approval_invalid_after_target_or_permission_scope_change`：目标集合被改、权限 required 被改，分别 409，目标文件不存在。
- 局限：HookRunner 当前没有“修改工具输入”的返回通道；本测试通过修改 pending 范围模拟 hook 后的范围变化，不是 hook 实际改写命令。房间/实例切换由 workspace/config/epoch 覆盖，未单独做 UI 切换测试。

## HA07-05

- `missing_or_expired_pending_approval_cannot_execute_after_restart`：
  - 清空 pending（模拟重启）后 approve -> 404，目标不存在；
  - 将 `created_at` 推过 5 分钟 TTL，approve 清理后 -> 404，目标不存在。
- 既有 reject 测试仍覆盖拒绝不授权。
- 最新 124 项回归含 Devin ACP、OpenAI subscription、devin_acp bridge；CLI 未单独运行（审批消费者位于 web-console）。
- 局限：没有真实 CLI 进程测试；没有真实进程重启后读取磁盘旧 grant 的测试（当前 grant 为进程内状态）。

## 实跑

- `runs/scope-lifecycle-01/scope.log`：1 passed。
- `runs/scope-lifecycle-01/lifecycle.log`：1 passed。
- `runs/scope-lifecycle-01/revoke-hook.log`：1 passed。
- `runs/final-regression-02/`：124 passed / 0 failed，190.08s，source_changed_during_run=[]。
- `runs/build-04/output.log`：生产 web-console 构建 exit 0。
