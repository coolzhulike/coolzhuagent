"""仅投影宿主执行事实与观察计数；不读取模型思考正文。"""
from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
f=json.loads((p/'facts.json').read_text(encoding='utf-8'))
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 run=c.execute('SELECT call_id,state,action_count,replan_count,no_progress_count FROM computer_use_runs WHERE turn_id=?',(f['run']['legacy_turn_id'],)).fetchone()
 if not run:
  print(json.dumps({'state':f['run']['state'],'cu_started':False}));raise SystemExit(0)
 steps=[dict(x) for x in c.execute('SELECT step_index,action_type,status,error_code,input_delivery,effect_status,input_release_status FROM computer_use_steps WHERE run_id=? ORDER BY step_index',(run['call_id'],))]
 observations=[dict(x) for x in c.execute("SELECT request_kind,observation_generation,json_extract(response_json,'$.observation_facts') facts FROM computer_use_planner_diagnostics WHERE call_id=? ORDER BY id",(run['call_id'],))]
 pages=[dict(x) for x in c.execute("SELECT event_type,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='tool.result_page_read' ORDER BY id",(f['run']['id'],))]
record={'run':dict(run),'steps':steps,'observation_counts':observations,'page_reads':pages}
(p/'progress-facts.json').write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'state':run['state'],'steps':steps,'last_observation_counts':[{'kind':x['request_kind'],'generation':x['observation_generation'],'visible':json.loads(x['facts'] or '{}').get('in_viewport_candidate_count'),'outside':json.loads(x['facts'] or '{}').get('outside_viewport_candidate_count')} for x in observations[-3:]],'page_reads':pages},ensure_ascii=False))
