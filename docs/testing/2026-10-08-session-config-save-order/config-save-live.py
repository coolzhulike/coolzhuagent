"""真实SQLite失败/恢复/同版本竞争，不发模型请求，不写原会话库。"""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import hashlib, json, os, socket, sqlite3, subprocess, time, tomllib, urllib.request, urllib.error

p=Path(__file__).resolve().parent
repo=p.parents[1]
folder=p/'config-save-live'
assert not folder.exists(), '不覆盖旧证据'
workspace=folder/'workspace';workspace.mkdir(parents=True)
port=8768
with socket.socket() as sock:assert sock.connect_ex(('127.0.0.1',port))!=0
(workspace/'coolzhu.toml').write_text(f'[web]\nbind_addr="127.0.0.1:{port}"\n[pet]\nenabled=false\n[model]\nenable_real_llm=false\n',encoding='utf-8')
binary=repo/'target/debug/coolzhu-web-console.exe'
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(folder/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'isolated-input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
out=(folder/'stdout.txt').open('wb');err=(folder/'stderr.txt').open('wb')
process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
facts={'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'pid':process.pid,'model_requests':0,'cloud_sessions_created':0,'original_runtime_writes':0,'port':port}

def api(path,data=None):
    request=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=None if data is None else json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request,timeout=15) as response:return response.status,json.load(response)
    except urllib.error.HTTPError as error:return error.code,json.load(error)

def dbread(sid):
    with sqlite3.connect((workspace/'.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as db:
        return db.execute('SELECT name,model FROM sessions WHERE id=?',(sid,)).fetchone()

def config():return tomllib.loads((workspace/'coolzhu.toml').read_text(encoding='utf-8-sig'))
def fault(on,sid):
    with sqlite3.connect(workspace/'.coolzhu/web-sessions.sqlite3') as db:
        if on:db.execute("CREATE TRIGGER config_save_fault BEFORE INSERT ON sessions WHEN NEW.id = '"+sid+"' BEGIN SELECT RAISE(ABORT,'配置保存故障验收'); END")
        else:db.execute('DROP TRIGGER config_save_fault')

try:
    for _ in range(120):
        assert process.poll() is None
        try:
            status,_=api('/api/sessions')
            if status==200:break
        except OSError:pass
        time.sleep(.1)
    else:raise RuntimeError('候选服务未就绪')
    status,created=api('/api/sessions',{'name':'配置失败恢复验收','provider':'custom','model':'configuration-save-check','base_url':'https://example.com/v1'})
    assert status==200,(status,created)
    sid=created['session']['id'];path='/api/sessions/'+sid+'/model-settings'
    facts['session_id']=sid
    baseline=api(path)[1]
    assert sid not in config().get('session_model_limits',{})
    rows=[]
    for case in ['absent','present']:
        before=api(path)[1];before_db=dbread(sid);before_map=config().get('session_model_limits',{}).get(sid)
        parameters=dict(before['parameters']);parameters['temperature']=.65
        fault(True,sid)
        status,response=api(path,{'session':{'name':'不得泄漏的失败草稿'},'parameters':parameters,'expected_revision':before['configuration_revision']})
        assert status==500,(status,response)
        after=api(path)[1]
        assert after['session']['name']==before['session']['name'] and after['parameters']==before['parameters']
        assert dbread(sid)==before_db and config().get('session_model_limits',{}).get(sid)==before_map
        assert after['configuration_revision']==before['configuration_revision']+2
        fault(False,sid)
        rows.append({'case':case,'status':status,'response':response,'parameters_restored':True,'database_and_memory_restored':True,'missing_entry_preserved':case=='absent','revision_before':before['configuration_revision'],'revision_after':after['configuration_revision']})
        parameters=dict(before['parameters']);parameters['temperature']=.45
        status,saved=api(path,{'session':{'name':'配置失败恢复验收'},'parameters':parameters,'expected_revision':after['configuration_revision']})
        assert status==200 and saved['parameters']['temperature']==.45
    before=api(path)[1]
    def race(value):
        parameters=dict(before['parameters']);parameters['temperature']=value
        status,result=api(path,{'session':{'name':'配置竞争胜者'+str(value)},'parameters':parameters,'expected_revision':before['configuration_revision']})
        return {'value':value,'status':status,'result':result}
    with ThreadPoolExecutor(max_workers=2) as pool:racers=list(pool.map(race,[.25,.75]))
    assert sorted(row['status'] for row in racers)==[200,409]
    winner=next(row for row in racers if row['status']==200)
    loaded=api(path)[1]
    assert loaded['parameters']['temperature']==winner['value']
    assert loaded['session']['name']=='配置竞争胜者'+str(winner['value'])
    assert dbread(sid)[0]==loaded['session']['name']
    facts.update(failure_cases=rows,concurrent_statuses=[row['status'] for row in racers],winner=winner['value'],saved_revision=loaded['configuration_revision'],all_passed=True)
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    (folder/'saved-settings.json').write_text(json.dumps(loaded,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'stage':'verified-awaiting-ui','port':port,'session_id':sid,'cases':2,'concurrent_statuses':facts['concurrent_statuses']},ensure_ascii=False),flush=True)
    while not (folder/'stop').exists():
        assert process.poll() is None
        time.sleep(.2)
finally:
    if process.poll() is None:process.terminate()
    process.wait(timeout=10);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n',encoding='utf-8')
