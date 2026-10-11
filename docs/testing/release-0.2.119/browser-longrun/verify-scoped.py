from pathlib import Path
import sqlite3,json
p=Path(__file__).resolve().parent;root=p.parents[2];facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 rows=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],))]
assert len(rows)==1
terminal=json.loads(rows[0]['terminal_result_json']);assert terminal['status']=='succeeded' and terminal['goal_achieved']
assert 8 <= len(terminal['input_steps']) <= 20
assert terminal['attempts']==terminal['steps_completed']==len(terminal['input_steps'])
assert all(x['input_delivery']=='sent' and x['effect_status']=='effect_observed' for x in terminal['input_steps'])
events=[json.loads(x) for x in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
events=[x for x in events if x['observed_ms']>=facts['run']['created_at']]
assert any(x['kind']=='verified' and x['accepted'] for x in events)
assert any(x['kind']=='navigate-requested' for x in events)
assert any(x['kind']=='final-submitted' and x['accepted'] for x in events)
assert not any(str(x.get('target','')).startswith('decoy') for x in events)
assert all(x['trusted'] for x in events if x['kind'] in ('pointerdown','pointerup','click','keydown','input','wheel'))
assert facts['run']['state']=='completed' and len(facts['calls'])==1 and facts['calls'][0]['status']=='completed'
assert len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert all(x['locked_attempt'] is None for x in facts['bindings'])
record={'stage':facts['stage'],'run_id':facts['run']['id'],'cu_run':rows[0],'terminal':terminal,'trusted_events':events,'cloud_bindings':facts['bindings'],'passed':True}
(p/'verification.json').write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'passed':True,'actions':len(terminal['input_steps']),'events':len(events),'cu_budget':terminal.get('cu_budget'),'run_id':facts['run']['id']},ensure_ascii=False))
