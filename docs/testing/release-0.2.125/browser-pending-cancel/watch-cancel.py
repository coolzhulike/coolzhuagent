"""只读追踪本次marker；sent/released后的关联观察登记后调用正常停止接口一次。"""
from pathlib import Path
import time,sqlite3,json,urllib.request
root=Path.cwd();p=root/'tmp/2026-10-10-browser-pending-cancel'
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
marker='BU-PENDING-CANCEL125-20261010'
assert not (p/'cancel-action.json').exists()
deadline=time.monotonic()+900
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True,timeout=2) as c:
 while time.monotonic()<deadline:
  source=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone()
  if not source:time.sleep(.05);continue
  run=c.execute("SELECT r.id,r.state,r.legacy_turn_id,r.session_id,r.chat_room_id FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",source).fetchone()
  if not run:time.sleep(.05);continue
  cu=c.execute('SELECT call_id,state FROM computer_use_runs WHERE turn_id=? ORDER BY created_at_ms DESC LIMIT 1',(run[2],)).fetchone()
  if cu:
   step=c.execute('SELECT step_index,input_delivery,input_release_status,completed_at_ms FROM computer_use_steps WHERE run_id=? ORDER BY step_index',(cu[0],)).fetchall()
   if cu[1]=='observing' and len(step)==1 and step[0][1:3]==('sent','released') and step[0][3] is not None:
    request=c.execute("SELECT id,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='browser.observation_requested' AND created_at>=? ORDER BY id LIMIT 1",(run[0],step[0][3])).fetchone()
    if request:
     observed_ms=time.time()*1000
     payload={'session_id':run[3],'chat_room_id':run[4],'turn_id':run[2]}
     before=time.time()*1000
     req=urllib.request.Request('http://127.0.0.1:8765/api/chat/turn/interrupt',json.dumps(payload).encode(),{'Content-Type':'application/json'})
     with urllib.request.urlopen(req,timeout=15) as response:
      raw=response.read();status=response.status
     after=time.time()*1000
     (p/'cancel-response.json').write_bytes(raw)
     result={'attempted':True,'normal_stop_interface':True,'run_id':run[0],'cu_call_id':cu[0],
       'step':step[0],'request':{'event_id':request[0],'created_at':request[1],'payload':json.loads(request[2])},
       'observed_ms':observed_ms,'submitted_ms':before,'response_ms':after,'http_status':status,'payload':payload,'response':json.loads(raw)}
     (p/'cancel-action.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
     print(json.dumps({'cancel_requested':True,'after_request_ms':round(before-request[1],3),'http_status':status}),flush=True);break
  if run[1] in ('completed','failed','interrupted'):
   (p/'cancel-action.json').write_text(json.dumps({'attempted':False,'reason':'任务已结束，未补发取消'}),encoding='utf-8');print('未命中，保留未完成',flush=True);break
  time.sleep(.01 if cu and cu[1]=='observing' else .05)
 else:
  (p/'cancel-action.json').write_text(json.dumps({'attempted':False,'reason':'监视期限耗尽，没有补发输入或取消'}),encoding='utf-8');print('期限耗尽',flush=True)
