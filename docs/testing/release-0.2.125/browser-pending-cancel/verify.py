from pathlib import Path
import sqlite3,json
root=Path.cwd();p=root/'tmp/2026-10-10-browser-pending-cancel'
f=json.loads((p/'facts.json').read_text());action=json.loads((p/'cancel-action.json').read_text())
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 runs=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(f['run']['legacy_turn_id'],))]
 events=[dict(x,payload=json.loads(x['payload_json'])) for x in c.execute('SELECT id,event_type,created_at,payload_json FROM runtime_run_events WHERE run_id=? ORDER BY id',(f['run']['id'],))]
 steps=[dict(x) for x in c.execute('SELECT * FROM computer_use_steps WHERE run_id=? ORDER BY step_index',(action['cu_call_id'],))]
assert len(runs)==1
terminal=json.loads(runs[0]['terminal_result_json']);rid=action['request']['payload']['request_id']
stops=[e for e in events if e['event_type']=='browser.observation_stopped' and e['payload']['request_id']==rid]
result={'passed':False,'run':f['run'],'terminal':terminal,'cu_run':runs[0],'steps':steps,'events':events,'associated_request':action['request'],'cancel_action':action,'associated_stops':stops,'attempts':f['attempts'],'bindings':f['bindings']}
(p/'inspection.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
assert action['attempted'] and action['http_status']==200 and action['response']['outcome']=='interrupt_requested'
assert action['submitted_ms']>=action['request']['created_at'] and action['submitted_ms']-action['request']['created_at']<5000
assert len(stops)==1, '必须有同一个在途请求的停止事件'
stop=stops[0]
assert stop['payload']['reason_code']=='native_observation_cancelled'
# 正常停止CAS先完成父运行时，子观察回包后会在父作用域校验处识别取消。
# 仍要求同请求、真实取消码、停止时序与零补发，不能把其它父失配算作取消通过。
assert stop['payload']['stage'] in ('waiting_cancelled','reply_cancelled','reply_parent_changed')
assert action['request']['created_at']<=f['run']['stop_requested_at']<=stop['created_at']
assert stop['payload']['elapsed_ms']<5000
assert len(steps)==1 and steps[0]['input_delivery']=='sent' and steps[0]['input_release_status']=='released'
assert terminal['status']!='succeeded' and not terminal['goal_achieved']
assert terminal['error']['code']=='native_observation_cancelled'
assert terminal['input_steps'][0]['input_delivery']=='sent' and terminal['input_steps'][0]['input_release_status']=='released'
page=[json.loads(l) for l in (p/'events.jsonl').read_text().splitlines()]
trusted=[e for e in page if e.get('trusted') and e['kind'] in ('pointerdown','pointerup','click','input','keydown')]
assert sum(e['kind']=='click' and e.get('target')=='target' for e in trusted)==1
assert sum(e['kind']=='pointerdown' for e in trusted)==1 and sum(e['kind']=='pointerup' for e in trusted)==1
assert len(f['calls'])==1 and f['calls'][0]['tool_name']=='computer_use_perform'
assert f['run']['state']=='interrupted' and len(f['attempts'])==1
assert f['attempts'][0]['protocol_stop']=='cancelled' and f['attempts'][0]['process_drained']==1
assert sum(b['remote_session_id']=='island-kayak' for b in f['bindings'])==1 and all(b['locked_attempt'] is None for b in f['bindings'])
stream=json.loads((p/'stream-finished.json').read_text());assert stream['event_counts']['done']==1
result.update(passed=True,strict_pending_cancel=True,page_events=page,trusted_input_events=trusted,
 reply_content_absent=True,model='swe-2-medium',cloud_session='island-kayak',
 limits=['只计已释放输入后观察在途取消，不计正常UI关闭、导航撤销、HRESULT竞争或按下期间撤销'])
(p/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'passed':True,'stage':stop['payload']['stage'],'elapsed_ms':stop['payload']['elapsed_ms'],
 'after_request_ms':round(action['submitted_ms']-action['request']['created_at'],3),'input':'sent/released','root':'interrupted','protocol_stop':'cancelled','drained':True},ensure_ascii=False))
