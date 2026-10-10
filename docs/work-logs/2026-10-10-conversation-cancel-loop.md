# Conversation 取消后停止模型循环

源码审查发现目标工具被取消阻断后，原循环仍可请求下一轮模型并正常返回。现于每轮模型调用前检查当前线程根控制，同批工具在审批前也检查取消/截止；最终 TurnCompleted 前再次检查，阻止迟到纯文本模型结果形成正常完成返回。

取消返回现有 RuntimeError，已写入 session 的工具结果与模型用量处理不主动清除。局部 event_log 在错误返回时不构成持久终态事件，本修改不宣称解决所有宿主终态映射或迟到消息隔离。当前 ApiClient 同步 stream 本身仍不能由此检查强制中断，在途网络取消需要 provider 实现。

回归覆盖启动前取消不调用模型、审批期间取消后目标执行器零调用且错误工具结果保留、模型返回期间取消不得正常完成。最终离线构建实际退出 0（tmp/analysis-conversation-loop-cancel-final-build.txt）；conversation::tests 实际退出 0，16 通过、0 失败、348 过滤（tmp/analysis-conversation-loop-cancel-final-tests.txt）。不是完整 core-runtime 或正式外部模型验收。

最新修改仍属于独立集成候选，未进入已安装 0.2.128。WBS 1.5、2.3、2.5 的全部入口及正式取消矩阵保持开放。
