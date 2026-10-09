from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent;root=p.parents[1]
previous=root/'tmp/2026-10-09-browser-progress'
checks=(previous/'checks.py').read_text(encoding='utf-8')
checks=checks.replace(" 'linkage':", " 'tooling':['cargo','check','-p','coolzhu-tool-registry','--offline','-j','1'],\n 'linkage':")
(p/'checks.py').write_text(checks,encoding='utf-8')
files=json.loads((previous/'candidate-source-v2.json').read_text(encoding='utf-8'))
files += [{'path':v} for v in (
 'modules/gui-desktop/packages/tauri-shell/src-tauri/src/native_browser_nodes.rs',
 'modules/gui-desktop/packages/tauri-shell/src-tauri/src/native_browser_observation.rs') if v not in [f['path'] for f in files]]
for f in files:
 raw=(root/f['path']).read_bytes();f.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
(p/'candidate-source.json').write_text(json.dumps(files,indent=2)+'\n',encoding='utf-8')
start=(previous/'start-candidate-v2.ps1').read_text(encoding='utf-8')
start=start.replace("$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-browser-progress'", "$taskTmp=Join-Path $taskRoot 'tmp/2026-10-09-browser-scope'")
start=start.replace("'tmp/2026-10-09-browser-progress/candidate-processes.json'", "'tmp/2026-10-09-browser-progress/candidate-v2-processes.json'")
for kind in ('web-build','web'):
 start=start.replace('tmp/2026-10-09-browser-progress/'+kind+'-v2-result.json','tmp/2026-10-09-browser-scope/'+kind+'-result.json')
start=start.replace('tmp/2026-10-09-browser-progress/shell-build-result.json','tmp/2026-10-09-browser-scope/shell-build-result.json')
start=start.replace('candidate-source-v2.json','candidate-source.json').replace('candidate-bundle-v2','candidate-bundle')
start=start.replace('candidate-v2-', 'candidate-').replace('第一候选三项身份不符','前一候选三项身份不符')
start=start.replace('AX索引漂移修正第二候选Web与壳','滚动稳定文档身份候选Web与壳')
start=start.replace("'tmp/2026-10-09-browser-progress/candidate-processes.json'", "'tmp/2026-10-09-browser-progress/candidate-v2-processes.json'")
start=start.replace("'candidate-bundle'", "'candidate-bundle-retry'")
(p/'start-candidate.ps1').write_text(start,encoding='utf-8')
print('候选源文件清单及构建/启动脚本已生成，前一失败事实不覆盖')
