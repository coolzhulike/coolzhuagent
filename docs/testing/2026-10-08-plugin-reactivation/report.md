# 095正式插件重新启用后的旧轮隔离验收

沿用真实SWE-2-medium、原聊天室与唯一island-kayak。使用固定真实DSH net-tools，不替换插件代码；本机HTTP服务只记录是否收到真实请求，不伪造模型、工具或宿主结果。通过正常产品接口暂时启用插件、加入工具白名单；每轮只要求一次net_fetch，无重试或新云端会话。

| 实测阶段 | 正常停用→重新启用区间（epoch ms） | 实际结果 |
| --- | --- | --- |
| ACP prepared | 1791480807789.2722→1791480809694.1418 | 声明/桥资格层拒绝，工具台账0、真实GET0 |
| ACP submitted | 1791480977288.1758→1791480979337.3096 | 冻结旧快照派发409拒绝，失败工具台账1、真实GET0；工具台账创建1791480986723，晚于重新启用 |

第二轮重启用实际成功，activation_id由旧轮的`dsh-6368-1791480809690440700-4`变为新世代；当时工具仍在会话白名单。第一轮重新启用后正常配置接口再次列出net_fetch，但旧轮不能因此获得新的派发资格。两轮均completed/end_turn/process_drained=true，唯一远端正常解锁，internal远端为空。父completed表示回复正常收尾，不表示抓取业务成功。

[prepared事实](prepared-result.json)、[prepared实机截图](01-prepared-rejected.jpg)；[submitted事实](submitted-result.json)、[submitted实机截图](02-submitted-rejected.jpg)。完整瞬时桥拒绝响应未独立持久化，模型文字为转述；最终以台账、启用世代变更、正常启用响应与真实网络0交叉核验。采证脚本首次错误假设两个阶段都应无台账，submitted实际存在失败台账1，已按两个阶段分别校验，不改写产品或运行结果。

仅覆盖prepared及submitted阶段插件停用/重新启用后的旧轮隔离。冻结权限扩大、每个清理阶段及独立Win32句柄证明仍开放，不外推全部竞争矩阵。

本轮结束仅撤回新增net_fetch白名单，net-tools恢复停用并正常关闭HTTP服务；原calculator、CU设置及安全记录保持。软件身份是正式0.2.95，见[095交接](../release-0.2.95/change-report-and-test-handoff.md)。后续审计投影修复不在此安装包内。
