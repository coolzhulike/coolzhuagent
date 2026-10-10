# 启动器复用自检保留同次观测

复用预检现在返回 VerifiedReuse：复用核对证据、同次 ServiceIdentity 的 ObservedBackground、已核对监听 PID。复用成功的 resolved 使用 with_observations 写入后台自报 workspace/session_db/build_version；自检后台 PID 使用真实监听观测，未提供时仍为 null。转发失败记录也保留这份观测。

没有为了生成自检再次获取后台身份，避免核对证据与另一次读取混用；这不消除预检结束到 shell 转发间的进程替换竞态，也不宣称 shell 连接到实例的最终握手已验证。用户观测历史记录更新另待审查。

磁盘自检回归扩展确认 observed.build_version、observed.session_db 和监听 PID，并保留当前 launch_id 与完整复用检查 JSON。离线构建最终实际退出 0；完整 launcher 测试实际退出 0，库 64/0、binary 9/0，共 73 通过。日志 tmp/analysis-launcher-reuse-observation-final-build.txt 与 tmp/analysis-launcher-reuse-observation-tests.txt。

该回归使用明确构造的观测记录测试自检落盘，不冒充真实产品实例复用；尚未进入已安装 0.2.128，正式升级/复用验收及 32WBS 仍未闭环。上一轮文档所述“后台 PID 为空、resolved 未带 observed”是当时版本，已由本修复更新。
