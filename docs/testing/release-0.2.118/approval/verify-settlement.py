from common import *
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
assert facts['network_request_count']==0 and len(facts['calls'])==1
assert facts['calls'][0]['status']=='failed'
assert len(facts['attempts'])==1 and facts['attempts'][0]['state']=='terminal' and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert sum(b['remote_session_id']=='island-kayak' for b in facts['bindings'])==1 and not any(b['locked_attempt'] for b in facts['bindings'])
old_run='run-chat-f51a5172bff48e501a7149d8c079fcc3e141daa262aaf9d3'
with connection() as c:
    old=c.execute('SELECT state,protocol_stop,process_drained FROM devin_acp_attempts WHERE attempt_id=?',('af87b8cedd2e560bd23826dd5a70813a86c07df0993bfec7',)).fetchone();assert old==('unknown','end_turn',1)
    old_calls=c.execute('SELECT tool_call_id,status FROM tool_calls WHERE run_id=?',(old_run,)).fetchall();assert len(old_calls)==1 and old_calls[0][1]=='failed'
    events=c.execute("SELECT event_type,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='tool.approval_not_resumable'",(old_run,)).fetchall();assert len(events)==1
    e=json.loads(events[0][1]);assert e['executed'] is False and e['previous_status']=='awaiting_approval' and e['settled_status']=='failed'
save('settlement-verification.json',{'passed':True,'old_attempt_preserved':old,'old_calls':old_calls,'old_reconciliation_events':events,'binding':facts['bindings'],'new_terminal_tool_status':facts['calls'][0]['status'],'new_attempt':facts['attempts'][0],'network_requests':0,'new_cloud_sessions':0,'source':'正常聊天接纳rotate/claim，未手写验收库'})
print('旧审批正常接纳收束；旧unknown保留；新调用failed、terminal/end_turn/排空、唯一原绑定解锁、HTTP0')
