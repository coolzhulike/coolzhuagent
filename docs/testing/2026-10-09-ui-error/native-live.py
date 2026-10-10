"""正常启动独立模型关闭的真实库副本，以原生界面验证损坏附件提示。"""
from pathlib import Path
import hashlib,json,os,subprocess,time,urllib.request
p=Path(__file__).resolve().parent;root=p.parents[1];f=root/'tmp/2026-10-09-attachment-integrity/candidate-v2';workspace=f/'workspace'
binary=root/'target/debug/coolzhu-web-console.exe'
assert hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()==json.loads((p/'build-result.json').read_text())['binary_sha256']
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_WEB_ATTACHMENT_STORE=str(f/'attachments'),COOLZHU_LOG_DIR=str(f/'native-logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(f/'input-safety'))
for key in ('COOLZHU_WEB_STATIC_ROOT','COOLZHU_GUI_WEB_URL'):env.pop(key,None)
hint=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';saved=hint.read_bytes() if hint.exists() else None
(p/'visible-integrity-order.txt').write_text('VISIBLE-INTEGRITY-ORDER：竹剑数量7，单价42；这是未改写的上传正文。',encoding='utf-8')
with (f/'native-out.log').open('wb') as out,(f/'native-err.log').open('wb') as err:
 proc=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
 try:
  for _ in range(150):
   assert proc.poll() is None
   try:
    with urllib.request.urlopen('http://127.0.0.1:8768/api/sessions',timeout=10) as r:json.load(r)
    break
   except OSError:time.sleep(.1)
  else:raise RuntimeError('候选未就绪')
  (p/'native-web-receipt.json').write_text(json.dumps({'pid':proc.pid,'binary':str(binary),'port':8768}),encoding='utf-8')
  print('候选完整性界面就绪，聊天室已关闭模型',flush=True)
  while not (p/'stop-native').exists():assert proc.poll() is None;time.sleep(.2)
 finally:
  proc.terminate();proc.wait(timeout=15)
  if saved is not None:hint.write_bytes(saved)
  elif hint.exists():hint.unlink()
  (p/'native-cleanup.json').write_text(json.dumps({'process_reaped':True,'gui_hint_restored':True}),encoding='utf-8')
