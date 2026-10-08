"""只读归档当前两次真实插件超时；不复制思考正文或凭据。"""
import pathlib,sqlite3,json,sys
repo=pathlib.Path('C:\\Users\\zhupu\\.codex\\worktrees\\input-recovery-20261004\\coolzhuagent')
folder=pathlib.Path(__file__).resolve().parent
marker,name=sys.argv[1:3]
db=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 source=c.execute("select id,created_at from chat_room_messages where role='user' and content like ? order by created_at desc limit 1",(marker+'%',)).fetchone()
 assert source,'未发送此任务'
 r=c.execute("select r.* from runtime_runs r join runtime_run_events e on e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs where e.event_type='chat.source_messages' and refs.value=? limit 1",(source['id'],)).fetchone()
 assert r['state'] in ('completed','failed','interrupted')
 calls=[dict(x) for x in c.execute('select * from tool_calls where run_id=?',(r['id'],))]
 assert len(calls)==1 and calls[0]['tool_name']=='dsh__1ef9be67fd4ddb42d93d68c084b38983'
 attempts=[dict(x) for x in c.execute("select attempt_id,state,protocol_stop,process_drained,model_json from devin_acp_attempts where json_extract(scope_json,'$.run_id')=?",(r['id'],))]
 assert attempts and all(x['process_drained']==1 and json.loads(x['model_json'])['effective']=='swe-2-medium' for x in attempts)
 bindings=[dict(x) for x in c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where room_id=? and agent_id=?',('room-1791131523339','session-1791131217833'))]
 assert any(x['remote_session_id']=='island-kayak' for x in bindings) and not any(x['locked_attempt'] for x in bindings)
 messages=[]
 for (payload,) in c.execute("select payload_json from runtime_run_events where run_id=? and event_type='chat.context_outputs'",(r['id'],)):
  for m in json.loads(payload)['messages']:
   row=c.execute('select content,kind from chat_room_messages where id=?',(m['id'],)).fetchone()
   if row and row['kind']!='reasoning':messages.append(row['content'])
 audit=[json.loads(line) for line in (db.parent/'tool-audit.jsonl').read_text(encoding='utf-8').splitlines() if calls[0]['tool_call_id'] in line]
 events=[json.loads(line) for line in (folder/'slow-events.jsonl').read_text(encoding='utf-8').splitlines()]
 # 两轮为不同path，按各真实轮次开始时间和终态时间截取网络事件。
 import datetime
 epoch=lambda e:datetime.datetime.fromisoformat(e['utc']).timestamp()*1000
 events=[e for e in events if r['created_at']<=epoch(e)<=r['finished_at']+2000]
 record={'revocation_observation':json.loads((folder/'revocation-observation.json').read_text(encoding='utf-8')),'version':'0.2.99','source_commit':'23bbb94a0e4c2799f98025e48e41660206287698','marker':marker,'run':dict(r),'calls':calls,'tool_audit':audit,'network_events':events,'attempts':attempts,'bindings':bindings,'actual_visible_reply':'\n'.join(messages),'boundary':'完整瞬时工具响应未独立持久化；回复是模型转述，执行与清理须结合宿主审计和真实网络断开事实。独立Win32进程句柄证据另见held-process-result.json；仍不外推所有清理阶段。'}
 target=folder/name
 target.write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
 print(json.dumps({'run':r['state'],'calls':[(x['status'],x['tool_name']) for x in calls],'network':events,'reply':messages},ensure_ascii=False))
