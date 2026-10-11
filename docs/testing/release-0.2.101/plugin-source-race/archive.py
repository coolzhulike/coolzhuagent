"""保留真实事实与证据边界，不以模型转述代替宿主审计。"""
from pathlib import Path
import json,shutil
folder=Path(__file__).resolve().parent;root=folder.parents[1]
target=root/'docs/testing/release-0.2.101/plugin-source-race';target.mkdir(parents=True,exist_ok=True)
facts=json.loads((folder/'result.json').read_text(encoding='utf-8'))
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
restored=json.loads((folder/'restored.json').read_text(encoding='utf-8'))
events=[json.loads(x) for x in (folder/'network-events.jsonl').read_text(encoding='utf-8').splitlines()]
assert facts['run']['state']=='completed' and not facts['tool_calls']
assert len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert facts['after']['attempts']==before['counts']['attempts']+1
assert not any(x['event']=='real_plugin_get' for x in events)
assert events[-1]['event']=='server_stopped'
assert restored['configuration_revision']==37 and restored['parameters']==before['parameters'] and restored['plugin_disabled']
facts['observed_conclusion']='submitted后源码注释变化，零工具执行登记、零网络请求；正常结束及恢复。具体拒绝原因仅模型转述，无独立宿主回执，不追认完整源码竞态矩阵通过。'
facts['network_requests']=0
(folder/'result.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
for name in ['collect.py','archive.py','prepare.py','restore.py','watch.py','send.py','server.py','address.json','before.json','enabled.json','temporary-tool.json','source-observation.json','submitted-request.json','stream-finished.json','network-events.jsonl','result.json','restored.json','formal-visible-reply.jpg']:
    shutil.copy2(folder/name,target/name)
report='''# 0.2.101 DSH 固定源码变化观察

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

归档脚本供审计，路径按原 tmp 层级解析；不可从归档目录直接重放。未导出模型思考、认证凭据或完整用户设置。未改产品源码，因此本轮不新增单元测试，也不重复已有完整编译回归。Browser严格按下期间新Target/跨来源commit与未发布候选修补仍开放。
'''
(target/'report.md').write_text(report,encoding='utf-8')
note='''101正式DSH源码变化观察：真实SWE submitted后仅改变既有验收插件的一处无语义注释，工具执行登记0、独立网络GET0，父run completed/ACP end_turn/drained/唯一绑定解锁；排空后按SHA恢复，临时白名单及插件启用状态恢复、revision37。具体登记前拒绝未独立持久化，仅有模型转述，因此只关闭零派发与清理观察，不追认完整源码/许可竞争矩阵通过。见[原事实与实拍](../../testing/release-0.2.101/plugin-source-race/report.md)。Browser严格时序及源码候选正式复验继续开放。

'''
for name in ['acceptance-summary-2026-10-08.md','current-acceptance-queue.md','wbs-implementation-audit-2026-10-07.md','implementation-status-and-change-inventory.md']:
    path=root/'docs/analysis/2026-09-21-integration-review'/name
    text=path.read_text(encoding='utf-8');assert note not in text
    first,rest=text.split('\n',1);path.write_text(first+'\n\n'+note+rest.lstrip('\n'),encoding='utf-8')
print('真实源码变化观察和四份台账已归档；完整竞态项保持开放')
