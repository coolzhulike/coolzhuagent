"""本轮submitted后仅正常重新配置插件；不修改源码、安全库或云端绑定。"""
from pathlib import Path
import sqlite3,json,time,urllib.request,hashlib
folder=Path(__file__).resolve().parent;root=folder.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
source=json.loads((root/'tmp/2026-10-08-plugin-reactivation/source.json').read_text(encoding='utf-8'))
settings_path=Path('C:/Users/zhupu/.claw/settings.json')
snapshot=json.loads(settings_path.read_text(encoding='utf-8'))['dshSnapshots'][before['plugin_id']]
assert snapshot['config']=={},'原配置不是空对象，不覆盖'
assert hashlib.sha256(Path(before['entry_path']).read_bytes()).hexdigest()==before['entry_sha256']
def row():
    with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
        return c.execute("SELECT r.id,r.state,a.attempt_id,a.state,a.process_drained FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id JOIN json_each(e.payload_json,'$.message_ids') refs JOIN chat_room_messages m ON m.id=refs.value JOIN devin_acp_attempts a ON json_extract(a.scope_json,'$.run_id')=r.id WHERE e.event_type='chat.source_messages' AND m.content LIKE ? ORDER BY m.created_at DESC LIMIT 1",('PLUGIN-CONFIG-RACE-104-20261008%',)).fetchone()
deadline=time.monotonic()+240
while time.monotonic()<deadline:
    observed=row()
    if observed and observed[3]=='submitted':break
    if observed and observed[3] in ('terminal','not_sent','unknown'):raise RuntimeError('未命中submitted，未重新配置')
    time.sleep(.02)
else:raise RuntimeError('未观察到submitted，未重新配置')
body={'expected_workspace':'ws-23f646a969206cb4','id':before['plugin_id'],
    'expected_source_sha256':source['source_sha256'],'session_id':'session-1791131217833',
    'chat_room_id':'room-1791131523339','config':{'coolzhuAcceptanceTag':'config-race-104-20261008'}}
observation={'run_id':observed[0],'attempt_id':observed[2],'attempt_state_at_change':observed[3],
    'original_activation_id':snapshot['activation_id'],'original_config':snapshot['config'],
    'request_begin_ms':time.time()*1000}
(folder/'config-observation.json').write_text(json.dumps(observation,ensure_ascii=False,indent=2),encoding='utf-8')
req=urllib.request.Request('http://127.0.0.1:8765/api/extension-market/dsh/enable',json.dumps(body).encode(),{'Content-Type':'application/json'})
with urllib.request.urlopen(req,timeout=135) as response:result=json.load(response)
(folder/'reconfigured.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
assert result['enabled'] is True
changed=json.loads(settings_path.read_text(encoding='utf-8'))['dshSnapshots'][before['plugin_id']]
assert changed['config']==body['config'] and changed['activation_id']!=snapshot['activation_id']
assert hashlib.sha256(Path(before['entry_path']).read_bytes()).hexdigest()==before['entry_sha256']
observation.update({'request_end_ms':time.time()*1000,'changed_activation_id':changed['activation_id'],
    'changed_config':changed['config'],'source_unchanged':True})
(folder/'config-observation.json').write_text(json.dumps(observation,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'reconfigured':True,'run_id':observed[0],'submitted_observed':True}),flush=True)
