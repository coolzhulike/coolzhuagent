"""独立收束事实；只有实际工具调用进入冻结权限判定才计此项通过。"""
from common import *
with connection() as c:
    c.row_factory=sqlite3.Row
    source=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone();assert source
    run=c.execute("SELECT r.* FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",(source['id'],)).fetchone();assert run
    calls=[dict(x) for x in c.execute('SELECT * FROM tool_calls WHERE run_id=?',(run['id'],))]
    attempts=[dict(x) for x in c.execute("SELECT attempt_id,state,protocol_stop,process_drained,model_json FROM devin_acp_attempts WHERE json_extract(scope_json,'$.run_id')=?",(run['id'],))]
    bindings=[dict(x) for x in c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(sid,))]
    visible=[]
    for (payload,) in c.execute("SELECT payload_json FROM runtime_run_events WHERE run_id=? AND event_type='chat.context_outputs'",(run['id'],)):
        for message in json.loads(payload)['messages']:
            x=c.execute('SELECT content,kind FROM chat_room_messages WHERE id=?',(message['id'],)).fetchone()
            if x and x['kind']!='reasoning':visible.append(x['content'])
    # 只留外部审计/阶段固定字段；不读取ACP原始隐藏思考。
    events=[{'id':x[0],'type':x[1],'created_at':x[2]} for x in c.execute('SELECT id,event_type,created_at FROM runtime_run_events WHERE run_id=? ORDER BY id',(run['id'],))]
network=[json.loads(x) for x in (p/'network-events.jsonl').read_text(encoding='utf-8').splitlines()]
mutation=json.loads((p/'grant-expansion.json').read_text(encoding='utf-8'))
audits=[]
for line in (db.parent/'tool-audit.jsonl').read_text(encoding='utf-8').splitlines():
    if any(x['tool_call_id'] in line for x in calls):audits.append(json.loads(line))
record={'version':'源码候选/正式117壳','backend':json.loads((p/'candidate-process.json').read_text(encoding='utf-8-sig')),'marker':marker,'run':dict(run),'calls':calls,'attempts':attempts,'bindings':bindings,'permission_expansion':mutation,'network_events':network,'tool_audit':audits,'event_types':events,'visible_reply':visible,'network_request_count':sum(x['event']=='network_request' for x in network)}
save('facts.json',record)
print(json.dumps({'run_state':run['state'],'calls':calls,'attempts':attempts,'network_request_count':record['network_request_count'],'visible_reply':visible},ensure_ascii=False))
