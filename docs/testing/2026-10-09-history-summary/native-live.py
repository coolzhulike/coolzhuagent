"""候选服务正常启动，真实历史副本的原生界面补验。"""
from pathlib import Path
import json,os,subprocess,time,urllib.request,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1];f=p/'candidate';workspace=f/'workspace'
binary=root/'target/debug/coolzhu-web-console.exe'
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==json.loads((p/'build-result.json').read_text())['binary_sha256']
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_LOG_DIR=str(f/'native-logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(f/'isolated-input-safety'))
for k in ('COOLZHU_WEB_STATIC_ROOT','COOLZHU_GUI_WEB_URL'):env.pop(k,None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';saved=gui.read_bytes() if gui.exists() else None
with (f/'native-out.log').open('wb') as out,(f/'native-err.log').open('wb') as err:
 proc=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
 try:
  for _ in range(150):
   assert proc.poll() is None
   try:
    with urllib.request.urlopen('http://127.0.0.1:8768/api/sessions',timeout=10) as r:json.load(r)
    break
   except OSError:time.sleep(.1)
  else:raise RuntimeError('候选服务未就绪')
  (p/'native-web-receipt.json').write_text(json.dumps({'pid':proc.pid,'binary':str(binary),'workspace':str(workspace),'port':8768}),encoding='utf-8')
  print('候选真实历史副本服务就绪；不发送模型',flush=True)
  while not (p/'stop-native').exists():assert proc.poll() is None;time.sleep(.2)
 finally:
  proc.terminate();proc.wait(timeout=15)
  if saved is not None:gui.write_bytes(saved)
  elif gui.exists():gui.unlink()
  (p/'native-cleanup.json').write_text(json.dumps({'pid':proc.pid,'process_reaped':True,'gui_hint_restored':True}),encoding='utf-8')
