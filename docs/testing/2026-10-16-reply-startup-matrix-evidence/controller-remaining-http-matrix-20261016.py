"""4.2/6.2 发布包隔离验收；合成数据不代表真实模型运行或机器断电。"""
import concurrent.futures, contextlib, datetime, hashlib, json, os, pathlib, re
import socket, sqlite3, subprocess, time, urllib.request, urllib.error, urllib.parse

REPO=pathlib.Path(r'D:\coolzhuagent')
OUT=REPO/'tmp/goal-32wbs-acceptance-20261013'
ROOT=OUT/'remaining-matrix-20261016'
ROOT.mkdir(exist_ok=True)
EXE=OUT/'release-0.2.135/package/bin/coolzhu-web-console.exe'
OLD=pathlib.Path(r'C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe')
REPORT=OUT/'controller-remaining-http-matrix-20261016.json'
result={'started_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'running',
 'boundary':'完整0.2.135发布二进制+隔离SQLite合成夹具，非正式界面/外部模型/OS重启/物理磁盘满/安装降级验收',
 'binary':str(EXE),'binary_sha256':hashlib.sha256(EXE.read_bytes()).hexdigest(),
 'production_database_touched':False,'external_model_calls':0,'cases':[],'processes':[]}
def save(): REPORT.write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
def case(name, fn):
    entry={'name':name}; result['cases'].append(entry)
    try: entry.update(state='passed',evidence=fn())
    except Exception as e: entry.update(state='failed',error=repr(e))
    save(); print(name,entry['state'],flush=True)
def require(value,detail=None):
    if not value: raise AssertionError(detail)
def dbopen(path):
    db=sqlite3.connect(path,timeout=10); db.execute('PRAGMA busy_timeout=10000'); return db
def snapshot(path):
    with dbopen(path) as db: return {'version':db.execute('PRAGMA user_version').fetchone()[0],
      'dump':list(db.iterdump()),'quick_check':db.execute('PRAGMA quick_check').fetchone()[0]}

class Server:
    def __init__(self,name,port):
        self.root=ROOT/name; self.root.mkdir(exist_ok=False)
        self.db=self.root/'sessions.sqlite3'; self.port=port; self.base=f'http://127.0.0.1:{port}'
        (self.root/'coolzhu.toml').write_text(f'[web]\nbind_addr="127.0.0.1:{port}"\n[pet]\nenabled=false\n[scheduled_tasks]\ntasks=[]\n',encoding='utf-8')
        self.proc=None; self.log=None; self.sequence=0
    def request(self,path,body=None,headers=None):
        req=urllib.request.Request(self.base+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json',**(headers or {})})
        try:
            with urllib.request.urlopen(req,timeout=12) as response: return response.status,json.load(response)
        except urllib.error.HTTPError as error:
            raw=error.read().decode();
            try: raw=json.loads(raw)
            except ValueError: pass
            return error.code,raw
    def launch(self,exe=EXE,healthy=True):
        with socket.socket() as s: s.bind(('127.0.0.1',self.port))
        env=dict(os.environ);env.update(COOLZHU_RUNTIME_DIR=str(self.root),COOLZHU_WEB_SESSION_DB=str(self.db),
          COOLZHU_WEB_SESSION_STORE=str(self.root/'sessions.json'),COOLZHU_WEB_ATTACHMENT_STORE=str(self.root/'attachments'),
          COOLZHU_INPUT_SAFETY_STATE_ROOT=str(self.root/'input-safety'),COOLZHU_DESKTOP_SHELL_MANAGED='1')
        self.sequence+=1; self.log=(self.root/f'server-{self.sequence}.log').open('wb')
        started=time.monotonic()
        self.proc=subprocess.Popen([str(exe)],cwd=self.root,env=env,stdout=self.log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
        record={'fixture':str(self.root),'pid':self.proc.pid,'exe':str(exe),'expected_healthy':healthy}
        result['processes'].append(record); save()
        for _ in range(240):
            if self.proc.poll() is not None: break
            try:
                code,w=self.request('/api/workspace')
                if code==200:
                    record['ready_seconds']=round(time.monotonic()-started,3)
                    require(healthy,'故障库意外开放HTTP'); self.workspace=w
                    self.headers={'x-coolzhu-workspace-id':w['workspace_id'],'x-coolzhu-configuration-scope':w['configuration_scope']}
                    save(); return record
            except (OSError,TimeoutError): pass
            time.sleep(.25)
        record['exit_code']=self.proc.poll(); save()
        require(not healthy and self.proc.poll() is not None,record)
        record['http_opened']=False; return record
    def stop(self):
        if self.proc:
            if self.proc.poll() is None:
                self.proc.terminate()
                try:self.proc.wait(timeout=10)
                except subprocess.TimeoutExpired:self.proc.kill();self.proc.wait()
            result['processes'][-1]['process_exited']=True
            self.proc=None;self.log.close(); save()
    def room(self,name):
        code,b=self.request('/api/chat/rooms',{'name':name}); require(code==200,b);return b['room']['id']
    def trace(self,room,message):return self.request(f'/api/chat/rooms/{room}/trace?message_id={message}')

def v12(server):
    text=(REPO/'modules/gui-web/packages/web-console/src/main.rs').read_text(encoding='utf-8')
    fragment=text[text.index('fn migration_v13_upgrades_v12_database_without_losing_rows()'):]
    sql=re.search(r'r#"(.*?)"#',fragment,re.S).group(1).replace("'running'","'completed'")
    with dbopen(server.db) as db:
        db.executescript(sql)
        # 原单元夹具只有Goal表；完整服务首次保存还要求v7-v10已存在的记忆侧表。
        # 这些历史阶梯按版本短路，不能把缺表的局部夹具冒充完整v12旧库。
        for version in [7,8,9,10]:
            migration=text[text.index(f'fn apply_session_migration_v{version}('):]
            db.executescript(re.search(r'r#"(.*?)"#',migration,re.S).group(1))
        db.execute('PRAGMA user_version=12')
        db.executescript("CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO metadata VALUES('fixture_marker','旧库中文保留');")
    return snapshot(server.db)
def backups(server):return sorted((server.root/'schema-backups').glob('*.sqlite3'))

def trace_matrix():
    s=Server('reply-concurrent',29461)
    try:
        s.launch(); room=s.room('隔离逐回复矩阵，非模型回复'); other=s.room('隔离异房间')
        require(s.request(f'/api/chat/rooms/{room}/trace')[0]==200)
        ws=s.workspace['workspace_id']; db=dbopen(s.db)
        for run,w,r in [('root-run',ws,room),('new-run',ws,room),('grand-run',ws,room),('foreign-run','foreign',room),('foreign-room-run',ws,other),('orphan-run',ws,room)]:
            db.execute("INSERT INTO runtime_runs(id,kind,workspace_id,session_id,chat_room_id,state,claim_token,created_at) VALUES(?,'chat_turn',?,'fixture-session',?,'completed','fixture-only',1)",(run,w,r))
        for msg,run in [('reply-old','root-run'),('reply-new','new-run')]:
            db.execute("INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES(?,?,'合成夹具','assistant','chat','明确的合成回复','assistant-reply','[]',1)",(room,msg))
            db.execute("INSERT INTO runtime_run_events(run_id,event_type,payload_json,created_at) VALUES(?,'chat.context_outputs',?,1)",(run,json.dumps({'messages':[{'id':msg}]})))
        for ident,parent,w,r,run,call in [('execution-one','grand',ws,room,'root-run',None),('child','execution-one',ws,room,None,'child-call'),('grand','child',ws,room,'grand-run',None),('foreign-workspace','execution-one','foreign',room,'foreign-run',None),('foreign-room','execution-one',ws,other,'foreign-room-run',None),('orphan','missing',ws,room,'orphan-run',None)]:
            spec=dict(id=ident,workspace_id=w,room_id=r,session_id='fixture-session',provider='openai',model='fixture-no-request',source='session_tool' if call else 'scheduled',source_id='fixture-only',parent_run_id='root-run' if call else None,parent_execution_id=parent,call_id=call,summary='合成归属夹具',deadline_ms=60000)
            db.execute("INSERT INTO execution_index(id,workspace_id,spec_json,state,started_ms,run_id) VALUES(?,?,?,'completed',1,?)",(ident,w,json.dumps(spec),run))
        for attempt,run,call,i,o,mask in [('first','root-run',None,10,2,3),('second','root-run','child-call',20,3,3),('third','grand-run',None,30,999,1),('unrelated','orphan-run',None,900,900,3)]:
            db.execute("INSERT INTO chat_usage_events(workspace_id,room_id,session_id,created_at,input_tokens,output_tokens,cache_read_tokens,cache_write_tokens,attempt_id,logical_request_id,status,dispatched,usage_known_mask,run_id,call_id) VALUES(?,?,'fixture-session',1,?,?,0,0,?,?,'completed',1,?,?,?)",(ws,room,i,o,attempt,attempt,mask,run,call))
        db.commit();db.close()
        def expect_trace(msg,run):
            code,b=s.trace(room,msg);require(code==200,b)
            require([v['run_id'] for v in b['runs']]==[run] and not b['has_more'],b)
            require(b['requested_message_id']==msg,b)
            return b
        def tree(expected_output):
            code,b=s.request('/api/executions/execution-one',headers=s.headers);require(code==200,b)
            t=b['subtree'];require(t['execution_ids']==['child','execution-one','grand'],t)
            require((t['descendant_count'],t['usage_attempt_count'],t['input_tokens'],t['output_tokens'])==(2,3,60,expected_output),t)
            return t
        case('4.2 后代环路去重、跨工程/房间及孤儿隔离、未知用量保留',lambda:tree(None))
        def concurrent_reads():
            def worker(n):
                msg,run=('reply-old','root-run') if n%2 else ('reply-new','new-run')
                expect_trace(msg,run);tree(None);return msg
            def writer():
                with dbopen(s.db) as c:
                    for n in range(24):
                        c.execute("UPDATE chat_room_messages SET content=? WHERE id='reply-old'",(f'合成迟到版本{n}',)); c.commit();time.sleep(.01)
            with concurrent.futures.ThreadPoolExecutor(max_workers=7) as pool:
                writing=pool.submit(writer); outputs=list(pool.map(worker,range(32)));writing.result()
            return {'reply_requests':32,'subtree_requests':32,'concurrent_commits':24,'reply_counts':{m:outputs.count(m) for m in set(outputs)}}
        case('4.2 并发定位和后代读取不串回复',concurrent_reads)
        def late():
            with dbopen(s.db) as c:c.execute("UPDATE chat_usage_events SET output_tokens=4,usage_known_mask=3 WHERE attempt_id='third'")
            t=tree(9);expect_trace('reply-old','root-run');return t
        case('4.2 迟到usage补全后只更新正确后代树',late)
        def scopes():
            observed=[]
            for path,headers,expected in [('/api/executions/execution-one',{},400),('/api/executions/execution-one',{**s.headers,'x-coolzhu-configuration-scope':'stale'},409),('/api/executions/foreign-workspace',s.headers,404),(f'/api/chat/rooms/{other}/trace?message_id=reply-old',{},404),(f'/api/chat/rooms/{room}/trace?run_id=foreign-run',{},404)]:
                code,body=s.request(path,headers=headers);require(code==expected,(path,code,body));observed.append({'path':path,'status':code})
            return observed
        case('4.2 缺失/陈旧scope及跨工程房间拒绝',scopes)
        def sse():
            def reconnect():
                events=[]
                query=urllib.parse.urlencode({'workspace':s.workspace['workspace']})
                with urllib.request.urlopen(s.base+f'/api/chat/rooms/{room}/history-events?{query}',timeout=8) as response:
                    while len(events)<2:
                        line=response.readline().decode().strip()
                        if line.startswith('event:'):events.append(line[6:].strip())
                require(events==['hello','history-changed'],events);return events
            first=reconnect()
            with dbopen(s.db) as c:
                c.execute("UPDATE chat_room_messages SET content='断线期间已提交的新中文正文' WHERE id='reply-old'")
            second=reconnect()
            query=urllib.parse.urlencode({'workspace':s.workspace['workspace']})
            code,b=s.request(f'/api/chat/rooms/{room}/history-messages?{query}');require(code==200,b)
            messages=b.get('messages',[]);rows=[m for m in messages if m.get('id')=='reply-old']
            require(len(rows)==1 and rows[0]['content']=='断线期间已提交的新中文正文',b)
            expect_trace('reply-old','root-run');return {'first_events':first,'reconnect_events':second,'latest_body_once':True}
        case('4.2 SSE重连后重读已提交正文且定位保持',sse)
        def restart():
            s.stop();s.launch();expect_trace('reply-old','root-run');tree(9)
            return {'reply':'root-run','subtree_output_tokens':9}
        case('4.2 服务重启后归属与后代去重保持',restart)
    finally:s.stop()

def migration_matrix():
    s=Server('migration-v12',29462); before=v12(s)
    try:
        def upgrade():
            launch=s.launch(); current=snapshot(s.db)
            require(current['version']==29 and current['quick_check']=='ok',current['version'])
            saved=backups(s);require(len(saved)==1,[str(p) for p in saved]);require(snapshot(saved[0])==before,'迁移前备份不等于原库')
            with dbopen(s.db) as c:
                rows=c.execute('SELECT goal_id,id,title,status,created_at FROM goal_phases ORDER BY goal_id').fetchall()
            require(rows==[('g1','implement','实现登录','completed',100),('g2','implement','实现下单','completed',300)],rows)
            return {'from':12,'to':29,'ready_seconds':launch['ready_seconds'],'legacy_rows':rows,'backup_equal_to_old':True}
        case('6.2 v12复合同名阶段旧库真实发布服务升级v29',upgrade)
        def reopen():
            s.stop(); n=len(backups(s));s.launch();require(len(backups(s))==n)
            return {'backup_count_unchanged':n,'version':snapshot(s.db)['version']}
        case('6.2 重启幂等，不重复迁移备份',reopen)
        def compatible_rollback():
            def history():
                with dbopen(s.db) as c:
                    return {t:c.execute('SELECT * FROM '+t+' ORDER BY rowid').fetchall() for t in ['goals','goal_phases','sessions','chat_room_messages','session_messages']}
            s.stop(); base=history();s.launch(OLD);s.stop();actual=history()
            require(actual==base,'同schema旧二进制启动改变了历史')
            s.launch();require(history()==base,'再启135改变了历史')
            return {'old_binary':str(OLD),'old_binary_sha256':hashlib.sha256(OLD.read_bytes()).hexdigest(),'schema':29,'history_tables_equal':list(base),'boundary':'兼容schema29二进制回退，不是安装器降级或schema降级；写者epoch正常递增，不要求运维metadata逐字相同'}
        case('6.2 旧0.2.134兼容二进制回退后再启135',compatible_rollback)
    finally:s.stop()
    for index,kind in enumerate(['metadata-abort','backup-path-file','future-version']):
        f=Server('migration-'+kind,29463+index);v12(f)
        with dbopen(f.db) as c:
            if kind=='metadata-abort':c.executescript("CREATE TRIGGER fixture_reject_contract BEFORE INSERT ON metadata WHEN NEW.key='schema_contract_version' BEGIN SELECT RAISE(ABORT,'fixture拒绝最终契约写入'); END;")
            if kind=='future-version':c.execute('PRAGMA user_version=30')
        if kind=='backup-path-file':(f.root/'schema-backups').write_text('这是故障夹具文件，不是目录',encoding='utf-8')
        # 提供诱饵JSON：不得因为SQLite升级失败而用JSON覆盖已有数据。
        (f.root/'sessions.json').write_text('{"sessions":[],"fixture_only":"不可覆盖旧SQLite"}',encoding='utf-8')
        before=snapshot(f.db); raw=hashlib.sha256(f.db.read_bytes()).hexdigest()
        def failure_case(f=f,kind=kind,before=before,raw=raw):
            try:
                launch=f.launch(healthy=False);f.stop();require(snapshot(f.db)==before,'迁移故障后schema或旧行被修改')
                if kind=='future-version':require(hashlib.sha256(f.db.read_bytes()).hexdigest()==raw,'前向库文件字节改变')
                if kind=='metadata-abort':require(len(backups(f))==1 and snapshot(backups(f)[0])==before)
                return {'kind':kind,'exit_code':launch['exit_code'],'http_opened':False,'old_schema_rows_unchanged':True,'json_did_not_replace_database':True,'raw_bytes_unchanged':kind=='future-version'}
            finally:f.stop()
        case('6.2 故障关闭且旧库保留：'+kind,failure_case)
        if kind!='future-version':
            def recover(f=f,kind=kind):
                if kind=='metadata-abort':
                    with dbopen(f.db) as c:c.execute('DROP TRIGGER fixture_reject_contract')
                else:(f.root/'schema-backups').rename(f.root/'schema-backups.blocked-fixture')
                try:
                    f.launch();require(snapshot(f.db)['version']==29)
                    with dbopen(f.db) as c:require(c.execute('SELECT count(*) FROM goal_phases').fetchone()[0]==2)
                    return {'schema':29,'legacy_phase_count':2,'fault_removed_in_isolated_fixture':True}
                finally:f.stop()
            case('6.2 移除隔离故障后重新升级：'+kind,recover)

if __name__=='__main__':
    save()
    try:
        trace_matrix();migration_matrix()
    except Exception as error:result['harness_error']=repr(error)
    result['passed']=sum(c.get('state')=='passed' for c in result['cases'])
    result['failed']=sum(c.get('state')=='failed' for c in result['cases'])
    result['state']='completed' if not result['failed'] and 'harness_error' not in result else 'failed'
    result['finished_at']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
    print(json.dumps({k:result[k] for k in ['state','passed','failed']},ensure_ascii=False),flush=True)
    raise SystemExit(0 if result['state']=='completed' else 1)
