# 2026-10-10 CLI 事件保存与恢复共用内容校验

生产 save_events 与 load_event_history 共用 validate_event_record，核对 schema=1、起点/非空身份、连续序号、同回合身份和唯一最后终态。保存时先校验，再有界序列化和预留 pending，避免发布读取端明确拒绝的单条记录。空事件仍作为无轨迹兼容不写文件，不冒称有效回合。

新增实际保存负例：未来 schema、序号跳跃、空身份、混入另一回合、错误起点、缺失终态，6 类均拒绝且不创建历史目录；合法 TurnFailed 仍接纳。新增测试后真实离线 CLI build 和完整 test 均退出 0，95 通过、0 失败、0 忽略。日志 tmp/analysis-cli-event-shared-validation-build-final.txt、tmp/analysis-cli-event-shared-validation-tests-final.txt。

此前初次共用校验回归 94 通过，也保留 analysis-cli-event-shared-validation-build.txt / tests.txt。本轮不解决历史跨回合身份重复、累计预算与跨进程原子保护、消息和事件事务或正式安装恢复。全部 32WBS 继续进行。
