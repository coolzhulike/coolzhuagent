"""归档本轮真实事实；模型误述单列，不读取思考。"""
from pathlib import Path
import json,sqlite3
folder=Path(__file__).resolve().parent;root=folder.parents[1]
facts=json.loads((folder/'latest-status.json').read_text(encoding='utf-8'))
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
 c.row_factory=sqlite3.Row
 source=c.execute("SELECT created_at FROM chat_room_messages WHERE room_id=? AND role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",('room-1791131523339','BU-RESOURCE-107-20261008%')).fetchone()
 reply=dict(c.execute("SELECT id,kind,content,created_at FROM chat_room_messages WHERE room_id=? AND created_at>=? AND kind='assistant-reply' ORDER BY created_at LIMIT 1",('room-1791131523339',source['created_at'])).fetchone())
 calls=[dict(r) for r in c.execute('SELECT tool_name,status,tool_call_id FROM tool_calls WHERE run_id=?',(facts['run']['id'],))]
 after={'attempts':c.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0],'messages':c.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0]}
events=[json.loads(l) for l in (folder/'events.jsonl').read_text(encoding='utf-8').splitlines()]
up=next(e for e in events if e.get('role')=='OLD' and e['kind']=='pointerup')
loaded=next(e for e in events if e.get('role')=='NEW' and e['kind']=='loaded')
before=json.loads((folder/'before.json').read_text(encoding='utf-8'))
assert facts['run']['state']=='failed' and len(calls)==1
assert len(facts['computer_use'])==1
cu=facts['computer_use'][0]
assert cu['terminal_result_json']['error']['code']=='native_browser_resource_changed'
projection=cu['terminal_result_json']['input_steps']
assert len(projection)==1 and projection[0]['input_delivery']=='sent' and projection[0]['input_release_status']=='released'
assert len(cu['steps'])==1 and cu['steps'][0]['input_delivery']=='sent' and cu['steps'][0]['input_release_status']=='released'
assert after['attempts']==before['attempts']+1
assert facts['attempts'][0]['protocol_stop']=='end_turn' and facts['attempts'][0]['process_drained']==1
assert not any(b['locked_attempt'] for b in facts['bindings'])
assert not [e for e in events if e.get('role')=='NEW' and e['kind'] in ('click','pointerdown','pointerup','input','keydown')]
result={'formal_version':'0.2.107','facts':facts,'before':before,'after':after,'calls':calls,'visible_reply':reply,'up_to_new_loaded_page_ms':loaded['page_ms']-up['page_ms'],'strict_down_up_window':'not_tested; navigation deliberately after trusted click','passed':'等待期间资源变化明确返回native_browser_resource_changed；已发送/释放保留，效果未知，零补发、新页零输入','open':'newTarget/跨来源commit严格按下窗口与其它总体矩阵仍开放'}
(folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:result[k] for k in ('formal_version','up_to_new_loaded_page_ms','passed','open')},ensure_ascii=False))
