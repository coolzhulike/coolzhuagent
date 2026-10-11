"""独立核验重新配置后的零执行、阶段拒绝事件及具体原因回传。"""
from pathlib import Path
import json,sqlite3
folder=Path(__file__).resolve().parent;root=folder.parents[1]
facts=json.loads((folder/'result.json').read_text(encoding='utf-8'))
observation=facts['observation'];before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
assert observation['attempt_state_at_change']=='submitted' and observation['source_unchanged']
assert observation['original_config']=={} and observation['changed_config']!={}
assert observation['original_activation_id']!=observation['changed_activation_id']
assert len(facts['tool_calls'])==1 and facts['tool_calls'][0]['status']=='failed'
assert len(facts['attempts'])==1 and facts['attempts'][0]['process_drained']==1 and facts['attempts'][0]['protocol_stop']=='end_turn'
assert facts['after']['attempts']==before['counts']['attempts']+1
assert facts['run']['state']=='completed'
assert not any(x['locked_attempt'] for x in facts['bindings'])
assert sum(x['remote_session_id']=='island-kayak' for x in facts['bindings'])==1
network=[json.loads(x) for x in (folder/'network-events.jsonl').read_text().splitlines()]
assert not any(x['event']=='real_plugin_get' for x in network)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
    row=c.execute('SELECT created_at_unix_ms,updated_at_unix_ms FROM tool_calls WHERE tool_call_id=?',(facts['tool_calls'][0]['tool_call_id'],)).fetchone()
assert row[0]>observation['request_end_ms'],(row,observation)
rejected=[json.loads(x['payload_json']) for x in facts['runtime_events'] if x['event_type']=='tool.dispatch_rejected']
assert len(rejected)==1,rejected
rejection=rejected[0]
assert rejection['stage']=='after_admission_before_executor'
assert rejection['reason_code']=='dsh_action_not_live' and rejection['executed'] is False
assert rejection['tool_call_id']==facts['tool_calls'][0]['tool_call_id']
assert rejection['tool_name']==facts['tool_calls'][0]['tool_name']
reply=facts['visible_reply']['content']
assert 'DSH原工具快照已失效' in reply and '配置更新' in reply and '未执行' in reply,reply
restored=json.loads((folder/'restored.json').read_text(encoding='utf-8'))
assert restored['plugin_disabled'] and restored['parameters']==before['parameters']
assert json.loads((folder/'config-restored.json').read_text(encoding='utf-8'))['config']=={}
summary={'version':'0.2.105','qualification_change_zero_network':'pass',
    'independent_rejection_diagnostic':'pass','rejection':rejection,
    'network_gets':0,'tool_records':facts['tool_calls'],'tool_created_ms':row[0],'tool_settled_ms':row[1],
    'configuration_published_ms':observation['request_end_ms'],'attempts_added':1,
    'run':facts['run'],'attempt':facts['attempts'][0],'binding':'island-kayak',
    'restored_revision':restored['configuration_revision'],
    'limitation':'具体原因传至模型并由最终回复可见；独立事件证明阶段、工具ID和零执行，不外推其它许可与所有Provider。'}
(folder/'verification.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(summary,ensure_ascii=False))
