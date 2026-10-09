from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2];facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 rows=[dict(x) for x in c.execute('SELECT s.step_index,s.action_type,s.status,s.visible_progress,s.input_delivery,s.input_release_status,s.effect_status,s.goal_verdict FROM computer_use_steps s JOIN computer_use_runs r ON r.call_id=s.run_id WHERE r.turn_id=? ORDER BY s.step_index',(facts['run']['legacy_turn_id'],))]
print(json.dumps(rows,ensure_ascii=False))
