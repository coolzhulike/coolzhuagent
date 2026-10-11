# 0.2.101 DSH 固定源码变化观察

沿用正式安装版、SWE-2-medium、原聊天室及唯一远端 island-kayak。marker `PLUGIN-SOURCE-RACE-101-20261008`，可见消息 #689/#690，无需刷新。

## 操作与实际结果

此前验收用 net-tools 插件通过产品正常接口启用，只临时增加单个 net_fetch 白名单。请求受理进入 ACP submitted 后，验收脚本仅在公共插件 index.js 尾部添加无语义注释；没有改权限、工具逻辑、计算器或云端绑定。

- 真实 ACP attempt 仅新增1，模型请求与实际选择均为 swe-2-medium。
- 提交后约1.5秒改变源码 SHA，父运行 running；原 SHA `6fea33400d3e1c1b3f2d2e38a16e784872022862fa6cd025af3ca04aaee83bdd`，变化后 `35760090509e38fc1b0847e5df7809044e76a089f6bdf843b0c64cece85ec74e`。
- 运行 completed，ACP end_turn/process_drained=1，工具执行台账0条，本机独立服务器实际 GET 0次。
- 最终回复称宿主拒绝“ACP 工具已被当前会话或房间撤销，未执行”。这段是模型转述，正式宿主并未单独持久化本次登记前拒绝回执，不能当作独立审计证明。
- 排空后才按字节恢复原 SHA；正常撤回新增白名单、停用本次插件，revision35→36→37，其他参数与原值完全一致。服务器正常退出，唯一绑定解锁、internal远端仍为空。

结论：本轮证明 submitted 后源码变化期间没有实际派发或网络请求，正常收尾和恢复通过；具体拒绝原因及完整源码/许可/配置竞争矩阵仍开放，不能只凭模型转述判为完整验收通过。

## 证据

- [正常软件实拍](formal-visible-reply.jpg)
- [只读宿主事实与最终回复](result.json)
- [变化与排空后恢复](source-observation.json)
- [独立网络观察](network-events.jsonl)
- [原配置](before.json) / [恢复核验](restored.json)

归档脚本供审计，路径按原 tmp 层级解析；不可从归档目录直接重放。未导出模型思考、认证凭据或完整用户设置。正式101实操未改EXE；随后为补齐登记前拒绝证据，另有[最小源码候选及验证](../../2026-10-08-acp-rejection-evidence/report.md)，未出包、不追认本次已有独立回执。Browser严格按下期间新Target/跨来源commit与未发布候选修补仍开放。
