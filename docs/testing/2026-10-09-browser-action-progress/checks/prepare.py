from pathlib import Path
import json,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1]
files=['modules/computer-use/packages/native-browser-protocol/src/lib.rs','modules/gui-desktop/packages/tauri-shell/src-tauri/src/native_browser_edit_input.rs','modules/gui-desktop/packages/tauri-shell/src-tauri/src/native_browser_editor.rs','modules/gui-desktop/packages/tauri-shell/src-tauri/src/native_browser_input.rs','modules/gui-web/packages/web-console/src/native_browser_adapter.rs','modules/gui-web/packages/web-console/src/native_browser_input.rs','modules/gui-web/packages/web-console/src/native_browser_verification.rs']
source=[]
for name in files:
 raw=(root/name).read_bytes();source.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(p/'candidate-source.json').write_text(json.dumps(source,indent=2)+'\n',encoding='utf-8')
text=(p.parent/'2026-10-09-browser-multiline/start-candidate.ps1').read_text(encoding='utf-8')
text=text.replace("'tmp/2026-10-09-browser-multiline'","'tmp/2026-10-09-browser-progress'").replace('release-121/installed-121-standard-processes','release-122/installed-122-standard-processes')
text=text.replace('browser-multiline/build-result.json','browser-progress/web-build-result.json').replace('browser-multiline/web-result.json','browser-progress/web-result.json').replace('browser-multiline/core-result.json','browser-progress/shell-build-result.json')
text=text.replace("Copy-Item -LiteralPath (Join-Path $taskRoot 'target/debug/coolzhu-web-console.exe') -Destination (Join-Path $taskBundle 'bin/coolzhu-web-console.exe')", "Copy-Item -LiteralPath (Join-Path $taskRoot 'target/debug/coolzhu-web-console.exe') -Destination (Join-Path $taskBundle 'bin/coolzhu-web-console.exe')\nCopy-Item -LiteralPath (Join-Path $taskRoot 'modules/gui-desktop/packages/tauri-shell/src-tauri/target/debug/coolzhu-tauri-shell.exe') -Destination (Join-Path $taskBundle 'bin/coolzhu-tauri-shell.exe')")
old="if($taskShellHash -ine $taskReceipt.shell_sha256){throw '候选壳并非121已核验文件'}"
assert old in text;text=text.replace(old,"if($taskShellHash -ine (Get-FileHash -LiteralPath (Join-Path $taskRoot 'modules/gui-desktop/packages/tauri-shell/src-tauri/target/debug/coolzhu-tauri-shell.exe')).Hash){throw '候选壳不是本轮构建'}")
text=text.replace('121三项身份','122三项身份').replace('多行引文/停止部分验收源码候选+121壳','本步效果源码候选Web与壳')
needle="$taskSource=Get-Content -Raw -LiteralPath (Join-Path $taskTmp 'candidate-source.json')|ConvertFrom-Json"
assert needle in text
text=text.replace(needle,"foreach($taskCheck in @('protocol','shell','linkage')){if((Get-Content -Raw -LiteralPath (Join-Path $taskTmp ($taskCheck+'-result.json'))|ConvertFrom-Json).exit_code -ne 0){throw '候选必需回归未通过'}}\n"+needle)
(p/'start-candidate.ps1').write_text(text,encoding='utf-8')
browser=p/'browser-ledger';browser.mkdir(exist_ok=True)
old=p.parent/'2026-10-09-release-122/browser-ledger'
for name in ('server.py','prepare.py','send.py','collect.py','watch.py','verify.py','verify-diagnostics.py','inspect-facts.py'):
 text=(old/name).read_text(encoding='utf-8').replace('BU-CROSS-ORIGIN-LIVE-LEDGER-INSTALLED122-20261009','BU-CROSS-ORIGIN-LIVE-LEDGER-PROGRESS-CANDIDATE-20261009').replace('0.2.122正式安装版综合长程','本步效果源码候选Web与壳，非正式安装验收')
 # 此修补会将只有业务文字变化的Enter判效果未知；目标成功继续独立验收。
 if name=='verify.py':
  text=text.replace("assert all(x['input_delivery']=='sent' and x['effect_status']=='effect_observed' for x in steps)","assert all(x['input_delivery']=='sent' for x in steps)\nassert any(x['action_kind']=='text_input' and x['effect_status']=='effect_observed' for x in steps)\nassert any(x['effect_status']=='inconclusive' for x in steps), '业务文字变化不应冒领本步效果'")
 (browser/name).write_text(text,encoding='utf-8')
print('新双端源码候选与独立长程驱动准备，尚未发模型请求')
