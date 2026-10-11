from pathlib import Path
import hashlib,json,os,socket,subprocess,time,urllib.request
p=Path(__file__).resolve().parent;root=p.parents[1];folder=p/'ui';workspace=folder/'workspace';assert not folder.exists();workspace.mkdir(parents=True)
binary=root/'target/debug/coolzhu-web-console.exe'
expected=json.loads((p/'legacy/facts.json').read_text())['binary_sha256'];assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==expected
(workspace/'coolzhu.toml').write_text('[web]\nbind_addr="127.0.0.1:8768"\n[pet]\nenabled=false\n[model]\nenable_real_llm=false\nlocal_chat_port=8082\nlocal_chat_context_window=8192\nlocal_chat_max_output_tokens=4096\n',encoding='utf-8')
with socket.socket() as s:assert s.connect_ex(('127.0.0.1',8768))!=0
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(folder/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
out=(folder/'stdout.txt').open('wb');err=(folder/'stderr.txt').open('wb');process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
def api(path,data=None):
    req=urllib.request.Request('http://127.0.0.1:8768'+path,data=None if data is None else json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(req,timeout=10) as r:return json.load(r)
try:
    for _ in range(150):
        assert process.poll() is None
        try:api('/api/sessions');break
        except OSError:time.sleep(.1)
    else:raise RuntimeError('未就绪')
    sid=api('/api/sessions',{'name':'配置服务边界复验','provider':'custom','model':'qwen3.8-flash','base_url':'http://127.0.0.1:8082/v1'})['session']['id']
    path='/api/sessions/'+sid
    api(path+'/model-settings',{'parameters':{'temperature':.25}})
    legacy=api(path+'/model-limit',{'context_window':32768,'max_output_tokens':1024});settings=api(path+'/model-settings')
    assert legacy['context_window']==settings['effective_context_window']==8192 and legacy['max_output_tokens']==settings['effective_max_output_tokens']==1024
    assert settings['parameters']['context_window']==32768 and settings['parameters']['temperature']==.25
    (folder/'facts.json').write_text(json.dumps({'session_id':sid,'binary_sha256':expected,'legacy':legacy,'settings':settings,'model_requests':0},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('候选服务旧API保存与统一页参数/生效预算一致，等待原生截图',flush=True)
    while not(folder/'stop').exists():assert process.poll() is None;time.sleep(.2)
finally:
    if process.poll() is None:process.terminate()
    process.wait(timeout=15);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n')
