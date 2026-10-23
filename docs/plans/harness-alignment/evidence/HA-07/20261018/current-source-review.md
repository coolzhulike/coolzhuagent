# 当前源码复核（HA07-01 改动后，对既有通过项的适用性）

本次新增源码：

- `modules/gui-web/packages/web-console/src/tool_dispatch_service.rs`：新增 `run_approval_hook`，复用既有 HookRunner；无 hook 配置时直接 allow。
- `modules/gui-web/packages/web-console/src/main.rs`：`execute_approved_pending_record` 在审批消费后、目标执行前运行 pre-hook；拒绝/未知则不执行；执行后运行 post-hook，仅写 evidence。

后续 HA07-03/05 追加：pending 保存规范化 targets 与 required permission，消费时重新计算；缺失/过期 pending 清理后 404。审批消费者改动仍不改变普通聊天/HA01～HA06/HA09 语义。

后续 HA07-02/04 追加：session scope 清理并消费同 workspace/session/room/tool 的并发 pending（`additional_outcomes`）；一次性批准不清理第二个；pending 记录配置版本与撤销世代，消费时校验；Full Access revoke 推进世代；审批消费前和 pre-hook 后各检查一次，hook 期间撤销会阻止目标执行。这些仍限定在审批消费者，不改变 HA00～HA06/HA09 的既有语义。

这些改动位于审批消费者，不改变 HA00 四路径、HA01～HA06 的提示词/连续性/暴露/绑定/等待/恢复语义，也不改变 HA09 的 Command 回执/验证消费者。旧结论仍适用；当前行为证据改用 `HA-07/20261018/runs/merge-regression-01` 的新执行：

- 124 项定向回归通过、0 失败、0 忽略，191.35 秒，`source_changed_during_run=[]`。
- 生产 `coolzhu-web-console` 构建 exit 0（`runs/build-01/output.log`）。

旧 E-HA09-04-FINAL/E-HA09-04-BUILD/E-HA00-HA09-REVIEW-R2 保留历史，不修改哈希；当前通过项改用本轮新证据。该复核不替代真实测试。
