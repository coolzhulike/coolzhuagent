from pathlib import Path
import sqlite3,json
p=Path(__file__).resolve().parent;root=p.parents[2]
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 rows=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],))]
assert len(rows)==1
terminal=json.loads(rows[0]['terminal_result_json'])
assert terminal['status']=='succeeded' and terminal['goal_achieved']
steps=terminal['input_steps'];assert 11<=len(steps)<=16 and terminal['attempts']==len(steps) and terminal['steps_completed']==len(steps)
assert all(x['input_delivery']=='sent' and x['effect_status']=='effect_observed' for x in steps)
assert all(x['input_release_status'] in ('released','not_needed') for x in steps)
events=[json.loads(x) for x in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
events=[x for x in events if x['observed_ms']>=facts['run']['created_at']]
codes=json.loads((p/'address.json').read_text(encoding='utf-8'))['codes']
for stage,code in enumerate(codes):
 assert any(x['kind']=='input' and x['trusted'] and x['stage']==stage and x.get('value')==code for x in events)
 assert any(x['kind']=='verified' and x['trusted'] and x['stage']==stage and x['accepted'] for x in events)
assert len([x for x in events if x['kind']=='advance'])==2
assert any(x['kind']=='final-submitted' and x['accepted'] and x['trusted'] for x in events)
assert any(x['kind']=='non-target-repaint' for x in events)
assert all(x['trusted'] for x in events if x['kind'] in ('pointerdown','pointerup','click','keydown','input'))
assert facts['run']['state']=='completed' and len(facts['calls'])==1 and facts['calls'][0]['status']=='completed'
assert len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert sum(x['remote_session_id']=='island-kayak' for x in facts['bindings'])==1 and all(x['locked_attempt'] is None for x in facts['bindings'])
record={'stage':facts['stage'],'run_id':facts['run']['id'],'cu_run':rows[0],'terminal':terminal,'page_events':events,'bindings':facts['bindings'],'passed':True}
(p/'verification.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'passed':True,'actions':len(steps),'trusted_events':sum(x['trusted'] for x in events),'run_id':facts['run']['id']},ensure_ascii=False))
