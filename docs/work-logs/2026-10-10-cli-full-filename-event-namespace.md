# 事件目录保留完整会话文件名

此前 with_extension("events.json") 使 main.json 与 main.txt 的新历史均写入 main.events.json。新写入改为在完整文件名后追加 .events.json，使用原生 OsString，不丢失扩展名或以有损 Unicode 文本替代路径。恢复同时检查新旧目录，保留此前格式可读取；无扩展名时新旧名字相同，只读取一次。

实际保存 main.json 的事件后读取 main.txt 为空，读回 main.json 与原事件一致；无扩展名会话保存/恢复也通过。最终离线 build 与完整 CLI 回归退出 0，92 通过、0 失败。日志 tmp/analysis-cli-event-namespace-build-2.txt、tmp/analysis-cli-event-namespace-tests-2.txt。首轮日志保留。

该修补隔离这类新写入，不自动拆分此前已混在旧目录的记录。兼容读取旧目录仍有历史归属歧义，特殊多后缀名字可能与另一会话的旧目录相交；版本化存储根、历史迁移及归属检查仍待完善。不能据此宣称所有跨会话隔离与正式恢复矩阵通过。当前安装版 0.2.130 未包含本改动，32WBS 继续。
