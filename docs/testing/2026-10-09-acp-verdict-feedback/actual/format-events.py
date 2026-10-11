"""只记录宿主格式反馈事件及脱敏结构；不读取思考或原模型正文。"""
from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2];f=json.loads((p/'facts.json').read_text(encoding='utf-8'))
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 events=[dict(x) for x in c.execute("select event_type,payload_json from runtime_run_events where run_id=? and event_type in ('devin.planning_format_rejected','tool.result_page_read') order by id",(f['run']['id'],))]
 diagnostics=[dict(x) for x in c.execute("select d.request_kind,d.error_code,d.response_json from computer_use_planner_diagnostics d join computer_use_runs r on r.call_id=d.call_id where r.turn_id=? and d.error_code is not null order by d.id",(f['run']['legacy_turn_id'],))]
record={'events':events,'redacted_error_diagnostics':diagnostics}
(p/'format-events.json').write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'rejected_format_count':sum(x['event_type']=='devin.planning_format_rejected' for x in events),'result_page_read_count':sum(x['event_type']=='tool.result_page_read' for x in events),'error_codes':[x['error_code'] for x in diagnostics]},ensure_ascii=False))
