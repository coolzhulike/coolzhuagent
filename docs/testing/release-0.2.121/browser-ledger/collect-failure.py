from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
    c.row_factory=sqlite3.Row
    rows=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],))]
assert len(rows)==1
terminal=json.loads(rows[0]['terminal_result_json'])
assert terminal['status']=='blocked' and not terminal['goal_achieved'] and terminal['error']['code']=='verification_failed'
assert len(terminal['input_steps'])==11 and all(x['input_delivery']=='sent' and x['effect_status']=='effect_observed' and x['input_release_status'] in ('released','not_needed') for x in terminal['input_steps'])
events=[json.loads(x) for x in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
assert len([x for x in events if x.get('trusted')])==27
assert any(x['kind']=='final-submitted' and x.get('accepted') and x.get('trusted') for x in events)
assert facts['run']['state']=='failed' and len(facts['calls'])==1 and facts['calls'][0]['status']=='failed'
assert len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert all(x['locked_attempt'] is None for x in facts['bindings'])
record={'passed':False,'page_completed':True,'calculator_called':False,'run':facts['run'],'cu_run':rows[0],'terminal':terminal,'page_events':events,'host_verification':json.loads((p/'last-host-verification.json').read_text(encoding='utf-8'))}
(p/'original-failure.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'passed':False,'actions':11,'trusted_events':27,'root':facts['run']['state'],'error':terminal['error']['code'],'calculator_called':False,'visible_reply_ids':[x['id'] for x in facts['visible_reply']]},ensure_ascii=False))
