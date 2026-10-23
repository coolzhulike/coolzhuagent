# HA09 验证状态：接替检查点

当前验收 **37/60**：HA00～HA06为accepted；HA09-01/02/03/05为passed，HA09-04为pending，HA09整包仍in_progress。HA07、HA08、HA10～HA12仍pending。逐条状态唯一来源是[state.json](state.json)，验收合同没有缩减。PLAN第4节允许HA09独立提前，未绕过HA07/08。

## 已完成和真实证据

- HA04已按更新决策验收：保留早期started，request_bound在模型内容前到达，原始发送绑定贯穿共享循环；真实HTTP请求/重试/撤销及ACP、订阅、CLI相应入口回归已留证。详情在HA04归档，不能再沿用“started必须等待最终MessageRequest”的旧解释。
- HA05已验收：相同参数不再单独触发假停滞；真实失败的有限重复计数、宿主等待/取消/根预算回归通过。
- HA06已验收：明确终止与有副作用后的失败不再触发额外模型恢复；参数修复有一次上限；实际断流、post-hook/存储故障、取消及身份去重留有计数和效果证据。
- HA09本次将Command结果改为passed/failed/not_required/skipped/invalid/unable；真实命令回执进入token-aware phase finalizer。禁用门、空命令、读失败、旧空结果不再被当作通过。Goal专属提示由runtime负责验证，普通聊天路由保持原状。
- 当前源码合并回归 **121项通过、0失败、0忽略**，测试区间2026-10-18T11:00:40.914482+00:00至2026-10-18T11:15:01.703898+00:00；实际用例192.63秒。生产三crate offline build退出0，区间2026-10-18T11:15:16.278950+00:00至2026-10-18T11:25:39.912474+00:00。两次执行均无输入源码变化。
- [逐条验收](evidence/HA-09/20261018/acceptance.md)、[审计数据](evidence/HA-09/20261018/audit.json)、[测试原始日志](evidence/HA-09/20261018/runs/web-regression-02/output.log)、[构建回执](evidence/HA-09/20261018/runs/build-01/receipt.json)、[当前验收快照](evidence/HA-09/20261018/partial-state.json)。证据ID为E-HA09-WEB-20261018、E-HA09-BUILD-20261018、E-HA00-HA09-REVIEW-20261018。

历史失败保留：web-regression-01因测试误写super::windows_process_guard而编译失败，未执行测试；修复真实crate引用后第二轮通过。日志还保留DIAG路径告警及HA06预期故障panic，未删减；本次不宣称诊断日志持久性经过独立验证。

## 下一项：HA09-04，尚未实施

合同要求模型诊断已执行相同验证时，有效可信宿主回执由单一路径消费；无有效回执才由runtime执行，并记录实际启动次数。当前仍会重新执行Command，不能因提示词调整或相同命令字符串关闭该条。

1. 从main.rs::RegistryExecutor::execute和tool-registry的真实进程出口采集命令回执。ToolOutcome目前无所需字段；不能把stdout中的成功文字、空returnCodeInterpretation或模型提供的ID当事实。
2. runtime_tool_supervision会进入spawn_blocking，任何当前Goal/run收集上下文必须显式传递。限制于当前真实Goal/run/合同，不把子任务、旧阶段、其他会话或迟到结果混入。
3. 实际解释器必须匹配：PowerShell工具优先pwsh再powershell；Goal Command目前Windows固定powershell.exe。还需核对cwd、命令、相关输入及运行时范围；无法确认依赖或解释器不同则不复用。HA10负责进一步的当前任务内输入失效和有限复用，不得提前算通过。
4. 加真实工具到Goal消费者的有/无回执、失败/取消/伪造/不匹配样例，用实际命令启动数和原始回执ID证明单消费；继续保留普通任务路由。修复阶段成功分支目前仅保存repair_result文本/run_id，后续消费设计要检查该分支，不能丢掉真实回执后在verifier重复执行。
5. 变更共享源码后仅将受影响旧条目标needs_recheck；新增真实测试/构建证据或有依据的差异复核，不能刷新历史哈希冒充新测试。HA09全部五条通过后才能accepted并推进HA10。

## 后续工作包盘点

HA07：session_grants按workspace/session/tool、30分钟TTL；PendingApprovalRecord按call_id逐个入队，未合并并发miss。api_tools_approve消费pending后普通工具直接执行，尚未经过ToolDispatchService hook包裹；MCP/DSH有独立路径。保留既有会话授权语义，补一次性批准范围、并发合并、hook后复核以及审批/hook/队列/领取资格四个撤销屏障，禁止用长全局锁或审核模型代替。

HA08：依赖HA06和HA07；安全独立读取重叠、冲突有序、资源资格、排队取消/撤销均需真实屏障和效果，未实施。HA11仍缺固定provider/model/effort与真实调用费用授权，15组模型配对未做；HA12依赖未满足，不允许--complete或宣称60/60。

## 接替操作与文件

当前无在途命令、无本轮未确认效果；session 76127/PID 2728及session 98183/PID 22448均已结束，不能按历史PID重启或结束进程。所有验证在隔离本地资源上运行，未安装/发布、未改变日常8765服务、未调用真实模型。原HA00可恢复基线保持不变。

工作目录D:\coolzhuagent。先读AGENT.md、START_HERE.md和state.json，再运行轻量check_plan.py；已有授权已写入state，无需再询问是否继续本轮本地实施。然后按上述HA09-04继续。不要reset、stash或清理现有300余项用户改动。

源码本段涉及main.rs、goal_command.rs、新goal_verification.rs及ha09_tests.rs，准确哈希及相对HA06的补丁见implementation-inputs.json/implementation.patch。改前副本在tmp/ha09-20261018-before；已归档事实不可覆盖。tmp/ha09_apply.py已执行，禁止重复应用；tmp中的旧草稿不是当前源码。

可复用tmp/ha09_run.py和ha09_regression.py，下一轮使用新标签。Cargo构建jobs=1，test/debug=0；生产构建dev/debug=0。真实测试与cargo build通过后才归档并修改通过状态。HA09归档/部分验收脚本已执行，不可重复调用来覆盖已验收记录；新变更使用新证据目录/ID。

11:16 UTC账户窗口快照97%已用。按用户“额度耗尽前保存接替”的要求，本轮完成当前验证和证据收尾后保留余量；未声称额度已耗尽。文件不会自动唤醒其他模型；接替会话须核对当前证据后继续。
