# HA10 起点：HA09-04 完成后的接替检查点

当前验收 **38/60**：HA00～HA06、HA09 已 accepted；HA10～HA12 仍 pending，HA07/HA08 是未完成必做项。逐条状态唯一来源是 [state.json](state.json)。

## 本轮完成

- HA09-04 已完成：真实 RegistryExecutor/PowerShell 产生 `executionReceipt`；Goal Command 按 call_id、命令、规范化 cwd、实际 shell、timeout、exit 0 消费可信回执；无回执/不匹配才启动 runtime。
- repair 阶段把 `command_verification` 持久化进 `repair_result`；verifier 从同阶段结构化回执复用，不丢回执后重复执行。
- 真实 token-aware finalizer 验证：phase 最终 `status=completed`、`verdict=pass`，`last_evidence.command_verification.receipts[0].id` 保留工具原始回执 ID，目标命令只启动一次。
- Goal Command 与 PowerShell 工具共用 `resolve_powershell_shell`（pwsh 优先、powershell 回退），实际解释器不同则不复用。
- 旧 `Vec<String>`、legacy 空结果、伪造/不匹配/取消/超时仍不能冒充 Command passed。

## 证据

- [HA09-04 验收记录](evidence/HA-09/20261018/ha09-04-acceptance.md)
- `evidence/HA-09/20261018/runs/diagnostic-01/`：有/无回执、command 不匹配的实际启动次数和 receipt ID。
- `evidence/HA-09/20261018/runs/repair-01/`：repair 持久化回执复用。
- `evidence/HA-09/20261018/runs/phase-diagnostic-01/`：真实 phase finalizer 最终状态。
- `evidence/HA-09/20261018/runs/ha09-04-regression-03/`：全量定向回归 124 passed / 0 failed，运行期间无源码变化。
- `evidence/HA-09/20261018/runs/build-02/output.log`：生产三 crate 离线构建 exit 0。

当前证据 ID：`E-HA09-04-FINAL-20261018`、`E-HA09-04-BUILD-20261018`、`E-HA00-HA09-REVIEW-20261018-R2`。旧 E-HA09-WEB/E-HA09-BUILD/E-HA00-HA09-REVIEW 保留为历史，不再作为当前通过声明的依据。

## 下一步

1. 开始 **HA-10 当前任务内有限复用**：复用 HA09 的合同/回执签名，按命令/cwd/shell、相关文件内容、测试/构建配置、锁文件和工具链判断；依赖不确定/网络外部状态不复用；相关输入变化即失效。
2. 完成后按依赖推进 **HA-07、HA-08**（不能跳过），再做 HA-11、HA-12。
3. HA-11 仍缺固定 provider/model/effort 与真实调用费用授权；不能用 mock 代替真实模型实验，也不能宣称 60/60。

## 在途与限制

- 无在途命令、无未确认效果；未安装/发布、未改变日常 8765 服务、未调用真实模型。
- 没有真实模型生成 tool call；HA09-04 是真实宿主进程 + 真实 Goal gate/finalizer 夹具，真实模型集成留给 HA-11。
- 未覆盖 pwsh 与 powershell 同时存在时的跨解释器复用；shell 不同即拒绝复用。
- 文件不自动唤醒其他模型；接替者从本文件和 state 继续。
