# 默认共享 CLI 校验期间取消

入口审查更正：main.rs 默认 prompt/REPL 与显式 chat 均路由 shared_chat；此前 ConversationRuntime 与 LiveCli 改动属于显式 --legacy-runtime 兼容入口。默认共享 CLI 已接入 tokio Ctrl+C，发送停止时使用原 session/room/turn 身份，不能把旧入口缺少线程根控制外推为默认 CLI 未接取消。

实际缺口是 run_async 先等待 validate_context，再轮询取消信号。现从上下文校验开始并行监听信号，信号已就绪时优先取消，返回明确“聊天请求尚未提交”。校验已结束后的提交/接纳/停止流程保持既有未知回执与不自动重试规则。

真实本机 TCP/HTTP 回归先收到 GET /api/state，持有未响应校验连接后触发取消，核对客户端退出且没有第二个连接或聊天提交、没有输出成功事件。该夹具不启动产品后台、模型或工具，不替代正式验收。

离线 CLI 构建实际退出 0；完整 crate 测试实际退出 0，84 通过、0 失败、0 忽略，包含既有共享流式接纳/原轮次取消回归。日志 tmp/analysis-cli-shared-preflight-cancel-build.txt 与 tmp/analysis-cli-shared-preflight-cancel-tests.txt。

实装 0.2.128 尚不含该候选；在途网络、回执未知、正式服务取消/终态完整矩阵以及 legacy 根取消接线仍开放。
