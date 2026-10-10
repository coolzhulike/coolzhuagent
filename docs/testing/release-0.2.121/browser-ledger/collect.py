from pathlib import Path
import json,sqlite3
root=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 source=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE 'BU-CROSS-ORIGIN-LIVE-LEDGER-INSTALLED121-20261009%' ORDER BY created_at DESC LIMIT 1").fetchone();assert source
 run=c.execute("SELECT r.* FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",(source['id'],)).fetchone();assert run
 calls=[dict(x) for x in c.execute('SELECT * FROM tool_calls WHERE run_id=?',(run['id'],))]
 attempts=[dict(x) for x in c.execute("SELECT attempt_id,state,protocol_stop,process_drained,model_json FROM devin_acp_attempts WHERE json_extract(scope_json,'$.run_id')=?",(run['id'],))]
 bindings=[dict(x) for x in c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(run['session_id'],))]
 visible=[]
 for (payload,) in c.execute("SELECT payload_json FROM runtime_run_events WHERE run_id=? AND event_type='chat.context_outputs'",(run['id'],)):
  for message in json.loads(payload)['messages']:
   x=c.execute('SELECT id,content,kind FROM chat_room_messages WHERE id=?',(message['id'],)).fetchone()
   if x and x['kind']!='reasoning':visible.append(dict(x))
 if not visible and run['finished_at']:
  visible=[dict(x) for x in c.execute("SELECT id,content,kind FROM chat_room_messages WHERE room_id=? AND role='assistant' AND kind!='reasoning' AND created_at>=? AND created_at<=?",(run['chat_room_id'],run['created_at'],run['finished_at']+1000))]
 record={'stage':'0.2.121 Program Files正式安装版独立动态长程','source_user_id':source['id'],'run':dict(run),'calls':calls,'attempts':attempts,'bindings':bindings,'visible_reply':visible}
(p/'facts.json').write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'stage':record['stage'],'run_id':record['run']['id'],'state':record['run']['state'],
 'tools':len(record['calls']),'attempts':len(record['attempts']),'bindings':record['bindings']},ensure_ascii=False))
