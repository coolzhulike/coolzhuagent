"""持有真实DSH宿主进程句柄，在正常插件停用后独立确认退出。不终止进程。"""
import ctypes,ctypes.wintypes as w,json,pathlib,time,urllib.request,hashlib
p=pathlib.Path(__file__).resolve().parent;folder=p/'plugin-proof'
receipt=json.loads((p/'installed-099-standard-processes.json').read_text(encoding='utf-8-sig'))
expected=pathlib.Path('C:/Program Files/CoolzhuAgent/bin/dsh-runtime/node/node.exe')
class Entry(ctypes.Structure):
    _fields_=[('dwSize',w.DWORD),('cntUsage',w.DWORD),('th32ProcessID',w.DWORD),('th32DefaultHeapID',ctypes.c_size_t),('th32ModuleID',w.DWORD),('cntThreads',w.DWORD),('th32ParentProcessID',w.DWORD),('pcPriClassBase',w.LONG),('dwFlags',w.DWORD),('szExeFile',w.WCHAR*260)]
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.CreateToolhelp32Snapshot.argtypes=[w.DWORD,w.DWORD];k.CreateToolhelp32Snapshot.restype=w.HANDLE
k.Process32FirstW.argtypes=[w.HANDLE,ctypes.POINTER(Entry)];k.Process32FirstW.restype=w.BOOL
k.Process32NextW.argtypes=[w.HANDLE,ctypes.POINTER(Entry)];k.Process32NextW.restype=w.BOOL
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.CloseHandle.argtypes=[w.HANDLE];k.CloseHandle.restype=w.BOOL
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)];k.QueryFullProcessImageNameW.restype=w.BOOL
k.GetProcessTimes.argtypes=[w.HANDLE,*([ctypes.POINTER(w.FILETIME)]*4)];k.GetProcessTimes.restype=w.BOOL
k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD];k.WaitForSingleObject.restype=w.DWORD
def children():
    snap=k.CreateToolhelp32Snapshot(2,0);assert snap and snap!=ctypes.c_void_p(-1).value
    entries=[];entry=Entry();entry.dwSize=ctypes.sizeof(Entry)
    try:
        more=k.Process32FirstW(snap,ctypes.byref(entry))
        while more:
            if entry.th32ParentProcessID==receipt['web_pid'] and entry.szExeFile.lower()=='node.exe':entries.append(entry.th32ProcessID)
            more=k.Process32NextW(snap,ctypes.byref(entry))
    finally:k.CloseHandle(snap)
    return entries
handles={};result={'marker':'PLUGIN-INSTALLED099-HELD-PROCESS-20261008','formal_version':'0.2.99','web_pid':receipt['web_pid']};deadline=time.monotonic()+240
try:
    while time.monotonic()<deadline:
        for pid in children():
            if pid in handles:continue
            h=k.OpenProcess(0x101000,False,pid)
            if not h:continue
            image=ctypes.create_unicode_buffer(32768);length=w.DWORD(len(image));ts=[w.FILETIME() for _ in range(4)]
            if not k.QueryFullProcessImageNameW(h,0,image,ctypes.byref(length)) or pathlib.Path(image.value)!=expected:
                k.CloseHandle(h);continue
            assert k.GetProcessTimes(h,*[ctypes.byref(x) for x in ts])
            info={'pid':pid,'path':image.value,'created_100ns':(ts[0].dwHighDateTime<<32)|ts[0].dwLowDateTime,'sha256':hashlib.file_digest(expected.open('rb'),'sha256').hexdigest(),'held_ms':time.time()*1000,'wait_before':k.WaitForSingleObject(h,0)}
            handles[pid]=(h,info)
        log=folder/'slow-events.jsonl'
        events=[json.loads(line) for line in log.read_text(encoding='utf-8').splitlines()] if log.exists() else []
        entered=next((x for x in events if x['event']=='function_network_entered'),None)
        if entered:
            live=[(h,info) for h,info in handles.values() if k.WaitForSingleObject(h,0)==258]
            assert len(live)==1,[(i,k.WaitForSingleObject(h,0)) for h,i in handles.values()]
            h,info=live[0];result['entered']=entered;result['held_process']=info
            result['disable_started_ms']=time.time()*1000
            body={'expected_workspace':'ws-23f646a969206cb4','id':'dsh-aa909f79795799f288de8a67@external','action':'disable'}
            req=urllib.request.Request('http://127.0.0.1:8765/api/extension-market/plugins/action',json.dumps(body).encode(),{'Content-Type':'application/json'})
            with urllib.request.urlopen(req,timeout=20) as response:plugins=json.load(response)
            result['disable_returned_ms']=time.time()*1000
            plugin=next(x for x in plugins['plugins'] if x['id']==body['id']);assert not plugin['enabled'];result['plugin']=plugin
            result['wait_after']=k.WaitForSingleObject(h,8000);result['wait_finished_ms']=time.time()*1000
            assert info['wait_before']==258 and result['wait_after']==0
            result['passed_scope']='真实函数网络进入后正常停用，原宿主持有句柄Wait258→0独立确认退出；没有脚本终止宿主'
            (folder/'revocation-observation.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
            print(json.dumps({'passed_scope':result['passed_scope'],'pid':info['pid'],'wait_before':258,'wait_after':0},ensure_ascii=False),flush=True)
            break
        time.sleep(.015)
    else:raise RuntimeError('未观察到真实网络进入，不计通过')
except BaseException as e:
    result['failure']=str(e);raise
finally:
    result['captured_processes']=[info for _,info in handles.values()]
    for h,_ in handles.values():k.CloseHandle(h)
    (folder/'held-process-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
