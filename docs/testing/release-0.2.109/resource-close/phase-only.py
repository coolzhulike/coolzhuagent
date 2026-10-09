"""正常网页导航只由只读真实步骤阶段触发，不访问宿主私有通道。"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,threading,time,sqlite3,os
folder=Path(__file__).resolve().parent
repo=Path('C:\\Users\\zhupu\\.codex\\worktrees\\input-recovery-20261004\\coolzhuagent')
db=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
marker='BU-RESOURCE-CLOSE-109-20261008'
lock=threading.Lock()
def record(item):
    item.update(server_ns=time.perf_counter_ns(),received_unix_ns=time.time_ns())
    with lock:
        with (folder/'events.jsonl').open('a',encoding='utf-8') as stream:stream.write(json.dumps(item,ensure_ascii=False)+'\n')
def phase():
    with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True,timeout=1) as c:
        source=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone()
        if not source:return None
        run=c.execute("SELECT r.id,r.state,r.legacy_turn_id FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",source).fetchone()
        if not run:return None
        cu=c.execute('SELECT call_id,state FROM computer_use_runs WHERE turn_id=? ORDER BY created_at_ms DESC LIMIT 1',(run[2],)).fetchone()
        if not cu:return None
        steps=c.execute('SELECT step_index,input_delivery,input_release_status,completed_at_ms FROM computer_use_steps WHERE run_id=? ORDER BY step_index',(cu[0],)).fetchall()
        if cu[1]=='observing' and len(steps)==1 and steps[0][1:3] == ('sent','released') and steps[0][3] is not None:
            requested=c.execute("SELECT id,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='browser.observation_requested' AND created_at>=? ORDER BY id LIMIT 1",(run[0],steps[0][3])).fetchone()
            if requested:
                return {'run_id':run[0],'cu_call_id':cu[0],'cu_state':cu[1],'steps':steps,
                    'observation_requested':{'event_id':requested[0],'created_at':requested[1],'payload':json.loads(requested[2])}}
        return None
