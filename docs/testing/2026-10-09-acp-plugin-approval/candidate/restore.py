"""正常产品入口只撤回本轮项，恢复落盘原配置；不重写版本/事实库。"""
from common import *
import re
identity(require_unlocked=False)
before=json.loads((p/'before.json').read_text(encoding='utf-8')) if (p/'before.json').exists() else None
permission_before=http('/api/chat/rooms/'+room+'/permissions')
if before:
    restored=permission(before['permission']['permission_profile']);save('permission-restored.json',restored)
    s=http('/api/sessions/'+sid+'/model-settings')
    if tool in s['parameters']['tool_allowlist']:
        r=http('/api/sessions/'+sid+'/model-settings',{'parameters':{**s['parameters'],'tool_allowlist':[x for x in s['parameters']['tool_allowlist'] if x!=tool]},'expected_revision':s['configuration_revision']},'POST')
        save('model-restored.json',{'parameters':r['parameters'],'revision':r['configuration_revision']})
        assert r['parameters']==before['parameters']
    disabled=http('/api/extension-market/plugins/action',{'expected_workspace':ws,'id':plugin,'action':'disable'});save('plugin-disabled.json',disabled)
if (p/'development-override.json').exists():
    override=json.loads((p/'development-override.json').read_text(encoding='utf-8'))
    config=db.parent.parent/'coolzhu.toml';raw=config.read_bytes()
    restored,count=re.subn(rb'(?m)^dev_open_permissions = false\r?$',lambda m:m.group(0).replace(b'false',b'true'),raw)
    if count:assert count==1;config.write_bytes(restored)
    save('config-restoration.json',{'sha256':hashlib.sha256(config.read_bytes()).hexdigest(),'original_sha256':override['before_sha256'],'byte_identical':hashlib.sha256(config.read_bytes()).hexdigest()==override['before_sha256']})
    http('/api/workspace/reload',{},'POST')
final=http('/api/chat/rooms/'+room+'/permissions');save('final-permission.json',final)
assert final['full_access'] and final['dev_open_permissions']
identity(require_unlocked=False);(p/'stop-server').write_text('正常结束\n',encoding='utf-8')
print('原完整权限及开发覆盖已恢复；本轮白名单撤回、插件恢复停用；旧未知发送锁保留待产品正常修复')
