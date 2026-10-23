"""2.4 完整发布服务双实例/混合版本与在线epoch故障矩阵，仅隔离DB。"""
import datetime,hashlib,json,os,pathlib,socket,sqlite3,subprocess,time,urllib.request,urllib.error

REPO=pathlib.Path(r'D:\coolzhuagent')
OUT=REPO/'tmp/goal-32wbs-acceptance-20261013'
ROOT=OUT/'other-matrix-20261016/writer-live'
ROOT.mkdir(parents=True,exist_ok=False)
DB=ROOT/'shared.sqlite3'
NEW=OUT/'release-0.2.135/package/bin/coolzhu-web-console.exe'
OLD=pathlib.Path(r'C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe')
REPORT=OUT/'controller-writer-live-matrix-20261016.json'
result={'state':'running','started_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'boundary':'完整发布服务真实HTTP、两个独立进程及隔离DB；没有向正式8765/生产库注入故障，不等同正式生产安装环境',
 'external_model_calls':0,'source_modified':False,'binary_sha256':{str(x):hashlib.sha256(x.read_bytes()).hexdigest() for x in [NEW,OLD]},'cases':[],'processes':[]}
def save():REPORT.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
def require(v,detail=None):
 if not v:raise AssertionError(detail)
def case(name,fn):
 row={'name':name};result['cases'].append(row)
 try:row.update(state='passed',evidence=fn())
 except Exception as e:row.update(state='failed',error=repr(e))
 save();print(name,row['state'],flush=True)
 return row['state']=='passed'
def connect():
 c=sqlite3.connect(DB,timeout=10);c.execute('PRAGMA busy_timeout=10000');return c
def epoch():
 with connect() as c:return int(c.execute("SELECT value FROM metadata WHERE key='session_writer_epoch'").fetchone()[0])
def rows():
 with connect() as c:return c.execute('SELECT id,name FROM chat_rooms ORDER BY id').fetchall()
class Server:
 def __init__(self,name,port,exe=NEW):
  self.root=ROOT/name;self.root.mkdir();self.port=port;self.exe=exe;self.proc=None;self.log=None;self.record=None
  (self.root/'coolzhu.toml').write_text(f'[web]\nbind_addr="127.0.0.1:{port}"\n[pet]\nenabled=false\n[scheduled_tasks]\ntasks=[]\n',encoding='utf-8')
 def request(self,path,body=None):
  req=urllib.request.Request(f'http://127.0.0.1:{self.port}'+path,data=None if body is None else json.dumps(body,ensure_ascii=False).encode(),headers={'Content-Type':'application/json'})
  try:
   with urllib.request.urlopen(req,timeout=10) as r:return r.status,json.load(r)
  except urllib.error.HTTPError as e:
   text=e.read().decode('utf-8');
   try:text=json.loads(text)
   except ValueError:pass
   return e.code,text
 def launch(self,healthy=True):
  with socket.socket() as s:s.bind(('127.0.0.1',self.port))
  env=dict(os.environ);env.update(COOLZHU_RUNTIME_DIR=str(self.root),COOLZHU_WEB_SESSION_DB=str(DB),COOLZHU_WEB_SESSION_STORE=str(self.root/'sessions.json'),COOLZHU_WEB_ATTACHMENT_STORE=str(self.root/'attachments'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(self.root/'input-safety'),COOLZHU_DESKTOP_SHELL_MANAGED='1')
  self.log=(self.root/'server.log').open('wb');start=time.monotonic()
  self.proc=subprocess.Popen([str(self.exe)],cwd=self.root,env=env,stdout=self.log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
  self.record={'name':self.root.name,'pid':self.proc.pid,'exe':str(self.exe),'port':self.port,'expected_healthy':healthy};result['processes'].append(self.record);save()
  for _ in range(240):
   if self.proc.poll() is not None:break
   try:
    code,body=self.request('/api/workspace')
    if code==200:
     self.record.update(ready_seconds=round(time.monotonic()-start,3),http_opened=True)
     require(healthy,'竞争写者意外开放HTTP');save();return self.record
   except (OSError,TimeoutError):pass
   time.sleep(.2)
  self.record.update(exit_code=self.proc.poll(),http_opened=False);save()
  require(not healthy and self.proc.poll() not in (None,0),self.record)
  return self.record
 def stop(self,crash=False):
  if not self.proc:return
  if self.proc.poll() is None:
   self.proc.kill() if crash else self.proc.terminate()
   self.proc.wait(timeout=20)
  self.record.update(process_exited=True,exit_code=self.proc.returncode,termination='forced_crash' if crash else 'stop_or_already_exited')
  self.proc=None;self.log.close();save()
 def room(self,name):
  code,body=self.request('/api/chat/rooms',{'name':name});require(code==200,(code,body));return body['room']['id']
servers=[]
def new(name,port,exe=NEW):
 s=Server(name,port,exe);servers.append(s);return s
try:
 a=new('owner-135',29501);a.launch();first=epoch();room=a.room('首写者提交中文记录');committed=rows()
 case('2.4 完整135服务取得epoch并通过HTTP提交中文房间',lambda:{'epoch':first,'room':room,'persisted':rows()==committed})
 def competitor(exe,name,port):
  before=(epoch(),rows());s=new(name,port,exe)
  try:
   r=s.launch(healthy=False);log=(s.root/'server.log').read_text(encoding='utf-8',errors='replace')
   require('会话库已由另一进程写入' in log,log[-2000:]);require((epoch(),rows())==before,'竞争者修改共享库');require(a.request('/api/chat/rooms')[0]==200)
   return {'rejected_exit':r['exit_code'],'http_opened':False,'epoch_and_rows_unchanged':True,'owner_still_healthy':True}
  finally:s.stop()
 case('2.4 同版第二实例在迁移和HTTP前拒绝',lambda:competitor(NEW,'rejected-135',29502))
 case('2.4 135持有期间旧134完整服务拒绝且不改库',lambda:competitor(OLD,'rejected-134',29503))
 a.stop(crash=True)
 b=new('successor-134',29504,OLD)
 def old_takeover():
  b.launch();require(epoch()==first+1);require(rows()==committed)
  ident=b.room('旧版接管后提交中文记录');require(any(x[0]==ident for x in rows()))
  return {'epoch':epoch(),'prior_rows_preserved':True,'new_room':ident,'old_owner_confirmed_exited':True}
 if not case('2.4 持有者强杀后134服务接管并实际保存',old_takeover):raise RuntimeError('旧版接管前置失败')
 def new_competes_old():
  before=(epoch(),rows());s=new('new-vs-old',29505)
  try:
   r=s.launch(healthy=False);require((epoch(),rows())==before);require(b.request('/api/chat/rooms')[0]==200)
   return {'rejected_exit':r['exit_code'],'old_owner_healthy':True,'db_unchanged':True}
  finally:s.stop()
 case('2.4 134持有期间135服务同样拒绝',new_competes_old)
 old_rows=rows();old_epoch=epoch();b.stop(crash=True)
 c=new('reacquired-135',29506)
 def back_to_new():
  c.launch();require(epoch()==old_epoch+1);require(rows()==old_rows)
  return {'epoch':epoch(),'both_versions_rows_preserved':True}
 if not case('2.4 再由135接管且跨版本业务记录保留',back_to_new):raise RuntimeError('重新接管前置失败')
 def stale_save():
  before=rows();forced=epoch()+100
  with connect() as db:db.execute("UPDATE metadata SET value=? WHERE key='session_writer_epoch'",(str(forced),))
  results=[]
  for name in ['旧缓存幽灵房间一','旧缓存幽灵房间二']:
   code,body=c.request('/api/chat/rooms',{'name':name});require(code==409,(code,body));require('写者版本已变化' in str(body),(code,body));results.append({'status':code,'error':body})
   require(rows()==before,'旧缓存被写入')
   status,body=c.request('/api/chat/rooms');require(status==200);require(name not in json.dumps(body,ensure_ascii=False),'失败写入泄露到缓存读取')
  require(epoch()==forced)
  return {'injected_epoch':forced,'responses':results,'database_and_visible_cache_unchanged':True,'retry_did_not_succeed':True}
 case('2.4 在线权威epoch改变后两次HTTP保存均409且无幽灵缓存',stale_save)
 before_restart=rows();forced=epoch();c.stop(crash=True)
 d=new('recovery-135',29507)
 def recover():
  d.launch();require(epoch()==forced+1);require(rows()==before_restart)
  ident=d.room('epoch异常重启后正常提交');require(any(x[0]==ident for x in rows()))
  with connect() as db:require(db.execute('PRAGMA quick_check').fetchone()[0]=='ok')
  return {'epoch':epoch(),'new_room':ident,'all_committed_rows_preserved':True,'quick_check':'ok','no_epoch_decrement':True}
 case('2.4 异常后重启取得更高epoch并恢复正常保存',recover)
except Exception as e:result['harness_error']=repr(e)
finally:
 for s in reversed(servers):s.stop()
result.update(passed=sum(x['state']=='passed' for x in result['cases']),failed=sum(x['state']=='failed' for x in result['cases']),finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
result['state']='completed' if not result['failed'] and 'harness_error' not in result else 'failed';save()
print(json.dumps({k:result[k] for k in ['state','passed','failed']},ensure_ascii=False),flush=True)
raise SystemExit(0 if result['state']=='completed' else 1)
