# 共享 CLI 完整断流客户端回归

扩展既有 run_async 真实本机 HTTP/SSE 夹具：服务先发送 started 原 run/turn/room，再以正确 HTTP chunk 结束流而不发送 done；客户端自动走既有停止与原运行查询流程。分别提供 completed 与 interrupted 的权威查询响应，验证前者成功、后者错误、JSON 来源为 run_status_after_disconnect 且保留 runtime_state。

服务端逐请求断言业务提交仅 1 次、停止仅 1 次、原运行查询仅 1 次，所有停止使用原 session/room/turn，不发生重复聊天提交。既有正常 completed、Ctrl+C interrupted、commit_pending 场景继续保留，同一测试函数内现为五组真实 HTTP 场景。

离线 CLI 构建实际退出 0；完整 crate 测试实际退出 0，88 通过、0 失败、0 忽略。日志 tmp/analysis-cli-disconnect-flow-build.txt 与 tmp/analysis-cli-disconnect-flow-tests.txt。测试数未增加，不能把新增场景计为额外通过函数。

本次仅补客户端协议证据，没有启动产品后台或调用外部模型，不证明提交丢回执、TCP 强制重置、在途工具取消及 outbox 故障恢复等正式矩阵；这些仍未闭环。当前实装 0.2.128 不含候选，32WBS 保持 active。
