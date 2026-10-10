from pathlib import Path
import sqlite3,json
p=Path(__file__).resolve().parent;root=p.parents[2]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 run=c.execute('SELECT state FROM runtime_runs WHERE id=?',(facts['run']['id'],)).fetchone()
 cu=[dict(x) for x in c.execute('SELECT state,terminal_result_json FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],))]
 page=[json.loads(x) for x in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
 trusted=[x for x in page if x.get('trusted')]
 print(json.dumps({'run':dict(run),'computer_use':[{k:v for k,v in x.items() if k!='terminal_result_json'} for x in cu],
  'terminal_present':any(x['terminal_result_json'] for x in cu),'trusted_events':len(trusted),'latest_trusted_kind':trusted[-1]['kind'] if trusted else None},ensure_ascii=False))
