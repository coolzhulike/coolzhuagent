"""正式版真实启动窗口竞争；不暂停进程、不插入等待、不修改模型/安全配置。"""
from pathlib import Path
import ctypes,ctypes.wintypes as w,json,time,threading,urllib.request,urllib.error,hashlib,sqlite3,sys
p=Path(__file__).resolve().parent
receipt=json.loads((p/'installed-098-standard-processes.json').read_text(encoding='utf-8-sig'))
root=Path(receipt['workspace']); room='room-1791131523339'; session='session-1791131217833'
phase=sys.argv[1] if len(sys.argv)>1 else 'lookup'
k=ctypes.WinDLL('kernel32',use_last_error=True)
class PE(ctypes.Structure):
 _fields_=[('dwSize',w.DWORD),('cntUsage',w.DWORD),('pid',w.DWORD),('heap',ctypes.c_size_t),('module',w.DWORD),('threads',w.DWORD),('parent',w.DWORD),('priority',w.LONG),('flags',w.DWORD),('exe',w.WCHAR*260)]
k.CreateToolhelp32Snapshot.argtypes=[w.DWORD,w.DWORD];k.CreateToolhelp32Snapshot.restype=w.HANDLE
k.Process32FirstW.argtypes=[w.HANDLE,ctypes.POINTER(PE)];k.Process32FirstW.restype=w.BOOL
k.Process32NextW.argtypes=[w.HANDLE,ctypes.POINTER(PE)];k.Process32NextW.restype=w.BOOL
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.CloseHandle.argtypes=[w.HANDLE];k.CloseHandle.restype=w.BOOL
k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD];k.WaitForSingleObject.restype=w.DWORD
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)];k.QueryFullProcessImageNameW.restype=w.BOOL
k.GetProcessTimes.argtypes=[w.HANDLE,*([ctypes.POINTER(w.FILETIME)]*4)];k.GetProcessTimes.restype=w.BOOL
def children():
 h=k.CreateToolhelp32Snapshot(2,0); assert h and h!=ctypes.c_void_p(-1).value
 try:
  e=PE();e.dwSize=ctypes.sizeof(e);ok=k.Process32FirstW(h,ctypes.byref(e));rows=[]
  while ok:
   if e.parent==receipt['web_pid'] and e.exe.lower() in ['rustup.exe','rust-analyzer.exe']:rows.append({'pid':e.pid,'name':e.exe,'parent':e.parent})
   ok=k.Process32NextW(h,ctypes.byref(e))
  return rows
 finally:k.CloseHandle(h)
def hold(row):
 h=k.OpenProcess(0x101000,False,row['pid']);assert h,ctypes.get_last_error()
 image=ctypes.create_unicode_buffer(32768);n=w.DWORD(len(image));assert k.QueryFullProcessImageNameW(h,0,image,ctypes.byref(n))
 expected=Path.home()/('.cargo/bin/rustup.exe' if row['name'].lower()=='rustup.exe' else '.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/rust-analyzer.exe')
 assert Path(image.value)==expected,(image.value,expected)
 ts=[w.FILETIME() for _ in range(4)];assert k.GetProcessTimes(h,*[ctypes.byref(t) for t in ts])
 row.update(image=image.value,created_100ns=(ts[0].dwHighDateTime<<32)|ts[0].dwLowDateTime,sha256=hashlib.file_digest(expected.open('rb'),'sha256').hexdigest(),wait_before=k.WaitForSingleObject(h,0))
 return h
t0=time.monotonic()
def api(route,body=None,post=False):
 st=time.monotonic()-t0
 r=urllib.request.Request('http://127.0.0.1:8765'+route,json.dumps(body).encode() if body is not None else (b'' if post else None),{'Content-Type':'application/json'})
 try:
  with urllib.request.urlopen(r,timeout=45) as f:status=f.status;v=json.load(f)
 except urllib.error.HTTPError as e:status=e.code;v=json.loads(e.read())
 return {'http':status,'body':v,'start_seconds':st,'end_seconds':time.monotonic()-t0}
def ledger():
 db=root/'.coolzhu/web-sessions.sqlite3'
 with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
  return {'counts':{name:c.execute('select count(*) from '+name).fetchone()[0] for name in ['devin_acp_attempts','chat_room_messages','chat_usage_events']},'bindings':c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=? order by lane',(session,)).fetchall()}
before=api('/api/lsp/status');assert before['http']==200 and before['body']['configured'] and not before['body']['running']
rooms=api('/api/chat/rooms'); assert rooms['http']==200
assert rooms['body']['active_room_id']==room
other=next(x['id'] for x in rooms['body']['rooms'] if x['id']!=room)
assert not children(),'保留原有子进程'
owned={}; result={'stage':'正式0.2.98','before':before,'ledger_before':ledger()};handles=[];response={}
try:
 for path,content in [(root/'Cargo.toml','[package]\nname="coolzhu-lsp-inflight"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[lib]\npath="lsp-inflight.rs"\n'),(root/'lsp-inflight.rs','pub fn bamboo() -> u32 { 98 }\n')]:
  with path.open('xb') as f:f.write(content.encode())
  owned[str(path)]=hashlib.file_digest(path.open('rb'),'sha256').hexdigest()
 request={'path':'lsp-inflight.rs','session_id':session,'chat_room_id':room,'expected_workspace':str(root)}
 def start():response.update(api('/api/lsp/start',request))
 worker=threading.Thread(target=start);worker.start();observed=[];deadline=time.monotonic()+15
 while worker.is_alive() and time.monotonic()<deadline:
  rows=children()
  if phase.startswith('server'):rows=[row for row in rows if row['name'].lower()=='rust-analyzer.exe']
  if rows:
   observed=rows
   for row in rows:
    h=hold(row);handles.append((h,row))
   break
  time.sleep(.005)
 result['observed_during_start']=observed
 result['start_pending_before_workspace_reload']=worker.is_alive()
 result['workspace_reload']=api('/api/workspace/reload',post=True)
 result['start_pending_before_room_switch']=worker.is_alive()
 if phase!='server-current':result['room_b']=api('/api/chat/rooms/'+other+'/activate',post=True)
 result['room_a']=api('/api/chat/rooms/'+room+'/activate',post=True)
 result['start_pending_after_aba']=worker.is_alive()
 worker.join(45);assert not worker.is_alive()
 result['start_response']=response
 result['status_after']=api('/api/lsp/status')
 # 未命中自然启动窗口时只正常关闭自己刚启动的实例，不宣称竞争通过。
 if result['status_after']['body'].get('handle'):
  result['normal_close']=api('/api/lsp/close',{'handle':result['status_after']['body']['handle']})
 for h,row in handles:row['wait_after']=k.WaitForSingleObject(h,5000)
 result['held_processes']=[row for _,row in handles]
 result['final_status']=api('/api/lsp/status')
 result['workspace_reload_after']=api('/api/workspace/reload',post=True)
 result['ledger_after']=ledger()
 result['workspace_reload_inflight_rejected']=result['start_pending_before_workspace_reload'] and result['workspace_reload']['http']==409
 result['room_switch_inflight_rejected']=result['start_pending_before_room_switch'] and response['http']==409
 result['room_aba_inflight_rejected']='room_b' in result and result['start_pending_after_aba'] and response['http']==409
 result['same_room_reactivation_inflight_rejected']=phase=='server-current' and result['start_pending_after_aba'] and response['http']==409
 assert result['ledger_before']==result['ledger_after'],'真实模型台账/绑定发生意外变化'
finally:
 for h,row in handles:k.CloseHandle(h)
 cleanup=[]
 for path,digest in owned.items():
  path=Path(path);assert hashlib.file_digest(path.open('rb'),'sha256').hexdigest()==digest
  path.unlink();cleanup.append({'path':str(path),'sha256':digest})
 result['owned_files_removed']=cleanup
 (p/('lsp-inflight-'+phase+'-result.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({k:result.get(k) for k in ['workspace_reload_inflight_rejected','room_aba_inflight_rejected','start_response','final_status']},ensure_ascii=False))
