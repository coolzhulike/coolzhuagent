from pathlib import Path
import hashlib, json, os, socket, subprocess, time, urllib.request
p=Path(__file__).resolve().parent; root=p.parents[1]; folder=p/'ui'; workspace=folder/'workspace'
binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe')
facts=json.loads((folder/'facts.json').read_text()); saved=facts['settings']
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==facts['binary_sha256']
with socket.socket() as sock: assert sock.connect_ex(('127.0.0.1',8768))!=0
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(folder/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT','COOLZHU_BROWSER_NAV_DIAGNOSTICS']:env.pop(key,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';oldgui=gui.read_bytes() if gui.exists() else None
out=(folder/'restart-out.txt').open('wb');err=(folder/'restart-err.txt').open('wb')
process=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
try:
    for _ in range(150):
        assert process.poll() is None
        try:
            with urllib.request.urlopen('http://127.0.0.1:8768/api/sessions/'+facts['session_id']+'/model-settings',timeout=10) as r:loaded=json.load(r)
            break
        except OSError:time.sleep(.1)
    else:raise RuntimeError('重启未就绪')
    assert loaded['parameters']==saved['parameters'] and loaded['session']['name']==saved['session']['name']
    assert loaded['configuration_revision']==saved['configuration_revision']
    assert loaded['effective_context_window']==8192 and loaded['effective_max_output_tokens']==1024
    with urllib.request.urlopen('http://127.0.0.1:8768/api/sessions/'+facts['session_id']+'/model-limit',timeout=10) as r:legacy=json.load(r)
    assert legacy['context_window']==8192 and legacy['max_output_tokens']==1024
    (folder/'restart-legacy.json').write_text(json.dumps(legacy,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    (folder/'restart-settings.json').write_text(json.dumps(loaded,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    (folder/'restart-result.json').write_text(json.dumps({'pid':process.pid,'binary_sha256':facts['binary_sha256'],'parameters_restored':True,'same_revision':True,'local_cap_passed':True})+'\n',encoding='utf-8')
    print('同一正式114EXE重启后名称、参数、revision及生效容量均恢复，等待原生复验',flush=True)
    while not (folder/'restart-stop').exists():
        assert process.poll() is None;time.sleep(.2)
finally:
    if process.poll() is None:process.terminate()
    process.wait(timeout=15);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'restart-cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n',encoding='utf-8')
