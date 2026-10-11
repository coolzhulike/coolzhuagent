from pathlib import Path
import json,sqlite3,sys
root=Path(__file__).resolve().parents[2]
p=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else Path(__file__).resolve().parent
candidate=p.name.endswith('-candidate')
f=json.loads((p/'facts.json').read_text());trigger=json.loads((p/'planning-trigger.json').read_text())
page=[json.loads(x) for x in (p/'events.jsonl').read_text().splitlines()]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 runs=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(f['run']['legacy_turn_id'],))]
 steps=[dict(x) for x in c.execute('SELECT * FROM computer_use_steps WHERE run_id=?',(runs[0]['call_id'],))]
 events=[dict(x) for x in c.execute('SELECT id,event_type,created_at,payload_json FROM runtime_run_events WHERE run_id=? ORDER BY id',(f['run']['id'],))]
record={'passed':False,'candidate':candidate,'facts':f,'cu_runs':runs,'steps':steps,'events':events,'page_events':page,'trigger':trigger}
(p/'inspection.json').write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
assert len(runs)==1 and len(steps)==1
cu=runs[0];step=steps[0];terminal=json.loads(cu['terminal_result_json'])
assert trigger['parent_run']==f['run']['id'] and trigger['cu_call_id']==cu['call_id']
assert trigger['state']=='planning' and trigger['action_count']==0
new=[x for x in page if x['kind']=='loaded' and x.get('role')=='NEW'];assert len(new)==1
old_exit=[x for x in page if x['kind']=='pagehide' and x.get('role')=='OLD'];assert len(old_exit)==1
assert trigger['captured_ms']<=old_exit[0]['page_ms']<=new[0]['page_ms']<step['started_at_ms']
response=[x for x in events if x['event_type']=='devin.planning_response'];assert len(response)==1
assert new[0]['page_ms']<response[0]['created_at']<=step['started_at_ms']
trusted=[x for x in page if x.get('trusted') and x['kind'] in ('pointerdown','pointerup','click','input','keydown')]
assert not trusted, trusted
assert step['action_type']=='click' and step['error_code']=='stale_observation'
assert step['input_delivery']=='not_sent' and step['input_release_status']=='not_needed'
assert terminal['goal_achieved'] is False and terminal['steps_completed']==0
assert terminal['input_steps'][0]['input_delivery']=='not_sent'
assert f['run']['state']=='failed' and len(f['calls'])==1 and f['calls'][0]['tool_name']=='computer_use_perform'
assert len(f['attempts'])==1 and f['attempts'][0]['protocol_stop']=='end_turn' and f['attempts'][0]['process_drained']==1
assert sum(b['remote_session_id']=='island-kayak' for b in f['bindings'])==1 and all(b['locked_attempt'] is None for b in f['bindings'])
submitted=json.loads((p/'submitted.json').read_text());assert submitted['model']=='swe-2-medium' and submitted['config_revision']==57
stream=json.loads((p/'stream-finished.json').read_text());assert stream['event_counts']['done']==1
requests=[x for x in events if x['event_type']=='browser.observation_requested']
if candidate:
 assert terminal['error']['code']=='stale_observation' and terminal['stage']=='execution'
 assert terminal['error']['retryable'] is False and terminal['error']['retry_owner']=='none'
 assert 'browser identity changed before input' in terminal['error']['message']
 assert terminal['supervisor']['reason']=='budget_exhausted' and terminal['supervisor']['replan_count']==0
 assert len(requests)==2,'无恢复预算，不再额外观察'
else:
 assert terminal['error']['code']=='budget_exhausted' and len(requests)==3
for name in ('native-initial.png','native-terminal.png'):assert (p/name).stat().st_size>10000
record.update(passed=True,terminal=terminal,guard_passed=True,terminal_reason_preserved=candidate,
 planning_to_loaded_ms=round(new[0]['page_ms']-trigger['captured_ms'],3),
 loaded_to_input_preflight_ms=round(step['started_at_ms']-new[0]['page_ms'],3),
 no_real_input=True,extra_recovery_observation_skipped=candidate,
 limits=['仅规划期间文档切换后的旧引用预检，未命中原生派发commit、按下/抬起期间撤销、UI关闭或HRESULT竞争',
 '源码候选不是新正式包验收' if candidate else '正式126终态原因丢失是已确认缺陷，不能计报告语义通过'])
(p/'verification.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:record[k] for k in ('passed','candidate','terminal_reason_preserved','planning_to_loaded_ms','loaded_to_input_preflight_ms','extra_recovery_observation_skipped')},ensure_ascii=False))
