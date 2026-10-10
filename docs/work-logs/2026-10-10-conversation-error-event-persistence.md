# Conversation 失败轨迹实际留存

## 本次实现

修复审计指出的局部事件日志随 run_turn 错误返回而丢失的问题：RuntimeError 携带已记录轨迹，失败和取消分别追加 turn_failed / turn_interrupted，不追加 turn_completed。CLI 在保存会话消息之外，将错误轨迹实际写入独占历史文件，写入并同步成功后才返回原执行错误。

轨迹文件使用 create_new 返回的原始句柄，避免预留后再次打开。普通写入或同步失败会尝试删除本次未完成文件；删除失败同时报告，不掩盖原执行错误。会话保存失败与轨迹保存失败均保留在返回错误中。

## 实际验证

- 离线串行 build：core-runtime、command-line，退出码 0，日志 tmp/analysis-conversation-error-events-build-2.txt。
- Conversation 定向测试：17 通过，日志 tmp/analysis-conversation-error-events-runtime-tests-2.txt。
- CLI 持久化定向测试：4 通过，覆盖工具事实留存、内部历史不覆盖、会话保存失败、非空轨迹保存失败仍保留模型错误。
- 完整 core-runtime 回归：370 通过、0 失败，主测试 1 忽略、两个集成测试各 1 忽略；日志 tmp/analysis-conversation-error-events-runtime-full.txt。
- 完整 CLI 回归：89 通过、0 失败；日志 tmp/analysis-conversation-error-events-cli-full.txt。
- 完整回归命令串行执行，工具会话 70800 最终退出码 0。

## 未闭环范围

本改动尚未进入已安装的 0.2.130。消息与事件不是原子事务；进程崩溃、断电或磁盘写满恢复矩阵未验证，不能据普通失败清理宣称崩溃一致性。同步模型请求仍无法保证即时打断；错误与取消竞争的完整终态矩阵、全部正式入口事件存储及跨重启回合身份仍需验收。32WBS 整体保持未完成。
