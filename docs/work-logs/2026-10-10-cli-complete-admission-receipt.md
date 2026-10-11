# 共享 CLI 完整接纳回执

实际服务 api_chat_turn_interrupt 要求有效 session/room/turn，返回 turn_id、status、outcome、idempotent；不要求服务提供该 DTO 中不存在的 run_id。客户端 started 原先先写 receipt.run，再校验 turn，缺少 turn 的坏事件会留下半个身份，协议错误清理据 run 非空尝试停止。

现先解析完整 run/turn 再更新 Receipt；缺失字段不显示接纳事件，也不留下半个 run。interrupt 在发送 HTTP 前检查 run、turn、session、room 非空（包括纯空白），不完整身份明确返回未发送/结果待确认。既有完整回执、同 turn 响应校验和不自动重试规则保持。

新增坏 started 字段组合回归，以及真实本机 TCP 监听确认 run 有值但 turn 缺失时没有发送连接。离线 CLI 构建实际退出 0；完整 crate 测试实际退出 0，87 通过、0 失败、0 忽略。日志 tmp/analysis-cli-admission-atomic-build.txt、tmp/analysis-cli-admission-atomic-tests.txt。

本修改避免部分状态更新，不宣称解决服务已接纳但客户端未获完整身份时的恢复；此类仍为 unknown，不推断未提交、不自动重试。尚未进入实装 0.2.128，正式服务/模型完整回执及取消矩阵仍未闭环。
