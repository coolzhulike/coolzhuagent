"""只读等待真实新用例开始规划；不修改模型、宿主或运行状态。"""
import json,pathlib,sqlite3,sys,time
repo=pathlib.Path(__file__).resolve().parents[2]
db=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
c=sqlite3.connect(db.as_uri()+'?mode=ro',uri=True)
c.row_factory=sqlite3.Row
deadline=time.monotonic()+35
prefix=sys.argv[1]
while time.monotonic()<deadline:
 m=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(prefix+'%',)).fetchone()
 if m:
  r=c.execute("SELECT e.run_id FROM runtime_run_events e,json_each(e.payload_json,'$.message_ids') x WHERE e.event_type='chat.source_messages' AND x.value=? LIMIT 1",(m['id'],)).fetchone()
  if r:
   u=c.execute('SELECT u.state,u.action_count FROM computer_use_runs u JOIN tool_calls t ON t.tool_call_id=u.provider_tool_call_id WHERE t.run_id=?',(r[0],)).fetchone()
   if u and u['state']=='planning' and u['action_count']==0:
    print(json.dumps({'state':u['state'],'action_count':u['action_count'],'observed_ms':time.time()*1000}));break
   if u and u['state'] not in ['planning','pending','observing']:
    raise SystemExit('真实任务已离开初次规划：'+json.dumps(dict(u)))
 time.sleep(.12)
else:raise SystemExit('35秒内未观察到真实新任务规划；不执行页面输入')
