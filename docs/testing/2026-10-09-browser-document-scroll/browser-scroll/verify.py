from pathlib import Path
import json,sqlite3,re
p=Path(__file__).resolve().parent;root=p.parents[2]
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'));address=json.loads((p/'address.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 rows=[dict(x) for x in c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],))]
assert len(rows)==1
terminal=json.loads(rows[0]['terminal_result_json']);assert terminal['status']=='succeeded' and terminal['goal_achieved']
steps=terminal['input_steps'];assert 8<=len(steps)<=14 and terminal['attempts']==len(steps) and terminal['steps_completed']==len(steps)
scrolls=[x for x in steps if x['action_kind']=='scroll']
assert len(scrolls)>=2 and all(x['effect_status']=='effect_observed' for x in scrolls),'真实目标文档滚动必须具有独立效果'
assert all(x['input_delivery']=='sent' and x['input_release_status'] in ('released','not_needed') for x in steps)
assert sum(x['action_kind']=='text_input' and x['effect_status']=='effect_observed' for x in steps)==2
assert any(x['action_kind']=='key_combination' and x['effect_status']=='inconclusive' for x in steps)
events=[json.loads(x) for x in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
events=[x for x in events if x['observed_ms']>=facts['run']['created_at']]
for order in address['orders']:
 item=order['item'];local=[x for x in events if x.get('item')==item]
 assert any(x['kind']=='wheel' and x['trusted'] for x in local)
 assert any(x['kind']=='document-scroll' and x['trusted'] and x['page_y']>0 for x in local)
 assert any(x['kind']=='ax-prefix-inserted' for x in local)
 entered=[x for x in local if x['kind']=='input' and x['trusted'] and x.get('value')==order['code']];assert len(entered)==1
 assert any(x['kind']=='verified' and x['trusted'] and x['accepted'] for x in local)
 assert min(x['observed_ms'] for x in local if x['kind']=='document-scroll' and x['page_y']>0)<entered[0]['observed_ms']
assert any(x['kind']=='non-target-repaint' for x in events)
assert all(x['trusted'] for x in events if x['kind'] in ('pointerdown','pointerup','click','keydown','input','wheel'))
assert facts['run']['state']=='completed' and len(facts['calls'])==2 and all(x['status']=='completed' for x in facts['calls'])
assert [x['tool_name'] for x in sorted(facts['calls'],key=lambda c:c['created_at_unix_ms'])]==['computer_use_perform','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab']
assert len(facts['attempts'])==1 and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert sum(x['remote_session_id']=='island-kayak' for x in facts['bindings'])==1 and all(x['locked_attempt'] is None for x in facts['bindings'])
reply='\n'.join(x['content'] for x in facts['visible_reply'])
assert re.search(r'(?<!\d)'+str(address['total'])+r'(?!\d)',reply)
assert all(str(o['price']) in reply and str(o['quantity']) in reply for o in address['orders'])
record={'stage':facts['stage'],'run_id':facts['run']['id'],'cu_run':rows[0],'terminal':terminal,'page_events':events,'bindings':facts['bindings'],'independent_total':address['total'],'passed':True}
(p/'verification.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'passed':True,'actions':len(steps),'scrolls':len(scrolls),'trusted_events':sum(x.get('trusted',False) for x in events),'tools':2,'total':address['total'],'run_id':facts['run']['id']},ensure_ascii=False))
