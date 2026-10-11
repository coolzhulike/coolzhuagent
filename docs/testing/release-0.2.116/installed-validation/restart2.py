from pathlib import Path
import hashlib,json,os,socket,subprocess,time,urllib.request
p=Path(__file__).resolve().parent;root=p.parents[2];f=p/'save-scope2';ws=f/'workspace-a'
binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe');facts=json.loads((f/'facts.json').read_text())
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==facts['binary_sha256']
with socket.socket() as s:assert s.connect_ex(('127.0.0.1',8768))!=0
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(ws),COOLZHU_LOG_DIR=str(f/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(f/'input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';old=gui.read_bytes() if gui.exists() else None
out=(f/'restart-out.txt').open('wb');err=(f/'restart-err.txt').open('wb')
proc=subprocess.Popen([str(binary)],cwd=ws,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
def get():
    with urllib.request.urlopen('http://127.0.0.1:8768/api/sessions/scope-shared-session/model-settings',timeout=10) as r:return json.load(r)
try:
    for _ in range(150):
        assert proc.poll() is None
        try:loaded=get();break
        except OSError:time.sleep(.1)
    else:raise RuntimeError('重启未就绪')
    saved=facts['saved_after_release']
    assert loaded['parameters']==saved['parameters'] and loaded['session']['name']==saved['session']['name']
    assert loaded['configuration_revision']==saved['configuration_revision'] and loaded['effective_context_window']==8192
    (f/'restart-settings.json').write_text(json.dumps(loaded,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    (f/'restart-result.json').write_text(json.dumps({'pid':proc.pid,'binary_sha256':facts['binary_sha256'],'same_parameters_revision_name':True,'model_requests':0})+'\n',encoding='utf-8')
    print('同候选EXE重启恢复工程A名称/温度/容量/revision；等待原生界面',flush=True)
    while not(f/'restart-stop').exists():assert proc.poll() is None;time.sleep(.2)
finally:
    if proc.poll() is None:proc.terminate()
    proc.wait(timeout=15);out.close();err.close()
    if old is not None:gui.write_bytes(old)
    elif gui.exists():gui.unlink()
    (f/'restart-cleanup.json').write_text(json.dumps({'pid':proc.pid,'exit_code':proc.returncode,'gui_hint_restored':True})+'\n',encoding='utf-8')
