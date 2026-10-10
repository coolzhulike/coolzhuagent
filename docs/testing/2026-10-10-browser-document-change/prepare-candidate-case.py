from pathlib import Path
p=Path(__file__).resolve().parent;q=p.parent/'2026-10-10-browser-document-change-candidate'
q.mkdir(exist_ok=True);assert not (q/'submitted.json').exists()
(p/'stop-server').touch()
for n in ('server.py','send.py','collect.py'):
 t=(p/n).read_text(encoding='utf-8').replace('BU-DOCUMENT-CHANGE126-20261010','BU-DOCUMENT-CHANGE-CANDIDATE-20261010')
 t=t.replace('0.2.126正式规划期间文档切换验收','源码候选规划期间文档切换验收').replace('正式0.2.126原生Browser','源码候选原生Browser')
 t=t.replace("root/'tmp/2026-10-10-release-126/installed-126-standard-processes.json'","root/'tmp/2026-10-10-browser-document-change/candidate-processes.json'")
 t=t.replace("installed['stage']=='0.2.126 Program Files正式安装版'","installed['stage']=='文档失效报告源码候选（原唯一Devin绑定）'")
 (q/n).write_text(t,encoding='utf-8')
print('候选独立场景已准备；尚未发送模型请求')
