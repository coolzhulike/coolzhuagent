"""旧容量接口真实服务验证，不发送任何模型请求。"""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import json,hashlib,os,socket,subprocess,threading,time,urllib.request,sys
root=Path(__file__).resolve().parents[2];p=Path(__file__).resolve().parent
case='legacy';folder=p/case;assert not folder.exists();workspace=folder/'workspace';workspace.mkdir(parents=True)
binary=root/'target/debug/coolzhu-web-console.exe'
(workspace/'coolzhu.toml').write_text('[web]\nbind_addr="127.0.0.1:8768"\n[pet]\nenabled=false\n[model]\nenable_real_llm=false\nlocal_chat_port=8082\nlocal_chat_context_window=8192\nlocal_chat_max_output_tokens=4096\n',encoding='utf-8')
with socket.socket() as s:assert s.connect_ex(('127.0.0.1',8768))!=0
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(folder/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
out=(folder/'stdout.txt').open('wb');err=(folder/'stderr.txt').open('wb')
process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
def api(path,data=None):
    req=urllib.request.Request('http://127.0.0.1:8768'+path,data=None if data is None else json.dumps(data).encode(),headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(req,timeout=20) as r:return json.load(r)
try:
    for _ in range(150):
        assert process.poll() is None
        try:api('/api/sessions');break
        except OSError:time.sleep(.1)
    else:raise RuntimeError('未就绪')
    row=api('/api/sessions',{'name':'旧容量接口专项','provider':'custom','model':'qwen3.8-flash','base_url':'http://127.0.0.1:8082/v1'})
    sid=row['session']['id'];path='/api/sessions/'+sid
    settings=api(path+'/model-settings',{'parameters':{'context_window':32768,'max_output_tokens':1024,'temperature':.25}})
    legacy=api(path+'/model-limit')
    facts={'binary':str(binary),'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'session_id':sid,'local_settings':settings,'local_legacy':legacy,'local_match':legacy['context_window']==settings['effective_context_window'] and legacy['max_output_tokens']==settings['effective_max_output_tokens'],'model_requests':0}
    # 正常旧API修改容量不得丢失采样，清零后仍受本地服务约束。
    cleared=api(path+'/model-limit',{'context_window':0,'max_output_tokens':0});after=api(path+'/model-settings')
    facts.update(cleared_legacy=cleared,cleared_settings=after,clear_match=cleared['context_window']==after['effective_context_window'] and cleared['max_output_tokens']==after['effective_max_output_tokens'],sampling_preserved=after['parameters']['temperature']==.25)
    # 外部服务两组配置，overridden标志与实际预算应来自同一版本。
    api(path+'/model-settings',{'parameters':{'context_window':16384,'max_output_tokens':1024,'base_url':'https://example.com/v1','temperature':.25}})
    stop=threading.Event()
    def reader(_):
        reads=0;samples=[]
        while not stop.is_set():
            loaded=api(path+'/model-limit');reads+=1
            expected=(16384,1024) if loaded['overridden'] else (loaded['default_context_window'],loaded['default_max_output_tokens'])
            if (loaded['context_window'],loaded['max_output_tokens'])!=expected and len(samples)<8:samples.append(loaded)
        return {'reads':reads,'mixed_samples':samples}
    with ThreadPoolExecutor(max_workers=6) as pool:
        jobs=[pool.submit(reader,i) for i in range(6)]
        try:
            for i in range(180):api(path+'/model-settings',{'parameters':{'context_window':0 if i%2 else 16384,'max_output_tokens':0 if i%2 else 1024,'base_url':'https://example.com/v1','temperature':.25}})
        finally:stop.set()
        readers=[j.result() for j in jobs]
    facts.update(writes=180,readers=readers,total_reads=sum(r['reads'] for r in readers),mixed_samples=sum(len(r['mixed_samples']) for r in readers))
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({k:facts[k] for k in ['local_match','clear_match','sampling_preserved','total_reads','mixed_samples','model_requests']},ensure_ascii=False))
    if case!='before113':assert facts['local_match'] and facts['clear_match'] and facts['sampling_preserved'] and facts['mixed_samples']==0
finally:
    if process.poll() is None:process.terminate()
    process.wait(timeout=15);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n')
