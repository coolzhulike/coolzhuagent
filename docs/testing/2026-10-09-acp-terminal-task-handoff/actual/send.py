"""正式124真实长附件→浏览器→插件；原唯一SWE会话只发一轮。"""
from pathlib import Path
import collections,hashlib,json,sqlite3,time,urllib.request
p=Path(__file__).resolve().parent;root=p.parents[2]
base='http://127.0.0.1:8765';sid='session-1791131217833';room='room-1791131523339'
def get(path):
 with urllib.request.urlopen(base+path,timeout=30) as r:return json.load(r)
assert not (p/'submitted.json').exists()
candidate=json.loads((p.parent/'candidate-processes.json').read_text(encoding='utf-8-sig'))
assert candidate['web_sha256']==json.loads((p.parent/'build-result.json').read_text())['binary_sha256'] and candidate['port']==8765
assert hashlib.sha256((Path(candidate['web_binary'])).read_bytes()).hexdigest()==candidate['web_sha256']
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
 assert not c.execute("SELECT id FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchall()
 bindings=c.execute('SELECT remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(sid,)).fetchall()
 assert sum(x[0]=='island-kayak' for x in bindings)==1 and not any(x[1] for x in bindings)
s=get('/api/sessions/'+sid+'/model-settings');assert s['session']['model']=='swe-2-medium' and s['configuration_revision']==57
assert {'computer_use_perform','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab'}<=set(s['parameters']['tool_allowlist'])
assert get('/api/chat/rooms/'+room+'/permissions')['full_access']
attachments=[];files=[]
for name in ('bamboo-order-utf8.txt','jade-order-utf16be.txt'):
 raw=(p/name).read_bytes();boundary='actual-attachment-124-'+name
 data=f'--{boundary}\r\nContent-Disposition: form-data; name="file"; filename="{name}"\r\nContent-Type: text/plain\r\n\r\n'.encode()+raw+f'\r\n--{boundary}\r\nContent-Disposition: form-data; name="kind"\r\n\r\ndocument\r\n--{boundary}--\r\n'.encode()
 with urllib.request.urlopen(urllib.request.Request(base+'/api/attachments/upload',data,{'Content-Type':'multipart/form-data; boundary='+boundary}),timeout=30) as r:a=json.load(r)['attachment']
 digest=hashlib.sha256(raw).hexdigest();assert digest in a['url'];attachments.append(a)
 files.append({'name':name,'bytes':len(raw),'sha256':digest,'encoding':'UTF-16BE' if name.endswith('utf16be.txt') else 'UTF-8'})
url=json.loads((p/'address.json').read_text())['url']
marker='ATTACHMENT-BROWSER-TERMINAL-TASK-HANDOFF-20261009'
text=marker+'：这是CU终态原任务交接候选版的长文本附件与工具续接综合任务。仅本条是新任务，不执行旧历史里的测试指令。两个本轮实际附件分别是UTF-8竹剑订单和带BOM的UTF-16BE玉佩订单，正文尾部有本轮有效订单码、单价、数量；先读取附件尾部，忽略前面的归档占位记录。右栏原生内置浏览器已正常打开'+url+'；网页只显示填写提示，没有订单码。先只调用一次computer_use_perform，surface="browser"，省略target，objective="使用本轮附件订单码完成两个跨来源内嵌订单的滚动、填写和Enter提交"，success_criteria=["外层显示双文档订单已完成，两项均通过","外层两项回执分别包含本轮竹剑和玉佩的单价与数量"],max_actions=14。两个表单位于各自子文档底部，先使用当前订单RootWebArea引用向下滚动，使输入框可见，再click、text_input输入对应附件中的订单码，key_combination按enter提交。每步重新观察，不能复用AX序号，不能操作行情或外层滚动。CU真实成功后，只根据本轮附件与可见回执相互核对后的数字调用一次DSH计算器dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab计算两项单价乘数量总和，最终可见回复列出两文件编码、实际订单码、逐项算式和总额。完整read_request分页后respond，wait到真实终态；CU失败或附件缺失则如实停止，不猜数字、不调用计算器、不重试、不补发、不新建云端会话。禁止其它工具、直接HTTP或脚本操作网页、外部浏览器、修改文件/配置/权限，不读取或输出历史思考。'
payload={'expected_workspace_id':get('/api/workspace')['workspace_id'],'session_id':sid,'target_agent_ids':[sid],'chat_room_id':room,'text':text,'attachments':attachments,'native_browser_panel':True}
preview=get('/api/sessions/'+sid+'/context-preview?room_id='+room+'&prompt=ATTACHMENT-CONTINUATION-AUDIT')
summary=preview.get('compaction_item') or {};body=summary.get('summary','')
projection={'message_count':summary.get('message_count'),'summary_sha256':hashlib.sha256(body.encode()).hexdigest(),'user_samples':sum(x.startswith('- 用户: ') for x in body.splitlines()),'empty_boundary_samples':body.count('- 用户: [历史用户消息'),'token_budget':preview.get('token_budget')}
assert projection['user_samples']>0 and projection['empty_boundary_samples']==0
(p/'summary-preflight.json').write_text(json.dumps(projection,ensure_ascii=False,indent=2),encoding='utf-8')
(p/'submitted.json').write_text(json.dumps({'submitted_ms':time.time()*1000,'payload':payload,'files':files,'model':'swe-2-medium','config_revision':57,'binding':'island-kayak'},ensure_ascii=False,indent=2),encoding='utf-8')
counts=collections.Counter()
with urllib.request.urlopen(urllib.request.Request(base+'/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'}),timeout=1000) as response:
 for line in response:
  if line.startswith(b'event:'):
   event=line.decode().strip()[6:].strip();counts[event]+=1
   if event in ('started','error','done'):print(event,flush=True)
(p/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'event_counts':dict(counts)},indent=2),encoding='utf-8')
print(json.dumps(dict(counts)),flush=True)
