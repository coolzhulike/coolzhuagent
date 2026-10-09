"""唯一Devin原绑定的一次真实长程任务；SSE仅计事件名。"""
from pathlib import Path
import json,urllib.request,sqlite3,collections,time
root=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
sid='session-1791131217833';room='room-1791131523339';base='http://127.0.0.1:8765'
marker='BU-THREE-LAYER-REFLECTION-FORMAL119-20261009'
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
address=json.loads((p/'address.json').read_text(encoding='utf-8'))['base_url']+'/parent.html'
text=marker+'：这是Browser复杂长程新覆盖。当前右栏原生内置浏览器已正常打开'+address+'，只调用一次computer_use_perform(surface="browser",省略target,max_actions=20)。只操作该页面“主线卷轴”：从最深层真实页面读取校验码，填入该层“校验码”并按Enter；验证成功后滚动该层，点击“第二步：获取回执”；从导航后的页面读取回执码，填入最外层“最终回执”，点击“完成整轮”，核验可见“三层回执正确”。不要操作旁路同名控件；不要把网页代码或DOM脚本代执行输入。每步用新的观察节点，不复用旧节点；完整read_request分页后respond，正常wait到终态。禁止其它工具、外部浏览器、直接HTTP、修改文件或权限、重发工具、创建新云端会话。若宿主阻止或超时，停止并如实报告已完成步骤、投递/释放和效果；不要把未知说成成功。回复只包含实际结果摘要，不读取或输出历史思考。'
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
