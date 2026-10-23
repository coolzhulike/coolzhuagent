# HA-04 实施接替记录

这是本轮实施检查点，实时状态和在途命令仍以 `state.json` 为准。用户已授权继续实施及逐条验收，未授权安装、发布或新增付费模型实验。

## 已完成的源码与行为验证

- 原实际发送的 `SentRequest` 随 SSE 首响应传入共享循环；普通入口使用请求级 collector，跨子任务 spawn 显式传递。保留 `started` 的运行接纳含义。
- adapter 在参数转换、覆盖后从实际 payload 生成摘要；沿已有 usage observer 的 logical_request_id/attempt_no 关联，写入 `request_binding_json`。请求正文与密钥不进入摘要。
- 宿主工具来源使用实际请求身份；重新采样和 Agent 子请求关联父请求，权限仍由当前执行门决定。
- Web 定向回归最新 `web-regression-03` **70 passed / 0 failed**；包含 HA-01～03、HA-04、ACP桥、订阅文本/能力边界、身份与根预算。
- adapter 新增测试 **2 passed**，既有测试 **5 passed**；另2项真实账户测试 ignored，不计通过。
- 固定 HA-00 清单在新目录实际恢复 **102项 / 0错误**；manifest和source.patch未修改。恢复说明修正固定提交和PowerShell二进制管道。
- 14个普通/SSE场景的原始HTTP、屏障顺序、接纳/执行/效果计数保存在 `evidence/HA-04/20261018/runs/web-regression-03/`；汇总为 `audit.json`。权限撤销可登记1条调用但执行器进入0，不能混同。

实现差异与文件哈希：`evidence/HA-04/20261018/implementation.patch`、`implementation-inputs.json`。它们相对于本轮开始的工作区，不是相对仓库HEAD；不能直接用来回滚其他人的改动。`tool_invocation_identity.rs` 另有来源注释更新。冻结基线与当前候选明确分开。

## 必须先核实的在途工作

此检查点初写时，CLI取消/期限回归正在编译（`cli-regression-01`，启动09:44:20 UTC，launcher PID9840，工具session82990）。**以state及receipt最终内容确认，不能把这段文字当作实时进程状态。**

- 日志：`tmp/ha04-20261018-runs/cli-regression-01/output.log`
- 回执：同目录 `receipt.json`；存在 `finished_at` 和真实 `exit_code` 才算结束。
- 只核对本任务PID、启动时间和命令，不全局杀cargo/rustc/python。
- 无用户系统上的未知业务效果；测试使用本地受控HTTP、临时workspace与独立DB。没有启动安装版或向8765写入。

## HA-04 关闭前的剩余步骤

1. CLI应实际匹配两项取消/期限测试，`2 passed / 0 failed`。失败则修复并保留旧回执，不用0匹配假通过。
2. 受影响crate真实离线构建：单任务、`CARGO_PROFILE_DEV_DEBUG=0`，构建web-console、llm-adapter、command-line；用 `tmp/ha04_run.py build-01 ...` 保存真实时间与输出。如果该标签已存在，先核对结果，不能覆盖或盲目重复。
3. `tmp/ha04_archive.py` 归档所有已完成回执；**只在HA-04源码尚未继续变化时运行**，避免把后续HA-05改动写入HA-04补丁。
4. 审查逐条合同与原始数据。`tmp/ha04_accept.py` 在全部结果和输入哈希吻合后生成验收说明并登记；它只做已审查结果的登记，不能代替人工判定。若当前state已完成HA-04，不再次运行。
5. 运行 `check_plan.py` 及 `--self-test`；只有五条成立且必要构建通过才标HA-04 accepted（累计24/60）。不用 `--complete`。

旧HA00～03证据的main.rs哈希已不匹配当前源码，原记录保持历史，不批量刷新。新回归覆盖HA01～03；HA00需新增冻结快照及调用链差异复核，登记脚本已准备。旧HA00文字3d1a…与后来输入d8a7…不一致须明确披露，不能再隐藏。

## HA-05 后续实施起点

HA-04验收后按依赖继续HA-05。`run_shared_model_tool_loop` 当前L1在相同参数第二次出现时直接说没有进展，未看实际结果。先移除这条错误断言；只对有真实失败证据的重复调用设置有记录的有限上限。成功读取、运行中查询和未知语义不得仅凭参数相同被迫换方法；结果变化允许继续，时间戳变化不重置失败计数。

HA05-01～04仍全部未验收：应补真实共享循环的同参数运行中→完成、仅时间戳变化的连续失败、外部状态变化后成功，以及等待取消/根期限的无后续派发证明。复用已有 `host_child_agent` 长等待与 `devin_acp/tool_wait.rs` 句柄，不自动重放未知工具或另建通用语义模型。HA06～10仍需各自实施，HA11真实模型15组配对仍缺模型/额度及费用授权。

2026-10-18本轮读取到共享周额度使用84%（约剩16%），不是保证剩余可用时长。每个结果及时写入state，未验证项不关闭；平台额度突然结束时从上述回执接替。
