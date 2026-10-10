# 共享 CLI 待提交状态契约

实际服务 ChatTurnStatus::CommitPending 明确执行结束但终态提交未确认，消费者不可宣布完成或重跑。CLI 原先已将非 completed 状态返回错误，但措辞称 commit_pending 为“原轮次终态”；现专门说明终态提交待确认、不能宣布完成、未自动重试。JSON 原始 SSE done 状态不改写为 failed/completed。

扩展既有真实本机 HTTP/SSE 回归，三组分别为 completed、Ctrl+C 后 interrupted、commit_pending。每组只发生一次业务提交，commit_pending 返回错误且保持原始状态，不额外执行停止或重新提交。这是服务协议夹具，不证明正式服务提交故障恢复。

离线 CLI 构建实际退出 0；完整 crate 测试实际退出 0，87 通过、0 失败、0 忽略（测试函数数量未增加，既有回归新增一组 wire 场景）。日志 tmp/analysis-cli-commit-pending-build.txt、tmp/analysis-cli-commit-pending-tests.txt。查找不存在的 chat_stream.rs 后，使用 src 文件清单与真实 chat_run_admission.rs/主入口 enum 确认契约，未按猜测实现。

断流查询按权威运行状态确认结束，尚未完成真实 commit_pending/outbox 故障矩阵。实装 0.2.128 不含候选；32WBS 保持未闭环。
