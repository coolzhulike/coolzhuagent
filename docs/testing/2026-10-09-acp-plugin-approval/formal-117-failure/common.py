"""正式117正常产品接口；不调用私有桥或改写事实库。"""
from pathlib import Path
import json,urllib.request,sqlite3,hashlib,time
p=Path(__file__).resolve().parent;root=p.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
base='http://127.0.0.1:8765';sid='session-1791131217833';room='room-1791131523339';ws='ws-23f646a969206cb4'
plugin='dsh-aa909f79795799f288de8a67@external';tool='dsh__1ef9be67fd4ddb42d93d68c084b38983'
marker='PLUGIN-FROZEN-PERMISSION-117-20261009'
def http(path,body=None,method=None):
    data=None if body is None else json.dumps(body).encode()
    req=urllib.request.Request(base+path,data,{'Content-Type':'application/json'},method=method)
    with urllib.request.urlopen(req,timeout=60) as r:return json.load(r)
def save(name,value): (p/name).write_text(json.dumps(value,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def connection():return sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True)
def permission(profile):
    current=http('/api/workspace');assert current['workspace_id']==ws
    return http('/api/chat/rooms/'+room+'/permissions',{'expected_workspace':current['workspace'],'permission_profile':profile,'risk_acknowledged':True,'confirmed_twice':True},'PATCH')
def identity(require_unlocked=True):
    v=json.loads((root/'tmp/2026-10-09-release-117/installed-117-verification.json').read_text(encoding='utf-8'))
    assert v['version']=='0.2.117' and v['source_commit']=='d094c7ae6e9def40bb4c0bfb6ed81d33bbb3f0ae'
    for binary,key in [('coolzhu-web-console.exe','web_sha256'),('coolzhu-tauri-shell.exe','shell_sha256')]:
        assert hashlib.sha256((Path('C:/Program Files/CoolzhuAgent/bin')/binary).read_bytes()).hexdigest()==v[key]
    with connection() as c:
        assert not c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
        b=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(sid,)).fetchall()
        assert sum(x[1]=='island-kayak' for x in b)==1
        if require_unlocked:assert not any(x[2] for x in b)
    return v
