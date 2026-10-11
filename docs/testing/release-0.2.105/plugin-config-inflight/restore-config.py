"""旧真实轮次已排空后，正常恢复插件空配置，不手改快照/世代。"""
from pathlib import Path
import json,urllib.request
folder=Path(__file__).resolve().parent;root=folder.parents[1]
facts=json.loads((folder/'result.json').read_text(encoding='utf-8'))
assert facts['run']['state'] in ('completed','failed','interrupted','cancelled','canceled')
assert len(facts['attempts'])==1 and facts['attempts'][0]['process_drained']==1
observation=json.loads((folder/'config-observation.json').read_text(encoding='utf-8'))
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
source=json.loads((root/'tmp/2026-10-08-plugin-reactivation/source.json').read_text(encoding='utf-8'))
settings_path=Path('C:/Users/zhupu/.claw/settings.json')
snapshot=json.loads(settings_path.read_text(encoding='utf-8'))['dshSnapshots'][before['plugin_id']]
assert snapshot['config']==observation['changed_config'] and snapshot['activation_id']==observation['changed_activation_id']
body={'expected_workspace':'ws-23f646a969206cb4','id':before['plugin_id'],
    'expected_source_sha256':source['source_sha256'],'session_id':'session-1791131217833',
    'chat_room_id':'room-1791131523339','config':observation['original_config']}
req=urllib.request.Request('http://127.0.0.1:8765/api/extension-market/dsh/enable',json.dumps(body).encode(),{'Content-Type':'application/json'})
with urllib.request.urlopen(req,timeout=135) as response:result=json.load(response)
assert result['enabled']
snapshot=json.loads(settings_path.read_text(encoding='utf-8'))['dshSnapshots'][before['plugin_id']]
assert snapshot['config']==observation['original_config']
(folder/'config-restored.json').write_text(json.dumps({'config':snapshot['config'],'activation_id':snapshot['activation_id'],'normal_enable_response':result},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('真实轮次排空后，已通过正常接口恢复空配置；新世代不倒退。')
