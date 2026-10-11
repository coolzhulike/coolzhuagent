# 普通HTTP与SSE实际模型循环收敛候选

前提：执行中心接线提交c9edcf0已完成。原main.rs中普通模型循环与SSE反馈循环分别维护派发、历史、重复纠偏、轮数看护和文本恢复；执行中心TurnRunner::run仅为生命周期，不能当作完整模型循环已经统一。

本轮实际删除SSE独立工具派发/反馈循环。普通入口装配多模态上下文后进入run_shared_model_tool_loop；SSE保持首轮网络流适配，完整结束后把原文本/思考/结构化工具响应交给同一个函数，不重发首轮。共享函数负责从首轮到收尾的工具目录校验、工具派发、ToolResult往返、完整历史续接、重复调用纠偏、CU熔断、反馈上限、一次无工具恢复、真实使用量及最终结果。CLI沿用原HTTP/SSE入口；原调用普通循环的子Agent/Goal/接力HTTP路径也进入此函数。ACP会话后端仍沿用原适配，不能据此宣称Devin子Agent/Goal/接力支持。

SSE通过事件通道展示requested/running/终态状态及成对工具消息。完成同一次poll前排空已产生事件，避免丢掉末批结果；保留真实tool_batch事件和状态。反馈模型失败标记运行failed并保存已有结果，不把失败吞成成功。完整任务的原外层取消、预算、工程pin及终态提交继续持有；共享函数本身没有另起脱离父任务的spawn。首响应无usage时不把内部占位值记为请求账目。

## 动态验证及失败记录

- 首次源码编译失败：错误使用不存在的reasoning_started，已去除；日志tmp/analysis-shared-model-loop-build.txt。
- 共享循环动态测试首次编译失败：测试错误使用crate::api路径，修正为本crate实际extern api；日志tmp/analysis-shared-model-loop-dynamic-tests.txt。
- 共享循环本地HTTP测试实际通过1项：普通模式3次模型请求，SSE首响应种子模式2次后续请求，两轮真实文件读取；历史哨兵和工具返回均保留，重复provider工具ID用不同响应身份隔离，不重复首轮请求或虚构账目。日志tmp/analysis-shared-model-loop-dynamic-tests-retry.txt。
- Web真实HTTP/SSE路由首次测试编译LLVM OOM，测试未开始；日志tmp/analysis-shared-model-loop-route-tests.txt。检查当时无其它Cargo/rustc，空闲物理内存6934456KiB/虚拟5283848KiB。将测试入口的大型API future置堆上后重试，不提高测试栈配置或删掉检查。
- 真实Web路由动态回归实际通过1项/7.07秒：通过真正api_chat_send/api_chat_send_stream入口及SSE响应收集，两入口各两次实际文件读取、三次模型请求，总共6个POST；SSE首响应仅1次；每个请求保留历史，每个最终反馈含两个文件原文；SQLite有4条read_file调用。日志tmp/analysis-shared-model-loop-route-tests-retry.txt。仅本地模拟provider，不是外部真实模型或正式原生UI验收。
- 最终源码offline build及完整回归日志为tmp/analysis-shared-model-loop-complete-build.txt与tmp/analysis-shared-model-loop-full-regression.txt，以实际终态为准。

## 仍未闭环

模型/provider/参数冻结的所有入口与恢复路径尚需进一步审查；完整取消/在途撤销/进程树矩阵、Goal统一索引、后代usage累计、durable调度与未知结果恢复仍开放。ACP的Goal/接力/子Agent目前明确拒绝。正式0.2.127没有替换，原唯一SWE云端没有访问，正式模型与原生UI、整包安装发布尚未验收。因此不能把32WBS或2.5全部验收宣布通过。
全量回归首次实际exit101：1451通过/1失败/6忽略（37.36秒），唯一失败为流式首响应requested事件被共享循环重复发送，预期requested/running/completed。保持原协议断言，共享循环仅对后续模型响应发送requested；种子首响应沿用网络流已发送的状态。最终修正构建/完整回归日志为tmp/analysis-shared-model-loop-status-build.txt与tmp/analysis-shared-model-loop-full-regression-final.txt。

最终状态：修正后offline build实际exit0（58.64秒）；完整Web回归实际exit0：1452通过、0失败、6忽略（37.05秒）；包括新增的核心种子与真实HTTP/SSE路由专项、原工具协议、空结果恢复和Goal子Agent专项。正式外部模型/原生UI验收仍未执行。
