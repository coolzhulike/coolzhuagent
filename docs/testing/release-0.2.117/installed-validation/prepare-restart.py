from pathlib import Path
import json,urllib.request
p=Path(__file__).resolve().parent;root=p.parents[1];f=p/'formal'
r=json.loads((f/'ready.json').read_text());port=r['port']
with urllib.request.urlopen(f'http://127.0.0.1:{port}/api/sessions/scope-shared-session/model-settings') as req:saved=json.load(req)
assert saved['parameters']['temperature']==.45 and saved['configuration_revision']==12
(f/'native-saved-settings.json').write_text(json.dumps(saved,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
old=root/'tmp/2026-10-09-session-config-late'
text=(old/'restart.py').read_text(encoding='utf-8').replace("f=p/'candidate2'","f=p/'formal'").replace("binary=root/'target/debug/coolzhu-web-console.exe'","binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe')").replace('50528',str(port)).replace('同候选EXE','同正式117EXE')
(p/'restart.py').write_text(text,encoding='utf-8')
stop=(old/'stop-shell.ps1').read_text(encoding='utf-8-sig').replace('tmp/2026-10-09-session-config-late/candidate2','tmp/2026-10-09-release-117/formal')
(p/'stop-isolated-shell.ps1').write_text(stop,encoding='utf-8')
print('正常GUI保存0.45/revision12已独立核对，正式同EXE重启脚本已准备')
