"""真实EXE/上传文件/发送前检查；同真实库副本的聊天室关闭模型，绝不调用模型。"""
from pathlib import Path
import hashlib,json,os,sqlite3,subprocess,time,tomllib,urllib.request,urllib.error,sys
p=Path(__file__).resolve().parent;root=p.parents[1];kind=sys.argv[1]
f=p/kind;workspace=f/'workspace';store=f/'attachments';(workspace/'.coolzhu').mkdir(parents=True);store.mkdir()
source=root/'tmp/2026-10-09-history-summary/snapshot.sqlite3'
db=workspace/'.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(source) as a,sqlite3.connect(db) as b:
 a.backup(b)
 b.execute("INSERT INTO chat_room_diagnostics(room_id,real_llm_enabled,updated_at) VALUES('room-1791131523339',0,0) ON CONFLICT(room_id) DO UPDATE SET real_llm_enabled=0")
 assert b.execute("SELECT real_llm_enabled FROM chat_room_diagnostics WHERE room_id='room-1791131523339'").fetchone()==(0,)
config=(root/'tmp/2026-10-09-history-summary/before/workspace/coolzhu.toml').read_text().replace('"enable_real_llm" = false','"enable_real_llm" = true')
assert tomllib.loads(config)['model']['enable_real_llm'] is True
(workspace/'coolzhu.toml').write_text(config,encoding='utf-8')
binary=Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe') if kind.startswith('before') else root/'target/debug/coolzhu-web-console.exe'
env=os.environ.copy();env.update(COOLZHU_RUNTIME_DIR=str(workspace),COOLZHU_WEB_ATTACHMENT_STORE=str(store),COOLZHU_LOG_DIR=str(f/'logs'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(f/'input-safety'))
for key in ('COOLZHU_WEB_STATIC_ROOT','COOLZHU_GUI_WEB_URL'):env.pop(key,None)
hint=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';saved=hint.read_bytes() if hint.exists() else None
base='http://127.0.0.1:8768';facts=[]
def request(path,data=None,headers=None):
 try:
  with urllib.request.urlopen(urllib.request.Request(base+path,data,headers or {}),timeout=15) as r:return r.status,r.read()
 except urllib.error.HTTPError as error:return error.code,error.read()
def counts():
 with sqlite3.connect(db) as c:return {table:c.execute('SELECT count(*) FROM '+table).fetchone()[0] for table in ('runtime_runs','devin_acp_attempts')}
before=counts()
with (f/'out.log').open('wb') as out,(f/'err.log').open('wb') as err:
 proc=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
 try:
  for _ in range(150):
   assert proc.poll() is None
   try:
    status,body=request('/api/sessions')
    if status==200:break
   except OSError:pass
   time.sleep(.1)
  else:raise RuntimeError('测试副本未就绪')
  image=(root/'modules/gui-web/packages/web-console/assets/icons/agent-green.png').read_bytes()
  for name,mime,original,changed in [('order.txt','text/plain','原始竹林订单'.encode(),'已被改写的订单'.encode()),('agent.png','image/png',image,image+b'changed-attachment-bytes')]:
   assert hashlib.sha256(original).digest()!=hashlib.sha256(changed).digest()
   boundary='coolzhu-integrity-'+kind
   # 头尾分别编码，保留真实文件字节。
   multipart=f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\nContent-Type: {mime}\r\n\r\n'.encode()+original+(f'\r\n--{boundary}\r\nContent-Disposition: form-data; name="kind"\r\n\r\n'+('image' if mime.startswith('image') else 'document')+f'\r\n--{boundary}--\r\n').encode()
   status,body=request('/api/attachments/upload',multipart,{'Content-Type':'multipart/form-data; boundary='+boundary});assert status==200,(status,body[:200])
   uploaded=json.loads(body);attachment=uploaded['attachment'];leaf=attachment['url'].split('/')[-1];path=store/leaf
   assert hashlib.sha256(path.read_bytes()).hexdigest() in leaf
   case={'name':name,'expected_sha256':hashlib.sha256(original).hexdigest(),'changed_sha256':hashlib.sha256(changed).hexdigest(),'checks':[]}
   for state,content in [('valid',original),('corrupted',changed),('restored',original)]:
    path.write_bytes(content)
    payload={'session_id':'session-1791131217833','target_agent_ids':['session-1791131217833'],'chat_room_id':'room-1791131523339','text':f'INTEGRITY-{kind}-{name}-{state}：检查附件，测试聊天室已关闭真实模型。','attachments':[attachment]}
    status,body=request('/api/chat/send',json.dumps(payload).encode(),{'Content-Type':'application/json'})
    expected=400 if state=='corrupted' and not kind.startswith('before') else 403
    assert status==expected,(kind,name,state,status,body[:300])
    assert ('摘要不匹配' in body.decode()) if expected==400 else ('真实模型调用已关闭' in body.decode())
    assert counts()==before,'发送前拒绝不得创建运行或模型尝试'
    case['checks'].append({'state':state,'http_status':status,'error':json.loads(body),'new_runtime_runs':0,'new_acp_attempts':0})
   facts.append(case)
 finally:
  proc.terminate();proc.wait(timeout=15)
  if saved is not None:hint.write_bytes(saved)
  elif hint.exists():hint.unlink()
result={'kind':kind,'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(),'cases':facts,'new_model_calls':0,'new_cloud_sessions':0,'original_database_writes':0,'own_process_reaped':True}
(f/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'kind':kind,'cases':len(facts),'passed':True,'new_model_calls':0}),flush=True)
