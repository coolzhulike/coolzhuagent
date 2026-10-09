from pathlib import Path
import json,sqlite3,hashlib,shutil
folder=Path(__file__).resolve().parent
root=Path('C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent')
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
status=json.loads((folder/'latest-status.json').read_text(encoding='utf-8'))
run=status['run']['id']
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
    c.row_factory=sqlite3.Row
    events=[dict(r) for r in c.execute("SELECT id,event_type,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND (event_type LIKE 'browser.%' OR event_type LIKE 'run.%' OR event_type='tool.dispatch_rejected') ORDER BY id",(run,))]
    for r in events:r['payload']=json.loads(r.pop('payload_json'))
    message=c.execute("SELECT id,role,content FROM chat_room_messages WHERE room_id=? AND id NOT LIKE '%-thinking' ORDER BY created_at DESC LIMIT 1",('room-1791131523339',)).fetchone()
    final_message=dict(message) if message else None
facts={'status':status,'runtime_events':events,'last_chat_message':final_message}
(folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'run':status['run'],'events':events,'last_message':final_message},ensure_ascii=False))
