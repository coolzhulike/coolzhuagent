# 2026-10-10 CLI 历史数量边界真实文件检查

在独立 tmp 构造 9999 个合法且身份唯一的实际 JSON 文件，调用生产 save_events 发布第 10000 条，load_event_history 完整恢复 10000 条；再尝试第 10001 条，得到历史预算错误。目录仍恰好 10000 个正式 JSON 文件，无 pending，拒绝后旧历史再次完整恢复。夹具逐文件清理，不涉及用户会话。

cargo build -p coolzhu-command-line --offline -j1 实际退出 0；完整 cargo test 同包实际退出 0，98 通过、0 失败、0 忽略。串行工具会话 21373 终态 0；日志 tmp/analysis-cli-count-budget-build.txt、tmp/analysis-cli-count-budget-tests.txt。前轮真实进程锁争用与字节预算检查也包含在该完整回归中。

此证据覆盖单次顺序数量边界，尚不证明两个写者竞争最后一个槽位、崩溃恢复预算、旧版本不持锁或外部编辑者。消息与事件统一事务、路径别名以及正式安装恢复仍未闭环，32WBS 总目标继续进行。
