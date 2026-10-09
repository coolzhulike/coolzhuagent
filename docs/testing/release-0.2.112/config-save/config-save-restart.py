from pathlib import Path
import hashlib, json, os, socket, subprocess, time, urllib.request
p=Path(__file__).resolve().parent/'config-save-live'
repo=p.parents[2]
workspace=p/'workspace';binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe')
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==facts['binary_sha256']
with socket.socket() as sock:assert sock.connect_ex(('127.0.0.1',8768))!=0
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(p/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(p/'isolated-input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
out=(p/'restart-stdout.txt').open('wb');err=(p/'restart-stderr.txt').open('wb')
process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
try:
    for _ in range(120):
        assert process.poll() is None
        try:
            with urllib.request.urlopen('http://127.0.0.1:8768/api/sessions/'+facts['session_id']+'/model-settings',timeout=3) as r:loaded=json.load(r)
            break
        except OSError:time.sleep(.1)
    else:raise RuntimeError('重启未就绪')
    assert loaded['session']['name']=='配置失败后恢复成功' and loaded['parameters']['temperature']==facts['winner']
    assert loaded['configuration_revision']==facts['saved_revision']+3
    (p/'restart-settings.json').write_text(json.dumps(loaded,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'restart_matches':True,'pid':process.pid,'revision':loaded['configuration_revision'],'model_requests':0}),flush=True)
    while not (p/'stop-restart').exists():
        assert process.poll() is None
        time.sleep(.2)
finally:
    if process.poll() is None:process.terminate()
    process.wait(timeout=10);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (p/'restart-cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n',encoding='utf-8')
