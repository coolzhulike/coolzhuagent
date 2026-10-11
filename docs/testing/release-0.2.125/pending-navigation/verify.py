"""只按真实时间、在途关联事件与原输入事实判定，不把晚导航算通过。"""
from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
f=json.loads((p/'facts.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
    c.row_factory=sqlite3.Row
    runs=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(f['run']['legacy_turn_id'],))]
    events=[dict(x) for x in c.execute("SELECT id,event_type,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND event_type IN ('browser.observation_requested','browser.observation_stopped') ORDER BY id",(f['run']['id'],))]
assert len(runs)==1
terminal=json.loads(runs[0]['terminal_result_json']);steps=terminal['input_steps']
assert len(steps)==1 and steps[0]['action_kind']=='click'
assert steps[0]['input_delivery']=='sent' and steps[0]['input_release_status']=='released'
page=[json.loads(line) for line in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
phase=[x for x in page if x['kind']=='observing-phase-found' and x['facts']['run_id']==f['run']['id']]
assert phase,'必须实际捕获释放后下一观察请求'
request=phase[0]['facts']['observation_requested'];req_id=request['payload']['request_id']
stops=[dict(x,payload=json.loads(x['payload_json'])) for x in events if x['event_type']=='browser.observation_stopped' and json.loads(x['payload_json'])['request_id']==req_id]
popup=[x for x in page if x['kind']=='popup-request']
assert len(popup)==1 and len(stops)==1
stop=stops[0]
assert stop['payload']['reason_code']=='native_browser_resource_changed'
assert stop['payload']['stage'] in ('waiting_resource_changed','reply_resource_changed','host_reply')
assert request['created_at']<=popup[0]['received_unix_ns']/1e6<=stop['created_at']+1
assert stop['payload']['elapsed_ms']<5000
assert terminal['status']!='succeeded' and not terminal['goal_achieved']
trusted=[x for x in page if x['kind'] in ('pointerdown','pointerup','click','input','keydown') and x.get('trusted')]
assert sum(x['kind']=='click' and x.get('role')=='OLD' for x in trusted)==1
assert not any(x.get('role')=='NEW' for x in trusted)
assert any(x['kind']=='http-get' and x.get('role')=='NEW' for x in page)
assert len(f['calls'])==1 and f['calls'][0]['tool_name']=='computer_use_perform'
assert f['run']['state'] in ('completed','failed') and len(f['attempts'])==1
assert f['attempts'][0]['protocol_stop']=='end_turn' and f['attempts'][0]['process_drained']==1
assert sum(x['remote_session_id']=='island-kayak' for x in f['bindings'])==1 and all(x['locked_attempt'] is None for x in f['bindings'])
result={'passed':True,'stage':f['stage'],'run_id':f['run']['id'],'terminal':terminal,'cu_run':runs[0],'associated_request':request,'observation_stop':stop,'popup_event':popup[0],'page_events':page,'trusted_input_events':trusted,'bindings':f['bindings'],'note':'仅该确切阶段与普通新窗口导航竞态，不冒严格HRESULT竞争或全矩阵完成'}
(p/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'passed':True,'stop_stage':stop['payload']['stage'],'elapsed_ms':stop['payload']['elapsed_ms'],'input_delivery':steps[0]['input_delivery'],'release':steps[0]['input_release_status'],'effect':steps[0]['effect_status']},ensure_ascii=False))
