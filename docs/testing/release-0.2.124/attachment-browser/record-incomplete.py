"""记录真实综合失败，同时独立核验已经完成的Browser/附件部分。"""
from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'));a=json.loads((p/'address.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 row=c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],)).fetchone()
 snapshots=json.loads(c.execute('SELECT attachments_json FROM chat_room_messages WHERE id=?',(facts['source_user_id'],)).fetchone()[0])
t=json.loads(row['terminal_result_json']);steps=t['input_steps']
assert t['status']=='succeeded' and t['goal_achieved'] and len(steps)==8
assert all(x['input_delivery']=='sent' and x['input_release_status'] in ('released','not_needed') for x in steps)
assert sum(x['action_kind']=='scroll' and x['effect_status']=='effect_observed' for x in steps)==2
assert sum(x['action_kind']=='text_input' and x['effect_status']=='effect_observed' for x in steps)==2
events=[json.loads(x) for x in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
events=[x for x in events if x['observed_ms']>=facts['run']['created_at']]
for order in a['orders']:
 local=[x for x in events if x.get('item')==order['item']]
 assert any(x['kind']=='wheel' and x['trusted'] for x in local)
 assert any(x['kind']=='document-scroll' and x['trusted'] and x['page_y']>0 for x in local)
 assert any(x['kind']=='ax-prefix-inserted' for x in local)
 assert len([x for x in local if x['kind']=='input' and x['trusted'] and x.get('value')==order['code']])==1
 assert len([x for x in local if x['kind']=='verified' and x['trusted'] and x['accepted']])==1
submitted=json.loads((p/'submitted.json').read_text(encoding='utf-8'))
for snapshot,original in zip(snapshots,submitted['files']):
 assert snapshot['text_snapshot']['sha256']==original['sha256'] and snapshot['text_snapshot']['encoding']==original['encoding']
 assert any(order['code'] in snapshot['text_snapshot']['text'] for order in a['orders'])
assert facts['run']['state']=='completed' and len(facts['calls'])==1 and facts['calls'][0]['tool_name']=='computer_use_perform'
assert facts['calls'][0]['status']=='completed' and len(facts['attempts'])==1
assert facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert sum(x['remote_session_id']=='island-kayak' for x in facts['bindings'])==1 and all(x['locked_attempt'] is None for x in facts['bindings'])
record={'stage':facts['stage'],'run_id':facts['run']['id'],'cu_run':dict(row),'terminal':t,'page_events':events,'bindings':facts['bindings'],'independent_total':a['total'],'passed':False,'browser_passed':True,'attachments_passed':True,'reason':'原任务要求DSH计算器，但模型终态没有调用；最终回复混入西班牙语且未报告两文件编码。根协议completed不证明用户综合任务完成。'}
(p/'verification.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'business_passed':False,'browser_passed':True,'attachments_passed':True,'actions':len(steps),'tools':1,'reason':record['reason']},ensure_ascii=False))
