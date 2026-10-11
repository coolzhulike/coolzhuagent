from pathlib import Path
import json,urllib.request,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
sid='session-1791131217833';room='room-1791131523339';base='http://127.0.0.1:8765'
endpoint=base+'/api/sessions/'+sid+'/model-settings'
calculator='dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 assert not c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
 bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(sid,)).fetchall()
 assert sum(x[1]=='island-kayak' for x in bindings)==1 and not any(x[2] for x in bindings)
with urllib.request.urlopen(endpoint,timeout=30) as r:s=json.load(r)
assert s['session']['model']=='swe-2-medium' and s['configuration_revision']==57
parameters=s['parameters'];assert parameters['tool_allowlist']==['computer_use_perform','plugin__cli_anything_status','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab']
assert not any('key' in key.lower() or 'token' in key.lower() and key not in ('max_output_tokens',) for key in parameters),'不保存凭据字段'
assert any(t['name']==calculator and 'dsh-tool-calculator' in t['description'] for t in s['available_plugin_tools'])
if (p/'before.json').exists():
    assert json.loads((p/'before.json').read_text(encoding='utf-8'))['parameters']==parameters
else:
    (p/'before.json').write_text(json.dumps({'parameters':parameters,'configuration_revision':57,'bindings':bindings,'configuration_scope':s['configuration_scope']},ensure_ascii=False,indent=2),encoding='utf-8')
print('原白名单已有DSH calculator实际名称；无需参数改动，revision57保持')
