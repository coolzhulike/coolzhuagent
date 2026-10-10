# Conversation 身份与失败矩阵复核

本次以前一提交 67dbfcb 的实现为基础，追加实际回归，而非以此前缺口描述代替当前源码。

## 已验证范围

模型请求错误、流缺少 MessageStop、空内容、零次迭代限制，均在 run_turn 实际返回路径上验证：首事件为 TurnStarted、末事件为 TurnFailed、仅一个终态、序号连续、同一轨迹保持 thread/turn 身份。每种情形连续运行两次，同一运行时两次 turn 不重复。已有请求返回后取消用例继续通过。

离线 cargo build -p coolzhu-core-runtime --offline -j1 退出码 0；conversation::tests 18 通过、0 失败。日志为 tmp/analysis-conversation-failure-matrix-build.txt 和 tmp/analysis-conversation-failure-matrix-tests.txt；工具会话 96738 真实终态退出码 0。此次新增测试未改生产行为，不将定向测试宣称为新增完整回归。

## 跨重启身份仍未闭环

conversation.rs 的 new_with_features 无条件设置 next_turn_number=0，run_turn 使用 turn-N；Session 仅保存 version/messages，其手写 to_json/from_json 不保存回合游标。恢复同一历史会话后首回合仍为 turn-1，不能证明历史 turn 唯一。默认 thread-default 也不能作为各独立会话的稳定唯一身份。

单独调整内存计数、按消息数量推算或新增一个未纳入手写 JSON 的字段都不能修复此缺口：压缩会改变消息数，未持久化游标仍会重复。后续需同时处理持久身份、旧格式迁移、并发/分支所有权和持久化失败后的身份分配语义，检查各调用方与结构体字面量兼容性，并验证恢复后的事件消费路径。

当前验证不覆盖跨重启唯一性、错误与取消竞争、同步模型请求即时打断及断电一致性。正式安装版仍为 0.2.130，未包含上述源码提交。32WBS 整体未完成。
