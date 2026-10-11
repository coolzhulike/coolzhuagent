from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
f=json.loads((p/'facts.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 row=c.execute('SELECT * FROM computer_use_runs WHERE turn_id=?',(f['run']['legacy_turn_id'],)).fetchone()
 pages=[dict(x) for x in c.execute("SELECT event_type,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='tool.result_page_read'",(f['run']['id'],))]
v={'passed':False,'reason':'真实正常订单在滚动后点击的原生命中预检拒绝；不是预期负例，也不移除检查。整轮未通过。','run_id':f['run']['id'],'cu_run':dict(row),'terminal':json.loads(row['terminal_result_json']),'page_reads':pages}
assert len(f['calls'])==1 and f['calls'][0]['status']=='failed'
assert f['attempts'][0]['process_drained']==1 and all(x['locked_attempt'] is None for x in f['bindings'])
(p/'verification.json').write_text(json.dumps(v,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'status':v['terminal']['status'],'error':v['terminal'].get('error'),'steps':v['terminal'].get('input_steps'),'page_reads':pages},ensure_ascii=False,indent=2))
