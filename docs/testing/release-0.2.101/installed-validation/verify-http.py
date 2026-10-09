from pathlib import Path
import json,sqlite3,urllib.request,time,sys
folder=Path(__file__).resolve().parent;root=folder.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
workspace='ws-23f646a969206cb4';room='room-1791131523339';session='session-1791131217833'
def get(path):
    before=time.monotonic()
    with urllib.request.urlopen('http://127.0.0.1:8765'+path,timeout=20) as response:data=json.load(response)
    return data,round((time.monotonic()-before)*1000,2)
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as connection:
    rows=connection.execute("SELECT state,protocol_stop,COUNT(*) FROM devin_acp_attempts WHERE json_extract(scope_json,'$.workspace_id')=? AND json_extract(scope_json,'$.room_id')=? AND json_extract(scope_json,'$.agent_id')=? GROUP BY state,protocol_stop",(workspace,room,session)).fetchall()
    http=connection.execute("SELECT COUNT(DISTINCT logical_request_id) FROM chat_usage_events u WHERE workspace_id=? AND room_id=? AND session_id=? AND NOT EXISTS(SELECT 1 FROM devin_acp_attempts a WHERE substr(u.attempt_id,1,4)='acp:' AND a.attempt_id=substr(u.attempt_id,5))",(workspace,room,session)).fetchone()[0]
    all_usage=connection.execute('SELECT COUNT(*) FROM chat_usage_events').fetchone()[0]
    scope_acp=sum(row[2] for row in rows)
    expected={'requests':scope_acp+http,'attempts':sum(n for state,stop,n in rows if state in ['submitted','terminal']),
      'pending_attempts':sum(n for state,stop,n in rows if state in ['prepared','submitted']),
      'not_sent_attempts':sum(n for state,stop,n in rows if state=='not_sent'),
      'unknown_outcome_attempts':sum(n for state,stop,n in rows if state=='unknown' or state=='terminal' and stop not in ['end_turn','cancelled','refusal','max_tokens','max_turn_requests']),
      'unknown_dispatch_attempts':sum(n for state,stop,n in rows if state=='unknown')}
insights,insight_ms=get(f'/api/chat/rooms/{room}/insights')
selected=next(row for row in insights['usage'] if row['session_id']==session)
for key,value in expected.items():assert selected[key]==value,(key,selected,expected)
assert selected['input_tokens'] is None and selected['output_tokens'] is None
trace,trace_ms=get(f'/api/chat/rooms/{room}/trace?limit=5')
previous=json.loads((root/'tmp/2026-10-08-release-100/integration-result.json').read_text(encoding='utf-8'))['records'][0]
run=next(run for run in trace['runs'] if run['run_id']==previous['run']['id'])
requests=run['requests'];assert len(requests)==1
request=requests[0]
assert request['attempt_id']=='acp:'+previous['attempts'][0]['attempt_id']
assert request['status']=='completed' and request['dispatched'] is True and request['process_drained'] is True
assert request['input_tokens'] is None and request['output_tokens'] is None and request['created_at'] is None
result={'stage':'0.2.101正式安装版','observed_ms':time.time()*1000,
  'expected_from_readonly_journal':expected,'scoped_states':rows,'raw_http_event_count':all_usage,
  'insights':insights,'trace_run':run,'insight_ms':insight_ms,'trace_ms':trace_ms}
if len(sys.argv)>1:
    before=json.loads((folder/'http-result.json').read_text(encoding='utf-8'))
    previous_selected=next(row for row in before['insights']['usage'] if row['session_id']==session)
    assert selected['requests']==previous_selected['requests']+1,(selected,previous_selected)
    current=json.loads((folder/'integration-result.json').read_text(encoding='utf-8'))['records'][0]
    current_run=next(run for run in trace['runs'] if run['run_id']==current['run']['id'])
    assert len(current_run['requests'])==1 and current_run['requests'][0]['attempt_id']=='acp:'+current['attempts'][0]['attempt_id']
    assert current_run['requests'][0]['status']=='completed' and current_run['requests'][0]['input_tokens'] is None
    result['incremental_trace_run']=current_run;result['request_increment']=1
(folder/('http-after-result.json' if len(sys.argv)>1 else 'http-result.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'selected':selected,'trace_request':request,'insight_ms':insight_ms,'trace_ms':trace_ms},ensure_ascii=False))
