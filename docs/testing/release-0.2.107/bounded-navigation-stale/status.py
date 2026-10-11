"""只读真实台账；不导出模型思考。"""
import sqlite3,json,pathlib
folder=pathlib.Path(__file__).resolve().parent
root=folder.parents[1]
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 source=c.execute("select id from chat_room_messages where role='user' and content like 'BU-RESOURCE-107-CHURN-20261008%' order by created_at desc limit 1").fetchone()
 if not source:print('未提交');raise SystemExit
 r=c.execute("select r.id,r.state,r.legacy_turn_id from runtime_runs r join runtime_run_events e on e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs where e.event_type='chat.source_messages' and refs.value=? limit 1",(source['id'],)).fetchone()
 if not r:print('消息已保存，运行未登记');raise SystemExit
 result={'run':dict(r),'computer_use':[]}
 for cu in c.execute('select call_id,state,terminal_result_json from computer_use_runs where turn_id=?',(r['legacy_turn_id'],)):
  item=dict(cu);item['terminal_result_json']=json.loads(item['terminal_result_json']) if item['terminal_result_json'] else None
  item['steps']=[dict(x) for x in c.execute('select step_index,action_type,status,error_code,input_delivery,input_release_status,effect_status from computer_use_steps where run_id=? order by step_index',(cu['call_id'],))]
  result['computer_use'].append(item)
 result['bindings']=[dict(x) for x in c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=?',('session-1791131217833',))]
 result['attempts']=[dict(x) for x in c.execute("select attempt_id,state,protocol_stop,process_drained,model_json from devin_acp_attempts where json_extract(scope_json,'$.run_id')=?",(r['id'],))]
 (folder/'latest-status.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps(result,ensure_ascii=False))
