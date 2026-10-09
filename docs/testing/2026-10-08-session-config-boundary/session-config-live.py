"""参数拆分候选的真实服务与保存/重载；不调用模型、不触碰原运行库。"""
from pathlib import Path
import hashlib, json, os, socket, sqlite3, subprocess, time, urllib.request

p=Path(__file__).resolve().parent
repo=p.parents[1]
folder=p/'session-config-live'
assert not folder.exists()
(folder/'workspace/.coolzhu').mkdir(parents=True)
workspace=folder/'workspace'
with socket.socket() as s: assert s.connect_ex(('127.0.0.1',8767))!=0
source=p/'rotation-live/workspace/.coolzhu/web-sessions.sqlite3'
db=workspace/'.coolzhu/web-sessions.sqlite3'
src=sqlite3.connect(source.resolve().as_uri()+'?mode=ro',uri=True)
dst=sqlite3.connect(db);src.backup(dst);dst.close();src.close()
(workspace/'coolzhu.toml').write_text('[web]\nbind_addr="127.0.0.1:8767"\n[pet]\nenabled=false\n[model]\nenable_real_llm=false\n',encoding='utf-8')
binary=repo/'target/debug/coolzhu-web-console.exe'
env=os.environ.copy()
env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(folder/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'isolated-input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt'
oldgui=gui.read_bytes() if gui.exists() else None
processes=[]
def api(path,data=None):
    request=urllib.request.Request('http://127.0.0.1:8767'+path,data=None if data is None else json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(request,timeout=4) as response:return json.load(response)
def start(name):
    out=(folder/(name+'-stdout.txt')).open('wb');err=(folder/(name+'-stderr.txt')).open('wb')
    process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
    processes.append((name,process,out,err))
    for _ in range(100):
        assert process.poll() is None
        try:api('/api/sessions');return process
        except OSError:time.sleep(.1)
    raise RuntimeError('候选服务未就绪')
try:
    first=start('before')
    session=api('/api/sessions',{'name':'参数边界保存验收','provider':'custom','model':'qwen3.8-flash','base_url':'https://dashscope.aliyuncs.com/compatible-mode/v1','reasoning_effort':'high'})['session']['id']
    settings=api('/api/sessions/'+session+'/model-settings')
    facts={'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'session_id':session,'initial_parameters':settings['parameters'],'model_requests':0,'cloud_sessions_created':0,'original_runtime_writes':0}
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'stage':'ready','session_id':session,'pid':first.pid}),flush=True)
    while not (folder/'restart').exists():time.sleep(.2)
    saved=api('/api/sessions/'+session+'/model-settings')
    assert saved['parameters']['context_window']==65536
    assert saved['parameters']['max_output_tokens']==4096
    assert saved['parameters']['temperature']==0.7
    assert saved['parameters']['supports_multimodal'] is True
    assert saved['base_url']=='https://dashscope.aliyuncs.com/compatible-mode/v1'
    (folder/'saved.json').write_text(json.dumps(saved,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    first.terminate();first.wait(timeout=10)
    second=start('after')
    loaded=api('/api/sessions/'+session+'/model-settings')
    assert saved['parameters']==loaded['parameters']
    assert saved['configuration_revision']==loaded['configuration_revision']
    facts.update(saved_revision=saved['configuration_revision'],persisted_parameters=loaded['parameters'],restart_matches=True)
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'stage':'restart-verified','pid':second.pid}),flush=True)
    while not (folder/'stop').exists():time.sleep(.2)
finally:
    cleanup=[]
    for name,process,out,err in processes:
        if process.poll() is None:process.terminate()
        process.wait(timeout=10);out.close();err.close()
        cleanup.append({'stage':name,'pid':process.pid,'exit_code':process.returncode})
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps(cleanup,indent=2)+'\n')
