"""执行中配置撤销需独立网络与实际持有句柄，不能写成零执行。"""
from pathlib import Path
import json,datetime,sqlite3
folder=Path(__file__).resolve().parent;root=folder.parents[1]
facts=json.loads((folder/'result.json').read_text(encoding='utf-8'))
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
held=json.loads((folder/'held-process-result.json').read_text(encoding='utf-8'))
restore=json.loads((folder/'restored.json').read_text(encoding='utf-8'))
assert not held.get('failure'),held
assert held['held_process']['wait_before']==258 and held['wait_after']==0
assert len(facts['tool_calls'])==1 and facts['tool_calls'][0]['status']=='failed'
assert facts['run']['state']=='completed'
assert len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert facts['after']['attempts']==before['counts']['attempts']+1
assert not any(x['locked_attempt'] for x in facts['bindings'])
assert sum(x['remote_session_id']=='island-kayak' for x in facts['bindings'])==1
assert restore['parameters']==before['parameters'] and restore['plugin_disabled']
events=[json.loads(x) for x in (folder/'slow-events.jsonl').read_text(encoding='utf-8').splitlines()]
entered=[x for x in events if x['event']=='function_network_entered'];closed=[x for x in events if x['event']=='connection_closed_before_response']
assert len(entered)==len(closed)==1 and not any(x['event']=='late_response_sent' for x in events)
assert any(x['event']=='server_stopped' for x in events)
epoch=lambda x:datetime.datetime.fromisoformat(x['utc']).timestamp()*1000
observation=facts['observation']
assert epoch(entered[0])<observation['request_begin_ms']<epoch(closed[0])
assert observation['original_config']=={} and observation['changed_config']!={}
assert observation['original_activation_id']!=observation['changed_activation_id']
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
tool_id=facts['tool_calls'][0]['tool_call_id']
audit=[json.loads(line) for line in (db.parent/'tool-audit.jsonl').read_text(encoding='utf-8').splitlines() if tool_id in line]
assert audit,'缺少该真实工具宿主审计，不以模型转述代替'
(folder/'matching-tool-audit.json').write_text(json.dumps(audit,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
summary={'version':'0.2.105','result':'pass','stage':'执行已经进入，配置更新取消旧执行并回收宿主',
         'network_gets':1,'response_bodies':0,'connection_closed_after_seconds':closed[0]['monotonic']-entered[0]['monotonic'],
         'held_process':held['held_process'],'wait_after':held['wait_after'],
         'run':facts['run'],'attempt':facts['attempts'][0],'tool':facts['tool_calls'][0],
         'binding':'island-kayak','restored_revision':restore['configuration_revision'],
         'boundary':'完整瞬时工具响应未另存；宿主审计、独立网络断开、原进程持有句柄退出与最终回复分开归档；已执行不冒称executed=false或登记前拒绝。'}
(folder/'verification.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'result':summary['result'],'network_gets':1,'wait_before':258,'wait_after':0,
                  'connection_closed_after_seconds':summary['connection_closed_after_seconds'],'audit':audit},ensure_ascii=False))
