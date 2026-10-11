"""唯一Devin原绑定的一次真实长程任务；SSE仅计事件名。"""
from pathlib import Path
import json,urllib.request,sqlite3,collections,time
root=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
sid='session-1791131217833';room='room-1791131523339';base='http://127.0.0.1:8765'
marker='BU-NON-TARGET-REPAINT-NEGATIVE-INSTALLED123-20261009'
def get(path):
 with urllib.request.urlopen(base+path,timeout=30) as r:return json.load(r)
assert not (p/'submitted.json').exists()
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
text=marker+'：这是中间进展归因的有界负例，不是人物绘图或重复基础连通。当前右栏原生内置浏览器已正常打开'+address+'。仅调用一次computer_use_perform(surface="browser",省略target,max_actions=4)。目标是让页面明确显示“目标已完成”：使用每次新观察中可见的“尝试完成”按钮，检查目标；如第一步未完成，可以使用新观察再尝试一次，最多两次按钮点击。目标文字未出现则如实失败，不再尝试任何第三次输入。忽略每100ms刷新行情，行情变化不证明按钮生效；若宿主按连续无进展收尾，原样报告。成功标准必须是实际可见的完整“目标已完成”，不得引用“目标未完成”或行情数字作为成功。完整read_request分页后respond并wait到真实终态。仅本工具一次，不用计算器或其它工具，不脚本/HTTP操作网页，不修改配置/文件/权限，不新建云端会话；不读取或输出历史思考。'

text+=' 工具请求必须使用这些正式字段：'+json.dumps({'objective':'使页面目标完成','surface':'browser','success_criteria':['页面完整显示目标已完成'],'constraints':['只点击尝试完成按钮，最多两次，未完成即停止','忽略动态行情'],'max_actions':4},ensure_ascii=False)+'；不要使用task或instruction字段。仅一次工具调用，参数错误也立即如实结束，不重试、不补发。'
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
