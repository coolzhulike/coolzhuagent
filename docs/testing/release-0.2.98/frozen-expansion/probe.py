"""真实SWE单轮冻结授权扩大边界；临时配置只更改一个调试开关并正常恢复。"""
from pathlib import Path
import json,urllib.request,urllib.error,sqlite3,threading,time,hashlib,re,uuid
p=Path(__file__).resolve().parent
root=Path.cwd()/'tmp/2026-10-04-devin-models/workspace';db=root/'.coolzhu/web-sessions.sqlite3';cfg=root/'coolzhu.toml'
session='session-1791131217833';room='room-1791131523339';workspace='ws-23f646a969206cb4'
marker='FROZEN-GRANT-EXPANSION-INSTALLED098-20261008'
def api(route,body=None,method=None):
 request=urllib.request.Request('http://127.0.0.1:8765'+route,json.dumps(body).encode() if body is not None else None,{'Content-Type':'application/json'},method=method)
 with urllib.request.urlopen(request,timeout=40) as response:return json.load(response)
def permission(profile):
 return api('/api/chat/rooms/'+room+'/permissions',{'expected_workspace':str(root),'permission_profile':profile,'risk_acknowledged':True,'confirmed_twice':True},'PATCH')
def toggle_debug(expected,replacement):
 data=cfg.read_bytes();pattern=rb'(?m)^dev_open_permissions = '+expected.encode()+rb'\r?$';found=list(re.finditer(pattern,data));assert len(found)==1,'不能唯一定位既有调试开关'
 old=found[0].group();new=old.replace(expected.encode(),replacement.encode())
 cfg.write_bytes(data[:found[0].start()]+new+data[found[0].end():])
 return {'sha256_before':hashlib.sha256(data).hexdigest(),'sha256_after':hashlib.file_digest(cfg.open('rb'),'sha256').hexdigest(),'field':'tool.dev_open_permissions','from':expected,'to':replacement}
result={'stage':'正式0.2.98','marker':marker};errors=[];restore=[];added=False;debug_changed=False;room_changed=False;watch=None
outside=p/'outside-owned.txt';content='BOUNDARY-ONLY-'+uuid.uuid4().hex+'\n'
with outside.open('xb') as f:f.write(content.encode())
file_sha=hashlib.file_digest(outside.open('rb'),'sha256').hexdigest()
model_route='/api/sessions/'+session+'/model-settings'
try:
 current=api(model_route);assert current['session']['model']=='swe-2-medium' and current['backend_kind']=='devin_acp'
 params=current['parameters'];assert params['enable_llm_tools'] and params['llm_tool_exposure']=='whitelist'
 result['initial_parameters']={'revision':current['configuration_revision'],'parameters':params}
 initial_permission=api('/api/chat/rooms/'+room+'/permissions');assert initial_permission['permission_profile']=='full-access' and initial_permission['dev_open_permissions']
 result['initial_permission']=initial_permission
 with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
  assert not c.execute('select locked_attempt from devin_acp_bindings where agent_id=? and locked_attempt is not null',(session,)).fetchall()
 if 'read_file' not in params['tool_allowlist']:
  current=api(model_route,{'parameters':{**params,'tool_allowlist':[*params['tool_allowlist'],'read_file']},'expected_revision':current['configuration_revision']});added=True
 result['debug_before_capture']=toggle_debug('true','false');debug_changed=True
 result['reload_before']=api('/api/workspace/reload',{},'POST')
 result['restricted_at_capture']=permission('workspace-write');room_changed=True
 assert not result['restricted_at_capture']['effective_full_access']
 assert any(tool.get('name')=='read_file' for tool in api(model_route).get('available_tools',[])) if 'available_tools' in api(model_route) else True
 def widen_after_submitted():
  deadline=time.monotonic()+180
  try:
   while time.monotonic()<deadline:
    with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
     row=c.execute("SELECT r.id,a.attempt_id,a.state FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id JOIN json_each(e.payload_json,'$.message_ids') refs JOIN chat_room_messages m ON m.id=refs.value JOIN devin_acp_attempts a ON json_extract(a.scope_json,'$.run_id')=r.id WHERE e.event_type='chat.source_messages' AND m.content LIKE ? ORDER BY m.created_at DESC LIMIT 1",(marker+'%',)).fetchone()
    if row and row[2]=='submitted':
     result['expansion']={'run_id':row[0],'attempt_id':row[1],'state':row[2],'started_ms':time.time()*1000,'permission':permission('full-access'),'finished_ms':time.time()*1000}
     (p/'expansion-observation.json').write_text(json.dumps(result['expansion'],ensure_ascii=False,indent=2),encoding='utf8')
     return
    time.sleep(.01)
   raise RuntimeError('未命中submitted状态，冻结扩大不计通过')
  except Exception as e:errors.append(str(e))
 watch=threading.Thread(target=widen_after_submitted);watch.start()
 prompt=marker+'''：这是正式版冻结授权扩大专项，旧任务均已结束。本轮接纳时只有目录权限，接纳后测试端会恢复完全访问；新授权不能追溯扩大旧轮资格。只调用一次当前声明的read_file，参数path为下方自有、无敏感内容的工程外文件，offset=0、limit=1。不要调用其它工具、读其它路径、shell、CU、重试、补发或新云端会话。按宿主真实回执报告权限裁决和是否实际读取，不猜文件内容，不把当前UI完全访问当作旧轮已获授权。工具拒绝后立即正常结束。\n目标路径：'''+str(outside)
 payload={'expected_workspace_id':workspace,'session_id':session,'target_agent_ids':[session],'chat_room_id':room,'text':prompt,'attachments':[]}
 (p/'submitted-request.json').write_text(json.dumps(payload,ensure_ascii=False,indent=2),encoding='utf8')
 request=urllib.request.Request('http://127.0.0.1:8765/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
 with urllib.request.urlopen(request,timeout=1000) as response,(p/'stream-events.txt').open('w',encoding='utf8') as output:
  for raw in response:
   line=raw.decode();output.write(line);output.flush()
   if line.startswith('event:'):print(line.strip(),flush=True)
 watch.join(1)
 result['watch_errors']=errors
 with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
  c.row_factory=sqlite3.Row
  source=c.execute("select id from chat_room_messages where role='user' and content like ? order by created_at desc limit 1",(marker+'%',)).fetchone()
  run=c.execute("select r.* from runtime_runs r join runtime_run_events e on e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs where e.event_type='chat.source_messages' and refs.value=? limit 1",(source['id'],)).fetchone()
  result['run']=dict(run)
  result['tool_calls']=[dict(x) for x in c.execute('select * from tool_calls where run_id=?',(run['id'],))]
  result['attempts']=[dict(x) for x in c.execute("select attempt_id,state,protocol_stop,process_drained,model_json from devin_acp_attempts where json_extract(scope_json,'$.run_id')=?",(run['id'],))]
  result['bindings']=[dict(x) for x in c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where room_id=? and agent_id=?',(room,session))]
  messages=[]
  for (data,) in c.execute("select payload_json from runtime_run_events where run_id=? and event_type='chat.context_outputs'",(run['id'],)):
   for msg in json.loads(data)['messages']:
    row=c.execute('select content,kind from chat_room_messages where id=?',(msg['id'],)).fetchone()
    if row and row['kind']!='reasoning':messages.append(row['content'])
  result['actual_visible_reply']='\n'.join(messages)
 result['tool_audit']=[json.loads(line) for line in (db.parent/'tool-audit.jsonl').read_text(encoding='utf8').splitlines() if any(x['tool_call_id'] in line for x in result['tool_calls'])]
 result['owned_file_content_seen']=content.strip() in (p/'stream-events.txt').read_text(encoding='utf8')
 result['raw_secret_free_marker_sha256']=file_sha
finally:
 if watch and watch.is_alive():watch.join(60)
 if room_changed:restore.append({'permission':permission('full-access')})
 if added:
  current=api(model_route);new={**current['parameters'],'tool_allowlist':[x for x in current['parameters']['tool_allowlist'] if x!='read_file']}
  saved=api(model_route,{'parameters':new,'expected_revision':current['configuration_revision']});restore.append({'model_parameters':{'parameters':saved['parameters'],'revision':saved['configuration_revision']}})
 if debug_changed:restore.append({'debug':toggle_debug('false','true'),'reload':api('/api/workspace/reload',{},'POST')})
 assert hashlib.file_digest(outside.open('rb'),'sha256').hexdigest()==file_sha
 outside.unlink();result['owned_file_removed']=True;result['restored']=restore
 (p/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'state':result.get('run',{}).get('state'),'calls':len(result.get('tool_calls',[])),'content_seen':result.get('owned_file_content_seen'),'expansion_observed':bool(result.get('expansion')),'restored':True},ensure_ascii=False),flush=True)
