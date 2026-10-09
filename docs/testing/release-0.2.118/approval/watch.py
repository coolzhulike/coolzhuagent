"""只读等待真实ACP接纳阶段，正常产品PATCH授权，不暂停产品。"""
from common import *
assert not (p/'grant-expansion.json').exists()
start=time.monotonic()
while time.monotonic()-start<240:
    with connection() as c:
        row=c.execute("SELECT r.id,r.state,a.attempt_id,a.state FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id JOIN json_each(e.payload_json,'$.message_ids') refs JOIN chat_room_messages m ON m.id=refs.value JOIN devin_acp_attempts a ON json_extract(a.scope_json,'$.run_id')=r.id WHERE e.event_type='chat.source_messages' AND m.role='user' AND m.content LIKE ? ORDER BY m.created_at DESC LIMIT 1",(marker+'%',)).fetchone()
    if row and row[3]=='prepared':
        out={'run_id':row[0],'run_state':row[1],'attempt_id':row[2],'observed_stage':row[3],'started_ms':time.time()*1000}
        out['permission']=permission('full-access');out['finished_ms']=time.time()*1000
        assert out['permission']['full_access'];save('grant-expansion.json',out);print(json.dumps(out,ensure_ascii=False),flush=True);break
    if row and row[3] in ('terminal','not_sent','unknown'):raise RuntimeError('未命中prepared阶段，不计冻结权限竞争通过')
    time.sleep(.01)
else:raise RuntimeError('接纳阶段未出现，不计验收通过')
