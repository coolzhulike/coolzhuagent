"""仅查询表结构和本轮非思考的宿主验证投影。"""
from pathlib import Path
import json,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[2]
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
    c.row_factory=sqlite3.Row
    tables=[x[0] for x in c.execute("SELECT name FROM sqlite_master WHERE type='table'") if any(k in x[0] for k in ('computer','fact','runtime','tool'))]
    cu=c.execute('SELECT call_id FROM computer_use_runs WHERE turn_id=?',(facts['run']['legacy_turn_id'],)).fetchone()[0]
    rows=[]
    for row in c.execute('SELECT request_kind,response_json,error_code,started_at_ms,completed_at_ms FROM computer_use_planner_diagnostics WHERE call_id=? ORDER BY id',(cu,)):
        data=json.loads(row['response_json']) if row['response_json'] else None
        rows.append({'kind':row['request_kind'],'error':row['error_code'],'started':row['started_at_ms'],'ended':row['completed_at_ms'],
                     'response_keys':list(data) if isinstance(data,dict) else type(data).__name__})
    for row in c.execute("SELECT response_json FROM computer_use_planner_diagnostics WHERE call_id=? AND request_kind='computer_use_browser_readonly_verification' ORDER BY id DESC LIMIT 1",(cu,)):
        data=json.loads(row[0])
        projected={key:data.get(key) for key in ('criteria','host_verification')}
        (p/'last-host-verification.json').write_text(json.dumps(projected,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        print(json.dumps(projected,ensure_ascii=False))
