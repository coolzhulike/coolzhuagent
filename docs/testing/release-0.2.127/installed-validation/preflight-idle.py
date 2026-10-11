"""仅只读确认正常安装前没有在途模型轮次。"""
import sqlite3,pathlib,json
root=pathlib.Path(__file__).resolve().parents[2]
p=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(p.resolve().as_uri()+'?mode=ro',uri=True) as c:
    active=c.execute("SELECT id,state FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
    bindings=c.execute('SELECT remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',)).fetchall()
facts={'active':active,'bindings':bindings}
print(json.dumps(facts))
assert not active and sum(x[0]=='island-kayak' for x in bindings)==1 and not any(x[1] for x in bindings)
(pathlib.Path(__file__).resolve().parent/'preflight-idle.json').write_text(json.dumps(facts,indent=2)+'\n',encoding='utf-8')
