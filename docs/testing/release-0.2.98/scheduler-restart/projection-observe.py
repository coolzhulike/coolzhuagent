"""只观察前轮真实回写失败的后续定时器恢复，不重新创建/派发任务。"""
from pathlib import Path
import json,sqlite3,time,urllib.request
p=Path(__file__).resolve().parent;file=p/'scheduler-projection-result.json';result=json.loads(file.read_text(encoding='utf8'))
root=Path(json.loads((p/'installed-098-standard-processes.json').read_text(encoding='utf8'))['workspace'])
task_id=result['scheduled_task']['id'];marker=result['marker']
assert result['locked_run_due']['ran'][0]['status']=='projection_pending'
observations=[];deadline=time.monotonic()+40
while time.monotonic()<deadline:
 value=json.load(urllib.request.urlopen('http://127.0.0.1:8765/api/task-schedules'))
 task=next(x for x in value['tasks'] if x['id']==task_id);observations.append({'generated_at':value['generated_at'],'status':task['status']})
 if task['status']=='completed':break
 time.sleep(.5)
else:raise RuntimeError('真实定时器未修复投影，保留计划和证据继续检查')
with sqlite3.connect((root/'.coolzhu/scheduled-jobs.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 claims=[dict(x) for x in c.execute('select workspace_id,task_id,scheduled_for_ms,fingerprint,state,started_at_ms,outcome_json from scheduled_job_attempts where task_id=?',(task_id,))]
assert claims==result['claims_before_restart']
with sqlite3.connect((root/'.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
 messages=c.execute("select id,content from chat_room_messages where room_id='room-scheduled-tasks' and content like ? order by created_at",('%'+marker+'%',)).fetchall()
 assert len(messages)==1
 ledger={'bindings':c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=? order by lane',('session-1791131217833',)).fetchall(),'attempts':c.execute('select count(*) from devin_acp_attempts').fetchone()[0],'usage':c.execute('select count(*) from chat_usage_events').fetchone()[0]}
assert json.loads(json.dumps(ledger))==result['before_ledger']
result.update(harness_failure=result.pop('failure'),recovery_observations=observations,recovered_list=value,claims_after_live_recovery=claims,status_messages_after_live_recovery=messages,after_ledger=ledger,passed_scope='正式真实文件占用导致计划回写失败；后续真实定时器仅修复投影，领取事实不变，状态消息1次、模型0次。未执行重启回写恢复。')
file.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print(json.dumps({'state':task['status'],'claims':len(claims),'status_messages':len(messages),'model_attempts':ledger['attempts'],'scope':result['passed_scope']},ensure_ascii=False))
