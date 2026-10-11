"""唯一既有 SWE 会话的真实源码审查；只输出 SSE 事件名，不保存模型思考。"""
from pathlib import Path
import json, sqlite3, urllib.request, time

folder = Path(__file__).resolve().parent
root = folder.parents[1]
db = root / 'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro', uri=True) as conn:
    assert conn.execute("SELECT COUNT(*) FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchone()[0] == 0
    bindings = conn.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?', ('session-1791131217833',)).fetchall()
    assert sum(b[1]=='island-kayak' for b in bindings)==1 and not any(b[2] for b in bindings), bindings
    before = {'attempts':conn.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0], 'messages':conn.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0]}
with urllib.request.urlopen('http://127.0.0.1:8765/api/sessions/session-1791131217833/model-settings') as response:
    settings = json.load(response)
assert settings['session']['model']=='swe-2-medium' and settings['configuration_revision']==35
assert settings['parameters']['tool_allowlist']==['computer_use_perform','plugin__cli_anything_status','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab']
(folder/'before.json').write_text(json.dumps({'counts':before,'bindings':bindings,'configuration_revision':35},ensure_ascii=False,indent=2),encoding='utf-8')
payload = {'expected_workspace_id':'ws-23f646a969206cb4','native_browser_panel':True,'session_id':'session-1791131217833','target_agent_ids':['session-1791131217833'],'chat_room_id':'room-1791131523339','text':(folder/'review-prompt.txt').read_text(encoding='utf-8'),'attachments':[]}
request = urllib.request.Request('http://127.0.0.1:8765/api/chat/send/stream', json.dumps(payload).encode(), {'Content-Type':'application/json'})
with urllib.request.urlopen(request, timeout=1000) as response:
    for raw in response:
        if raw.startswith(b'event:'): print(raw.decode('utf-8').strip(), flush=True)
(folder/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'status':'EOF；终态另核验'}),encoding='utf-8')
