"""已排空后撤回临时白名单并正常停用验收插件，不覆写其它参数。"""
from pathlib import Path
import json,hashlib,urllib.request,sqlite3
folder=Path(__file__).resolve().parent;root=folder.parents[1]
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
facts=json.loads((folder/'result.json').read_text(encoding='utf-8'))
assert facts['attempts'][0]['process_drained']==1 and facts['run']['state'] in ('completed','failed','interrupted','cancelled','canceled')
obs={'process_drained':1,'restored_sha256':before['entry_sha256']}
address=json.loads((folder/'address.json').read_text(encoding='utf-8'))
assert obs['process_drained']==1 and obs['restored_sha256']==before['entry_sha256']
assert hashlib.sha256(Path(before['entry_path']).read_bytes()).hexdigest()==before['entry_sha256']
base='http://127.0.0.1:8765'
path='/api/sessions/session-1791131217833/model-settings'
def request(path,body=None):
    req=urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(req,timeout=135) as r:return json.load(r)
current=request(path)
assert current['session']['model']=='swe-2-medium'
parameters={**current['parameters'],'tool_allowlist':[x for x in current['parameters']['tool_allowlist'] if x!=address['tool_name']]}
assert parameters==before['parameters'],'其它参数发生变化，需独立核验'
saved=request(path,{'parameters':parameters,'expected_revision':current['configuration_revision']})
disabled=request('/api/extension-market/plugins/action',{'expected_workspace':'ws-23f646a969206cb4','id':before['plugin_id'],'action':'disable'})
settings=json.loads(Path('C:/Users/zhupu/.claw/settings.json').read_text(encoding='utf-8'))
assert settings['enabledPlugins'].get(before['plugin_id'])!=True
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
    bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',)).fetchall()
    assert not any(b[2] for b in bindings) and sum(b[1]=='island-kayak' for b in bindings)==1
result={'configuration_revision':saved['configuration_revision'],'parameters':saved['parameters'],'model':saved['session']['model'],'plugin_disabled':True,'disable_response':disabled,'bindings':bindings,'restored_entry_sha256':obs['restored_sha256']}
(folder/'restored.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
(folder/'stop-server').write_text('normal cleanup\n',encoding='utf-8')
print(json.dumps({'revision':saved['configuration_revision'],'plugin_disabled':True,'sha_restored':True,'binding':'island-kayak'},ensure_ascii=False))
