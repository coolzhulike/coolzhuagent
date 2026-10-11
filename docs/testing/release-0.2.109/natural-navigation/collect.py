"""原样保留真实终态与阶段；未命中目标分支时明确保持开放。"""
from pathlib import Path
import json,sqlite3
folder=Path(__file__).resolve().parent;repo=folder.parents[1]
facts=json.loads((folder/'latest-status.json').read_text(encoding='utf-8'))
with sqlite3.connect((repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
    c.row_factory=sqlite3.Row
    source=c.execute("SELECT created_at,id FROM chat_room_messages WHERE room_id=? AND role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",('room-1791131523339','BU-OBSERVE-WAIT-109-20261008%')).fetchone()
    reply=c.execute("SELECT id,kind,content,created_at FROM chat_room_messages WHERE room_id=? AND created_at>=? AND kind='assistant-reply' ORDER BY created_at LIMIT 1",('room-1791131523339',source['created_at'])).fetchone()
    calls=[dict(r) for r in c.execute('SELECT tool_name,status,tool_call_id FROM tool_calls WHERE run_id=?',(facts['run']['id'],))]
    stop_events=[]
    for r in c.execute("SELECT id,event_type,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='browser.observation_stopped' ORDER BY id",(facts['run']['id'],)):
        event=dict(r);event['payload']=json.loads(event.pop('payload_json'));stop_events.append(event)
events=[json.loads(line) for line in (folder/'events.jsonl').read_text(encoding='utf-8').splitlines()]
new_inputs=[e for e in events if e.get('role')=='NEW' and e['kind'] in ('click','pointerdown','pointerup','input','keydown')]
cu=facts['computer_use'][0] if len(facts['computer_use'])==1 else None
terminal=cu.get('terminal_result_json') if cu else None
steps=(terminal or {}).get('input_steps') or []
phase=next((e for e in events if e['kind']=='observing-phase-found'),None)
old_up=next((e for e in events if e.get('role')=='OLD' and e['kind']=='pointerup'),None)
new_load=next((e for e in events if e.get('role')=='NEW' and e['kind']=='loaded'),None)
invariants={
    'single_tool_call':len(calls)==1 and calls[0]['tool_name']=='computer_use_perform',
    'single_model_attempt':len(facts['attempts'])==1,
    'end_turn_drained':len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1,
    'unique_binding_unlocked':sum(b['remote_session_id']=='island-kayak' for b in facts['bindings'])==1 and not any(b['locked_attempt'] for b in facts['bindings']),
    'single_input_sent_released':len(steps)==1 and steps[0]['input_delivery']=='sent' and steps[0]['input_release_status']=='released',
    'new_page_zero_old_input':bool(new_load) and not new_inputs,
    'actual_post_release_registered_observation':phase is not None and 'observation_requested' in phase['facts'],
}
requested_id=phase['facts']['observation_requested']['payload']['request_id'] if phase and 'observation_requested' in phase['facts'] else None
target=any(e['payload']['stage']=='waiting_resource_changed' and e['payload']['reason_code']=='native_browser_resource_changed' and e['payload']['request_id']==requested_id for e in stop_events)
passed=all(invariants.values()) and target and (terminal or {}).get('error',{}).get('code')=='native_browser_resource_changed'
result={'formal_version':'0.2.109','facts':facts,'calls':calls,'source_message_id':source['id'],'visible_reply':dict(reply) if reply else None,
    'observation_stop_events':stop_events,'observing_phase_event':phase,'invariants':invariants,
    'up_to_new_loaded_page_ms':new_load['page_ms']-old_up['page_ms'] if new_load and old_up else None,
    'waiting_resource_changed_branch_passed':passed,
    'strict_down_up_window':'not_tested; normal navigation deliberately after input release',
    'remaining':'严格newTarget/跨来源commit按下窗口及其它总体矩阵继续开放；若本轮未命中waiting_resource_changed，该子项也继续开放'}
(folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:result[k] for k in ('formal_version','invariants','observation_stop_events','up_to_new_loaded_page_ms','waiting_resource_changed_branch_passed','remaining')},ensure_ascii=False))
