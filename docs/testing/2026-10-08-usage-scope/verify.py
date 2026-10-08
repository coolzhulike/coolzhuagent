"""只读核验现有真实SWE请求台账；不写库、不生成答案或模型请求。"""
import pathlib, sqlite3, urllib.request, json, time
folder=pathlib.Path(__file__).resolve().parent
root=next(parent for parent in folder.parents if (parent/'Cargo.toml').is_file())
output=root/'tmp/2026-10-08-usage-scope'
output.mkdir(parents=True,exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
room='room-1791131523339'
workspace='ws-23f646a969206cb4'
def get(route):
    with urllib.request.urlopen('http://127.0.0.1:8765'+route,timeout=15) as response:
        return json.load(response)
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as connection:
    connection.row_factory=sqlite3.Row
    rows=[dict(row) for row in connection.execute('select logical_request_id,attempt_id,dispatched,status,usage_known_mask,input_tokens,output_tokens from chat_usage_events where room_id=? and workspace_id=?',(room,workspace))]
    latest=connection.execute("select id from chat_room_messages where room_id=? and kind='assistant-reply' order by created_at desc,id desc limit 1",(room,)).fetchone()[0]
    bindings=[dict(x) for x in connection.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=?',('session-1791131217833',))]
    counts_before={name:connection.execute('select count(*) from '+name).fetchone()[0] for name in ['chat_usage_events','devin_acp_attempts','chat_room_messages']}
expected={'requests':len({r['logical_request_id'] for r in rows if r['logical_request_id'] is not None}), 'attempts':sum(r['dispatched']==1 for r in rows),'failed_attempts':sum(r['status'] not in ['legacy','prepared','dispatched','completed'] for r in rows),'pending_attempts':sum(r['status'] in ['prepared','dispatched'] for r in rows)}
actual=get('/api/chat/rooms/'+room+'/insights?message_ids='+latest)
assert len(actual['usage'])==1
assert all(actual['usage'][0][key]==value for key,value in expected.items())
assert not any(r['usage_known_mask']&3 for r in rows)
assert actual['usage'][0]['input_tokens'] is None and actual['usage'][0]['output_tokens'] is None
assert latest in actual['indices']
other=get('/api/chat/rooms/main-room/insights?message_ids='+latest)
assert other['room_id']=='main-room' and other['usage']==[] and other['indices']=={} and other['timings']=={}
again=get('/api/chat/rooms/'+room+'/insights?message_ids='+latest)
assert again==actual, '只读切换前后结果不同，需要核验是否有真实新写入'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as connection:
    counts_after={name:connection.execute('select count(*) from '+name).fetchone()[0] for name in counts_before}
assert counts_before==counts_after
result={'installed_version':'0.2.97','verified_ms':time.time()*1000,'expected_from_read_only_real_ledger':expected,'swe_insights':actual,'other_room_with_foreign_message_id':other,'counts_before':counts_before,'counts_after':counts_after,'bindings':bindings,'generated_model_requests':0,'cross_room_usage_and_foreign_message_projection':'passed','late_response_race':'not_tested'}
(output/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'counts':expected,'other_usage':other['usage'],'other_indices':other['indices'],'model_requests':0},ensure_ascii=False))
