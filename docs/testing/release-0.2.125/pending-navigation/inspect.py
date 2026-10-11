from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
f=json.loads((p/'facts.json').read_text(encoding='utf-8'))
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 runs=[dict(x) for x in c.execute('select * from computer_use_runs where turn_id=?',(f['run']['legacy_turn_id'],))]
 events=[dict(x) for x in c.execute("select id,event_type,created_at,payload_json from runtime_run_events where run_id=? and event_type like 'browser.%' order by id",(f['run']['id'],))]
 diagnostics=[dict(x) for x in c.execute('select request_kind,response_json,error_code from computer_use_planner_diagnostics where call_id=? order by id',(runs[0]['call_id'],))]
result={'run':f['run'],'terminal':json.loads(runs[0]['terminal_result_json']),'events':events,'diagnostics':diagnostics}
(p/'inspection-facts.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'run':f['run']['id'],'terminal':result['terminal'],'events':events,'diagnostic_structure':[{'kind':d['request_kind'],'error':d['error_code'],'keys':list(json.loads(d['response_json'] or '{}'))} for d in diagnostics]},ensure_ascii=False))
