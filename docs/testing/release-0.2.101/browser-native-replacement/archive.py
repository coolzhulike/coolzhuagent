"""归档正式实操原事实，严格窗口未命中不追认通过。"""
from pathlib import Path
import json, sqlite3, shutil
folder=Path(__file__).resolve().parent
root=folder.parents[1]
target=root/'docs/testing/release-0.2.101/browser-native-replacement'
target.mkdir(parents=True,exist_ok=True)
facts=json.loads((folder/'latest-status.json').read_text(encoding='utf-8'))
assert facts['run']['state']=='failed'
cu=facts['computer_use'];assert len(cu)==1 and cu[0]['state']=='blocked'
assert cu[0]['terminal_result_json']['error']['code']=='native_browser_resource_changed'
assert len(cu[0]['steps'])==1 and cu[0]['steps'][0]['input_delivery']=='sent' and cu[0]['steps'][0]['input_release_status']=='released'
assert facts['attempts'][0]['state']=='terminal' and facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert not any(b['locked_attempt'] for b in facts['bindings'])
events=[json.loads(line) for line in (folder/'events.jsonl').read_text(encoding='utf-8').splitlines()]
down=next(e for e in events if e['role']=='OLD' and e['kind']=='pointerdown')
up=next(e for e in events if e['role']=='OLD' and e['kind']=='pointerup')
click=next(e for e in events if e['role']=='OLD' and e['kind']=='click')
loaded=next(e for e in events if e['role']=='NEW' and e['kind']=='loaded')
assert all(e['trusted'] for e in [down,up,click,loaded])
assert down['server_ns']<up['server_ns']<loaded['server_ns']
new_inputs=[e for e in events if e['role']=='NEW' and e['kind'] in ['pointerdown','pointerup','click','input','keydown']]
assert not new_inputs
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as conn:
    conn.row_factory=sqlite3.Row
    source=conn.execute("SELECT created_at FROM chat_room_messages WHERE room_id=? AND role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",('room-1791131523339','BU-NATIVE-REPLACEMENT-101-20261008%')).fetchone()
    reply=dict(conn.execute("SELECT id,role,kind,content,created_at FROM chat_room_messages WHERE room_id=? AND created_at>=? AND kind='assistant-reply' ORDER BY created_at LIMIT 1",('room-1791131523339',source['created_at'])).fetchone())
    calls=[dict(r) for r in conn.execute('SELECT tool_name,status,tool_call_id FROM tool_calls WHERE run_id=?',(facts['run']['id'],))]
    after={'attempts':conn.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0],'messages':conn.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0]}
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
assert after['attempts']==before['counts']['attempts']+1 and len(calls)==1 and calls[0]['tool_name']=='computer_use_perform'
result={'formal_version':'0.2.101','runtime':facts,'before':before,'after':after,'tool_calls':calls,'visible_reply':reply,'strict_replacement_window':'not_hit; new document loaded after old pointerup','old_down_to_up_server_ms':(up['server_ns']-down['server_ns'])/1e6,'old_up_to_new_loaded_server_ms':(loaded['server_ns']-up['server_ns'])/1e6,'new_document_input_events':new_inputs,'bug':'Final observation error text incorrectly says no input despite sent/released step and trusted old DOM click. Candidate changes observation wording only; not installed.'}
(target/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
for name in ['server.py','status.py','send.py','archive.py','address.json','before.json','submitted-request.json','events.jsonl','ui-action-times.json','stream-finished.json','new-view-during-run.jpg','101-final-visible-reply.jpg']:
    shutil.copy2(folder/name,target/name)
print(json.dumps({'strict_window':result['strict_replacement_window'],'down_up_ms':result['old_down_to_up_server_ms'],'up_new_load_ms':result['old_up_to_new_loaded_server_ms'],'run':facts['run']['state'],'steps':cu[0]['steps'],'new_inputs':len(new_inputs),'attempts_added':after['attempts']-before['counts']['attempts']},ensure_ascii=False))
