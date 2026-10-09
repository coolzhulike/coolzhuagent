"""只采集宿主终态/工具事实与最终回复，核真实大回执原文对象。"""
from pathlib import Path
import sqlite3,json,hashlib
folder=Path(__file__).resolve().parent;root=folder.parents[1];db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
marker='INSTALLED102-LARGE-RESULT-NO-READER-20261008'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 row=c.execute("SELECT r.id,r.state,r.legacy_turn_id,m.created_at FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id JOIN json_each(e.payload_json,'$.message_ids') refs JOIN chat_room_messages m ON m.id=refs.value WHERE e.event_type='chat.source_messages' AND m.content LIKE ? ORDER BY m.created_at DESC LIMIT 1",(marker+'%',)).fetchone();assert row
 run=dict(row)
 attempts=[dict(x) for x in c.execute("SELECT attempt_id,state,protocol_stop,process_drained,model_json FROM devin_acp_attempts WHERE json_extract(scope_json,'$.run_id')=?",(run['id'],))]
 calls=[dict(x) for x in c.execute('SELECT tool_call_id,tool_name,status FROM tool_calls WHERE run_id=? ORDER BY created_at_unix_ms',(run['id'],))]
 events=[dict(x) for x in c.execute("SELECT event_type,payload_json,created_at FROM runtime_run_events WHERE run_id=? AND (event_type LIKE 'tool.%' OR event_type LIKE 'run.%') ORDER BY id",(run['id'],))]
 reply=dict(c.execute("SELECT id,kind,content,created_at FROM chat_room_messages WHERE room_id='room-1791131523339' AND created_at>=? AND kind='assistant-reply' ORDER BY created_at LIMIT 1",(run['created_at'],)).fetchone())
 bindings=[dict(x) for x in c.execute("SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id='session-1791131217833'")]
 expected=json.loads((folder/'expected.json').read_text(encoding='utf-8'))
 known=set(json.loads((folder/'blobs-before.json').read_text(encoding='utf-8')))
 blobs=[]
 for p in (db.parent/'tool-results').glob('*.txt'):
  if p.name in known:continue
  b=p.read_bytes();text=b.decode('utf-8');parsed=json.loads(text)
  normalized=json.dumps(parsed,ensure_ascii=False)
  item={'file_name':p.name,'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest(),'contains_tail':expected['sentinel'] in normalized,'character_count':len(text)}
  assert p.name=='sha256-'+item['sha256']+'.txt'
  if item['contains_tail']:
   (folder/'original-tool-result.json').write_bytes(b)
  blobs.append(item)
 result={'version':'0.2.102','marker':marker,'run':run,'attempts':attempts,'calls':calls,'events':events,'visible_reply':reply,'bindings':bindings,'expected':expected,'new_blobs':blobs}
 (folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps({'run':run,'attempts':attempts,'calls':calls,'new_blobs':blobs,'visible_reply':reply['content']},ensure_ascii=False))
