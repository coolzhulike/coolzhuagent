"""只读实际运行与阶段，保留未知、不读取模型思考。"""
from pathlib import Path
import sqlite3,json
p=Path(__file__).resolve().parent
root=p.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 source=c.execute("select id from chat_room_messages where role='user' and content like 'BU-PENDING-CLOSE-123B-20261009%' order by created_at desc limit 1").fetchone()
 r=c.execute("select r.id,r.state,r.legacy_turn_id from runtime_runs r join runtime_run_events e on e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs where e.event_type='chat.source_messages' and refs.value=? limit 1",(source['id'],)).fetchone()
 result={'run':dict(r),'computer_use':[]}
 for cu in c.execute('select call_id,state,terminal_result_json from computer_use_runs where turn_id=?',(r['legacy_turn_id'],)):
  item=dict(cu);item['terminal_result_json']=json.loads(item['terminal_result_json']) if item['terminal_result_json'] else None
  item['steps']=[dict(x) for x in c.execute('select step_index,action_type,status,error_code,input_delivery,input_release_status,effect_status,completed_at_ms from computer_use_steps where run_id=? order by step_index',(cu['call_id'],))]
  result['computer_use'].append(item)
 result['events']=[dict(x) for x in c.execute("select id,event_type,created_at,payload_json from runtime_run_events where run_id=? and event_type in ('browser.observation_requested','browser.observation_stopped') order by id",(r['id'],))]
 result['bindings']=[dict(x) for x in c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=?',('session-1791131217833',))]
 result['attempts']=[dict(x) for x in c.execute("select attempt_id,state,protocol_stop,process_drained,model_json from devin_acp_attempts where json_extract(scope_json,'$.run_id')=?",(r['id'],))]
 (p/'latest-status.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps(result,ensure_ascii=False))
