# Conversation 终态取实际错误来源

此前错误收尾重新读取取消令牌，只要收尾时取消为真，就将模型/解析错误标为 TurnInterrupted。这会保留错误文字却改变终态来源。本次 RuntimeError 携带私有类型化 interruption 原因，仅在运行时明确检测取消/超时时生成中断错误；普通模型或解析失败保持 TurnFailed，不用收尾时的新取消覆盖。

新增竞争回归在 ApiClient 中设置取消并返回模型原始错误，验证原错误文字、TurnFailed、interruption=null 与唯一终态。既有模型取消后返回成功的回归保持 TurnInterrupted 且没有 TurnCompleted。这样两个返回路径按实际错误来源区分，不从字符串猜测取消。

离线 core-runtime/CLI build 退出 0；Conversation 定向 20 通过、0 失败；CLI 完整 91 通过、0 失败。串行工具会话 8485 最终退出 0。日志 tmp/analysis-conversation-terminal-cause-build.txt、tmp/analysis-conversation-terminal-cause-tests.txt、tmp/analysis-conversation-terminal-cause-cli-tests.txt。本轮没有把定向回归宣称为完整 core-runtime 回归。

未覆盖所有 provider 的取消原因映射、真正同步模型在途打断、严格线程时序及超时完整矩阵。此前轨迹读写大小一致性、pending 恢复处置与消息/事件原子性仍开放。安装版 0.2.130 未包含本改动，32WBS 目标继续未完成。
