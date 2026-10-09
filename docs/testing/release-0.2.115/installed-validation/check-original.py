from pathlib import Path
import json, sqlite3, urllib.request
p=Path(__file__).resolve().parent; repo=p.parents[1]
with urllib.request.urlopen('http://127.0.0.1:8765/api/sessions/session-1791131217833/model-settings',timeout=10) as r: s=json.load(r)
assert s['session']['model']=='swe-2-medium' and s['configuration_revision']==51
db=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
    bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',)).fetchall()
    assert sum(b[1]=='island-kayak' for b in bindings)==1 and not any(b[2] for b in bindings)
    active=c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
    assert not active
(p/'original-idle.json').write_text(json.dumps({'model':'swe-2-medium','configuration_revision':51,'bindings':bindings,'active_runs':0},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('原SWE、唯一云端绑定和revision51不变，活动轮次0')
