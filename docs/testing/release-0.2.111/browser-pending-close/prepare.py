from pathlib import Path
root=Path(__file__).resolve().parents[2]
old=root/'tmp/2026-10-08-release-109/resource-close'
new=Path(__file__).resolve().parent
assert not (new/'submitted-request.json').exists()
for name in ('server.py','send.py','status.py'):
    data=(old/name).read_text(encoding='utf-8').replace('BU-RESOURCE-CLOSE-109-20261008','BU-PENDING-CLOSE-111-20261008').replace('OBSERVE109','OBSERVE111')
    if name=='send.py':
        data=data.replace("folder.parent/'installed-109-verification.json'", "repo/'tmp/2026-10-08-release-111/installed-111-verification.json'")
        data=data.replace("installed['version']=='0.2.109'", "installed['version']=='0.2.111'")
        data=data.replace("db=repo/", "assert installed['source_commit']=='20facc6c2c03f8969fe592b01ea3e59e12381c2b'\nfrom hashlib import sha256\nassert sha256(Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe').read_bytes()).hexdigest()==installed['web_sha256']\nassert sha256(Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-tauri-shell.exe').read_bytes()).hexdigest()==installed['shell_sha256']\ndb=repo/")
    if name=='server.py':
        data=data.replace("self.path=='/await-observation'", "self.path in ('/await-observation','/watch-phase')")
        data=data.replace("{'ready':bool(found)}", "{'ready':bool(found),'facts':found,'returned_ms':time.time()*1000}")
    (new/name).write_text(data,encoding='utf-8')
print('准备完成；尚未提交模型请求')
