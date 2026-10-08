"""正式安装版真实Rust服务异常退出/旧句柄/显式重启，独立持有进程句柄确认。"""
from pathlib import Path
import ctypes,ctypes.wintypes as w,json,subprocess,time,urllib.request,urllib.error,hashlib
folder=Path(__file__).resolve().parent
owner=json.loads((folder/'lsp-owner.json').read_text());receipt=json.loads((folder/'installed-098-standard-processes.json').read_text(encoding='utf-8-sig'))
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD];k.WaitForSingleObject.restype=w.DWORD
k.TerminateProcess.argtypes=[w.HANDLE,w.UINT];k.TerminateProcess.restype=w.BOOL
k.CloseHandle.argtypes=[w.HANDLE];k.CloseHandle.restype=w.BOOL
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)];k.QueryFullProcessImageNameW.restype=w.BOOL
k.GetProcessTimes.argtypes=[w.HANDLE,*([ctypes.POINTER(w.FILETIME)]*4)];k.GetProcessTimes.restype=w.BOOL

def own_process():
 ps=subprocess.run(['powershell.exe','-NoLogo','-NoProfile','-Command',"@(Get-CimInstance Win32_Process -Filter \"Name='rust-analyzer.exe'\" | Where-Object ParentProcessId -eq "+str(receipt['web_pid'])+" | Select-Object ProcessId,ParentProcessId,ExecutablePath) | ConvertTo-Json -Compress"],capture_output=True,check=True)
 rows=json.loads(ps.stdout.decode('utf-8-sig'));rows=[rows] if isinstance(rows,dict) else rows
 assert len(rows)==1,rows
 row=rows[0];expected=Path.home()/'.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin/rust-analyzer.exe'
 assert Path(row['ExecutablePath'])==expected,row
 h=k.OpenProcess(0x101001,False,row['ProcessId']);assert h,ctypes.get_last_error()
 image=ctypes.create_unicode_buffer(32768);size=w.DWORD(len(image));assert k.QueryFullProcessImageNameW(h,0,image,ctypes.byref(size)) and Path(image.value)==expected
 times=[w.FILETIME() for _ in range(4)];assert k.GetProcessTimes(h,*[ctypes.byref(t) for t in times])
 created=(times[0].dwHighDateTime<<32)|times[0].dwLowDateTime
 row.update(created_100ns=created,sha256=hashlib.file_digest(expected.open('rb'),'sha256').hexdigest());return h,row

def api(route,body=None,expected=None):
 request=urllib.request.Request('http://127.0.0.1:8765'+route,json.dumps(body).encode() if body is not None else None,{'Content-Type':'application/json'})
 try:
  with urllib.request.urlopen(request,timeout=45) as r:status=r.status;value=json.load(r)
 except urllib.error.HTTPError as e:status=e.code;value=json.loads(e.read())
 if expected is not None:assert status==expected,(status,value)
 return {'http':status,'body':value}

old=owner['started']['handle'];path=owner['request']['path'];h,row=own_process()
try:
 before=api('/api/lsp/status',expected=200);assert before['body']['handle']==old and before['body']['running']
 assert k.WaitForSingleObject(h,0)==258
 assert k.TerminateProcess(h,98)
 assert k.WaitForSingleObject(h,5000)==0
finally:k.CloseHandle(h)
time.sleep(.3)
status_dead=api('/api/lsp/status',expected=200);assert not status_dead['body']['running'] and status_dead['body']['handle'] is None,status_dead
old_dead=api('/api/lsp/diagnostics',{'handle':old,'path':path});assert old_dead['http']>=400,old_dead
no_respawn=api('/api/lsp/status',expected=200);assert not no_respawn['body']['running']
new=api('/api/lsp/start',owner['request'],200);handle=new['body']['handle'];assert handle and handle!=old
(folder/'lsp-restarted-owner.json').write_text(json.dumps({'request':owner['request'],'started':new['body']},ensure_ascii=False,indent=2)+'\n',encoding='utf8')
old_restarted=api('/api/lsp/navigation',{'handle':old,'path':path,'line':4,'character':20,'kind':'definition'},409)
result={'stage':'0.2.98正式安装版','before':before,'terminated_owned_process':row,'held_handle_before':258,'held_handle_after':0,'dead_status':status_dead,'dead_old_request':old_dead,'no_implicit_restart':no_respawn,'explicit_restart':new,'old_handle_after_restart':old_restarted,'generated_model_requests':0}
(folder/'lsp-crash-restart-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'actual_crash_confirmed':True,'old_request_http':old_dead['http'],'explicit_new_handle':handle,'old_handle_after_restart_http':409},ensure_ascii=False))

