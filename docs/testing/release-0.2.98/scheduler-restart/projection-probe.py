"""正式098停机期间到期的取消Goal收尾；真实定时器/SQLite，不调用模型或新云端会话。"""
from pathlib import Path
import ctypes,ctypes.wintypes as w,datetime,hashlib,json,os,sqlite3,subprocess,time,urllib.request,urllib.error,concurrent.futures,sys
p=Path(__file__).resolve().parent
receipt_path=p/'installed-098-standard-processes.json'
receipt=json.loads(receipt_path.read_text(encoding='utf-8-sig'))
root=Path(receipt['workspace']);db=root/'.coolzhu/web-sessions.sqlite3';jobs=root/'.coolzhu/scheduled-jobs.sqlite3'
session='session-1791131217833';room='room-1791131523339'
projection=len(sys.argv)>1 and sys.argv[1]=='projection'
marker=('SCHEDULER-PROJECTION-RECOVERY' if projection else 'SCHEDULER-OFFLINE-CANCELLED-GOAL')+'-INSTALLED098-20261008'
result={'marker':marker,'stage':'正式0.2.98','model_requests_expected':0};stopped=False;owned_task=None;config_lock=None
k=ctypes.WinDLL('kernel32',use_last_error=True)
k.OpenProcess.argtypes=[w.DWORD,w.BOOL,w.DWORD];k.OpenProcess.restype=w.HANDLE
k.CloseHandle.argtypes=[w.HANDLE];k.CloseHandle.restype=w.BOOL
k.QueryFullProcessImageNameW.argtypes=[w.HANDLE,w.DWORD,w.LPWSTR,ctypes.POINTER(w.DWORD)];k.QueryFullProcessImageNameW.restype=w.BOOL
k.GetProcessTimes.argtypes=[w.HANDLE,*([ctypes.POINTER(w.FILETIME)]*4)];k.GetProcessTimes.restype=w.BOOL
k.TerminateProcess.argtypes=[w.HANDLE,w.UINT];k.TerminateProcess.restype=w.BOOL
k.WaitForSingleObject.argtypes=[w.HANDLE,w.DWORD];k.WaitForSingleObject.restype=w.DWORD
k.CreateFileW.argtypes=[w.LPCWSTR,w.DWORD,w.DWORD,w.LPVOID,w.DWORD,w.DWORD,w.HANDLE];k.CreateFileW.restype=w.HANDLE
def opened(pid,access=0x101000):
 h=k.OpenProcess(access,False,pid);assert h,ctypes.get_last_error();return h
def identity(h):
 image=ctypes.create_unicode_buffer(32768);n=w.DWORD(len(image));assert k.QueryFullProcessImageNameW(h,0,image,ctypes.byref(n))
 ts=[w.FILETIME() for _ in range(4)];assert k.GetProcessTimes(h,*[ctypes.byref(x) for x in ts])
 return image.value,(ts[0].dwHighDateTime<<32)|ts[0].dwLowDateTime
def api(route,body=None,method=None):
 request=urllib.request.Request('http://127.0.0.1:8765'+route,json.dumps(body).encode() if body is not None else None,{'Content-Type':'application/json'},method=method)
 with urllib.request.urlopen(request,timeout=35) as r:return json.load(r)
def ledger():
 with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
  assert not c.execute("select id from runtime_runs where state not in ('completed','failed','cancelled','interrupted','aborted')").fetchall()
  assert not c.execute("select call_id from computer_use_runs where state not in ('succeeded','failed','blocked','cancelled','timed_out')").fetchall()
  bindings=c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=? order by lane',(session,)).fetchall()
  assert sum(x[1]=='island-kayak' for x in bindings)==1 and all(x[2] is None for x in bindings)
  return {'bindings':bindings,'attempts':c.execute('select count(*) from devin_acp_attempts').fetchone()[0],'usage':c.execute('select count(*) from chat_usage_events').fetchone()[0]}
def claims():
 if not jobs.exists():return []
 with sqlite3.connect(jobs.as_uri()+'?mode=ro',uri=True) as c:
  c.row_factory=sqlite3.Row
  try:return [dict(x) for x in c.execute('select workspace_id,task_id,scheduled_for_ms,fingerprint,state,started_at_ms,outcome_json from scheduled_job_attempts where task_id=?',(owned_task,))]
  except sqlite3.OperationalError as e:
   if 'no such table' in str(e):return []
   raise
def status_messages():
 with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
  return c.execute("select id,content from chat_room_messages where chat_room_id='room-scheduled-tasks' and content like ? order by created_at",('%'+marker+'%',)).fetchall()
def restart():
 global stopped,receipt
 env=os.environ.copy();env.pop('COOLZHU_WEB_STATIC_ROOT',None);env['COOLZHU_RUNTIME_DIR']=str(root);env['COOLZHU_INPUT_SAFETY_STATE_ROOT']='C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety'
 with (p/'scheduler-restart-web-out.log').open('wb') as out,(p/'scheduler-restart-web-err.log').open('wb') as err:
  process=subprocess.Popen([receipt['web_binary']],cwd=root,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
 h=opened(process.pid)
 try:
  image,created=identity(h);assert Path(image)==Path(receipt['web_binary'])
 finally:k.CloseHandle(h)
 dt=datetime.datetime(1601,1,1,tzinfo=datetime.timezone.utc)+datetime.timedelta(microseconds=created//10)
 receipt.update(web_pid=process.pid,web_started=dt.strftime('%Y-%m-%dT%H:%M:%S')+'.'+f'{created%10000000:07d}'+'Z')
 receipt_path.write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n',encoding='utf8');stopped=False
 result['restarted_process']=receipt
 deadline=time.monotonic()+20
 while time.monotonic()<deadline:
  try:api('/api/task-schedules');return
  except (OSError,urllib.error.URLError):time.sleep(.2)
 raise RuntimeError('原正式后台未恢复')
try:
 result['before_ledger']=ledger()
 initial=api('/api/task-schedules');assert not initial['tasks'],'保留现有计划，不进入本专项'
 assert not api('/api/terminal?session_id='+session+'&room_id='+room)['active'],'保留现有终端'
 assert not api('/api/lsp/status')['running'],'保留现有LSP'
 h=opened(receipt['web_pid'],0x101001)
 try:
  image,created=identity(h);assert Path(image)==Path(receipt['web_binary'])
  dt=datetime.datetime.fromisoformat(receipt['web_started'].replace('Z','+00:00'));delta=dt-datetime.datetime(1601,1,1,tzinfo=datetime.timezone.utc)
  expected=(delta.days*86400+delta.seconds)*10000000+delta.microseconds*10;assert abs(expected-created)<10
  assert hashlib.file_digest(Path(image).open('rb'),'sha256').hexdigest()==receipt['web_sha256']
  goal=api('/api/goals',{'title':marker,'chat_room_id':room,'max_iterations':1,'background':False})['goal'];result['created_goal']=goal
  cancelled=api('/api/goals/'+goal['id']+'/cancel',{},'POST')['goal'];assert cancelled['status']=='cancelled';result['cancelled_goal']=cancelled
  due=int(time.time()*1000)+(5000 if projection else 15_000)
  payload={'target_session_id':session,'content':marker,'run_at_ms':due,'schedule_kind':'once','permissions':[],'task_kind':'goal','goal_id':goal['id']}
  created_task=api('/api/task-schedules',payload)['tasks'];assert len(created_task)==1
  task=created_task[0];owned_task=task['id'];result['scheduled_task']=task
  assert not claims();assert ledger()==result['before_ledger'];assert time.time()*1000<due-3000
  if projection:
   config_path=root/'coolzhu.toml';config_sha=hashlib.file_digest(config_path.open('rb'),'sha256').hexdigest()
   config_lock=k.CreateFileW(str(config_path),0x80000000,1,None,3,0x80,None)
   assert config_lock and config_lock!=ctypes.c_void_p(-1).value,ctypes.get_last_error()
   while time.time()*1000<due+100:time.sleep(.05)
   result['locked_run_due']=api('/api/task-schedules/run-due',{},'POST')
   assert any(x['id']==owned_task and x['status']=='projection_pending' for x in result['locked_run_due']['ran'])
   result['while_locked_list']=api('/api/task-schedules');result['claims_before_restart']=claims();result['status_messages_before_restart']=status_messages()
   assert len(result['claims_before_restart'])==1 and result['claims_before_restart'][0]['state']=='settled'
   assert len(result['status_messages_before_restart'])==1
   assert any(x['id']==owned_task and x['state']=='recorded' for x in result['while_locked_list']['occurrences'])
   result['locked_config_sha256']=hashlib.file_digest(config_path.open('rb'),'sha256').hexdigest();assert result['locked_config_sha256']==config_sha
  result['stopped_ms']=time.time()*1000
  assert k.TerminateProcess(h,98);stopped=True;assert k.WaitForSingleObject(h,5000)==0
 finally:
  k.CloseHandle(h)
  if config_lock:k.CloseHandle(config_lock);config_lock=None
 while time.time()*1000<due+1000:time.sleep(.25)
 result['offline_due_ms']=time.time()*1000;result['claims_while_offline']=claims()
 if not projection:assert not result['claims_while_offline']
 restart();deadline=time.monotonic()+20
 while time.monotonic()<deadline:
  current=api('/api/task-schedules');task=next(x for x in current['tasks'] if x['id']==owned_task)
  if task['status']=='completed':break
  time.sleep(.1)
 else:raise RuntimeError('启动扫描未收尾已取消Goal任务')
 result['after_startup']=current;result['claims_after_startup']=claims()
 assert len(result['claims_after_startup'])==1 and result['claims_after_startup'][0]['state']=='settled'
 outcome=json.loads(result['claims_after_startup'][0]['outcome_json']);assert outcome['executed'] and outcome['goal_completed']
 with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:result['concurrent_run_due'] =list(pool.map(lambda _:api('/api/task-schedules/run-due',{},'POST'),range(5)))
 assert all(x['ran']==[] for x in result['concurrent_run_due'])
 assert claims()==result['claims_after_startup'];result['after_ledger']=ledger();assert result['before_ledger']==result['after_ledger']
 if projection:
  result['status_messages_after_restart']=status_messages();assert result['status_messages_after_restart']==result['status_messages_before_restart']
  assert result['claims_after_startup']==result['claims_before_restart']
 result['passed_scope']=('正式结果已结账但计划写入失败，重启只恢复投影、不重复Goal投递' if projection else '正式定时器启动补偿已取消Goal收尾、一次持久领取及终态重复扫描零执行')+'；不是真实模型派发或running中断恢复'
except Exception as e:
 result['failure']=str(e);raise
finally:
 if config_lock:k.CloseHandle(config_lock)
 if stopped:restart()
 (p/('scheduler-projection-result.json' if projection else 'scheduler-restart-result.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'scope':result.get('passed_scope'),'task_id':owned_task,'pid':receipt['web_pid'],'model_requests':0},ensure_ascii=False),flush=True)
