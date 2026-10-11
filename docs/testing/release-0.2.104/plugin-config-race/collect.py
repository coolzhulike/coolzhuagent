"""只读正式事实与最终回复，不读取模型思考。"""
from pathlib import Path
import json,sqlite3
folder=Path(__file__).resolve().parent;root=folder.parents[1]
observation=json.loads((folder/'config-observation.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
    c.row_factory=sqlite3.Row
    run=dict(c.execute('SELECT id,state,legacy_turn_id FROM runtime_runs WHERE id=?',(observation['run_id'],)).fetchone())
    attempts=[dict(r) for r in c.execute("SELECT attempt_id,state,protocol_stop,process_drained,model_json FROM devin_acp_attempts WHERE json_extract(scope_json,'$.run_id')=?",(run['id'],))]
    calls=[dict(r) for r in c.execute('SELECT tool_name,status,tool_call_id FROM tool_calls WHERE run_id=?',(run['id'],))]
    events=[dict(r) for r in c.execute("SELECT event_type,payload_json,created_at FROM runtime_run_events WHERE run_id=? AND (event_type LIKE 'tool.%' OR event_type LIKE 'run.%') ORDER BY id",(run['id'],))]
    source=c.execute("SELECT created_at FROM chat_room_messages WHERE room_id=? AND role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",('room-1791131523339','PLUGIN-CONFIG-RACE-104-20261008%')).fetchone()
    reply=dict(c.execute("SELECT id,kind,content,created_at FROM chat_room_messages WHERE room_id=? AND created_at>=? AND kind='assistant-reply' ORDER BY created_at LIMIT 1",('room-1791131523339',source['created_at'])).fetchone())
    bindings=[dict(r) for r in c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',))]
    after={'attempts':c.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0],'messages':c.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0]}
result={'version':'0.2.104','run':run,'attempts':attempts,'tool_calls':calls,'runtime_events':events,'visible_reply':reply,'bindings':bindings,'after':after,'observation':observation}
(folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'run':run,'attempts':attempts,'tool_calls':calls,'runtime_events':events,'after':after},ensure_ascii=False))
