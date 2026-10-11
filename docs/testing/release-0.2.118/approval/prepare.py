from common import *
import re
assert not (p/'before.json').exists()
v=identity();s=http('/api/sessions/'+sid+'/model-settings');assert s['session']['model']=='swe-2-medium'
perm=http('/api/chat/rooms/'+room+'/permissions');assert perm['full_access']
if perm['dev_open_permissions']:
    config=db.parent.parent/'coolzhu.toml';original=config.read_bytes()
    changed,count=re.subn(rb'(?m)^dev_open_permissions = true\r?$',lambda m:m.group(0).replace(b'true',b'false'),original)
    assert count==1
    save('development-override.json',{'before_sha256':hashlib.sha256(original).hexdigest(),'temporary_sha256':hashlib.sha256(changed).hexdigest(),'original_enabled':True})
    try:
        config.write_bytes(changed);http('/api/workspace/reload',{},'POST')
    finally:
        assert config.read_bytes()==changed;config.write_bytes(original)
    perm=http('/api/chat/rooms/'+room+'/permissions');assert perm['full_access'] and not perm['dev_open_permissions']
assert s['parameters']['llm_tool_exposure']=='whitelist' and tool not in s['parameters']['tool_allowlist']
source=json.loads((root/'tmp/2026-10-08-plugin-reactivation/source.json').read_text(encoding='utf-8'))
save('before.json',{'model':s['session']['model'],'parameters':s['parameters'],'revision':s['configuration_revision'],'permission':perm,'source_commit':v['source_commit'],'plugin_source_sha256':source['source_sha256']})
enabled=http('/api/extension-market/dsh/enable',{'expected_workspace':ws,'id':plugin,'expected_source_sha256':source['source_sha256'],'session_id':sid,'chat_room_id':room,'config':{}})
save('enabled.json',enabled);assert enabled['enabled']
s=http('/api/sessions/'+sid+'/model-settings');assert any(x['name']==tool for x in s['available_plugin_tools'])
added=http('/api/sessions/'+sid+'/model-settings',{'parameters':{**s['parameters'],'tool_allowlist':[*s['parameters']['tool_allowlist'],tool]},'expected_revision':s['configuration_revision']},'POST')
save('tool-added.json',{'parameters':added['parameters'],'revision':added['configuration_revision']})
restricted=permission('workspace-write');save('restricted.json',restricted);assert not restricted['effective_full_access']
print('118身份、原SWE/唯一绑定、实际插件及本轮目录权限准备完成；尚未调用模型')
