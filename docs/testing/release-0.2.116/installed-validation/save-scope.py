"""用实际SQLite写锁覆盖参数发布到会话更新窗口；正常API验证工程pin。"""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import hashlib,json,os,shutil,socket,sqlite3,subprocess,time,tomllib,urllib.request,urllib.error
p=Path(__file__).resolve().parent;root=p.parents[2];folder=p/'save-scope';folder.mkdir(exist_ok=False)
spaces=[]
for label in ['a','b']:
    ws=folder/('workspace-'+label);shutil.copytree(p/'candidate'/('workspace-'+label),ws);spaces.append(ws)
    old_port=json.loads((p/'candidate/facts.json').read_text(encoding='utf-8'))['port']
    cfg=(ws/'coolzhu.toml').read_text(encoding='utf-8');cfg=cfg.replace(f'bind_addr="127.0.0.1:{old_port}"','bind_addr="127.0.0.1:8768"')
    assert 'bind_addr="127.0.0.1:8768"' in cfg
    (ws/'coolzhu.toml').write_text(cfg,encoding='utf-8')
with socket.socket() as s:assert s.connect_ex(('127.0.0.1',8768))!=0
binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe');env=os.environ.copy()
env.update(COOLZHU_RUNTIME_DIR=str(spaces[0]),COOLZHU_LOG_DIR=str(folder/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'input-safety'))
env.pop('COOLZHU_WEB_STATIC_ROOT',None);env.pop('COOLZHU_BROWSER_NAV_DIAGNOSTICS',None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
out=(folder/'stdout.txt').open('wb');err=(folder/'stderr.txt').open('wb')
process=subprocess.Popen([str(binary)],cwd=spaces[0],env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
facts={'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'pid':process.pid,'port':8768,'model_requests':0,'new_cloud_sessions':0,'original_runtime_writes':0}
sid='scope-shared-session';path='/api/sessions/'+sid+'/model-settings'
def api(path,data=None):
    req=urllib.request.Request('http://127.0.0.1:8768'+path,data=None if data is None else json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=20) as r:return r.status,json.load(r)
    except urllib.error.HTTPError as e:return e.code,json.load(e)
try:
    for _ in range(150):
        assert process.poll() is None
        try:
            if api(path)[0]==200:break
        except OSError:pass
        time.sleep(.1)
    else:raise RuntimeError('未就绪')
    before=api(path)[1];params=dict(before['parameters']);params['temperature']=.35
    lock=sqlite3.connect(spaces[0]/'.coolzhu/web-sessions.sqlite3');lock.execute('BEGIN IMMEDIATE')
    with ThreadPoolExecutor(max_workers=1) as pool:
        pending=pool.submit(api,path,{'session':{'name':'工程-a-作用域保存'},'parameters':params,'expected_revision':before['configuration_revision']})
        try:
            deadline=time.monotonic()+2
            while time.monotonic()<deadline:
                config=tomllib.loads((spaces[0]/'coolzhu.toml').read_text(encoding='utf-8-sig'))
                if config['session_model_limits'][sid]['temperature']==.35:break
                assert not pending.done(),'保存未进入实际SQLite等待'
                time.sleep(.003)
            else:raise RuntimeError('未观察到参数发布')
            facts['parameters_published_ms']=time.time()*1000
            status,response=api('/api/workspace',{'path':str(spaces[1])})
            facts['switch_during_sqlite_wait']={'status':status,'response':response,'returned_ms':time.time()*1000}
            assert status==409 and not pending.done(),(status,response)
        finally:lock.rollback();lock.close()
        status,saved=pending.result();assert status==200 and saved['session']['name']=='工程-a-作用域保存' and saved['parameters']['temperature']==.35
    facts['saved_after_release']=saved
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    b=api(path)[1];assert b['session']['name']=='工程-b' and b['parameters']['temperature']==.75 and b['configuration_revision']==20
    facts['other_workspace_untouched']=True
    # 错误返回也必须释放pin，不能卡住后续正常工程选择。
    invalid=dict(b['parameters']);invalid['temperature']=3
    assert api(path,{'parameters':invalid,'expected_revision':20})[0]==400
    assert api('/api/workspace',{'path':str(spaces[0])})[0]==200
    assert api(path,{'parameters':params,'expected_revision':0})[0]==409
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    facts['pins_released_after_400_and_409']=True
    facts['all_passed']=True
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'stage':'scope-save-passed-awaiting-native-ui','port':8768,'switch_during_save':409,'other_workspace_untouched':True,'pins_released_after_errors':True},ensure_ascii=False),flush=True)
    while not(folder/'stop').exists():assert process.poll() is None;time.sleep(.2)
finally:
    if process.poll() is None:process.terminate()
    process.wait(timeout=15);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n',encoding='utf-8')
