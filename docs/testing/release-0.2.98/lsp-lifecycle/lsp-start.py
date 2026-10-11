"""正式版真实rust-analyzer生命周期补验；仅创建本轮无依赖Rust工程。"""
from pathlib import Path
import json,urllib.request,urllib.error,time,hashlib
folder=Path(__file__).resolve().parent
receipt=json.loads((folder/'installed-098-standard-processes.json').read_text(encoding='utf-8-sig'))
root=Path(receipt['workspace']);probe=root/'tmp/lsp-098/src';probe.mkdir(parents=True,exist_ok=True)
def api(path,body=None,expected=200):
 request=urllib.request.Request('http://127.0.0.1:8765'+path,json.dumps(body,ensure_ascii=False).encode() if body is not None else None,{'Content-Type':'application/json'})
 try:
  with urllib.request.urlopen(request,timeout=50) as r:status=r.status;data=json.load(r)
 except urllib.error.HTTPError as e:status=e.code;data=json.loads(e.read())
 assert status==expected,(status,data)
 return data
before=api('/api/lsp/status');assert before['configured'] and not before['running'],'保留已存在实例或配置状态'
manifest='[package]\nname="coolzhu-lsp-098-acceptance"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[lib]\npath="tmp/lsp-098/src/lib.rs"\n'
source='pub fn bamboo_total(a: u32, b: u32) -> u32 { a * b }\n\npub fn invoice() -> u32 {\n    let total = bamboo_total(47, 13);\n    let = ;\n    total\n}\n'
for path,content in [(root/'Cargo.toml',manifest),(probe/'lib.rs',source)]:
 with path.open('x',encoding='utf-8') as f:f.write(content)
owners={str(path):hashlib.sha256(content.encode()).hexdigest() for path,content in [(root/'Cargo.toml',manifest),(probe/'lib.rs',source)]}
(folder/'lsp-owned-files.json').write_text(json.dumps(owners,indent=2)+'\n',encoding='utf-8')
body={'path':'tmp/lsp-098/src/lib.rs','session_id':'session-1791131217833','chat_room_id':'room-1791131523339','expected_workspace':str(root)}
started=api('/api/lsp/start',body)
assert started['running'] and started['handle']
(folder/'lsp-owner.json').write_text(json.dumps({'request':body,'started':started,'web_pid':receipt['web_pid']},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
diagnostics=[];deadline=time.monotonic()+30
while time.monotonic()<deadline:
 result=api('/api/lsp/diagnostics',{'handle':started['handle'],'path':body['path']});diagnostics=result['diagnostics']
 if any(d['line']==5 and 'expected pattern' in d['message'] for d in diagnostics):break
 time.sleep(1)
else:raise RuntimeError('真实rust-analyzer未返回实际语法诊断')
nav=api('/api/lsp/navigation',{'handle':started['handle'],'path':body['path'],'line':4,'character':20,'kind':'definition'})
assert any(x['path']==body['path'] and x['line']==1 for x in nav['locations']),nav
(folder/'lsp-start-result.json').write_text(json.dumps({'stage':'0.2.98正式安装版','before':before,'started':started,'diagnostics':diagnostics,'definition':nav,'generated_model_requests':0},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'real_rust_analyzer':True,'syntax_diagnostic_line':5,'definition_line':1,'handle':started['handle']},ensure_ascii=False))
