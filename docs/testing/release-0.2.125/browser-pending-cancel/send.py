"""唯一Devin原绑定的一次真实长程任务；SSE仅计事件名。"""
from pathlib import Path
import json,urllib.request,sqlite3,collections,time
root=Path(__file__).resolve().parents[2];p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
sid='session-1791131217833';room='room-1791131523339';base='http://127.0.0.1:8765'
marker='BU-PENDING-CANCEL125-20261010'
def get(path):
 with urllib.request.urlopen(base+path,timeout=30) as r:return json.load(r)
assert not (p/'submitted.json').exists()
installed=json.loads((root/'tmp/2026-10-09-large-text-preview/restored-formal125.json').read_text(encoding='utf-8-sig'))
assert installed['stage']=='0.2.125 Program Files正式安装版' and installed['port']==8765
log=root/'tmp/2026-10-04-devin-models/workspace/err.log'
(p/'diagnostic-start.json').write_text(json.dumps({'log_path':str(log),'offset':log.stat().st_size if log.exists() else 0}),encoding='utf-8')
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 assert not c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
 b=c.execute('SELECT remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(sid,)).fetchall()
 assert sum(x[0]=='island-kayak' for x in b)==1 and not any(x[1] for x in b)
s=get('/api/sessions/'+sid+'/model-settings');assert s['session']['model']=='swe-2-medium' and {'computer_use_perform','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab'}<=set(s['parameters']['tool_allowlist'])
perm=get('/api/chat/rooms/'+room+'/permissions');assert perm['full_access']
workspace=get('/api/workspace')
address=json.loads((p/'address.json').read_text(encoding='utf-8'))['url']
text=marker+'：正式0.2.125 Browser输入已释放、下一观察请求仍在等待时的取消验收。右栏已正常打开'+address+'。只调用一次computer_use_perform，surface=browser、省略target、max_actions=1，objective=点击交接观察验收按钮一次并读取新观察，success_criteria=[看到真实点击已完成，等待观察交接]，constraints=[只点击一次；取消后立即结束；不得补发或重试]。测试者会在点击sent/released且下一个观察请求真正登记后，通过正常聊天停止接口取消本轮。不要自行操控关闭或取消按钮，不调用其它工具/HTTP/脚本，不改文件/配置/权限，不创建新云会话；完整读取分页、正常wait；收到取消立即如实结束，保留原投递和释放事实，不报告目标达成，不补发新调用。'

payload={'expected_workspace_id':workspace['workspace_id'],'session_id':sid,'target_agent_ids':[sid],'chat_room_id':room,'text':text,'attachments':[],'native_browser_panel':True}
(p/'submitted.json').write_text(json.dumps({'submitted_ms':time.time()*1000,'payload':payload,'model':s['session']['model'],'config_revision':s['configuration_revision'],'binding':'island-kayak'},ensure_ascii=False,indent=2),encoding='utf-8')
req=urllib.request.Request(base+'/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
counts=collections.Counter()
with urllib.request.urlopen(req,timeout=1000) as r:
 for line in r:
  if line.startswith(b'event:'):
   event=line.decode().strip()[6:].strip();counts[event]+=1
   if event in ('started','error','done'):print(event,flush=True)
(p/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'event_counts':dict(counts)},indent=2),encoding='utf-8')
print(json.dumps(dict(counts)),flush=True)
