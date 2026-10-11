"""真实正式099的旧索引新文件与根目录预算验证，未调用模型。"""
import hashlib,json,pathlib,sqlite3,sys,urllib.request,urllib.parse
folder=pathlib.Path(__file__).resolve().parent
receipt=json.loads((folder/'installed-099-standard-processes.json').read_text(encoding='utf-8-sig'))
root=pathlib.Path(receipt['workspace']);name='新增文件-099-竹林.rs'
owned=root/name
content='// 正式0.2.99新增文件，无需重建符号索引即可精确打开。\npub fn jade_scroll() -> u32 { 99 }\n'
def api(path,query=None):
    url='http://127.0.0.1:8765'+path+('?' +urllib.parse.urlencode(query) if query else '')
    with urllib.request.urlopen(url,timeout=10) as response:return json.load(response)
def state():
    with sqlite3.connect((root/'.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as db:
        bindings=db.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=?',('session-1791131217833',)).fetchall()
        assert sum(x[1]=='island-kayak' for x in bindings)==1 and all(x[2] is None for x in bindings)
        counts={t:db.execute('select count(*) from '+t).fetchone()[0] for t in ['devin_acp_attempts','chat_usage_events']}
    return {'counts':counts,'bindings':bindings}
def save(name,value): (folder/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
mode=sys.argv[1]
if mode=='prepare':
    assert not owned.exists()
    before=state()
    index=root/'.coolzhu/files.json'
    # 索引位置由项目目录递归读取真实文件，避免假设它存在。
    indexes=list((root/'.coolzhu').rglob('files.json'))
    assert indexes,'需有真实旧索引作为回归条件'
    old_indexes=[{'path':str(p),'sha256':hashlib.file_digest(p.open('rb'),'sha256').hexdigest()} for p in indexes]
    assert all(name not in p.read_text(encoding='utf-8') for p in indexes)
    owned.write_text(content,encoding='utf-8')
    tree=api('/api/project/tree',{'depth':4,'limit':400})
    search=api('/api/project/search-files',{'query':name,'limit':50})
    assert any(x['name']==name for x in tree['root']['children'])
    assert search['files'][0]['path']==name
    assert old_indexes==[{'path':str(p),'sha256':hashlib.file_digest(p.open('rb'),'sha256').hexdigest()} for p in indexes]
    assert state()==before
    save('project-browser-result.json',{'stage':'0.2.99正式安装版HTTP通过，UI另验','tree':tree,'search':search,'old_indexes':old_indexes,'state_before':before,'state_after':state(),'file':{'path':str(owned),'sha256':hashlib.file_digest(owned.open('rb'),'sha256').hexdigest()},'model_requests':0})
    print('正式099通过：旧索引不变、根文件可见、新Unicode文件精确路径首项，模型调用0')
elif mode=='cleanup':
    saved=json.loads((folder/'project-browser-result.json').read_text(encoding='utf-8'))
    assert hashlib.file_digest(owned.open('rb'),'sha256').hexdigest()==saved['file']['sha256']
    assert json.loads(json.dumps(state()))==saved['state_before']
    owned.unlink()
    save('project-browser-cleanup.json',{'only_owned_file_removed':str(owned),'model_requests':0,'same_binding_and_counts':True})
    print('仅清理本轮自有文件，原绑定及模型计数不变')
else:raise ValueError(mode)
