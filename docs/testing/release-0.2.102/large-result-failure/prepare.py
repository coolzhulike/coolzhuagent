"""只暂时恢复既有验收插件与单个工具白名单；不改房间权限或云端绑定。"""
from pathlib import Path
import hashlib,json,sqlite3,urllib.request
folder=Path(__file__).resolve().parent;root=folder.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
address=json.loads((folder/'address.json').read_text(encoding='utf-8'))
plugin=address['plugin_id'];tool=address['tool_name']
assert not (folder/'before.json').exists()
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
    assert c.execute("SELECT COUNT(*) FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchone()[0]==0
    bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE room_id=? AND agent_id=?',('room-1791131523339','session-1791131217833')).fetchall()
    assert sum(b[1]=='island-kayak' for b in bindings)==1 and not any(b[2] for b in bindings)
    counts={'attempts':c.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0],'messages':c.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0]}
home=Path('C:/Users/zhupu/.claw')
registry=json.loads((home/'plugins/installed.json').read_text(encoding='utf-8'))['plugins'][plugin]
install=Path(registry['install_path']).resolve()
assert install==home/'plugins/installed/dsh-aa909f79795799f288de8a67-external'
entry=install/'index.js';assert not entry.is_symlink()
manifest=json.loads((install/'plugin.json').read_text(encoding='utf-8'))
for item in manifest['dsh']['receipt']['files']:
    assert hashlib.sha256((install/item['path']).read_bytes()).hexdigest()==item['sha256'],item['path']
settings=json.loads((home/'settings.json').read_text(encoding='utf-8'))
assert settings['enabledPlugins'].get(plugin)!=True
endpoint='http://127.0.0.1:8765/api/sessions/session-1791131217833/model-settings'
with urllib.request.urlopen(endpoint,timeout=30) as r:session=json.load(r)
assert session['session']['model']=='swe-2-medium' and session['configuration_revision']==41
parameters=session['parameters'];assert tool not in parameters['tool_allowlist']
assert parameters['tool_allowlist']==['computer_use_perform','plugin__cli_anything_status','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab']
before={'counts':counts,'bindings':bindings,'parameters':parameters,'configuration_revision':41,'plugin_id':plugin,'entry_path':str(entry),'entry_sha256':hashlib.sha256(entry.read_bytes()).hexdigest(),'installation_id':registry['installation_id']}
(folder/'before.json').write_text(json.dumps(before,ensure_ascii=False,indent=2),encoding='utf-8')
(folder/'entry-original.bin').write_bytes(entry.read_bytes())
blobroot=db.parent/'tool-results'
(folder/'blobs-before.json').write_text(json.dumps(sorted(p.name for p in blobroot.glob('*.txt'))),encoding='utf-8')
def post(path,body,name):
    req=urllib.request.Request('http://127.0.0.1:8765'+path,json.dumps(body).encode(),{'Content-Type':'application/json'})
    with urllib.request.urlopen(req,timeout=135) as r:result=json.load(r)
    (folder/(name+'.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    return result
source=json.loads((root/'tmp/2026-10-08-plugin-reactivation/source.json').read_text(encoding='utf-8'))
enabled=post('/api/extension-market/dsh/enable',{'expected_workspace':'ws-23f646a969206cb4','id':plugin,'expected_source_sha256':source['source_sha256'],'session_id':'session-1791131217833','chat_room_id':'room-1791131523339','config':{}},'enabled')
assert enabled['enabled']
with urllib.request.urlopen(endpoint,timeout=30) as r:current=json.load(r)
assert any(t['name']==tool for t in current['available_plugin_tools'])
req=urllib.request.Request(endpoint,json.dumps({'parameters':{**current['parameters'],'tool_allowlist':[*current['parameters']['tool_allowlist'],tool]},'expected_revision':current['configuration_revision']}).encode(),{'Content-Type':'application/json'})
with urllib.request.urlopen(req,timeout=30) as r:changed=json.load(r)
assert changed['session']['model']=='swe-2-medium'
(folder/'temporary-tool.json').write_text(json.dumps({'configuration_revision':changed['configuration_revision'],'parameters':changed['parameters']},ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'enabled':True,'tool':tool,'revision':changed['configuration_revision'],'entry_sha256':before['entry_sha256'],'binding':'island-kayak'},ensure_ascii=False))
