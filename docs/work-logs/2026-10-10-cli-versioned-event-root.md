# 事件存储独立版本化根

新事件改存 turn-events-v2/<完整会话文件名>/，不再在 internal-turns 下生成带 events 后缀的目录。内部会话快照仍保持原存储位置。独占 pending 创建与不覆盖发布复用同一底层函数。

恢复读取 v2、此前完整文件名后缀及最早替换扩展名的三个目录，并按路径去重。旧会话历史不自动迁移或删除；已有无效记录仍拒绝恢复。同名不同扩展、额外后缀 main.json.txt、无扩展名的新历史隔离测试通过。

离线 CLI build 与完整回归实际退出 0，92 通过；补充后缀断言后的定向持久化回归 7 通过。实际 debug CLI 再执行九个旧格式恢复场景，全部退出码符合预期且会话 SHA 未变化，证据 docs/testing/2026-10-10-cli-history-restore/v2-root-legacy-process.json。

日志 tmp/analysis-cli-v2-event-root-build.txt、tmp/analysis-cli-v2-event-root-tests.txt、tmp/analysis-cli-v2-event-root-focused.txt。旧格式已混合记录的归属仍有歧义，不将版本化新写入宣称为旧历史修复。迁移、读写上限一致性、pending 恢复与正式安装验收仍开放，当前安装 0.2.130 不含本修补，32WBS 继续。
