"""完整发布服务HTTP恢复契约；仅隔离夹具，不发送外部消息或模型请求。"""
import concurrent.futures,datetime,hashlib,json,os,pathlib,socket,sqlite3,subprocess,time,urllib.request,urllib.error

REPO=pathlib.Path(r'D:\coolzhuagent')
OUT=REPO/'tmp/goal-32wbs-acceptance-20261013'
ROOT=OUT/'other-matrix-20261016/recovery-contract'
ROOT.mkdir(parents=True,exist_ok=False)
EXE=OUT/'release-0.2.135/package/bin/coolzhu-web-console.exe'
DB=ROOT/'sessions.sqlite3'
REPORT=OUT/'controller-recovery-contract-matrix-20261016.json'
PORT=29511
result={'state':'running','started_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),
 'boundary':'完整135发布服务与真实HTTP，隔离SQLite中人工预置outbox和execution_index；只证明持久化状态/恢复投影/HTTP门控，不代表外部投递exactly-once、真实模型在途取消、正式安装环境或队列UI已清理',
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
  env=dict(os.environ);env.update(COOLZHU_RUNTIME_DIR=str(ROOT),COOLZHU_WEB_SESSION_DB=str(DB),COOLZHU_WEB_SESSION_STORE=str(ROOT/'sessions.json'),COOLZHU_WEB_ATTACHMENT_STORE=str(ROOT/'attachments'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(ROOT/'input-safety'),COOLZHU_DESKTOP_SHELL_MANAGED='1')
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
GATEWAY=ROOT/'.coolzhu/clawbot-gateway.sqlite3'
def now():return int(time.time()*1000)
def seed_outbox(ident,state='pending',attempts=0,due=None,updated=None):
 t=now()
 with connect(GATEWAY) as db:
  db.execute('INSERT INTO clawbot_outbox(id,account_id,peer_id,body,state,attempts,next_attempt_at_ms,created_at_ms,updated_at_ms) VALUES(?,?,?,?,?,?,?,?,?)',(ident,'isolated-no-provider','isolated-no-recipient','隔离反馈'+str(ident),state,attempts,t if due is None else due,t,t if updated is None else updated))
def outbox_rows():
 with connect(GATEWAY) as db:return db.execute('SELECT id,state,attempts,next_attempt_at_ms,last_error FROM clawbot_outbox ORDER BY id').fetchall()
def claim():
 code,body=s.request('/api/channels/clawbot/outbox?limit=100');require(code==200,(code,body));return body
def ack(ident):return s.request(f'/api/channels/clawbot/outbox/{ident}/ack',{})
def fail(ident,error):return s.request(f'/api/channels/clawbot/outbox/{ident}/fail',{'error':error})
try:
 s.launch();require(claim()==[],'新夹具应无发件');require(GATEWAY.is_file())
 def concurrent_claim():
  seed_outbox(1)
  with concurrent.futures.ThreadPoolExecutor(max_workers=16) as pool:batches=list(pool.map(lambda _:claim(),range(16)))
  ids=[row['id'] for batch in batches for row in batch];require(ids==[1],ids)
  return {'parallel_requests':16,'total_claims':len(ids),'id':1,'state':outbox_rows()[0][1]}
 case('2.4 outbox真实HTTP十六并发仅一次认领',concurrent_claim)
 def ack_idempotent():
  a=ack(1);before=outbox_rows();b=ack(1);c=fail(1,'迟到失败')
  require(a[0]==b[0]==c[0]==200,(a,b,c));require(outbox_rows()==before);require(a[1]['state']=='sent');require(claim()==[])
  return {'first_ack':a[0],'repeat_ack':b[0],'late_fail':c[0],'terminal_rows_unchanged':True,'reclaimed':False}
 case('2.4 sent重复ack和迟到fail不回退或重投',ack_idempotent)
 def future_and_invalid():
  seed_outbox(2,due=now()+3600000);seed_outbox(3,state='dead_letter',attempts=3)
  before=outbox_rows();a=ack(2);b=ack(3);c=fail(2,'  ');require(a[0]==b[0]==c[0]==400,(a,b,c));require(claim()==[]);require(outbox_rows()==before)
  return {'future_pending_ack':a[0],'dead_letter_ack':b[0],'empty_fail':c[0],'database_unchanged':True,'future_or_dead_claimed':False}
 case('2.4 未到期/死信不认领及非法回执无写入',future_and_invalid)
 def retry_deadline():
  seed_outbox(4);require([x['id'] for x in claim()]==[4]);start=now();code,row=fail(4,'隔离瞬时错误');end=now()
  require(code==200 and row['state']=='pending' and row['attempts']==1,(code,row));require(start+1000<=row['next_attempt_at_ms']<=end+1000);require(claim()==[])
  time.sleep(max(0,(row['next_attempt_at_ms']-now())/1000)+.08)
  items=claim();require([x['id'] for x in items]==[4]);require(ack(4)[0]==200)
  return {'attempts':1,'actual_delay_ms':row['next_attempt_at_ms']-start,'before_due_empty':True,'after_due_claims':1}
 case('2.4 HTTP失败后真实一秒退避到期才重认领',retry_deadline)
 def dead_limit():
  seed_outbox(5,state='sending',attempts=2);code,row=fail(5,'第三次隔离失败');require(code==200 and row['state']=='dead_letter' and row['attempts']==3,(code,row));before=outbox_rows()
  require(claim()==[]);require(fail(5,'再重复失败')[0]==200);require(outbox_rows()==before)
  return {'attempts':3,'state':'dead_letter','not_reclaimed':True,'repeat_fail_unchanged':True}
 case('2.4 重试上限进入死信且重复失败保持终态',dead_limit)
 seed_outbox(6);require([x['id'] for x in claim()]==[6]);before_crash=outbox_rows();old_headers=dict(s.headers)
 s.stop();s.launch();require(claim()==[]);require(outbox_rows()==before_crash)
 case('2.4 认领后强杀重启未过租约不重投且sent死信保留',lambda:{'rows_identical':outbox_rows()==before_crash,'prelease_reclaimed':False,'old_process_exited':result['processes'][0]['process_exited']})
 def lease_recovery():
  with connect(GATEWAY) as db:db.execute("UPDATE clawbot_outbox SET updated_at_ms=? WHERE id=6",(now()-31000,))
  with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:batches=list(pool.map(lambda _:claim(),range(12)))
  items=[x for b in batches for x in b];require(len(items)==1 and items[0]['id']==6 and items[0]['attempts']==1,items)
  require('lease expired' in items[0]['last_error']);require(ack(6)[0]==200)
  return {'clock_boundary':'只修改隔离DB时间戳至租约外，未修改系统时钟','parallel_requests':12,'total_claims':1,'lease_expiry_attempts':1,'acknowledged':True}
 case('2.4 租约过期后十二并发唯一回收并保留失败事实',lease_recovery)
 def exhausted_lease():
  seed_outbox(7,state='sending',attempts=2,updated=now()-31000);require(claim()==[])
  row=next(x for x in outbox_rows() if x[0]==7);require(row[1:3]==('dead_letter',3),row)
  return {'state':row[1],'attempts':row[2],'not_reclaimed':True}
 case('2.4 租约耗尽直接死信不再认领',exhausted_lease)

 # 持久化恢复夹具不是实际执行会话：不创建agent/调度对象、不调用模型。
 states=['running','cancel_requested','completed','failed','interrupted','outcome_unknown']
 with connect(DB) as db:
  for i,state in enumerate(states):
   spec={'id':f'fixture-{state}','workspace_id':s.workspace['workspace_id'],'room_id':'isolated-room','session_id':'isolated-not-a-live-session','provider':'Devin','model':'swe-2','source':'session_tool','source_id':'fixture-only','parent_run_id':None,'parent_execution_id':None,'call_id':None,'summary':'仅持久化契约夹具','deadline_ms':now()+60000}
   db.execute('INSERT INTO execution_index(id,workspace_id,spec_json,state,started_ms,finished_ms,result,error) VALUES(?,?,?,?,?,?,?,?)',(spec['id'],spec['workspace_id'],json.dumps(spec,ensure_ascii=False),state,now()-5000,None if state in states[:2] else now(),None if state in states[:2] else '已有结果'+state,None if state=='completed' else '已有错误'+state))
  other=dict(spec,id='fixture-other-workspace',workspace_id='different-workspace')
  db.execute('INSERT INTO execution_index(id,workspace_id,spec_json,state,started_ms) VALUES(?,?,?,?,?)',(other['id'],other['workspace_id'],json.dumps(other),'running',now()))
 def execution_snapshot():
  with connect(DB) as db:return db.execute('SELECT * FROM execution_index ORDER BY id').fetchall()
 persisted=execution_snapshot();s.stop();s.launch()
 def unknown_projection():
  code,body=s.request('/api/executions',headers=s.headers);require(code==200,(code,body));items=body['executions'];require(len(items)==6,items)
  for item in items:
   state=item['spec']['id'].removeprefix('fixture-');want='outcome_unknown' if state in states[:2] else state
   require(item['status']==want and item['can_cancel'] is False,item);require(item['input_tokens'] is None and item['output_tokens'] is None,item)
   if state in states[:2]:require(item['finished_ms'] is None and item['result'] is None,item)
  require(execution_snapshot()==persisted)
  return {'visible_states':{x['spec']['id']:x['status'] for x in items},'unknown_usage_preserved':True,'no_database_terminal_rewrite':True,'synthetic_index_only':True}
 case('1.1/2.5 重启后无活句柄运行与取消请求投影未知而非成功',unknown_projection)
 def nohandle_cancel():
  evidence=[]
  for state in states:
   code,body=s.request(f'/api/executions/fixture-{state}/cancel',{},s.headers);require(code==409 and '无存活控制句柄' in str(body),(code,body));evidence.append([state,code])
  require(execution_snapshot()==persisted)
  return {'responses':evidence,'no_false_cancel_success':True,'rows_unchanged':True}
 case('1.1/2.5 无活句柄取消六类状态均409且不伪造终态',nohandle_cancel)
 def scope_gate():
  results=[]
  for header in [{},{'x-coolzhu-workspace-id':s.workspace['workspace_id']},{**s.headers,'x-coolzhu-workspace-id':'stale'},old_headers]:
   code,body=s.request('/api/executions',headers=header);require(code in (400,409),(code,body));results.append(code)
  for path in ['/api/executions/fixture-other-workspace','/api/executions/fixture-other-workspace/trace']:
   code,body=s.request(path,headers=s.headers);require(code==404,(code,body))
  code,_=s.request('/api/executions/fixture-other-workspace/cancel',{},s.headers);require(code==409);require(execution_snapshot()==persisted)
  return {'missing_or_stale_headers':results,'foreign_detail_trace':404,'foreign_cancel':409,'rows_unchanged':True}
 case('2.5 旧配置身份缺失身份与跨工程访问拒绝',scope_gate)
 def detail_trace():
  code,item=s.request('/api/executions/fixture-running',headers=s.headers);require(code==200,(code,item));require(item['subtree']['execution_ids']==['fixture-running']);require(item['subtree']['input_tokens'] is None and item['subtree']['usage_attempt_count']==0)
  code,body=s.request('/api/executions/fixture-running/trace',headers=s.headers);require(code==409 and '尚未绑定真实运行' in str(body),(code,body));require(execution_snapshot()==persisted)
  return {'detail_status':200,'unbound_trace_status':409,'trace_not_guessed_from_room':True,'subtree_unknown_usage':True,'rows_unchanged':True}
 case('1.1/2.5 未绑定run不猜测房间轨迹且未知用量不补零',detail_trace)
 def repeated_recovery():
  s.stop();s.launch();require(execution_snapshot()==persisted);require(unknown_projection()['no_database_terminal_rewrite'])
  require(claim()==[]);require(outbox_rows()==stable_outbox)
  checks={}
  for name,path in [('session',DB),('gateway',GATEWAY)]:
   with connect(path) as db:checks[name]=db.execute('PRAGMA quick_check').fetchone()[0]
  require(all(x=='ok' for x in checks.values()))
  return {'second_restart_idempotent':True,'quick_check':checks,'no_outbox_replay':True,'no_execution_replay':True}
 stable_outbox=outbox_rows()
 case('2.4/2.5 再次强杀重启结果幂等无自动重放',repeated_recovery)
except Exception as e:result['harness_error']=repr(e)
finally:s.stop()
result.update(passed=sum(x['state']=='passed' for x in result['cases']),failed=sum(x['state']=='failed' for x in result['cases']),finished_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
result['state']='completed' if not result['failed'] and 'harness_error' not in result else 'failed';save()
print(json.dumps({k:result[k] for k in ['state','passed','failed']},ensure_ascii=False),flush=True)
raise SystemExit(0 if result['state']=='completed' else 1)
