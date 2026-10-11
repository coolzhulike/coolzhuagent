from pathlib import Path
p=Path(__file__).resolve().parent
text=(p/'reproduce.py').read_text(encoding='utf-8')
prefix=text.split('try:\n    for _ in range(150):')[0]
prefix=prefix.replace('import hashlib,json,os,socket,subprocess,threading,time,urllib.request,urllib.error,sys','import hashlib,json,os,socket,sqlite3,subprocess,threading,time,urllib.request,urllib.error,sys')
prefix=prefix.replace('def api(path,data=None):','def api(path,data=None,headers=None):').replace("headers={'Content-Type':'application/json'})", "headers={'Content-Type':'application/json',**(headers or {})})")
body='''
path=f'/api/sessions/{sid}/model-settings'
def ready():
    for _ in range(150):
        assert process.poll() is None
        try:
            if api('/api/sessions')[0]==200:return
        except OSError:pass
        time.sleep(.1)
    raise RuntimeError('未就绪')
def scope_headers():
    status,w=api('/api/workspace');assert status==200
    return {'X-Coolzhu-Workspace-Id':w['workspace_id'],'X-Coolzhu-Configuration-Scope':w['configuration_scope']}
def record(label,status,response):
    facts.setdefault('checks',[]).append({'label':label,'status':status,'response':response})
def attempt(label,endpoint,payload,headers,expected):
    status,response=api(endpoint,payload,headers);record(label,status,response);assert status==expected,(label,status,response)
def store_snapshot(ws):
    with sqlite3.connect(ws/'.coolzhu/web-sessions.sqlite3') as conn:
        data=conn.execute("SELECT name,sql FROM sqlite_master WHERE type='table'").fetchall()
        assert conn.execute('SELECT COUNT(*) FROM runtime_runs').fetchone()[0]==0
        result={name:conn.execute('SELECT * FROM '+name).fetchall() for name,_ in data if name in ['sessions','web_sessions']}
        assert result,'未找到会话表，不能凭HTTP断言存储'
    return {'config':(ws/'coolzhu.toml').read_bytes().hex(),'sessions':result}
try:
    ready()
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    assert api('/api/workspace',{'path':str(spaces[0])})[0]==200
    old=scope_headers();a=api(path,headers=old)[1]
    assert a['configuration_scope']==old['X-Coolzhu-Configuration-Scope']
    params=dict(a['parameters']);params['temperature']=.35
    payload={'session':{'name':'工程-a-作用域保存'},'parameters':params,'expected_revision':10}
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    b=api(path)[1];assert b['session']['name']=='工程-b' and b['configuration_revision']==10
    before_b=store_snapshot(spaces[1])
    attempt('旧A配置保存迟到B',path,payload,old,409)
    attempt('旧A配置读取迟到B',path,None,old,409)
    attempt('只有旧工程ID的首次读取',path,None,{'X-Coolzhu-Workspace-Id':old['X-Coolzhu-Workspace-Id']},409)
    attempt('旧A容量保存迟到B',f'/api/sessions/{sid}/model-limit',{'context_window':4096,'max_output_tokens':512},old,409)
    attempt('旧A新建请求迟到B','/api/sessions',{'name':'不得创建','provider':'custom','model':'scope-local-config'},old,409)
    assert before_b==store_snapshot(spaces[1]),'错误请求写入B配置或SQLite'
    facts['other_workspace_bytes_and_rows_unchanged']=True
    assert api('/api/workspace',{'path':str(spaces[0])})[0]==200
    current=scope_headers();assert current!=old
    attempt('A→B→A旧标识',path,payload,old,409)
    assert api('/api/workspace/reload',{})[0]==200
    after_reload=scope_headers();assert after_reload!=current
    attempt('同工程重载后旧标识',path,payload,current,409)
    # 失败目录选择不作废当前页。
    attempt('不存在的工程目录','/api/workspace',{'path':str(folder/'does-not-exist')},None,400)
    assert scope_headers()==after_reload
    invalid=dict(params);invalid['temperature']=3
    attempt('参数非法且不泄漏pin',path,{'parameters':invalid,'expected_revision':10},after_reload,400)
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    assert api('/api/workspace',{'path':str(spaces[0])})[0]==200
    current=scope_headers()
    attempt('同工程旧配置版本',path,{'parameters':params,'expected_revision':0},current,409)
    attempt('无头旧容量读取兼容',f'/api/sessions/{sid}/model-limit',None,None,200)
    attempt('当前工程正常配置保存',path,payload,current,200)
    assert scope_headers()==current,'配置保存不应作废其它同工程页'
    saved=api(path,headers=current)[1];assert saved['session']['name']=='工程-a-作用域保存' and saved['parameters']['temperature']==.35
    attempt('保存后旧revision冲突',path,payload,current,409)
    attempt('当前工程新建会话','/api/sessions',{'name':'同工程新建','provider':'custom','model':'scope-local-config'},current,200)
    attempt('无头既有统一配置读取兼容',path,None,None,200)
    facts['before_restart']=saved
    process.terminate();process.wait(timeout=15);facts['first_process_exit_code']=process.returncode
    process=subprocess.Popen([str(binary)],cwd=spaces[0],env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
    ready();restarted=scope_headers();assert restarted!=current
    attempt('程序重启后旧标识',path,payload,current,409)
    restored=api(path,headers=restarted)[1]
    assert restored['parameters']==saved['parameters'] and restored['configuration_revision']==saved['configuration_revision'] and restored['session']['name']==saved['session']['name']
    facts['after_restart']=restored;facts['restart_preserved_parameters_revision_name']=True
    assert api('/api/workspace',{'path':str(spaces[1])})[0]==200
    facts.update(all_api_checks_passed=True,pid=process.pid)
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\\n',encoding='utf-8')
    (folder/'ready.json').write_text(json.dumps({'pid':process.pid,'port':port,'workspace':str(spaces[1]),'binary':str(binary),'binary_sha256':facts['binary_sha256']},ensure_ascii=False)+'\\n',encoding='utf-8')
    print(json.dumps({'stage':'真实API和存储验证通过，等待原生界面','port':port,'checks':len(facts['checks']),'model_requests':0},ensure_ascii=False),flush=True)
    while not(folder/'stop').exists():assert process.poll() is None;time.sleep(.2)
finally:
    if process.poll() is None:process.terminate()
    process.wait(timeout=15);out.close();err.close()
    if oldgui is None:
        if gui.exists():gui.unlink()
    else:gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\\n',encoding='utf-8')
'''
(p/'verify.py').write_text(prefix+body,encoding='utf-8')
print('已准备真实API/SQLite/重启驱动；零模型，正式原工程不变')
