from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2];facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row;rows=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],))]
assert len(rows)==1
terminal=json.loads(rows[0]['terminal_result_json']);steps=terminal['input_steps']
assert terminal['status']=='blocked' and not terminal['goal_achieved'] and terminal['error']['code']=='no_progress'
assert len(steps)==2 and terminal['supervisor']['no_progress_count']==2
assert all(x['action_kind']=='click' for x in steps)
assert all(x['input_delivery']=='sent' and x['input_release_status']=='released' and x['effect_status']=='inconclusive' for x in steps)
events=[json.loads(line) for line in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
events=[x for x in events if x['observed_ms']>=facts['run']['created_at']]
clicks=[x for x in events if x['kind']=='click'];assert len(clicks)==2 and all(x['trusted'] and x['target']=='try' for x in clicks)
assert any(x['kind']=='non-target-repaint' for x in events)
assert len(facts['calls'])==1 and facts['calls'][0]['tool_name']=='computer_use_perform'
assert facts['run']['state']=='failed'
assert len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert sum(x['remote_session_id']=='island-kayak' for x in facts['bindings'])==1 and not any(x['locked_attempt'] for x in facts['bindings'])
record={'passed':True,'expected_negative':True,'cu_run':rows[0],'terminal':terminal,'page_events':events,'bindings':facts['bindings']}
(p/'verification.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'passed':True,'actions':2,'no_progress_count':2,'stop':'no_progress','trusted_clicks':2,'goal_achieved':False},ensure_ascii=False))
