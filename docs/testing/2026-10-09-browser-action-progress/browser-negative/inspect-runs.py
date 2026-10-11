from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 for row in c.execute('SELECT state,terminal_result_json FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],)):
  terminal=json.loads(row['terminal_result_json']) if row['terminal_result_json'] else {}
  print(json.dumps({'state':row['state'],'status':terminal.get('status'),'error':(terminal.get('error') or {}).get('code'),'message':str((terminal.get('error') or {}).get('message',''))[:240],'attempts':terminal.get('attempts'),'steps':terminal.get('input_steps'),'supervisor':terminal.get('supervisor')},ensure_ascii=False))
print(json.dumps({'call_fields':list(facts['calls'][0])},ensure_ascii=False))
print(json.dumps([{'tool':x['tool_name'],'status':x['status']} for x in facts['calls']],ensure_ascii=False))
