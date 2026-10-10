# Conversation 错误终态持久化缺口

对当前源码的直接核查：modules/core-runtime/packages/core-runtime/src/conversation.rs 的 run_turn 只在成功路径把 event_log 放入 TurnSummary。模型请求和消息解析使用问号传播错误，迭代超限与根取消直接返回 RuntimeError；该错误类型仅有 message，因此已建立的 turn/thread 标识及事件轨迹不会随失败结果交给调用方。

agent_event.rs 的 AgentEventKind 当前只包含 TurnStarted、ContextSnapshot、ReasoningDelta、MessageDone、ToolCall、TurnCompleted，没有失败或中断终态。session.rs 的 Session 持久化只有 version/messages。CLI 的 turn_persistence::persist_result 无论成功失败都会保存 Session，但不能保存不在 Session/RuntimeError 中的 event_log。已有失败保存测试证明工具消息保留，不证明错误终态恢复。

因此 1.1 身份/终态、1.5 取消、2.5 各入口恢复仍缺完整证明。修补须同时处理：运行时失败/中断轨迹及同一 turn/thread 身份、实际持久化消费路径、保存失败不得返回成功。仅新增内存 accessor 或只给 RuntimeError 添字段不足以闭环；不能把同步 ApiClient::stream 改名当作在途可取消。

下一实现验证须覆盖模型错误、消息解析错误、迭代超限、请求返回后取消及工具执行后下一次模型失败，核对唯一错误/中断终态、无正常完成、历史工具事实与持久化后身份一致。成功路径与旧会话格式兼容也必须验证。此核查没有修改运行时代码，不代表上述矩阵通过。

当前实际安装版本为 0.2.130、构建源码 35cbcf238d194d7fbaac648359d8bddc9858f811；离线源码审查不能替代该安装版正式运行验收。此前后台启动命令被自动审批拒绝的边界继续有效。
