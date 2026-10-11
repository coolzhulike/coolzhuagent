# 工具派发服务：先收敛模型调用的接纳与收尾

目前Web两条模型循环、ACP桥和宿主子任务均调用main.rs里的同一个模型派发协调函数。其身份缺项拒绝、来源ID、冻结父预算、取消登记、一次性接纳、终态持久化和结果投影属于运行服务职责，不应继续留在HTTP入口大文件。

先将这一协调函数原体提取到独立ToolDispatchService，使用显式借用的DispatchRequest传递模型调用和已冻结的父上下文。原入口保留兼容薄适配，调用方和工具名称不改。服务无第二份存储、锁、缓存或注册表；仍沿用ToolDispatchSettlement、已有预算/取消注册和真实执行路由，不持std锁跨await。原体逐字节比对，既有回归和新的真实长程分别核验，不以文件挪动宣称2.3全完成。

当前分阶段界限：具体内置/MCP/DSH/CU/handoff路由及registry executor仍在既有模块/main.rs，审批权威服务与hook合并尚未完成；不在本轮同时搬迁或调整其语义。后续迁移以这些依赖为ports收窄，而非用super通配符隐藏耦合。跨进程写者/outbox和共享TurnRunner有各自工作包，不塞入本次派发提取。

风险：薄适配丢失raw provider ID、父预算、host_scope会改变取消和重复调用语义；请求结构逐项保留全部八个输入参数。Task-local来源/ACP作用域必须在同一异步任务中继续运行，禁止另起后台任务。终态finish发生于返回前、持久化失败禁止自动重试，原行为保留。大future继续Box::pin，避免默认线程栈溢出。非ACP结果投影与ACP外桥只保存一次的边界保持。

验证：offline实际build退出0，最终完整Web1426通过/0失败/6既有忽略、模块联动8通过；首次LLVM内存不足exit101保留。新源码候选使用真实SWE-2-medium及原唯一island-kayak，一次工具调用完成动态订单页11步、27条可信输入事件，工具/父轮completed，单end_turn/process_drained并解锁，回复#752可见。这是提取后入口的独立候选实操，119原失败仍保留；正式安装包复验待办。具体registry/审批/hook与完整2.3工作包仍未关闭。证据见[动态长程报告](../../testing/2026-10-09-browser-live-repaint/report.md)。
