# 2026-10-10 CLI 历史目录重定向拒绝

审计发现：仅规范工作区父目录不足以统一锁身份，turn-events-v2 或会话历史子目录可被 junction/symlink 重定向。读写端新增目录检查：当前/兼容历史根及子目录只接纳普通目录，Windows 拒绝 REPARSE_POINT；缺失目录保持旧无历史兼容。持锁加载再次执行该检查，发布仍使用规范工作区路径。

Windows 真实 tmp junction 测试创建事件根链接，生产保存及恢复都明确拒绝；目标哨兵原字节不变，目标目录仍只有哨兵，无新增轨迹。无跳过，junction 创建失败会使测试失败。原始目标与链接按各自路径清理，不触碰用户目录。

离线 CLI build 退出 0，完整回归 99 通过、0 失败、0 忽略。同一串行会话 12688 终态 0；日志 tmp/analysis-cli-history-link-final-build.txt、tmp/analysis-cli-history-link-final-tests.txt。前次无新增 junction 测试回归亦通过，日志 analysis-cli-history-link-build.txt / tests.txt。

边界：只证明预先存在的事件根 junction 被拒绝；子目录和兼容根检查在源码中接入，未做全部实际组合。外部非协作进程在检查后替换目录的 TOCTOU、硬链接/别名、共享锁根安全、旧不持锁进程与消息/事件事务仍需处理，不宣称路径竞态或全部 32WBS 已闭环。
