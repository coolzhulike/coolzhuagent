from common import *
assert (p/'before.json').exists() and not (p/'tool-added.json').exists()
assert json.loads((p/'enabled.json').read_text(encoding='utf-8'))['enabled'] is False
(p/'enabled-initial-failed.json').write_bytes((p/'enabled.json').read_bytes())
identity()
source=json.loads((root/'tmp/2026-10-08-plugin-reactivation/source.json').read_text(encoding='utf-8'))
enabled=http('/api/extension-market/dsh/enable',{'expected_workspace':ws,'id':plugin,'expected_source_sha256':source['source_sha256'],'session_id':sid,'chat_room_id':room,'config':{}})
save('enabled.json',enabled);assert enabled['enabled']
s=http('/api/sessions/'+sid+'/model-settings');assert any(x['name']==tool for x in s['available_plugin_tools'])
added=http('/api/sessions/'+sid+'/model-settings',{'parameters':{**s['parameters'],'tool_allowlist':[*s['parameters']['tool_allowlist'],tool]},'expected_revision':s['configuration_revision']},'POST')
save('tool-added.json',{'parameters':added['parameters'],'revision':added['configuration_revision']})
# 重启会重新读原开发覆盖，临时重载关闭，再恢复磁盘布尔字节。
import re
config=db.parent.parent/'coolzhu.toml';raw=config.read_bytes()
changed,n=re.subn(rb'(?m)^dev_open_permissions = true\r?$',lambda m:m.group(0).replace(b'true',b'false'),raw);assert n==1
try:config.write_bytes(changed);http('/api/workspace/reload',{},'POST')
finally:assert config.read_bytes()==changed;config.write_bytes(raw)
restricted=permission('workspace-write');save('restricted.json',restricted);assert not restricted['effective_full_access']
print('候选真实DSH及目录权限就绪，尚未调用模型')
