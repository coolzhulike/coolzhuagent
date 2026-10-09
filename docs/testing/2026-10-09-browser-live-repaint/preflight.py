from pathlib import Path
import sqlite3,json
root=Path(__file__).resolve().parents[2]
p=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(p.resolve().as_uri()+'?mode=ro',uri=True) as c:
 active=c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
 bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',)).fetchall()
 assert not active and sum(x[1]=='island-kayak' for x in bindings)==1 and not any(x[2] for x in bindings)
 print(json.dumps({'active':active,'bindings':bindings}),flush=True)
