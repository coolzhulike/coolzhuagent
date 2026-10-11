"""正常更新插件配置后，独立持有旧宿主句柄确认退出；不终止进程。"""
from pathlib import Path
exec(compile((Path(__file__).parent/'watch-prefix.py').read_text(encoding='utf-8'),str(Path(__file__).parent/'watch-prefix.py'),'exec'))
import sqlite3
root=folder.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
source=json.loads((root/'tmp/2026-10-08-plugin-reactivation/source.json').read_text(encoding='utf-8'))
settings_path=Path('C:/Users/zhupu/.claw/settings.json')
snapshot=json.loads(settings_path.read_text(encoding='utf-8'))['dshSnapshots'][before['plugin_id']]
assert snapshot['config']=={}
handles={};result={'version':'0.2.105','web_pid':receipt['web_pid']}
deadline=time.monotonic()+240
try:
    while time.monotonic()<deadline:
        for pid in children():
            if pid in handles:continue
            h=k.OpenProcess(0x101000,False,pid)
            if not h:continue
            image=ctypes.create_unicode_buffer(32768);length=w.DWORD(len(image));ts=[w.FILETIME() for _ in range(4)]
            if not k.QueryFullProcessImageNameW(h,0,image,ctypes.byref(length)) or Path(image.value)!=expected:
                k.CloseHandle(h);continue
            assert k.GetProcessTimes(h,*[ctypes.byref(x) for x in ts])
            with expected.open('rb') as stream:digest=hashlib.file_digest(stream,'sha256').hexdigest()
            info={'pid':pid,'path':image.value,'created_100ns':(ts[0].dwHighDateTime<<32)|ts[0].dwLowDateTime,
                  'sha256':digest,'held_ms':time.time()*1000,'wait_before':k.WaitForSingleObject(h,0)}
            handles[pid]=(h,info)
        log=folder/'slow-events.jsonl'
        events=[json.loads(line) for line in log.read_text(encoding='utf-8').splitlines()] if log.exists() else []
        entered=next((x for x in events if x['event']=='function_network_entered'),None)
        if entered:
            live=[(h,info) for h,info in handles.values() if k.WaitForSingleObject(h,0)==258]
            assert len(live)==1,[(info,k.WaitForSingleObject(h,0)) for h,info in handles.values()]
            h,info=live[0]
            with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
                observed=c.execute("SELECT r.id,a.attempt_id,a.state FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id JOIN json_each(e.payload_json,'$.message_ids') refs JOIN chat_room_messages m ON m.id=refs.value JOIN devin_acp_attempts a ON json_extract(a.scope_json,'$.run_id')=r.id WHERE e.event_type='chat.source_messages' AND m.content LIKE ? ORDER BY m.created_at DESC LIMIT 1",('PLUGIN-CONFIG-INFLIGHT-105-20261008%',)).fetchone()
            assert observed and observed[2]=='submitted',observed
            body={'expected_workspace':'ws-23f646a969206cb4','id':before['plugin_id'],
                  'expected_source_sha256':source['source_sha256'],'session_id':'session-1791131217833',
                  'chat_room_id':'room-1791131523339','config':{'coolzhuAcceptanceTag':'config-inflight-105-20261008'}}
            observation={'run_id':observed[0],'attempt_id':observed[1],'attempt_state_at_change':observed[2],
                         'original_activation_id':snapshot['activation_id'],'original_config':snapshot['config'],
                         'request_begin_ms':time.time()*1000,'network_entered':entered}
            req=urllib.request.Request('http://127.0.0.1:8765/api/extension-market/dsh/enable',json.dumps(body).encode(),{'Content-Type':'application/json'})
            with urllib.request.urlopen(req,timeout=135) as response:enabled=json.load(response)
            assert enabled['enabled'] is True
            changed=json.loads(settings_path.read_text(encoding='utf-8'))['dshSnapshots'][before['plugin_id']]
            assert changed['config']==body['config'] and changed['activation_id']!=snapshot['activation_id']
            assert hashlib.sha256(Path(before['entry_path']).read_bytes()).hexdigest()==before['entry_sha256']
            observation.update({'request_end_ms':time.time()*1000,'changed_activation_id':changed['activation_id'],
                                'changed_config':changed['config'],'source_unchanged':True})
            (folder/'config-observation.json').write_text(json.dumps(observation,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
            result.update({'observation':observation,'held_process':info,'enable_response':enabled})
            result['wait_after']=k.WaitForSingleObject(h,8000);result['wait_finished_ms']=time.time()*1000
            assert info['wait_before']==258 and result['wait_after']==0
            result['passed_scope']='真实网络函数进入后正常重新配置；原宿主独立持有句柄Wait258→0，没有测试脚本终止进程'
            print(json.dumps({'passed_scope':result['passed_scope'],'pid':info['pid'],'wait_after':result['wait_after']},ensure_ascii=False),flush=True)
            break
        time.sleep(.015)
    else:raise RuntimeError('未观察到真实网络函数进入，不计通过')
except BaseException as error:
    result['failure']=str(error);raise
finally:
    result['captured_processes']=[info for _,info in handles.values()]
    for handle,_ in handles.values():k.CloseHandle(handle)
    (folder/'held-process-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
