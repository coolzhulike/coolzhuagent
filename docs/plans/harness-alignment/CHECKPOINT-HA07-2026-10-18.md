# HA07-01 完成后的接替检查点

当前验收 **43/60**：HA00～HA07、HA09 accepted；HA08、HA10～HA12 pending。逐条状态唯一来源是 [state.json](state.json)。

## 已完成

- HA07-01：审批消费后、目标执行前再次运行真实 pre-hook；hook 拒绝/未知时不执行，post-hook 只记录已发生效果。
- `tool_dispatch_service::run_approval_hook` 复用启动时 HookRunner，不新增用户审批、不新增审核模型。
- 测试 `approved_tool_rechecks_pre_hook_and_does_not_execute_on_deny`：marker 1 行、目标文件不存在、outcome rejected。
- HA07 改动后重跑 124 项定向回归：124 passed / 0 failed / 0 ignored，191.35 秒，运行期间无源码变化。
- `cargo build -p coolzhu-web-console --offline` exit 0。

证据：`evidence/HA-07/20261018/approval-hook.md`、`runs/hook-recheck-01/output.log`、`runs/current-regression-01/`、`runs/build-01/output.log`；当前证据 ID：`E-HA07-01-HOOK-20261018`、`E-HA07-CURRENT-WEB-20261018`、`E-HA07-CURRENT-BUILD-20261018`、`E-HA07-CURRENT-REVIEW-20261018`。

## HA07-02/04 追加

- session scope 批准会清理并消费同 workspace/session/room/tool 的并发 pending，返回 `additional_outcomes`；once 只消费当前 call_id。
- Pending 记录 `config_revision`/`approval_epoch`，消费时校验；Full Access revoke 推进撤销世代，旧批准返回 409。
- 实跑：session 合并、once 不越权、撤销世代各 1 passed；合并改动后 124 项回归 124 passed/0 failed，生产构建 exit 0。
- 证据：`evidence/HA-07/20261018/approval-merge.md`、`runs/approval-merge-01/`、`runs/merge-regression-01/`、`runs/build-02/`。

## HA07-03/05 追加

- Pending 保存 targets/required permission；消费时重新计算，变化返回 409。
- pending 清空（模拟重启）或 TTL 过期后 approve 404，目标不执行。
- 证据：`evidence/HA-07/20261018/scope-lifecycle.md`、`runs/scope-lifecycle-01/`、`runs/final-regression-02/`、`runs/build-04/`。

## 未完成

- HA07-03：hook 无 input 改写通道；范围失效通过 pending 范围变更模拟，未做真实 hook 改写。
- HA07-04：审批/hook/队列/领取资格四个撤销屏障和实际效果计数。
- HA07-05：拒绝/超时/重启旧 grant、CLI/ACP 回归。
- HA08 依赖 HA07，未开始。

## HA07-04 已完成

- 一次性批准使用 call_id 一次性 runtime 授权，不写 30 分钟 session grant。
- 真实 PowerShell 目标启动后撤销：approval outcome=Ok，finished 文件存在；效果保留。
- hook 中撤销的 primary/队列 extra 均 rejected；清空/TTL 过期 pending 404。
- 证据：`evidence/HA-07/20261018/running-effect.md`、`runs/final-barriers-02/`、`runs/final-regression-04/`、`runs/build-05/`。

## 下一步

HA07 accepted，开始 HA08 Web 有限并发：安全读并行、写/未知有序、资源资格、取消/失败收尾；随后 HA10，最后 HA11/HA12。

## 在途

无在途命令、无未确认效果；未安装/发布、未调用真实模型。
