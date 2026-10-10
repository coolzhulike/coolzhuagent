# CLI 成功轨迹留存

核查发现此前 persist_result 仅消费 RuntimeError 的轨迹，成功 TurnSummary 被返回或展示后，其事件没有实际保存。本次通过 PersistedTurn 约束显式消费成功事件；正式调用方均为 TurnSummary，空单元实现仅用于既有测试。成功和失败使用同一独占文件写入路径，避免错误终态之外的历史缺失。

会话消息与事件分别保存。成功执行但事件保存失败时，返回终态轨迹保存错误，不伪造执行错误，也不返回成功。现有交互文本和 JSON 入口均在调用 persist_result 通过后才报告完成或输出成功结果。

新增测试使用真实 ConversationRuntime 成功回合，回读落盘 JSON 并逐事件比较，核对最后为 turn_completed；随后阻止轨迹目录创建，验证执行成功仍因持久化失败返回错误，且会话消息已实际保存。既有错误、内部快照测试继续通过。

实际离线 cargo build -p coolzhu-command-line --offline -j1 与完整 CLI 回归均退出 0；90 通过、0 失败、0 忽略。日志：tmp/analysis-cli-completed-events-build.txt、tmp/analysis-cli-completed-events-tests.txt。

本次不宣称消息与事件原子提交、断电恢复或全部正式入口完成。默认线程身份、历史读取/索引和正式安装验收仍未闭环；当前源码未进入安装版 0.2.130。32WBS 整体继续进行。
