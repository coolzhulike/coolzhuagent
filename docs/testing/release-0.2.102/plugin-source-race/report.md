# 正式102：插件源码变化拒绝独立证据

真实SWE-2-medium、原房间、唯一island-kayak；marker `PLUGIN-SOURCE-RACE-102-20261008`。正式Program Files配套102运行，诊断开关默认关闭。可见#693/#694，26.8秒。

正常启用此前验收net-tools插件，只增加其net_fetch白名单；ACP submitted后只追加无语义注释，使固定源码失效。独立宿主事实如下：

- 单ACP attempt `666e7d5e98ff9dbeb7a1a48cbd491a5d33b2df0e2fd120b4`，请求/实际模型均swe-2-medium，end_turn、process_drained=1。
- 运行 `run-chat-c493afd87fbf7b8cccdf4b488a723ddc0e89744e4b9be58d` completed；一次宿主 `tool.dispatch_rejected`，tool_not_live/before_dispatch/executed=false，与该attempt、工具名及请求摘要绑定。
- 工具执行登记0、本机独立服务器GET0；并非只依赖最终模型转述。
- 只在父终态与ACP排空后恢复原SHA；正常撤回单个临时工具、停用插件，revision39→40→41。其余参数精确保持，原唯一绑定解锁，internal仍空。
- 首次预检因沿用旧revision37断言失败，在任何启用/配置修改/模型派发前终止；核当前revision39参数与原恢复值逐项相同后再执行。保留[预检事实](preflight-revision.json)，不把预检失败抹除。

[软件实拍](formal-visible-reply.jpg)、[只读运行与独立事件](result.json)、[源码变化/排空后恢复](source-observation.json)、[独立网络事实](network-events.jsonl)、[恢复核验](restored.json)。脚本按原tmp层级解析，归档目录不可直接重放；未导出reasoning、密钥或完整用户设置。

仅关闭“submitted后源码变化、旧资格零派发、独立拒绝事件、排空恢复”子项；许可与配置变化全矩阵、所有RPC拒绝入口及独立Win32后代句柄清理仍未完整。101原轮证据保持当时缺少独立事件的结论。
