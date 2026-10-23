"""以明确标识的合成数据验证候选真实HTTP路由，不写生产库或调用模型。"""
import datetime, hashlib, json, os, pathlib, socket, sqlite3, subprocess, time, urllib.error, urllib.parse, urllib.request

root = pathlib.Path(r'D:\coolzhuagent')
out = root / 'tmp/goal-32wbs-acceptance-20261013'
fixture = out / 'reply-http-20261016'
fixture.mkdir(exist_ok=True)
receipt = out / 'controller-reply-http-20261016.json'
exe = out / 'cargo-target/debug/coolzhu-web-console.exe'
base = 'http://127.0.0.1:29456'
data = {'started_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'state':'running', 'boundary':'隔离候选HTTP路由+合成回复/归属数据；不代表正式安装UI或真实模型执行验收', 'exe':str(exe), 'exe_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(), 'cases':[], 'production_database_touched':False, 'model_calls':0}
def save(): receipt.write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8')
def request(path, body=None):
    req = urllib.request.Request(base+path,data=None if body is None else json.dumps(body).encode(),headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=10) as response: return response.status,json.load(response)
    except urllib.error.HTTPError as error: return error.code,json.load(error)
def check(name,path,expected,run=None):
    status,body=request(path)
    case={'name':name,'path':path,'expected':expected,'actual':status}
    if status == 200:
        case['run_ids']=[r['run_id'] for r in body['runs']]
        case['requested_message_id']=body['requested_message_id']
        case['has_more']=body['has_more']
        assert case['run_ids']==[run] and not body['has_more']
    data['cases'].append(case)
    save()
    assert status==expected,case
sock=socket.socket()
try:
    sock.bind(('127.0.0.1',29456))
finally:
    sock.close()
(fixture/'coolzhu.toml').write_text('[web]\nbind_addr="127.0.0.1:29456"\n[pet]\nenabled=false\n[scheduled_tasks]\ntasks=[]\n',encoding='utf-8')
env=dict(os.environ)
env.update(COOLZHU_RUNTIME_DIR=str(fixture),COOLZHU_WEB_SESSION_DB=str(fixture/'sessions.sqlite3'),COOLZHU_WEB_SESSION_STORE=str(fixture/'sessions.json'),COOLZHU_WEB_ATTACHMENT_STORE=str(fixture/'attachments'),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(fixture/'input-safety'),COOLZHU_DESKTOP_SHELL_MANAGED='1')
save()
with (fixture/'server.log').open('wb') as log:
    proc=subprocess.Popen([str(exe)],cwd=fixture,env=env,stdout=log,stderr=subprocess.STDOUT)
    data['pid']=proc.pid
    save()
    try:
        for _ in range(60):
            try:
                status,workspace=request('/api/workspace')
                if status==200: break
            except (OSError,TimeoutError): pass
            if proc.poll() is not None: raise RuntimeError('隔离服务提前退出')
            time.sleep(1)
        else: raise TimeoutError('隔离服务未就绪')
        data['workspace_response_keys']=list(workspace)
        # SQLite中的元数据/配置不参与期待值猜测；按产品同一规范路径散列取得fixture作用域。
        normalized=('\\\\?\\'+str(fixture.resolve())).replace('\\','/').lower().encode()
        h=0xcbf29ce484222325
        for b in normalized: h=((h^b)*0x100000001b3)&0xffffffffffffffff
        ws=f'ws-{h:016x}'
        # workspace HTTP若已返回标识则优先采用宿主事实。
        ws=workspace.get('workspace_id',ws)
        status,created=request('/api/chat/rooms',{'name':'合成回复定位HTTP验收，非模型回复'})
        assert status==200,(status,created)
        room=created['room']['id']
        status,other=request('/api/chat/rooms',{'name':'跨房间拒绝夹具'})
        assert status==200
        other_room=other['room']['id']
        db=sqlite3.connect(fixture/'sessions.sqlite3',timeout=5)
        for run,w,r in [('owned-old',ws,room),('unrelated-new',ws,room),('foreign-workspace','ws-foreign',room),('foreign-room',ws,other_room)]:
            db.execute("INSERT INTO runtime_runs(id,kind,workspace_id,session_id,chat_room_id,state,claim_token,created_at) VALUES(?,'chat_turn',?,'fixture-session',?,'completed','fixture-only',1)",(run,w,r))
        for msg in ['owned-reply','foreign-workspace-reply','foreign-room-reply','unmapped-reply']:
            db.execute("INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES(?,?,'合成夹具','assistant','chat','明确的合成测试数据','assistant-reply','[]',1)",(room,msg))
        for run,msg in [('owned-old','owned-reply'),('foreign-workspace','foreign-workspace-reply'),('foreign-room','foreign-room-reply')]:
            db.execute("INSERT INTO runtime_run_events(run_id,event_type,payload_json,created_at) VALUES(?,'chat.context_outputs',?,1)",(run,json.dumps({'messages':[{'id':msg}]})))
        db.commit()
        data['fixture_workspace_id']=ws
        path=f'/api/chat/rooms/{room}/trace'
        check('唯一归属旧回复，不能返回无关新运行',path+'?message_id=owned-reply',200,'owned-old')
        for msg in ['foreign-workspace-reply','foreign-room-reply','unmapped-reply','missing-reply']:
            check(msg,path+'?message_id='+msg,404)
        check('跨房间查询',f'/api/chat/rooms/{other_room}/trace?message_id=owned-reply',404)
        check('冲突筛选',path+'?message_id=owned-reply&run_id=unrelated-new',400)
        check('禁止分页混用',path+'?message_id=owned-reply&before=1',400)
        db.execute("INSERT INTO runtime_run_events(run_id,event_type,payload_json,created_at) VALUES('unrelated-new','chat.context_outputs',?,1)",(json.dumps({'messages':[{'id':'owned-reply'}]}),))
        db.commit()
        check('同回复多重归属不选最新',path+'?message_id=owned-reply',409)
        db.execute("DELETE FROM chat_room_messages WHERE id='owned-reply'")
        db.commit()
        check('已删除回复不得直达',path+'?message_id=owned-reply',404)
        data['database_quick_check']=db.execute('PRAGMA quick_check').fetchone()[0]
        db.close()
        data['state']='completed'
    except Exception as error:
        data['state']='failed'
        data['error']=repr(error)
    finally:
        proc.terminate()
        try: proc.wait(timeout=10)
        except subprocess.TimeoutExpired: proc.kill();proc.wait()
        data['process_exited']=proc.poll() is not None
        data['finished_at_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat()
        save()
print(json.dumps(data,ensure_ascii=False))
