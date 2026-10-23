# HA-00～HA-03 的证据适用性复核

HA-00的冻结manifest及source.patch与原记录哈希一致；本轮在全新隔离目录按manifest固定提交恢复73项变更和29项未跟踪源码，102项核验0错误。原确定性权限测试输出另复制为original-baseline-core-test.log。恢复演练没有重跑完整基线构建或模型实验。

旧E-HA00记录的文字引用3d1a…，inputs曾被先前脚本更新到d8a7…，本轮没有续写该做法；旧对象保留为历史，新证据明确复核范围。规划baseline.json也不重写。restore.md修正了未固定HEAD及PowerShell二进制管道问题，清单/补丁/快照内容未改。

当前main.rs与本轮开始d8a7…之间的实际差异在implementation.patch，当前哈希和符号位置在audit.json。改变范围为请求返回/传递、实际adapter摘要、usage持久绑定、跨spawn收集；RegistryExecutor仅添加cfg(test)进入计数。host_child_agent及订阅额外变化为测试；tool_invocation_identity仅来源注释。原授权/hook/取消/收尾链未旁路，审批缓存粒度、未知链路及HA06～HA10缺口仍按current-gap-map，行号以当前符号为准。HA04不再是该旧图中的未实现状态，以acceptance.md为准。

四入口：Web HTTP共享循环消费SentRequest；CLI仍自己的ConversationRuntime/adapter；ACP仍远端会话/宿主桥；订阅仍文本路径、原生工具明确不可用。没有给ACP/订阅伪造MessageRequest。隔离从旧静态参数改用测试夹具的临时workspace/DB/state和127.0.0.1:0；未启动完整Web服务或触碰安装实例。进程级安装/插件账户目录隔离不在本轮证明范围。

HA01～HA03相关测试已在web-regression-03实际复跑；源码哈希变化后用新回执替换当前引用，原始历史日志与证据不改写。复测范围涵盖能力化提示、上下文来源/取消压缩、工具暴露/发现/能力拒绝及共享两入口；真实模型完成质量依然只归HA11。
