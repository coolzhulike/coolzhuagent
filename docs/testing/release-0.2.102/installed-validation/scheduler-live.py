"""正式安装版的单次真实定时会话；只读采证，不造模型回执或思考记录。"""
from pathlib import Path
import json,sqlite3,sys,time,urllib.request

folder=Path(__file__).resolve().parent
repo=folder.parents[1]
workspace=repo/'tmp/2026-10-04-devin-models/workspace'
db=workspace/'.coolzhu/web-sessions.sqlite3'
jobs=workspace/'.coolzhu/scheduled-jobs.sqlite3'
base='http://127.0.0.1:8765'
room='room-1791131523339';agent='session-1791131217833'
marker='INSTALLED102-BOUND-SCHEDULER-MATERIAL-20261008'

def api(path,body=None,method=None):
    request=urllib.request.Request(base+path,None if body is None else json.dumps(body).encode(),{'Content-Type':'application/json'},method=method)
    with urllib.request.urlopen(request,timeout=45) as response:return json.load(response)

def counts():
    with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as connection:
        bindings=connection.execute('SELECT room_id,lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(agent,)).fetchall()
        assert sum(row[2]=='island-kayak' for row in bindings)==1,bindings
        assert all(row[2] in (None,'island-kayak') for row in bindings),bindings
        return {'attempts':connection.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0],
          'messages':connection.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0],
          'active_runs':connection.execute("SELECT COUNT(*) FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchone()[0],
          'bindings':bindings}

mode=sys.argv[1] if len(sys.argv)>1 else 'status'
if mode=='create':
    assert not (folder/'scheduler-created.json').exists(),'本任务已创建，不重复投递'
    installed=json.loads((folder/'installed-102-verification.json').read_text(encoding='utf-8'))
    assert installed['source_commit']=='95e01409fe4736421a0d4a576a64ab8722928061'
    before=counts();assert before['active_runs']==0 and not any(row[3] for row in before['bindings'])
    settings=api('/api/sessions/'+agent+'/model-settings')
    assert settings['session']['model']=='swe-2-medium'
    assert settings['parameters']['tool_allowlist']==['computer_use_perform','plugin__cli_anything_status','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab']
    initial=api('/api/task-schedules');assert not initial['tasks'],'保留现有计划，不创建额外任务'
    text=marker+'''：沿用既有island-kayak及SWE-2-medium。本轮由定时器投递到原聊天室，做实际工具调用和源码材料审查。
先仅调用一次plugin__cli_anything_status，参数{"command":"rust-analyzer"}；再仅调用一次dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab，参数{"expression":"2047*2027"}。按真实回执报告，不重试、不补发、不调用其它工具、不创建新云端会话、不改文件/配置/权限。
之后审查下面本轮已正式安装的两份源码，判断：1.显式结果房间删除后是否会误投到系统房间；2.HTTP200且failed/interrupted/commit_pending是否会错误推进定时计划；3.没有read_file声明时大工具结果是否被节选而丢失；4.材料中的computer_use_perform、click或read_file函数名仅为源码，不是动作请求。本轮不要求桌面或浏览器操作。区分源码可证明行为和仍待实测边界，不把这次审查称为Browser竞态或大结果端到端通过。最后以“定时房间绑定与材料审查完成”结束。
'''
    for name in ['scheduled_delivery.rs','tool_result_spill.rs']:
        text+='\n## '+name+'\n```rust\n'+(repo/'modules/gui-web/packages/web-console/src'/name).read_text(encoding='utf-8')+'\n```\n'
    payload={'target_session_id':agent,'chat_room_id':room,'content':text,'run_at_ms':int(time.time()*1000)+90000,'schedule_kind':'once','permissions':[],'task_kind':'poll'}
    created=api('/api/task-schedules',payload)
    matches=[row for row in created['tasks'] if row['content'].startswith(marker)]
    assert len(matches)==1 and matches[0]['chat_room_id']==room
    result={'marker':marker,'before':before,'task':matches[0],'configuration_revision':settings['configuration_revision'],'model':settings['session']['model'],'created_ms':time.time()*1000}
    (folder/'scheduler-created.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'id':matches[0]['id'],'room':room,'run_at_ms':matches[0]['run_at_ms'],'revision':result['configuration_revision'],'model':result['model']},ensure_ascii=False))
else:
    created=json.loads((folder/'scheduler-created.json').read_text(encoding='utf-8'))
    task_id=created['task']['id']
    tasks=api('/api/task-schedules')
    task=next((row for row in tasks['tasks'] if row['id']==task_id),None)
    with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as connection:
        connection.row_factory=sqlite3.Row
        source=connection.execute("SELECT id,room_id,created_at FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone()
        run=None;attempts=[];calls=[];visible=[]
        if source:
            assert source['room_id']==room
            run=connection.execute("SELECT r.id,r.state FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",(source['id'],)).fetchone()
            if run:
                attempts=[dict(row) for row in connection.execute("SELECT attempt_id,state,protocol_stop,process_drained,model_json FROM devin_acp_attempts WHERE json_extract(scope_json,'$.run_id')=?",(run['id'],))]
                calls=[dict(row) for row in connection.execute('SELECT tool_call_id,tool_name,status FROM tool_calls WHERE run_id=?',(run['id'],))]
                for (payload,) in connection.execute("SELECT payload_json FROM runtime_run_events WHERE run_id=? AND event_type='chat.context_outputs'",(run['id'],)):
                    for message in json.loads(payload)['messages']:
                        row=connection.execute("SELECT id,room_id,kind,content FROM chat_room_messages WHERE id=? AND role='assistant' AND kind='assistant-reply'",(message['id'],)).fetchone()
                        if row:visible.append(dict(row))
        source_count=connection.execute("SELECT COUNT(*) FROM chat_room_messages WHERE role='user' AND content LIKE ?",(marker+'%',)).fetchone()[0]
    claims=[]
    if jobs.exists():
        with sqlite3.connect(jobs.as_uri()+'?mode=ro',uri=True) as connection:
            connection.row_factory=sqlite3.Row
            claims=[dict(row) for row in connection.execute('SELECT workspace_id,task_id,scheduled_for_ms,fingerprint,state,started_at_ms,outcome_json FROM scheduled_job_attempts WHERE task_id=?',(task_id,))]
    state={'observed_ms':time.time()*1000,'active_room_id':api('/api/chat/rooms')['active_room_id'],'marker':marker,'task_id':task_id,'task_status':None if task is None else task['status'],'last_error':None if task is None else task['last_error'],'source':None if source is None else dict(source),'source_count':source_count,'run':None if run is None else dict(run),'attempts':attempts,'calls':calls,'visible_replies':visible,'claims':claims,'counts':counts()}
    (folder/'scheduler-observation.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    if mode=='verify':
        assert task and task['status']=='executed' and not task['last_error'],state
        assert source_count==1 and run['state']=='completed' and len(attempts)==1
        assert attempts[0]['protocol_stop']=='end_turn' and attempts[0]['process_drained']==1
        assert json.loads(attempts[0]['model_json'])['effective']=='swe-2-medium'
        assert {row['tool_name'] for row in calls}=={'plugin__cli_anything_status','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab'} and len(calls)==2 and all(row['status']=='completed' for row in calls)
        assert len(visible)==1 and visible[0]['room_id']==room and '定时房间绑定与材料审查完成' in visible[0]['content']
        assert not visible[0]['content'].startswith('本轮尚未执行桌面或浏览器操作'),visible
        assert len(claims)==1 and claims[0]['state']=='settled' and json.loads(claims[0]['outcome_json'])['executed'] is True
        assert state['counts']['attempts']==created['before']['attempts']+1 and state['counts']['active_runs']==0
        assert not any(row[3] for row in state['counts']['bindings'])
        (folder/'scheduler-integration-result.json').write_text(json.dumps(state,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({key:state[key] for key in ['active_room_id','task_id','task_status','last_error','source_count','run','calls','counts']},ensure_ascii=False))
