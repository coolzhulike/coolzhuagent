"""归档正常GUI撤销的实际请求阶段，不追认为在途等待命中。"""
from pathlib import Path
import json,sqlite3
folder=Path(__file__).resolve().parent
repo=folder.parents[2]
facts=json.loads((folder/'latest-status.json').read_text(encoding='utf-8'))
with sqlite3.connect((repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
    c.row_factory=sqlite3.Row
    source=c.execute("SELECT id,created_at FROM chat_room_messages WHERE role='user' AND content LIKE 'BU-RESOURCE-CLOSE-109-20261008%' ORDER BY created_at DESC LIMIT 1").fetchone()
    reply=c.execute("SELECT id,kind,content,created_at FROM chat_room_messages WHERE room_id=? AND created_at>=? AND kind='assistant-reply' ORDER BY created_at LIMIT 1",('room-1791131523339',source['created_at'])).fetchone()
    events=[]
    for r in c.execute("SELECT id,event_type,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND event_type IN ('browser.observation_requested','browser.observation_stopped') ORDER BY id",(facts['run']['id'],)):
        e=dict(r);e['payload']=json.loads(e.pop('payload_json'));events.append(e)
    calls=[dict(r) for r in c.execute('SELECT tool_name,status,tool_call_id FROM tool_calls WHERE run_id=?',(facts['run']['id'],))]
gate=json.loads((folder/'close-gate.json').read_text(encoding='utf-8'))
target_id=gate['facts']['observation_requested']['payload']['request_id']
stop=[e for e in events if e['event_type']=='browser.observation_stopped']
target=any(e['payload']['request_id']==target_id and e['payload']['stage']=='waiting_resource_changed' for e in stop)
terminal=facts['computer_use'][0]['terminal_result_json']
steps=terminal.get('input_steps') or []
invariants={
 'single_tool':len(calls)==1 and calls[0]['tool_name']=='computer_use_perform',
 'single_attempt':len(facts['attempts'])==1,
 'end_turn_drained':len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1,
 'unique_binding_unlocked':sum(b['remote_session_id']=='island-kayak' for b in facts['bindings'])==1 and not any(b['locked_attempt'] for b in facts['bindings']),
 'single_input_sent_released':len(steps)==1 and steps[0]['input_delivery']=='sent' and steps[0]['input_release_status']=='released',
 'stopped_no_replay':terminal['status']=='blocked' and terminal['attempts']==1,
}
result={'formal_version':'0.2.109','facts':facts,'source_message_id':source['id'],'visible_reply':dict(reply) if reply else None,
 'calls':calls,'observation_events':events,'close_gate':gate,'close_action':json.loads((folder/'close-action.json').read_text()),
 'invariants':invariants,'waiting_resource_changed_branch_passed':target and all(invariants.values()),
 'remaining':'GUI正常资源撤销与精确在途等待不同；只依据原事件判定。严格按下窗口和其它矩阵仍开放。'}
(folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:result[k] for k in ('invariants','observation_events','close_action','waiting_resource_changed_branch_passed')},ensure_ascii=False))
