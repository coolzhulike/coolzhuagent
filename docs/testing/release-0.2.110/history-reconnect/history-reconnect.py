"""正式EXE断线重连/跨页外部写入实操；生成用户存储资料，不伪造模型回复。"""
from pathlib import Path
import hashlib,json,os,sqlite3,socket,subprocess,time,urllib.request
p=Path(__file__).resolve().parent;repo=p.parents[1];folder=p/'history-reconnect';folder.mkdir(exist_ok=True)
workspace=folder/'workspace';assert not workspace.exists();(workspace/'.coolzhu').mkdir(parents=True)
with socket.socket() as s:assert s.connect_ex(('127.0.0.1',8767))!=0
source=p/'rotation-live/workspace/.coolzhu/web-sessions.sqlite3';db=workspace/'.coolzhu/web-sessions.sqlite3'
src=sqlite3.connect(source.resolve().as_uri()+'?mode=ro',uri=True);dst=sqlite3.connect(db);src.backup(dst);src.close()
room='history-reconnect-110';base=int(time.time())-10000
dst.execute('INSERT INTO chat_rooms VALUES(?,?,?,?)',(room,'断线跨页存储验收',base,base))
def row(i,new=False):
    return (room,('new-' if new else 'old-')+f'{i:04d}','存储验收','user','',('重连新增' if new else '原始资料')+f' {i:04d}','user-message','[]',base+(1000 if new else 0)+i)
dst.executemany('INSERT INTO chat_room_messages (room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)',[row(i) for i in range(1,351)]);dst.commit();dst.close()
(workspace/'coolzhu.toml').write_text('[web]\nbind_addr="127.0.0.1:8767"\n[pet]\nenabled=false\n[model]\nenable_real_llm=true\n',encoding='utf-8')
binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe');facts=json.loads((p/'installed-110-verification.json').read_text())
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==facts['web_sha256']
env=os.environ.copy();env['COOLZHU_RUNTIME_DIR']=str(workspace);env['COOLZHU_LOG_DIR']=str(folder/'logs');env['COOLZHU_INPUT_SAFETY_STATE_ROOT']=str(folder/'isolated-input-safety')
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
processes=[]
def start(name):
    out=(folder/(name+'-stdout.txt')).open('wb');err=(folder/(name+'-stderr.txt')).open('wb')
    process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
    processes.append((name,process,out,err))
    for _ in range(80):
        assert process.poll() is None
        try:
            with urllib.request.urlopen('http://127.0.0.1:8767/api/chat/rooms',timeout=1) as r:json.load(r)
            return process
        except OSError:time.sleep(.1)
    raise RuntimeError('正式隔离服务未就绪')
try:
    first=start('before');print(json.dumps({'stage':'initial-ready','room':room,'pid':first.pid,'initial_rows':350}),flush=True)
    while not (folder/'disconnect').exists():time.sleep(.2)
    first.terminate();first.wait(timeout=10)
    (folder/'disconnected.json').write_text(json.dumps({'pid':first.pid,'exit_code':first.returncode,'reason':'仅停止自有空闲验收后台，产生真实断线'})+'\n')
    print('服务已停止，等待页面确认真实断线',flush=True)
    while not (folder/'mutate-and-restart').exists():time.sleep(.2)
    connection=sqlite3.connect(db)
    with connection:
        connection.execute('DELETE FROM chat_room_messages WHERE room_id=? AND id IN (?,?,?)',(room,'old-0271','old-0300','old-0350'))
        connection.execute('UPDATE chat_room_messages SET content=? WHERE room_id=? AND id=?',('原始资料 0320 已外部修改',room,'old-0320'))
        connection.executemany('INSERT INTO chat_room_messages (room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES(?,?,?,?,?,?,?,?,?)',[row(i,True) for i in range(1,451)])
    rows=connection.execute('SELECT id,content FROM chat_room_messages WHERE room_id=? ORDER BY created_at,id',(room,)).fetchall();connection.close()
    expected=[x for x in rows if x[0].startswith('new-') or int(x[0].split('-')[1])>=271]
    (folder/'expected-ui.json').write_text(json.dumps({'rows':expected,'count':len(expected),'model_requests':0,'cloud_sessions_created':0,'original_runtime_database_writes':0},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    second=start('after');print(json.dumps({'stage':'restarted','pid':second.pid,'expected_ui_rows':len(expected),'db_rows':len(rows)}),flush=True)
    while not (folder/'stop').exists():time.sleep(.2)
finally:
    cleanup=[]
    for name,process,out,err in processes:
        if process.poll() is None:process.terminate()
        process.wait(timeout=10);out.close();err.close();cleanup.append({'phase':name,'pid':process.pid,'exit_code':process.returncode})
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps(cleanup,indent=2)+'\n')
