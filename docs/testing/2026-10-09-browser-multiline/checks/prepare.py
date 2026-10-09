from pathlib import Path
import json,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1]
source=[]
for name in ('modules/gui-web/packages/web-console/src/native_browser_verification.rs','modules/gui-web/packages/web-console/src/computer_use_planner.rs','modules/computer-use/packages/computer-use-core/src/controller.rs'):
 raw=(root/name).read_bytes();source.append({'path':name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(p/'candidate-source.json').write_text(json.dumps(source,indent=2)+'\n',encoding='utf-8')
text=(root/'tmp/2026-10-09-browser-live-repaint/start-candidate.ps1').read_text(encoding='utf-8')
text=text.replace("'tmp/2026-10-09-browser-live-repaint'","'tmp/2026-10-09-browser-multiline'")
text=text.replace('release-119/installed-119-standard-processes','release-121/installed-121-standard-processes')
text=text.replace('tmp/2026-10-09-tool-dispatch-service/build-lowmem-result.json','tmp/2026-10-09-browser-multiline/build-result.json')
text=text.replace('tmp/2026-10-09-tool-dispatch-service/web-lowmem-result.json','tmp/2026-10-09-browser-multiline/web-result.json')
text=text.replace('119','121').replace('局部证据新鲜度/ToolDispatchService源码候选+121壳','多行引文/停止部分验收源码候选+121壳')
(p/'start-candidate.ps1').write_text(text,encoding='utf-8')
browser=p/'browser-ledger';browser.mkdir(exist_ok=True)
old=root/'tmp/2026-10-09-release-121/browser-ledger'
for name in ('server.py','prepare.py','send.py','collect.py','watch.py','verify.py','verify-diagnostics.py','inspect-facts.py'):
 text=(old/name).read_text(encoding='utf-8').replace('BU-CROSS-ORIGIN-LIVE-LEDGER-INSTALLED121-20261009','BU-CROSS-ORIGIN-LIVE-LEDGER-MULTILINE-CANDIDATE-20261009')
 text=text.replace('0.2.121 Program Files正式安装版独立动态长程','多行引文源码候选+121壳，非正式安装验收')
 # 同深度目录保持真正原工作区；不复制旧submitted/事件/订单。
 (browser/name).write_text(text,encoding='utf-8')
print('候选源码摘要及新独立随机订单驱动准备完成，未发模型请求')
