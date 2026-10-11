"""只读核验原日常及验收库，避免中止活动任务；不导出其它会话内容。"""
from pathlib import Path
import json, sqlite3
repo=Path(__file__).resolve().parents[2]
result={}
for name,root in [('daily',Path('C:/Users/zhupu/coolzhuagent')),('acceptance',repo/'tmp/2026-10-04-devin-models/workspace')]:
    db=root/'.coolzhu/web-sessions.sqlite3'
    with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
        active=c.execute("SELECT id,state FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
        assert not active,(name,active)
        result[name]={'active_runs':0}
        if name=='acceptance':
            bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',)).fetchall()
            assert sum(b[1]=='island-kayak' for b in bindings)==1 and not any(b[2] for b in bindings),bindings
            result[name]['bindings']=bindings
(Path(__file__).parent/'idle-before-install.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result,ensure_ascii=False))
