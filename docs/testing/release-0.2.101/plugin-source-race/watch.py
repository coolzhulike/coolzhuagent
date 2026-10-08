"""真实ACP submitted后只改变自有验收插件注释，终态排空后按SHA恢复。"""
from pathlib import Path
import sqlite3,json,time,hashlib
folder=Path(__file__).resolve().parent;root=folder.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
entry=Path(before['entry_path']);original=(folder/'entry-original.bin').read_bytes()
assert hashlib.sha256(original).hexdigest()==before['entry_sha256']
changed=original+b'\n// COOLZHU_SOURCE_RACE_101_20261008: inert integrity observation.\n'
def source():
    with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
        return c.execute("SELECT r.id,r.state,a.attempt_id,a.state,a.process_drained FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id JOIN json_each(e.payload_json,'$.message_ids') refs JOIN chat_room_messages m ON m.id=refs.value JOIN devin_acp_attempts a ON json_extract(a.scope_json,'$.run_id')=r.id WHERE e.event_type='chat.source_messages' AND m.content LIKE ? ORDER BY m.created_at DESC LIMIT 1",('PLUGIN-SOURCE-RACE-101-20261008%',)).fetchone()
deadline=time.monotonic()+240
while time.monotonic()<deadline:
    row=source()
    if row and row[3]=='submitted':break
    if row and row[3] in ('terminal','not_sent','unknown'):raise RuntimeError('未命中submitted窗口，未改动文件')
    time.sleep(.02)
else:raise RuntimeError('未观察到submitted，未改动文件')
assert not entry.is_symlink() and entry.read_bytes()==original
observation={'run_id':row[0],'run_state_at_change':row[1],'attempt_id':row[2],'attempt_state_at_change':row[3],'mutation_begin_ms':time.time()*1000,'original_sha256':before['entry_sha256'],'changed_sha256':hashlib.sha256(changed).hexdigest()}
entry.write_bytes(changed)
observation['mutation_end_ms']=time.time()*1000
(folder/'source-observation.json').write_text(json.dumps(observation,indent=2),encoding='utf-8')
print(json.dumps({'phase':'source_changed','run':row[0],'attempt_state':row[3]},ensure_ascii=False),flush=True)
deadline=time.monotonic()+360
while time.monotonic()<deadline:
    row=source()
    if row and row[1] in ('completed','failed','interrupted','cancelled','canceled') and row[3] in ('terminal','not_sent') and row[4]==1:break
    time.sleep(.1)
else:raise RuntimeError('运行未确认排空，保留失效源码并等待核验；不可提前恢复旧资格')
assert entry.read_bytes()==changed,'文件被第三方改动，不覆盖'
entry.write_bytes(original)
assert hashlib.sha256(entry.read_bytes()).hexdigest()==before['entry_sha256']
observation.update({'terminal_run_state':row[1],'terminal_attempt_state':row[3],'process_drained':row[4],'restore_ms':time.time()*1000,'restored_sha256':hashlib.sha256(entry.read_bytes()).hexdigest()})
(folder/'source-observation.json').write_text(json.dumps(observation,indent=2),encoding='utf-8')
print(json.dumps({'phase':'restored_after_drained','run_state':row[1],'sha256':observation['restored_sha256']}),flush=True)
