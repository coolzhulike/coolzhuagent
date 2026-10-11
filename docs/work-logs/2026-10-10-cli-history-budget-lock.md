# 2026-10-10 CLI 历史预算发布临界区（进行中）

当前未提交实现：导出运行时既有 lock_file_mutation 给 CLI；按规范父目录与完整事件目录身份加锁，持锁加载全部兼容历史并取得实际原始字节计数，检查新增后 64MiB / 10000 条预算和回合身份重复，再沿既有 pending / hard-link 发布。锁文件保留在既有系统临时锁根，避免删除重建导致 inode 分裂。

真实离线 core-runtime/CLI build 已完成退出 0，完整 CLI test 仍在同一串行工具会话 34552 运行。日志 tmp/analysis-cli-history-budget-build.txt、tmp/analysis-cli-history-budget-tests.txt。本轮尚无终态测试声明。

需要继续：真实多进程争用、预算边界、旧历史重复/损坏拒绝、失败不新增 pending，及核心导出变化完整回归。旧版本不持同锁、外部编辑器、路径硬链接/别名、共享系统 tmp 锁安全与消息/事件统一事务仍需审查，不宣称全部恢复或 32WBS 已闭环。

后续终态：同一会话 34552 实际退出 0，CLI 完整回归 95 通过、0 失败。新增锁/预算专门边界与真实多进程检查尚未执行，因此修改仍待完善、未提交。

后续实现与验证：发布使用同一规范路径，避免锁住规范目录却写回原路径。真实第二测试进程在父进程持锁时 save_events 返回 WouldBlock 且无目录，释放锁后第二进程保存并恢复；同回合重复保存拒绝且原目录仍一条。实际文件预算夹具含 9 个各约 7MiB 合法旧回合，总量低于 64MiB，新增约 7MiB 回合被拒绝，无新增记录，旧 9 回合仍恢复。
本轮离线 core-runtime/CLI build 退出 0，完整 CLI 97 通过/0 失败，日志 tmp/analysis-cli-history-budget-final-build.txt、tmp/analysis-cli-history-budget-final-tests.txt。核心完整回归当前串行会话 22334 仍待终态；10000 条边界、路径别名、旧不持锁进程及完整事务仍待验证，不判全部闭环。

核心完整回归终态：同一会话 22334 实际退出 0，库测试 375 通过、0 失败、1 忽略，两个 DSH 集成各 1 忽略，Doc-tests 0；3 忽略仍不计通过。日志 tmp/analysis-cli-history-lock-core-regression.txt。
