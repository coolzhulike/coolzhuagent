"""隔离服务的真实调度/共享worker重启链；模型仅为本机协议夹具，不调用远端或建立监督。"""
import datetime, hashlib, http.server, json, os, pathlib, socket, subprocess, threading, time, urllib.request, urllib.error, urllib.parse
REPO=pathlib.Path(r'D:\coolzhuagent'); R=REPO/'tmp/goal-32wbs-acceptance-20261013'
W=R/'scheduled-worker-key-20261016';W.mkdir(exist_ok=False)
EXE=R/'cargo-target/debug/coolzhu-web-console.exe';PORT=29544
REPORT=R/'controller-scheduled-worker-key-20261016.json'
result=dict(state='running',boundary='隔离真实HTTP服务、调度和worker；本机确定性模型协议夹具，不是外部模型自主行为或正式GUI',
    binary=str(EXE),binarySha256=hashlib.sha256(EXE.read_bytes()).hexdigest(),steps=[],processes=[],model_requests=[],productionTouched=False)
release=threading.Event()
def save():REPORT.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
class Model(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_POST(self):
  body=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
  idx=len(result['model_requests'])+1
  result['model_requests'].append(dict(index=idx,path=self.path,stream=body.get('stream',False),model=body.get('model')));save()
  if idx>1:release.wait(120)
  text='ISOLATED_SCHEDULE_WORKER_OK' if idx==1 else 'LATE_OUTPUT_MUST_NOT_COMMIT'
  try:
   if body.get('stream'):
    chunks=[dict(id='isolated-'+str(idx),object='chat.completion.chunk',model=body.get('model'),choices=[dict(index=0,delta=dict(role='assistant',content=text),finish_reason=None)]),dict(id='isolated-'+str(idx),object='chat.completion.chunk',model=body.get('model'),choices=[dict(index=0,delta={},finish_reason='stop')])]
    payload=(''.join('data: '+json.dumps(x)+'\n\n' for x in chunks)+'data: [DONE]\n\n').encode();ctype='text/event-stream'
   else:
    payload=json.dumps(dict(id='isolated-'+str(idx),object='chat.completion',model=body.get('model'),choices=[dict(index=0,message=dict(role='assistant',content=text),finish_reason='stop')],usage=dict(prompt_tokens=8,completion_tokens=4,total_tokens=12))).encode();ctype='application/json'
   self.send_response(200);self.send_header('Content-Type',ctype);self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload)
  except (BrokenPipeError,ConnectionResetError,ConnectionAbortedError):pass
model=http.server.ThreadingHTTPServer(('127.0.0.1',0),Model);model.daemon_threads=True
threading.Thread(target=model.serve_forever,daemon=True).start()
headers={};proc=None;server_log=None
def request(path,body=None):
 req=urllib.request.Request(f'http://127.0.0.1:{PORT}'+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json',**headers})
 try:
  with urllib.request.urlopen(req,timeout=20) as res:return res.status,json.load(res)
 except urllib.error.HTTPError as e:
  raw=e.read().decode('utf-8');return e.code,json.loads(raw)
def call(path,body=None):
 code,data=request(path,body);assert code==200,(path,code,data);return data
def wait(check,seconds=70):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  value=check()
  if value:return value
  time.sleep(.3)
 raise TimeoutError('等待事实超时')
def start():
 global proc,headers,server_log
 with socket.socket() as s:s.bind(('127.0.0.1',PORT))
 env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(W),COOLZHU_WEB_SESSION_DB=str(W/'sessions.sqlite3'),COOLZHU_WEB_SESSION_STORE=str(W/'store.json'),
  COOLZHU_WEB_ATTACHMENT_STORE=str(W/'attachments'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(W/'input-safety'),COOLZHU_DESKTOP_SHELL_MANAGED='1',
  CLAW_CONFIG_HOME=str(W/'config-home'),TEMP=str(W/'temp'),TMP=str(W/'temp'))
 server_log=(W/f'server-{len(result["processes"])+1}.log').open('wb')
 proc=subprocess.Popen([str(EXE)],cwd=W,env=env,stdout=server_log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
 result['processes'].append(dict(pid=proc.pid));save()
 def ready():
  assert proc.poll() is None,'隔离服务提前退出'
  try:return call('/api/workspace')
  except (OSError,TimeoutError):return None
 workspace=wait(ready)
 assert pathlib.Path(workspace['workspace']).resolve()==W.resolve(),workspace
 headers.update({'x-coolzhu-workspace-id':workspace['workspace_id'],'x-coolzhu-configuration-scope':workspace['configuration_scope']})
def stop():
 global proc
 if proc:
  if proc.poll() is None:proc.kill()
  proc.wait(timeout=20);result['processes'][-1]['exited']=True;server_log.close();proc=None;save()
def step(name,evidence):result['steps'].append(dict(name=name,state='passed',evidence=evidence));save();print(name,flush=True)
def execution(ident):return call('/api/executions/'+urllib.parse.quote(ident,safe=''))
def schedule(session,room,text,delay):
 tasks=call('/api/task-schedules',dict(target_session_id=session,chat_room_id=room,content=text,run_at_ms=int(time.time()*1000)+delay,
  schedule_kind='once',task_kind='poll',permissions=[]))['tasks']
 return next(t for t in tasks if t['content']==text)
try:
 (W/'temp').mkdir();(W/'coolzhu.toml').write_text(f'[web]\nbind_addr="127.0.0.1:{PORT}"\n[pet]\nenabled=false\n[model]\nenable_real_llm=true\nenable_llm_tools=false\n[scheduled_tasks]\ntasks=[]\n',encoding='utf-8')
 start()
 session=call('/api/sessions',dict(name='隔离调度协议夹具，不是工程执行会话',provider='custom',model='isolated-worker-model',base_url=f'http://127.0.0.1:{model.server_port}/v1',api_key_ref=''))['session']['id']
 room=call('/api/chat/rooms',dict(name='隔离调度验收'))['room']['id']
 task=schedule(session,room,'完成唯一一次隔离调度验收',15000)
 ident=f'scheduled:{task["id"]}:{task["run_at_ms"]}'
 stop();assert len(result['model_requests'])==0
 start();assert any(t['id']==task['id'] for t in call('/api/task-schedules')['tasks'])
 wait(lambda:len(result['model_requests'])==1)
 def completed():
  item=execution(ident);return item if item['state']=='completed' else None
 first=wait(completed);assert first['run_id'] and 'ISOLATED_SCHEDULE_WORKER_OK' in first['result']
 stop();start()
 after=execution(ident);assert after['run_id']==first['run_id'] and after['state']=='completed'
 call('/api/task-schedules/run-due',{});assert len(result['model_requests'])==1
 step('4.5 一次性任务落盘后重启，真实执行一次，终态重启不重放',dict(executionId=ident,runId=first['run_id'],modelCalls=1))
 task2=schedule(session,room,'等待取消的独立共享worker',0)
 second_id=f'scheduled:{task2["id"]}:{task2["run_at_ms"]}'
 wait(lambda:len(result['model_requests'])==2)
 live=execution(second_id);assert live['run_id'] and live['state']=='running',live
 call('/api/executions/'+urllib.parse.quote(second_id,safe='')+'/cancel',{})
 release.set()
 def terminal():
  item=execution(second_id);return item if item['state'] not in ('running','cancel_requested') else None
 second=wait(terminal);assert second['state']!='completed' and 'LATE_OUTPUT_MUST_NOT_COMMIT' not in str(second.get('result'))
 stop();start()
 final=execution(second_id);assert final['run_id']==live['run_id'] and final['state']!='completed'
 call('/api/task-schedules/run-due',{});assert len(result['model_requests'])==2
 step('2.5 真实共享worker运行、取消、重启后同身份查询，不自动重放',dict(executionId=second_id,runId=live['run_id'],state=final['state'],totalModelCalls=2))
 result['state']='completed'
except Exception as e:result.update(state='failed',error=repr(e))
finally:
 release.set();stop();model.shutdown();model.server_close()
 result['finishedAtUtc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
print(json.dumps(result,ensure_ascii=False));raise SystemExit(0 if result['state']=='completed' else 1)
