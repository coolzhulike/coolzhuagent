"""完整发布服务HTTP恢复契约；仅隔离夹具，不发送外部消息或模型请求。"""
import concurrent.futures,datetime,hashlib,json,os,pathlib,socket,sqlite3,subprocess,time,urllib.request,urllib.error

REPO=pathlib.Path(r'D:\coolzhuagent')
OUT=REPO/'tmp/goal-32wbs-acceptance-20261013'
ROOT=OUT/'other-matrix-20261016/permission-expiry'
ROOT.mkdir(parents=True,exist_ok=False)
EXE=OUT/'release-0.2.135/package/bin/coolzhu-web-console.exe'
DB=ROOT/'sessions.sqlite3'
REPORT=OUT/'controller-permission-expiry-matrix-20261016.json'
PORT=29512
result={'state':'running','started_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'boundary':'完整135发布服务真实HTTP、隔离Devin SWE-2本地对象；只读取人工文本验证FullAccess与pending自然过期，不代表模型调用、DSH冻结grant、在途撤权或正式页面完整验收',
 'production_database_touched':False,'external_model_calls':0,'external_message_sends':0,
 'source_modified':False,'binary':str(EXE),'binary_sha256':hashlib.sha256(EXE.read_bytes()).hexdigest(),
 'cases':[],'processes':[]}
def save():REPORT.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def require(value,detail=None):
 if not value:raise AssertionError(detail)
def case(name,fn):
 row={'name':name};result['cases'].append(row)
 try:row.update(state='passed',evidence=fn())
 except Exception as e:row.update(state='failed',error=repr(e))
 save();print(name,row['state'],flush=True);return row['state']=='passed'
def connect(path):
 db=sqlite3.connect(path,timeout=10);db.execute('PRAGMA busy_timeout=10000');return db
class Server:
 def __init__(self):self.proc=None;self.log=None;self.record=None;self.serial=0
 def request(self,path,body=None,headers=None):
  req=urllib.request.Request(f'http://127.0.0.1:{PORT}'+path,data=None if body is None else json.dumps(body,ensure_ascii=False).encode(),headers={'Content-Type':'application/json',**(headers or {})})
  try:
   with urllib.request.urlopen(req,timeout=10) as response:return response.status,json.load(response)
  except urllib.error.HTTPError as e:
   raw=e.read().decode('utf-8')
   try:raw=json.loads(raw)
   except ValueError:pass
   return e.code,raw
 def launch(self):
  with socket.socket() as sock:sock.bind(('127.0.0.1',PORT))
  env=dict(os.environ);env.update(COOLZHU_LOG_DIR=str(ROOT/"logs"),COOLZHU_RUNTIME_DIR=str(ROOT),COOLZHU_WEB_SESSION_DB=str(DB),COOLZHU_WEB_SESSION_STORE=str(ROOT/'sessions.json'),COOLZHU_WEB_ATTACHMENT_STORE=str(ROOT/'attachments'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(ROOT/'input-safety'),COOLZHU_DESKTOP_SHELL_MANAGED='1')
  self.serial+=1;self.log=(ROOT/f'server-{self.serial}.log').open('wb')
  self.proc=subprocess.Popen([str(EXE)],cwd=ROOT,env=env,stdout=self.log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
  self.record={'pid':self.proc.pid,'serial':self.serial,'port':PORT};result['processes'].append(self.record);save()
  start=time.monotonic()
  while time.monotonic()-start<90:
   require(self.proc.poll() is None,'隔离服务启动失败')
   try:
    code,w=self.request('/api/workspace')
    if code==200:
     require(pathlib.Path(w['workspace']).resolve()==ROOT.resolve(),w)
     self.workspace=w;self.headers={'x-coolzhu-workspace-id':w['workspace_id'],'x-coolzhu-configuration-scope':w['configuration_scope']}
     self.record['ready_seconds']=round(time.monotonic()-start,3);save();return
   except (OSError,TimeoutError):pass
   time.sleep(.2)
  raise RuntimeError('隔离服务启动超时')
 def stop(self):
  if self.proc:
   if self.proc.poll() is None:self.proc.kill();self.proc.wait(timeout=20)
   self.record.update(process_exited=True,exit_code=self.proc.returncode);self.log.close();self.proc=None;save()
s=Server()
(ROOT/'coolzhu.toml').write_text(f'[web]\nbind_addr="127.0.0.1:{PORT}"\n[pet]\nenabled=false\n[scheduled_tasks]\ntasks=[]\n',encoding='utf-8')
# 此文件由同目录组装命令接到自包含Server前缀后执行。
OUTSIDE=ROOT.parent/'permission-external-fixture.txt'
OUTSIDE.write_text('仅用于权限验收的无敏感文本',encoding='utf-8')
INSIDE=ROOT/'inside.txt';INSIDE.write_text('工作区内最低权限文本',encoding='utf-8')
def request(method,path,body=None):
 req=urllib.request.Request(f'http://127.0.0.1:{PORT}'+path,data=None if body is None else json.dumps(body,ensure_ascii=False).encode(),method=method,headers={'Content-Type':'application/json',**s.headers})
 try:
  with urllib.request.urlopen(req,timeout=10) as response:return response.status,json.load(response)
 except urllib.error.HTTPError as e:return e.code,json.loads(e.read())
def tool(name,path=None,session=None,call=None):
 payload={'tool_name':name,'input':{} if path is None else {'path':str(path)},'session_id':session or A,'user_authorized':False,'user_confirmed_twice':False}
 if call:payload['call_id']=call
 code,body=request('POST','/api/tools/runtime-execute',payload);require(code==200,(code,body));return body
def pending():
 code,body=request('GET',f'/api/tools/pending?session_id={A}');require(code==200,(code,body));return body['pending']
def grant(ttl=60,**kwargs):return request('POST','/api/tools/full-access',{'session_id':A,'ttl_secs':ttl,'confirmed_twice':True,'risk_acknowledged':True,**kwargs})
def status():
 code,body=request('GET','/api/tools/full-access');require(code==200,(code,body));return body
def not_executed(body):
 require(body['outcome']['status']=='dry-run-only' and body['outcome']['output'] is None and body['outcome']['permission_gate']['decision']=='require-approval',body)
try:
 s.launch()
 sessions=[]
 for name in ['权限自然过期A','权限隔离B']:
  code,body=request('POST','/api/sessions',{'name':name,'provider':'Devin','model':'swe-2','model_type':'text'})
  require(code==200,(code,body));sessions.append(body['session']['id'])
 A,B=sessions;require(request('POST',f'/api/sessions/{A}/activate',{})[0]==200)
 result['test_sessions']={'provider':'Devin','model':'swe-2','local_objects_only':True,'ids':sessions}
 def baseline():
  body=tool('read_file',OUTSIDE,call='pending-natural-expiry');not_executed(body)
  require(body['pending_call_id']=='pending-natural-expiry');inside=tool('read_file',INSIDE)
  require(inside['outcome']['status']=='ok' and inside['outcome']['output'] is not None,inside)
  return {'outside':'require-approval','inside':'ok','output_outside':None,'pending_registered':True}
 if not case('1.4 隔离对象最低权限读取与待审批基线',baseline):raise RuntimeError('权限夹具前置失败')
 pending_started=time.monotonic()
 def invalid_grant():
  responses=[]
  for fields in [{'confirmed_twice':False},{'risk_acknowledged':False}]:
   code,body=grant(**fields);require(code==400,(code,body));require(status()['active'] is False);responses.append(code)
  return {'responses':responses,'no_grant_created':True}
 case('1.4 不完整确认不产生授权',invalid_grant)
 grant_start=time.monotonic();code,g=grant(ttl=1);require(code==200 and g['active'],(code,g))
 def active_scope():
  body=tool('read_file',OUTSIDE);require(body['outcome']['status']=='ok' and body['outcome']['output'] is not None,body)
  other=tool('read_file',OUTSIDE,session=B);not_executed(other)
  unknown=tool('no_such_tool');require(unknown['outcome']['status']=='rejected' and unknown['outcome']['output'] is None,unknown)
  require(58<=g['ttl_secs_remaining']<=60,g)
  return {'requested_ttl_secs':1,'actual_minimum_ttl_secs':60,'active_scope_read':'ok','other_session':'require-approval','unknown_tool':'rejected'}
 case('1.4 最短TTL钳制与授权会话隔离及未知工具拒绝',active_scope)
 print('等待60秒授权自然过期及300秒pending自然失效；未改系统时钟',flush=True)
 while time.monotonic()-grant_start<61:time.sleep(.25)
 def expired():
  stat=status();require(stat['active'] is False and stat['ttl_secs_remaining']==0,stat)
  body=tool('read_file',OUTSIDE,call='after-grant-expiry');not_executed(body)
  require(tool('read_file',INSIDE)['outcome']['status']=='ok')
  return {'wall_elapsed_seconds':round(time.monotonic()-grant_start,3),'active':False,'outside_output':None,'outside':'require-approval','inside':'ok','clock_modified':False}
 case('1.4 授权真实自然过期后恢复最低权限且零越界输出',expired)
 def wrong_pending_scope():
  code,body=request('POST','/api/tools/approve',{'call_id':'pending-natural-expiry','session_id':B,'scope':'once','confirmed_twice':True});require(code==404,(code,body))
  require(any(x['call_id']=='pending-natural-expiry' for x in pending()))
  return {'wrong_session_http':404,'original_pending_preserved':True,'no_output':True}
 case('1.4 跨会话消费pending拒绝且原待办保留',wrong_pending_scope)
 while time.monotonic()-pending_started<301:
  result['waiting']={'phase':'pending自然过期','elapsed_seconds':round(time.monotonic()-pending_started,1),'target_seconds':301};save();time.sleep(1)
 def pending_expired():
  rows=pending();require(all(x['call_id']!='pending-natural-expiry' for x in rows),rows)
  code,body=request('POST','/api/tools/approve',{'call_id':'pending-natural-expiry','session_id':A,'scope':'once','confirmed_twice':True})
  require(code==404 and '过期' in str(body),(code,body));require(status()['active'] is False)
  return {'wall_elapsed_seconds':round(time.monotonic()-pending_started,3),'absent_from_pending':True,'expired_approve_http':404,'no_execution_outcome':True,'no_session_grant':True,'clock_modified':False}
 case('1.4 pending真实五分钟自然失效且旧审批不能执行',pending_expired)
 def restart_ephemeral():
  require(grant()[0]==200);require(status()['active'] is True);s.stop();s.launch()
  require(status()['active'] is False);require(pending()==[])
  body=tool('read_file',OUTSIDE,call='after-restart');not_executed(body)
  with connect(DB) as db:
   rows=db.execute("SELECT provider,model FROM sessions WHERE id IN (?,?)",(A,B)).fetchall()
   require(len(rows)==2 and all(x[0]=='Devin' and x[1]=='swe-2' for x in rows),rows)
  return {'grant_not_persisted':True,'pending_not_replayed':True,'local_session_objects_preserved':True,'post_restart_outside_output':None}
 case('1.4 重启不恢复临时授权或重放pending',restart_ephemeral)
except Exception as e:result['harness_error']=repr(e)
finally:s.stop()
result.pop('waiting',None)
result.update(passed=sum(x['state']=='passed' for x in result['cases']),failed=sum(x['state']=='failed' for x in result['cases']),finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
result['state']='completed' if not result['failed'] and 'harness_error' not in result else 'failed';save()
print(json.dumps({k:result[k] for k in ['state','passed','failed']},ensure_ascii=False),flush=True)
raise SystemExit(0 if result['state']=='completed' else 1)
