from pathlib import Path
import sqlite3,json
folder=Path(__file__).resolve().parent
root=folder.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
records=[]
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as connection:
    connection.row_factory=sqlite3.Row
    for marker in ['SPILL-CAPABILITY-REVIEW-101-20261008']:
        source=connection.execute("SELECT id,created_at FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone()
        run=connection.execute("SELECT r.* FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",(source['id'],)).fetchone()
        assert run['state']=='completed',dict(run)
        attempts=[dict(row) for row in connection.execute("SELECT attempt_id,state,protocol_stop,process_drained,model_json FROM devin_acp_attempts WHERE json_extract(scope_json,'$.run_id')=?",(run['id'],))]
        assert attempts and all(row['process_drained']==1 and json.loads(row['model_json'])['effective']=='swe-2-medium' for row in attempts)
        calls=[dict(row) for row in connection.execute('SELECT * FROM tool_calls WHERE run_id=?',(run['id'],))]
        if True:
            assert len(calls)==1 and calls[0]['tool_name']=='plugin__cli_anything_status',calls
            assert calls[0]['status']=='completed',calls
        messages=[]
        for (payload,) in connection.execute("SELECT payload_json FROM runtime_run_events WHERE run_id=? AND event_type='chat.context_outputs'",(run['id'],)):
            for message in json.loads(payload)['messages']:
                row=connection.execute("SELECT id,content,kind FROM chat_room_messages WHERE id=?",(message['id'],)).fetchone()
                if row and row['kind']!='reasoning':messages.append(dict(row))
        audits=[]
        for line in (db.parent/'tool-audit.jsonl').read_text(encoding='utf-8').splitlines():
            if any(call['tool_call_id'] in line for call in calls):audits.append(json.loads(line))
        records.append({'marker':marker,'source':dict(source),'run':dict(run),'attempts':attempts,'calls':calls,'tool_audits':audits,'visible_messages':messages})
    bindings=[dict(row) for row in connection.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE room_id=? AND agent_id=?',('room-1791131523339','session-1791131217833'))]
    assert sum(row['remote_session_id']=='island-kayak' for row in bindings)==1 and not any(row['locked_attempt'] for row in bindings),bindings
result={'stage':'0.2.101正式真实源码审查；修补尚为源码候选','bindings':bindings,'records':records,'facts':'正式101仅完成实际源码审查和一次真实CLI可用性查询；不等同大结果修补、原文分页或Browser竞态验收。'}
(folder/'integration-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
for record in records:
    print(json.dumps({'marker':record['marker'],'run':record['run']['state'],'calls':[(call['tool_name'],call['status']) for call in record['calls']],'reply':record['visible_messages']},ensure_ascii=False))
