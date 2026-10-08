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

owner=json.loads((folder/'lsp-ui-owner.json').read_text());handle=owner['started']['handle'];h,row=own_process()
try:
 assert k.WaitForSingleObject(h,0)==258
 closed=api('/api/lsp/close',{'handle':handle},200)
 closed_wait=k.WaitForSingleObject(h,5000);assert closed_wait==0
finally:k.CloseHandle(h)
status=api('/api/lsp/status',expected=200);assert not status['body']['running'] and status['body']['handle'] is None
stale=api('/api/lsp/diagnostics',{'handle':handle,'path':owner['request']['path']},409)
result={'stage':'0.2.98正式安装版','closed_owned_process':row,'close':closed,'held_handle_wait_before':258,'held_handle_wait_after':closed_wait,'status':status,'old_handle_request':stale,'generated_model_requests':0}
(folder/'lsp-close-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('正常关闭真实LSP，持有句柄确认退出，旧句柄409')
