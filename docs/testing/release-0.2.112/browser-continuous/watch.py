"""持续只读真实数据库；不注入产品暂停、不调用宿主或UI。"""
from pathlib import Path
import json,sqlite3,time
folder=Path(__file__).resolve().parent;repo=folder.parents[1]
db=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
marker='BU-CONTINUOUS-CLOSE-112-20261008'
started=time.monotonic();polls=0;previous=None;max_gap=0.;terminal=None
print(json.dumps({'watch_started_ms':time.time()*1000,'continuous':True}),flush=True)
while time.monotonic()-started<240:
    now=time.monotonic();max_gap=max(max_gap,now-previous) if previous else 0.;previous=now;polls+=1
    with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True,timeout=1) as c:
        source=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone()
        if source:
            run=c.execute("SELECT r.id,r.state,r.legacy_turn_id FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",source).fetchone()
            if run:
                cu=c.execute('SELECT call_id,state FROM computer_use_runs WHERE turn_id=? ORDER BY created_at_ms DESC LIMIT 1',(run[2],)).fetchone()
                if cu:
                    step=c.execute('SELECT input_delivery,input_release_status,completed_at_ms FROM computer_use_steps WHERE run_id=? AND step_index=0',(cu[0],)).fetchone()
                    if cu[1]=='observing' and step and step[0:2]==('sent','released') and step[2] is not None:
                        requested=c.execute("SELECT id,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='browser.observation_requested' AND created_at>=? ORDER BY id LIMIT 1",(run[0],step[2])).fetchone()
                        if requested:
                            found={'ready':True,'run_id':run[0],'cu_call_id':cu[0],'observed_cu_state':cu[1],'request_event_id':requested[0],'request_created_at':requested[1],'request':json.loads(requested[2]),'detected_ms':time.time()*1000,'polls':polls,'max_poll_gap_ms':max_gap*1000}
                            (folder/'watch-result.json').write_text(json.dumps(found,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
                            print(json.dumps(found,ensure_ascii=False),flush=True);raise SystemExit(0)
                    if cu[1] in ('succeeded','blocked','failed','cancelled','interrupted'):terminal=cu[1];break
                if run[1] in ('completed','failed','cancelled','interrupted'):terminal=run[1];break
    time.sleep(.002)
result={'ready':False,'terminal':terminal,'polls':polls,'max_poll_gap_ms':max_gap*1000,'observed_ms':time.time()*1000}
(folder/'watch-result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8');print(json.dumps(result),flush=True)
