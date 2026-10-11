"""唯一Devin原绑定的一次真实长程任务；SSE仅计事件名。"""
from pathlib import Path
import json,urllib.request,sqlite3,collections,time
root=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
sid='session-1791131217833';room='room-1791131523339';base='http://127.0.0.1:8765'
marker='BU-NON-TARGET-LIVE-REPAINT-CANDIDATE-20261009'
def get(path):
 with urllib.request.urlopen(base+path,timeout=30) as r:return json.load(r)
assert not (p/'submitted.json').exists()
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 assert not c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
 b=c.execute('SELECT remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(sid,)).fetchall()
 assert sum(x[0]=='island-kayak' for x in b)==1 and not any(x[1] for x in b)
s=get('/api/sessions/'+sid+'/model-settings');assert s['session']['model']=='swe-2-medium' and 'computer_use_perform' in s['parameters']['tool_allowlist']
perm=get('/api/chat/rooms/'+room+'/permissions');assert perm['full_access']
workspace=get('/api/workspace')
address=json.loads((p/'address.json').read_text(encoding='utf-8'))['url']
text=marker+'：这是动态页面非目标变化长程验收。当前右栏原生内置浏览器已正常打开'+address+'。只调用一次computer_use_perform(surface="browser",省略target,max_actions=16)，完成页面“竹林动态订单”的三步流程：每步从当前真实页面读取新的订单码，填入“订单码”并按Enter验证；前两步验证通过后点击“下一步”；第三步按Enter，核验可见“整单已完成，三步均通过”。行情/公告区域持续刷新，不需操作。每步根据当前新鲜控件，不能复用被替换的表单引用，不能直接HTTP或脚本操作输入。完整read_request分页后respond、wait到真实终态。禁止外部浏览器、其它工具、修改文件或权限、重发工具、创建云端会话；宿主阻止或超时立即如实报告输入投递/释放及效果，未知保持未知，不读取或输出历史思考。'
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
