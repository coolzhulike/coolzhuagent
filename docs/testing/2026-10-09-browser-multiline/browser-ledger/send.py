"""唯一Devin原绑定的一次真实长程任务；SSE仅计事件名。"""
from pathlib import Path
import json,urllib.request,sqlite3,collections,time
root=Path(__file__).resolve().parents[3];p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True)
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
sid='session-1791131217833';room='room-1791131523339';base='http://127.0.0.1:8765'
marker='BU-CROSS-ORIGIN-LIVE-LEDGER-MULTILINE-CANDIDATE-20261009'
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
text=marker+'：这是跨来源内嵌网页、动态行情和插件计算的综合长程验收。当前右栏原生内置浏览器已正常打开'+address+'。沿用当前唯一Devin云端会话。先只调用一次computer_use_perform(surface="browser",省略target,max_actions=16)，完成“竹林订单与账本”的三项订单流程：每步从当前真实跨来源内嵌订单页读取本步订单码，填入订单码后按Enter，通过后前两步点下一步，第三步按Enter。忽略非目标行情变化，必要时按真实文档引用滚动以显示控件或最终结果；不得复用被替换的表单引用。Browser成功标准必须包含可见整单已完成三项均通过，并把外层已验证回执中三项物品的单价与数量保留为可读证据。真实终态成功后，根据本轮工具回传的三项价格和数量，调用一次DSH计算器 dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab，算三项单价乘数量的总和；最终如实报告逐项算式和总额。两个工具严格顺序、每个只调用一次；CU或证据失败则不猜数字、不补发计算器。完整read_request分页后respond，并wait到真实终态。禁止直接HTTP或脚本操作网页、外部浏览器、其它工具、修改文件或权限、创建云端会话；阻止或超时即如实收尾，输入投递/释放与效果分开，未知保持未知，不读取或输出历史思考。'
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
