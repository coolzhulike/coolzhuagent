"""正式EXE对隔离副本的WAL迁移/恢复演练；原库仅只读，不发送模型请求。"""
from pathlib import Path
import hashlib,json,os,sqlite3,socket,subprocess,time,urllib.request,uuid
folder=Path(__file__).resolve().parent/'wal-drill3';folder.mkdir(exist_ok=True)
repo=folder.parents[2]
source=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe')
verified=json.loads((folder.parent/'installed-109-verification.json').read_text(encoding='utf-8'))
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==verified['web_sha256']
for port in (8767,8768):
    with socket.socket() as s:assert s.connect_ex(('127.0.0.1',port))!=0,f'{port}已占用，保留服务'
for name in ('upgrade','restored'):
    workspace=folder/name
    assert not workspace.exists(),'不覆盖既有演练'
    (workspace/'.coolzhu').mkdir(parents=True)
    (workspace/'coolzhu.toml').write_text(f'[web]\nbind_addr = "127.0.0.1:{8767 if name=="upgrade" else 8768}"\n[pet]\nenabled = false\n[model]\nenable_real_llm = true\n',encoding='utf-8')
def ro(path):return sqlite3.connect(path.resolve().as_uri()+'?mode=ro',uri=True)
def marker_count(c,marker):return c.execute('SELECT COUNT(*) FROM chat_room_messages WHERE id=?',(marker,)).fetchone()[0]
def metadata(c):
    return {'schema_version':c.execute('PRAGMA user_version').fetchone()[0],
            'quick_check':c.execute('PRAGMA quick_check').fetchone()[0],
            'messages':c.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0]}
db=folder/'upgrade/.coolzhu/web-sessions.sqlite3'
with ro(source) as src,sqlite3.connect(db) as dest:
    assert src.execute("SELECT COUNT(*) FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchone()[0]==0
    original=metadata(src);src.backup(dest)
src.close();dest.close()
writer=sqlite3.connect(db)
writer.execute('PRAGMA journal_mode=DELETE').fetchall()
# 仅隔离副本回到真实v27结构，不能只改版本号伪装旧库。
writer.execute('DROP INDEX IF EXISTS idx_cu_native_ticket')
for name in ('native_dispatch_state','native_ticket_id','native_binding_json'):
    writer.execute('ALTER TABLE computer_use_steps DROP COLUMN '+name)
writer.execute('PRAGMA user_version=27')
writer.execute("UPDATE metadata SET value='27' WHERE key='schema_contract_version'")
writer.commit()
writer.close();writer=sqlite3.connect(db)
writer.execute('PRAGMA journal_mode=WAL').fetchall();writer.execute('PRAGMA wal_autocheckpoint=0').fetchall()
writer.execute('PRAGMA wal_checkpoint(TRUNCATE)').fetchall()
reader=sqlite3.connect(db);reader.execute('BEGIN');reader.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()
room='wal-restore-'+uuid.uuid4().hex
before_id='WAL-BEFORE-'+uuid.uuid4().hex
after_id='WAL-AFTER-'+uuid.uuid4().hex
now=int(time.time()*1000)
writer.execute('INSERT INTO chat_rooms(id,name,created_at,updated_at) VALUES (?,?,?,?)',(room,'WAL迁移恢复验收',now,now))
writer.execute('INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES (?,?,?,?,?,?,?,?,?)',
    (room,before_id,'用户','user','broadcast','WAL迁移前消息：这条记录必须进入迁移备份。','user','[]',now))
writer.commit()
with ro(db) as c:before=metadata(c);assert marker_count(c,before_id)==1
# 验证主文件单独复制确实缺这条WAL记录，避免把普通checkpoint当WAL验收。
raw=folder/'main-file-without-wal.sqlite3';raw.write_bytes(db.read_bytes())
with ro(raw) as c:assert marker_count(c,before_id)==0
wal_bytes=Path(str(db)+'-wal').stat().st_size
processes=[]
gui_file=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt'
gui_original=gui_file.read_bytes() if gui_file.exists() else None
def start(name,port):
    workspace=folder/name
    env=os.environ.copy();env['COOLZHU_RUNTIME_DIR']=str(workspace)
    env['COOLZHU_INPUT_SAFETY_STATE_ROOT']=str(folder/'isolated-input-safety')
    env.pop('COOLZHU_WEB_STATIC_ROOT',None);env.pop('COOLZHU_BROWSER_NAV_DIAGNOSTICS',None)
    out=(folder/(name+'-stdout.txt')).open('wb');err=(folder/(name+'-stderr.txt')).open('wb')
    p=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
    processes.append((name,p,out,err))
    for _ in range(60):
        if p.poll() is not None:raise RuntimeError(f'{name}正式进程退出{p.returncode}')
        try:
            with urllib.request.urlopen(f'http://127.0.0.1:{port}/api/chat/rooms',timeout=1) as r:json.load(r)
            return p.pid
        except OSError:time.sleep(.2)
    raise RuntimeError(f'{name}服务未就绪')
try:
    pid=start('upgrade',8767)
    backups=list((db.parent/'schema-backups').glob('*.sqlite3'));assert len(backups)==1
    backup=backups[0]
    with ro(backup) as c:
        saved=metadata(c);assert saved['schema_version']==27 and saved['quick_check']=='ok' and marker_count(c,before_id)==1
    with ro(db) as c:
        upgraded=metadata(c);assert upgraded['schema_version']==28 and upgraded['quick_check']=='ok' and marker_count(c,before_id)==1
        cols={r[1] for r in c.execute('PRAGMA table_info(computer_use_steps)')}
        assert {'native_dispatch_state','native_ticket_id','native_binding_json'}<=cols
    writer.execute('INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES (?,?,?,?,?,?,?,?,?)',
        (room,after_id,'用户','user','broadcast','升级后新写入：恢复到独立目录时，这条新记录不得被覆盖。','user','[]',now+1000))
    writer.commit()
    restore=folder/'restored/.coolzhu/web-sessions.sqlite3'
    with ro(backup) as src,sqlite3.connect(restore) as dest:src.backup(dest)
    restored_pid=start('restored',8768)
    with ro(restore) as c:
        restored=metadata(c);assert restored['schema_version']==28 and restored['quick_check']=='ok' and marker_count(c,before_id)==1 and marker_count(c,after_id)==0
    with ro(db) as c:assert marker_count(c,before_id)==1 and marker_count(c,after_id)==1
    result={'version':'0.2.109','binary_sha256':verified['web_sha256'],'original_read_only':original,'legacy_copy_before_upgrade':before,
            'wal_bytes_before_upgrade':wal_bytes,'raw_main_copy_has_before_marker':False,'product_backup':saved,'upgraded':upgraded,'restored':restored,
            'room_id':room,'before_marker':before_id,'after_marker':after_id,'upgrade_pid':pid,'restored_pid':restored_pid,
            'backup_contains_wal_marker':True,'restored_has_before_not_after':True,'upgraded_new_write_preserved':True,
            'source_writes_by_drill':0,'model_requests':0,'cloud_sessions_created':0,'status':'独立事实通过，待正常UI实拍'}
    (folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result,ensure_ascii=False),flush=True)
    while not (folder/'stop').exists():time.sleep(.2)
finally:
    reader.close();writer.close()
    exits=[]
    for name,p,out,err in processes:
        if p.poll() is None:p.terminate()
        p.wait(timeout=10);out.close();err.close();exits.append({'name':name,'pid':p.pid,'exit_code':p.returncode,'termination':'仅自有独立演练服务，非模型/插件清理证明'})
    if gui_original is not None:gui_file.write_bytes(gui_original)
    elif gui_file.exists():gui_file.unlink()
    (folder/'cleanup.json').write_text(json.dumps(exits,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
