"""真实独立HTTP服务：工程切换与配置读取竞争；零模型，不写原工作区。"""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import hashlib,json,os,socket,subprocess,threading,time,urllib.request,urllib.error,sys
root=Path(__file__).resolve().parents[2];p=Path(__file__).resolve().parent
case=sys.argv[1];folder=p/case;folder.mkdir(parents=True,exist_ok=False)
(folder/'driver.py').write_bytes(Path(__file__).read_bytes())
binary=Path(sys.argv[2]).resolve();sid='scope-shared-session'
base_state=json.loads((root/'tmp/2026-10-08-release-115/snapshot/workspace/.coolzhu/web-sessions.json').read_text(encoding='utf-8'))
with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
spaces=[]
for label,cap,revision in [('a',8192,10),('b',16384,10)]:
    ws=folder/('workspace-'+label);(ws/'.coolzhu').mkdir(parents=True);spaces.append(ws)
    cfg=f'configuration_revision={revision}\n[web]\nbind_addr="127.0.0.1:{port}"\n[pet]\nenabled=false\n[model]\nenable_real_llm=false\nlocal_chat_port=8082\nlocal_chat_context_window={cap}\nlocal_chat_max_output_tokens=4096\n[session_model_limits.{sid}]\ncontext_window=32768\nmax_output_tokens=1024\ntemperature={"0.25" if label=="a" else "0.75"}\n'
    (ws/'coolzhu.toml').write_text(cfg,encoding='utf-8')
    state=json.loads(json.dumps(base_state));session=state['sessions'][1]
    session.update(id=sid,name='工程-'+label,provider='custom',model='scope-local-config',base_url='http://127.0.0.1:8082/v1',endpoint=None,api_key_ref='api-key.txt',messages=[],memory_beads=[])
    state['sessions']=[session];state['active_session_id']=sid;state['chat_rooms']=[];state['active_chat_room_id']=None
    (ws/'.coolzhu/web-sessions.json').write_text(json.dumps(state,ensure_ascii=False),encoding='utf-8')
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(spaces[0]),COOLZHU_LOG_DIR=str(folder/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'input-safety'))
env.pop('COOLZHU_WEB_STATIC_ROOT',None);env.pop('COOLZHU_BROWSER_NAV_DIAGNOSTICS',None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
out=(folder/'stdout.txt').open('wb');err=(folder/'stderr.txt').open('wb')
process=subprocess.Popen([str(binary)],cwd=spaces[0],env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
stop=threading.Event();facts={'case':case,'binary':str(binary),'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'pid':process.pid,'port':port,'model_requests':0,'original_runtime_writes':0,'new_cloud_sessions':0}
def api(path,data=None):
    req=urllib.request.Request(f'http://127.0.0.1:{port}'+path,data=None if data is None else json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=30) as r:return r.status,json.load(r)
    except urllib.error.HTTPError as e:return e.code,json.load(e)
try:
    for _ in range(150):
        assert process.poll() is None
        try:
            if api('/api/sessions')[0]==200:break
        except OSError:pass
        time.sleep(.1)
    else:raise RuntimeError('未就绪')
    # 普通GUI/HTTP工程选择入口，不调用私有reload或直接变更内存。
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    assert api(f'/api/sessions/{sid}/model-settings')[0]==200, '准备会话未正常加载'
    assert api('/api/workspace',{'path':str(spaces[0])})[0]==200
    assert api(f'/api/sessions/{sid}/model-settings')[0]==200, '准备会话未正常加载'
    a=api(f'/api/sessions/{sid}/model-settings')[1]
    a_workspace=api('/api/workspace')[1]
    assert a['session']['name']=='工程-a' and a['configuration_revision']==10
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    b=api(f'/api/sessions/{sid}/model-settings')[1]
    assert b['session']['name']=='工程-b' and b['configuration_revision']==10
    parameters=dict(a['parameters']);parameters['temperature']=.35
    # 页面已取得旧A快照后，普通工程入口正常切到B；随后才到达旧页面保存请求。
    payload={'session':{'name':'工程-a-旧页面保存'},'parameters':parameters,'expected_revision':a['configuration_revision']}
    saved_status,saved=api(f'/api/sessions/{sid}/model-settings',payload)
    after=api(f'/api/sessions/{sid}/model-settings')[1]
    facts.update(before_a=a,before_b=b,workspace_a=a_workspace,submitted=payload,status=saved_status,after_b=after,
                 wrong_workspace_mutated=saved_status==200 and after['session']['name']=='工程-a-旧页面保存' and after['parameters']['temperature']==.35)
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'status':saved_status,'wrong_workspace_mutated':facts['wrong_workspace_mutated'],'binary_sha256':facts['binary_sha256'],'model_requests':0},ensure_ascii=False),flush=True)
    assert facts['wrong_workspace_mutated'], '正式基线未复现，不凭假设改产品'
finally:
    stop.set()
    if process.poll() is None:process.terminate()
    process.wait(timeout=15);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n',encoding='utf-8')
