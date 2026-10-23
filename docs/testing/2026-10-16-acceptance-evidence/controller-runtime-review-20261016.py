import datetime, hashlib, json, pathlib, sqlite3, urllib.request

out=pathlib.Path(r'D:\coolzhuagent\tmp\goal-32wbs-acceptance-20261013')
backup=out/'cold-start-20261016/session-before.sqlite3'
current=pathlib.Path(r'D:\tmp\.coolzhu\web-sessions.sqlite3')
tables=['chat_room_messages','tool_calls','runtime_runs','runtime_run_events','devin_acp_attempts','devin_acp_bindings','goals','goal_phases','goal_events','computer_use_runs','memory_beads']
def rows(path, table):
    with sqlite3.connect(path.as_uri()+'?mode=ro',uri=True,timeout=3) as db:
        columns=[x[1] for x in db.execute(f'PRAGMA table_info("{table}")')]
        if not columns: return {'exists':False}
        # 完整行的无序多重集散列，避免未声明行顺序导致假差异。
        digests=[]
        for row in db.execute(f'SELECT * FROM "{table}"'):
            raw=json.dumps(list(row),ensure_ascii=False,separators=(',',':'),default=lambda x:x.hex() if isinstance(x,bytes) else str(x)).encode()
            digests.append(hashlib.sha256(raw).digest())
        return {'exists':True,'rows':len(digests),'columns':columns,'rows_sha256':hashlib.sha256(b''.join(sorted(digests))).hexdigest()}
data={'at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'mode':'read_only','backup':str(backup),'database':str(current),'tables':[]}
for table in tables:
    before=rows(backup,table);after=rows(current,table)
    data['tables'].append({'table':table,'before':before,'after':after,'unchanged':before==after})
with urllib.request.urlopen('http://127.0.0.1:8765/api/workspace',timeout=8) as response:
    ws=json.load(response)
data['workspace']={k:v for k,v in ws.items() if k in ('workspace_id','path','workspace','workspace_path')}
with sqlite3.connect(current.as_uri()+'?mode=ro',uri=True,timeout=3) as db:
    data['binding']=db.execute("SELECT workspace_id,agent_id,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id='session-1791131217833'").fetchall()
    data['quick_check']=db.execute('PRAGMA quick_check').fetchone()[0]
data['history_unchanged']=all(t['unchanged'] for t in data['tables'])
data['state']='completed' if data['history_unchanged'] and data['quick_check']=='ok' else 'needs_review'
(out/'controller-runtime-history-20261016.json').write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'state':data['state'],'history_unchanged':data['history_unchanged'],'counts':{t['table']:t['after'].get('rows') for t in data['tables']},'workspace':data['workspace'],'binding':data['binding']},ensure_ascii=False))
