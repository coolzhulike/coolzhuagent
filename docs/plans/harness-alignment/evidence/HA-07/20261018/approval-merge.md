# HA07-02 / HA07-04 部分：并发 pending 与撤销世代

状态：**HA07-02 passed；HA07-04 只有审批消费前撤销世代一项，四个屏障未完成，仍 running。**

## HA07-02

- session scope 批准后，按 workspace/session/room/tool 清理并消费同工具并发 pending，返回 `additional_outcomes`；一次用户审批覆盖同工具同范围 pending。
- once scope 只消费当前 call_id，第二个逻辑调用保留 pending、目标文件不存在。
- 测试 `session_scope_approval_consumes_concurrent_pending_same_tool`：两个 pending，一次 session 批准后两个目标文件存在、`additional_outcomes.len()==1`、pending 清空。
- 测试 `once_approval_does_not_consume_second_logical_call`：一次批准后第一文件存在、第二文件不存在、第二 pending 仍在。
- 真实执行：`cargo test ...` 各 1 passed，exit 0；日志 `runs/approval-merge-01/session-scope.log`、`once.log`。

## HA07-04 部分

- PendingApprovalRecord 记录 `approval_epoch`；Full Access revoke 推进 workspace/session 世代。
- 测试 `approval_invalid_after_revocation_epoch`：入队后撤销，approve 返回 409，目标文件不存在。
- 测试 `revocation_during_pre_hook_blocks_target_effect`：pre-hook 运行中撤销，hook 结束后再次校验，目标文件不存在、outcome rejected；日志 `runs/revoke-during-hook-01/output.log`，1 passed。
- 已覆盖审批消费前和 hook 后两个时点；队列领取前和已开始动作的取消/未知效果仍未单独证明。不能标 HA07-04 passed。
- 日志 `runs/approval-merge-01/revoke.log`，1 passed，exit 0。

## 当前源码回归/构建

- `runs/merge-regression-01/`：124 passed / 0 failed / 0 ignored，186.72s，运行期间无源码变化。
- `runs/build-02/output.log`：`cargo build -p coolzhu-web-console --offline` exit 0。
- hook 复核在合并改动后重跑：`runs/hook-recheck-02/output.log`，1 passed，exit 0。

## 局限

- 没有真实模型/CLI/ACP 入口调用；测试为进程内审批 API 与真实工具执行。
- session 合并按工具范围消费，不区分不同参数；这与 session 授权现有 tool 级语义一致。
- 配置版本检查只覆盖 WorkspaceConfig.configuration_revision；房间/实例切换的细粒度版本仍未单独证明。
