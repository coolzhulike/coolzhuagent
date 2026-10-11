"""唯一Devin原绑定的一次真实长程任务；SSE仅计事件名。"""
from pathlib import Path
import json,urllib.request,sqlite3,collections,time
root=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
sid='session-1791131217833';room='room-1791131523339';base='http://127.0.0.1:8765'
marker='BU-SCROLL-DOCUMENT-IDENTITY-INSTALLED123-20261009'
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
text=marker+'：这是双子文档滚动、AX序号漂移、订单提交和DSH工具链长程验收。当前右栏原生内置浏览器已打开'+address+'。沿用当前唯一Devin云端会话，只调用一次computer_use_perform。网页包含竹剑订单与玉佩订单两个独立内嵌文档，每个表单在自己的文档底部：先按当前新观察中本订单的RootWebArea引用向下滚动（amount=3），使订单码输入框实际可见；从该订单当前真实页面读取随机码，click该输入框，text_input填入对应订单码，然后key_combination按enter提交。再处理另一个文档，不能滚动外层或借另一个文档的引用。滚动后外层会插入回执文字造成AX序号漂移，引用必须每步重新观察；无需操作行情。目标是外层可见“双文档订单已完成，两项均通过”以及两项单价与数量回执，不要从投递ACK推导成功。工具正式参数为{"objective": "完成竹剑与玉佩两个子文档的滚动、真实订单码填写和提交", "surface": "browser", "success_criteria": ["外层完整显示双文档订单已完成，两项均通过", "外层两项回执明确列出各自物品、单价及数量"], "constraints": ["只操作两个订单子文档，各先滚动至真实输入框后填写随机订单码按enter", "忽略动态行情；每步使用最新观察引用；禁止脚本或HTTP操作网页"], "max_actions": 14}。省略target，使用objective字段，不使用task或instruction。CU只能调用一次；成功后仅根据本轮工具证据调用一次DSH计算器 dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab，计算两项单价乘数量的总和，并在最终可见回复列出逐项算式和总额。CU失败、阻止、参数错误或超时则如实收尾，不重试、不补发、不猜数字，不调用计算器。完整read_request分页后respond，wait到真实终态。禁止其它工具、外部浏览器、修改文件/配置/权限、新建云端会话，不读取或输出历史思考。'
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
