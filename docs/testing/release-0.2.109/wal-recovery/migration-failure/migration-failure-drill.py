"""正式安装EXE的隔离副本迁移失败与修复后重启演练；不改原库、不发模型请求。"""
from pathlib import Path
import hashlib,json,os,sqlite3,socket,subprocess,time,urllib.request
root=Path(__file__).resolve().parent
folder=root/'migration-failure';folder.mkdir(exist_ok=True)
workspace=folder/'workspace';assert not workspace.exists(),'不覆盖既有演练'
(workspace/'.coolzhu').mkdir(parents=True)
with socket.socket() as s:assert s.connect_ex(('127.0.0.1',8767))!=0,'保留端口既有服务'
binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe')
verified=json.loads((root/'installed-109-verification.json').read_text(encoding='utf-8'))
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==verified['web_sha256']
source=list((root/'wal-drill3/upgrade/.coolzhu/schema-backups').glob('*.sqlite3'))
assert len(source)==1
db=workspace/'.coolzhu/web-sessions.sqlite3'
def ro(p):return sqlite3.connect(p.resolve().as_uri()+'?mode=ro',uri=True)
src=ro(source[0]);dest=sqlite3.connect(db);src.backup(dest);src.close();dest.close()
c=sqlite3.connect(db)
assert c.execute('PRAGMA user_version').fetchone()[0]==27
# v28先补三列再建唯一索引；同名表产生真实SQLite错误，验证三列也随事务回滚。
c.execute('CREATE TABLE idx_cu_native_ticket(migration_failure_probe TEXT)');c.commit();c.close()
def facts(path):
    c=ro(path)
    result={'schema_version':c.execute('PRAGMA user_version').fetchone()[0],
      'contract_version':c.execute("SELECT value FROM metadata WHERE key='schema_contract_version'").fetchone()[0],
      'quick_check':c.execute('PRAGMA quick_check').fetchone()[0],
      'messages':c.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0],
      'new_columns':[x[1] for x in c.execute('PRAGMA table_info(computer_use_steps)') if x[1] in ('native_dispatch_state','native_ticket_id','native_binding_json')],
      'logical_dump_sha256':hashlib.sha256('\n'.join(c.iterdump()).encode()).hexdigest()}
    c.close();return result
before=facts(db)
(workspace/'coolzhu.toml').write_text('[web]\nbind_addr="127.0.0.1:8767"\n[pet]\nenabled=false\n[model]\nenable_real_llm=true\n',encoding='utf-8')
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';original=gui.read_bytes() if gui.exists() else None
env=os.environ.copy();env['COOLZHU_RUNTIME_DIR']=str(workspace);env['COOLZHU_INPUT_SAFETY_STATE_ROOT']=str(folder/'isolated-input-safety')
env.pop('COOLZHU_WEB_STATIC_ROOT',None);env.pop('COOLZHU_BROWSER_NAV_DIAGNOSTICS',None)
process=None
try:
    start=time.monotonic()
    failed=subprocess.run([str(binary)],cwd=workspace,env=env,capture_output=True,timeout=30,creationflags=subprocess.CREATE_NO_WINDOW)
    (folder/'failed-stdout.txt').write_bytes(failed.stdout);(folder/'failed-stderr.txt').write_bytes(failed.stderr)
    assert failed.returncode!=0,'注入冲突不得继续启动'
    assert b'idx_cu_native_ticket' in failed.stdout+failed.stderr,'必须明确对应同名表冲突'
    after=facts(db);assert after==before,'失败不得留下半套结构或改动数据'
    backups=list((db.parent/'schema-backups').glob('*.sqlite3'));assert len(backups)==1
    saved=facts(backups[0]);assert saved==before,'备份须保持失败之前的原状'
    result={'version':'0.2.109','binary_sha256':verified['web_sha256'],'failed_start_exit_code':failed.returncode,
      'failed_start_elapsed_ms':round((time.monotonic()-start)*1000),'before':before,'after_failure':after,'backup':saved,
      'rollback_logical_dump_unchanged':True,'new_columns_rolled_back':True,'source_writes':0,'model_requests':0,'cloud_sessions_created':0}
    # 仅撤销独立副本中本脚本创建的测试表，再用相同正式EXE正常启动。
    c=sqlite3.connect(db);c.execute('DROP TABLE idx_cu_native_ticket');c.commit();c.close()
    out=(folder/'recovered-stdout.txt').open('wb');err=(folder/'recovered-stderr.txt').open('wb')
    process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
    for _ in range(80):
        assert process.poll() is None,'恢复后正式服务提前退出'
        try:
            with urllib.request.urlopen('http://127.0.0.1:8767/api/chat/rooms',timeout=1) as r:json.load(r)
            break
        except OSError:time.sleep(.2)
    else:raise RuntimeError('恢复后服务未就绪')
    recovered=facts(db);assert recovered['schema_version']==28 and len(recovered['new_columns'])==3 and recovered['quick_check']=='ok'
    assert recovered['messages']==before['messages']
    result.update({'recovered':recovered,'recovered_pid':process.pid,'status':'独立事实通过，待正常UI实拍'})
    (folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(result,ensure_ascii=False),flush=True)
    while not (folder/'stop').exists():time.sleep(.2)
finally:
    if process is not None:
        if process.poll() is None:process.terminate()
        process.wait(timeout=10);out.close();err.close()
        (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'termination':'仅自有独立恢复演练服务，非业务取消证明'},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    if original is not None:gui.write_bytes(original)
    elif gui.exists():gui.unlink()
