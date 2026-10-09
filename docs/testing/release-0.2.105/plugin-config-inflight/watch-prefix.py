"""持有真实DSH宿主进程句柄，在正常插件停用后独立确认退出。不终止进程。"""
import ctypes,ctypes.wintypes as w,json,pathlib,time,urllib.request,hashlib
folder=pathlib.Path(__file__).resolve().parent;p=folder.parents[1]/'tmp/2026-10-08-release-105'
receipt=json.loads((p/'installed-105-standard-processes.json').read_text(encoding='utf-8-sig'))
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
