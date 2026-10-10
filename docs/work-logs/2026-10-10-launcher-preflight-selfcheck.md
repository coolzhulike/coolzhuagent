# 启动预检失败自检遗漏修复

用户截图中的端口 8765 冲突发生于启动预检。现场 package-selfcheck-last.json 仍是 10 月 7 日成功记录；源码确认 preflight_web_console_port 的错误通过问号直接返回，未进入 launch 的失败记录逻辑。

在独立集成工作区补写预检错误自检：保存当前错误、时间、已解析工作区快照及诊断说明，启动进程 PID 留空。路径解析拒绝复用同一保存函数。记录保存失败时明确输出旧文件不能代表本次结果。未修改冲突判定或进程恢复策略。

验证：cargo build -p coolzhu-app-launcher --offline -j1 实际 exit 0；cargo test -p coolzhu-app-launcher --offline -j1 实际 exit 0，库 64、二进制 8 项均通过。新增真实文件回归先写旧成功记录，再写工作区/会话库冲突，确认失败覆盖成功、时间更新、错误保存且无虚构进程 PID。日志保留在 tmp/analysis-launcher-preflight-selfcheck-build.txt 和 tmp/analysis-launcher-preflight-selfcheck-tests.txt。既有 windows-process-guard 未用导入警告保留。

本修复是 0.2.128 之后的源码候选，不在已经安装的 0.2.128 MSI 中。现场正式启动及完整 32WBS 验收尚未通过；后台启动此前被自动审批策略拒绝，不绕过该拒绝。
