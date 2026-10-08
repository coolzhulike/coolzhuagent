"""正式版真实启动窗口竞争；不暂停进程、不插入等待、不修改模型/安全配置。"""
from pathlib import Path
import ctypes,ctypes.wintypes as w,json,time,threading,urllib.request,urllib.error,hashlib,sqlite3,sys
p=Path(__file__).resolve().parent
receipt=json.loads((p.parent/'2026-10-08-release-101/installed-101-standard-processes.json').read_text(encoding='utf-8-sig'))
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
 row.update(image=image.value,created_100ns=(ts[0].dwHighDateTime<<32)|ts[0].dwLowDateTime,wait_before=k.WaitForSingleObject(h,0))
 return h

import http.client
origin=time.monotonic_ns()
def elapsed():return (time.monotonic_ns()-origin)/1e6
def api(conn,route,body=None,post=False):
 started=elapsed();method='POST' if post or body is not None else 'GET'
 conn.request(method,route,json.dumps(body).encode() if body is not None else None,{'Content-Type':'application/json'})
 response=conn.getresponse();headers=elapsed();raw=response.read();finished=elapsed()
 return {'http':response.status,'body':json.loads(raw),'start_ms':started,'headers_ms':headers,'end_ms':finished}
def ledger():
 with sqlite3.connect((root/'.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
  return {'counts':{name:c.execute('select count(*) from '+name).fetchone()[0] for name in ['devin_acp_attempts','chat_room_messages','chat_usage_events']},'bindings':c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=? order by lane',(session,)).fetchall()}
actor=http.client.HTTPConnection('127.0.0.1',8765,timeout=45)
launcher=http.client.HTTPConnection('127.0.0.1',8765,timeout=45)
before=api(actor,'/api/lsp/status');assert before['http']==200 and before['body']['configured'] and not before['body']['running']
assert api(launcher,'/api/lsp/status')['http']==200
rooms=api(actor,'/api/chat/rooms');assert rooms['body']['active_room_id']==room
other=next(x['id'] for x in rooms['body']['rooms'] if x['id']!=room)
assert not children(),'保留原有语言服务子进程'
owned={};handles=[];response={};done=threading.Event();worker=None
result={'stage':'0.2.101正式后台/正式壳；自然启动窗口，无产品修改','before':before,'ledger_before':ledger(),'original_room':room,'other_room':other}
try:
 for path,content in [(root/'Cargo.toml','[package]\nname="coolzhu-lsp-aba"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[lib]\npath="lsp-aba.rs"\n'),(root/'lsp-aba.rs','pub fn bamboo() -> u32 { 101 }\n')]:
  with path.open('xb') as f:f.write(content.encode())
  owned[str(path)]=hashlib.file_digest(path.open('rb'),'sha256').hexdigest()
 request={'path':'lsp-aba.rs','session_id':session,'chat_room_id':room,'expected_workspace':str(root)}
 def start():
  try:response.update(api(launcher,'/api/lsp/start',request))
  except Exception as error:response.update({'error':str(error),'exception_ms':elapsed()})
  finally:done.set()
 worker=threading.Thread(target=start);worker.start();deadline=time.monotonic()+15
 while not done.is_set() and time.monotonic()<deadline:
  rows=children()
  if rows:
   row=rows[0];h=hold(row);handles.append((h,row));result['observed_ms']=elapsed();break
  time.sleep(.002)
 result['observed_during_start']=[row for _,row in handles]
 result['pending_before_b']=not done.is_set()
 if handles and not done.is_set():
  result['room_b']=api(actor,'/api/chat/rooms/'+other+'/activate',post=True)
  result['room_a']=api(actor,'/api/chat/rooms/'+room+'/activate',post=True)
  result['pending_after_a']=not done.is_set()
  result['held_wait_after_a']=[k.WaitForSingleObject(h,0) for h,_ in handles]
 worker.join(45);assert not worker.is_alive(),'原启动未收尾，保留状态待核实'
 result['start_response']=response
 result['status_after']=api(actor,'/api/lsp/status')
 if result['status_after']['body'].get('handle'):
  result['normal_close']=api(actor,'/api/lsp/close',{'handle':result['status_after']['body']['handle']})
 for h,row in handles:
  row['wait_after']=k.WaitForSingleObject(h,5000)
  with Path(row['image']).open('rb') as f:row['sha256']=hashlib.file_digest(f,'sha256').hexdigest()
 result['final_status']=api(actor,'/api/lsp/status')
 result['rooms_after']=api(actor,'/api/chat/rooms')['body']['active_room_id']
 result['ledger_after']=ledger()
 result['aba_window_observed']=bool(result.get('pending_after_a') and result.get('room_a',{}).get('http')==200 and result.get('room_b',{}).get('http')==200 and result['room_a']['end_ms']<response.get('headers_ms',-1) and result.get('held_wait_after_a')==[258])
 result['aba_old_start_rejected']=result['aba_window_observed'] and response.get('http')==409 and not result['final_status']['body']['running'] and all(row['wait_after']==0 for _,row in handles)
 assert result['ledger_before']==result['ledger_after'],'模型台账或云端绑定变化'
 assert result['rooms_after']==room,'未恢复原房间'
finally:
 if worker and worker.is_alive():worker.join(45)
 for h,_ in handles:k.CloseHandle(h)
 try:
  if api(actor,'/api/chat/rooms')['body']['active_room_id']!=room:result['finally_restore_room']=api(actor,'/api/chat/rooms/'+room+'/activate',post=True)
 finally:
  actor.close();launcher.close()
 cleanup=[]
 for name,digest in owned.items():
  path=Path(name);assert hashlib.file_digest(path.open('rb'),'sha256').hexdigest()==digest
  path.unlink();cleanup.append({'path':name,'sha256':digest})
 result['owned_files_removed']=cleanup
 (p/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({key:result.get(key) for key in ['aba_window_observed','aba_old_start_rejected','pending_after_a','held_wait_after_a','start_response','final_status']},ensure_ascii=False))
