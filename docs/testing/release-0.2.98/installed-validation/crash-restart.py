"""正式宿主异常退出的真实ConPTY回收；持有Win32句柄独立确认，非PID消失推断。"""
import ctypes,ctypes.wintypes as w,datetime,hashlib,json,os,pathlib,re,sqlite3,subprocess,time,urllib.request,urllib.parse,urllib.error
folder=pathlib.Path(__file__).resolve().parent
receipt_path=folder/'installed-098-standard-processes.json'
receipt=json.loads(receipt_path.read_text(encoding='utf-8-sig'))
old_web_pid=receipt['web_pid']
scope={'session_id':'session-1791131217833','room_id':'room-1791131523339'}
base='http://127.0.0.1:8765'
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.CloseHandle.argtypes=[w.HANDLE];k.CloseHandle.restype=w.BOOL
k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD];k.WaitForSingleObject.restype=w.DWORD
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)];k.QueryFullProcessImageNameW.restype=w.BOOL
k.GetProcessTimes.argtypes=[w.HANDLE,*([ctypes.POINTER(w.FILETIME)]*4)];k.GetProcessTimes.restype=w.BOOL
k.TerminateProcess.argtypes=[w.HANDLE,w.UINT];k.TerminateProcess.restype=w.BOOL
def opened(pid,access=0x101000):
    h=k.OpenProcess(access,False,pid)
    if not h:raise ctypes.WinError(ctypes.get_last_error())
    return h
def identity(h):
    b=ctypes.create_unicode_buffer(32768);n=w.DWORD(len(b))
    if not k.QueryFullProcessImageNameW(h,0,b,ctypes.byref(n)):raise ctypes.WinError(ctypes.get_last_error())
    times=[w.FILETIME() for _ in range(4)]
    if not k.GetProcessTimes(h,*[ctypes.byref(t) for t in times]):raise ctypes.WinError(ctypes.get_last_error())
    return b.value,(times[0].dwHighDateTime<<32)|times[0].dwLowDateTime
def api(path,body=None,query=None,expected=200):
    request=urllib.request.Request(base+path+('?' +urllib.parse.urlencode(query) if query else ''),json.dumps(body).encode() if body is not None else None,{'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request,timeout=8) as response:status=response.status;value=json.load(response)
    except urllib.error.HTTPError as e:status=e.code;value=json.loads(e.read())
    assert status==expected,(path,status,value)
    return value
def idle():
    db=pathlib.Path(receipt['workspace'])/'.coolzhu/web-sessions.sqlite3'
    with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
        assert not c.execute("select id from runtime_runs where state not in ('completed','failed','cancelled','interrupted','aborted')").fetchall()
        assert not c.execute("select call_id from computer_use_runs where state not in ('succeeded','failed','blocked','cancelled','timed_out')").fetchall()
        bindings=c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=?',(scope['session_id'],)).fetchall()
        assert sum(b[1]=='island-kayak' for b in bindings)==1 and all(b[2] is None for b in bindings)
        return {'bindings':bindings,'counts':{table:c.execute('select count(*) from '+table).fetchone()[0] for table in ['devin_acp_attempts','chat_room_messages','chat_usage_events']}}
assert receipt['stage']=='0.2.98 Program Files正式安装版'
assert hashlib.file_digest(open(receipt['web_binary'],'rb'),'sha256').hexdigest()==receipt['web_sha256']
before=idle();assert not api('/api/terminal',query=scope)['active'],'存在用户终端，保留'
web=opened(receipt['web_pid'],0x101001);child=None;new_web=None;terminated=False;files=[]
try:
    binary,created=identity(web)
    assert pathlib.Path(binary)==pathlib.Path(receipt['web_binary'])
    dt=datetime.datetime.fromisoformat(receipt['web_started'].replace('Z','+00:00'))
    delta=dt-datetime.datetime(1601,1,1,tzinfo=datetime.timezone.utc)
    expected=(delta.days*86400+delta.seconds)*10000000+delta.microseconds*10
    assert abs(created-expected)<10,'后台创建时间不匹配'
    started=api('/api/terminal/start',{**scope,'cols':90,'rows':25});handle=started['handle']
    (folder/'crash-owned-terminal.json').write_text(json.dumps({'scope':scope,'handle':handle}),encoding='utf-8')
    time.sleep(.8)
    cursor=api('/api/terminal/'+handle+'/output',query=scope)['next_cursor']
    api('/api/terminal/'+handle+'/input',{**scope,'text':"$p=Start-Process -FilePath \"$PSHOME\\powershell.exe\" -ArgumentList '-NoLogo -NoProfile -Command Start-Sleep -Seconds 60' -WindowStyle Hidden -PassThru; [Console]::WriteLine(('CHILD-'+'PID:')+$p.Id)\r"})
    output='';deadline=time.monotonic()+10
    while time.monotonic()<deadline:
        batch=api('/api/terminal/'+handle+'/output',query={**scope,'cursor':cursor});cursor=batch['next_cursor'];output+=batch['text']
        match=re.search(r'CHILD-PID:(\d+)',output)
        if match:break
        time.sleep(.1)
    else:raise RuntimeError('终端未返回实际子进程PID')
    child=opened(int(match[1]));child_binary,child_created=identity(child)
    assert pathlib.Path(child_binary).name.lower()=='powershell.exe' and k.WaitForSingleObject(child,0)==258
    assert idle()==before,'出现其它实际任务，保留宿主和终端'
    if not k.TerminateProcess(web,98):raise ctypes.WinError(ctypes.get_last_error())
    terminated=True
    assert k.WaitForSingleObject(web,5000)==0,'宿主没有退出'
    child_wait=k.WaitForSingleObject(child,5000)
    assert child_wait==0,'宿主退出后Job后代仍未退出'
finally:
    # 即使退出证明失败，也恢复同一个正式后台；不清除安全库或重建云端绑定。
    if terminated:
        env=os.environ.copy();env.pop('COOLZHU_WEB_STATIC_ROOT',None)
        env['COOLZHU_RUNTIME_DIR']=receipt['workspace'];env['COOLZHU_INPUT_SAFETY_STATE_ROOT']='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
        files=[open(folder/'after-crash-web-out.log','wb'),open(folder/'after-crash-web-err.log','wb')]
        new_web=subprocess.Popen([receipt['web_binary']],cwd=receipt['workspace'],env=env,stdout=files[0],stderr=files[1],creationflags=subprocess.CREATE_NO_WINDOW)
        new_handle=opened(new_web.pid)
        try:new_binary,new_created=identity(new_handle)
        finally:k.CloseHandle(new_handle)
        assert pathlib.Path(new_binary)==pathlib.Path(receipt['web_binary'])
        new_started=datetime.datetime(1601,1,1,tzinfo=datetime.timezone.utc)+datetime.timedelta(microseconds=new_created//10)
        new_started_exact=new_started.strftime('%Y-%m-%dT%H:%M:%S')+'.'+f'{new_created%10000000:07d}'+'Z'
        receipt.update(web_pid=new_web.pid,web_started=new_started_exact)
        receipt_path.write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    if child:k.CloseHandle(child)
    k.CloseHandle(web)
    for file in files:file.close()
deadline=time.monotonic()+20
while time.monotonic()<deadline:
    try:after_status=api('/api/terminal',query=scope);break
    except (OSError,urllib.error.URLError):time.sleep(.2)
else:raise RuntimeError('正式后台未重新就绪')
assert not after_status['active'] and after_status['handle'] is None
stale=api('/api/terminal/'+handle+'/output',query=scope,expected=409)
after=idle();assert after==before,'模型台账或唯一绑定发生变化'
result={'stage':'0.2.98正式安装版异常退出','old_web_pid':old_web_pid,'owned_terminal_handle':handle,'child_process_image':pathlib.Path(child_binary).name,'child_creation_100ns':child_created,'held_child_handle_wait_before':258,'held_child_handle_wait_after':child_wait,'new_web_pid':new_web.pid,'after_restart':after_status,'old_handle_http':409,'old_handle_error':stale,'model_counts_unchanged':True,'generated_model_requests':0,'bindings':after['bindings'],'safety_records_modified':False}
(folder/'crash-restart-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'real_job_descendant_signaled':True,'restart_terminal_absent':True,'stale_handle_http':409,'same_cloud_binding':True,'model_requests':0},ensure_ascii=False))
