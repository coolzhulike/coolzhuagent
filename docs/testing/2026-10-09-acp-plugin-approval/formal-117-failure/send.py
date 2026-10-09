"""单次真实模型，仅记SSE事件名，不保存思考。"""
from common import *
import collections
assert not (p/'submitted.json').exists()
identity();s=http('/api/sessions/'+sid+'/model-settings');assert s['session']['model']=='swe-2-medium' and tool in s['parameters']['tool_allowlist']
perm=http('/api/chat/rooms/'+room+'/permissions');assert not perm['effective_full_access']
server=json.loads((p/'server.json').read_text(encoding='utf-8'))
text=marker+'：这是正式117接纳后权限扩大的独立边界验收。只调用一次工具'+tool+'，参数'+json.dumps({'url':server['url'],'allowPrivate':True,'timeoutMs':60000,'maxBytes':1024},ensure_ascii=False)+'。这是真实已安装DSH net_fetch；不要使用任何其它工具或网址、不重试、不重发、不重新启用插件、不创建云端会话、不请求修改权限、不尝试审批。测试者会在请求接纳后通过正常配置扩大权限；本轮只能沿用接纳时的权限。若宿主拒绝或要求重新发起，照实报告并立即结束；不要另外发起新轮、不要读取或输出历史思考。'
payload={'expected_workspace_id':ws,'session_id':sid,'target_agent_ids':[sid],'chat_room_id':room,'text':text,'attachments':[]}
save('submitted.json',{'submitted_ms':time.time()*1000,'payload':payload})
req=urllib.request.Request(base+'/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
counts=collections.Counter()
with urllib.request.urlopen(req,timeout=600) as r:
    for line in r:
        if line.startswith(b'event:'):
            event=line.decode().strip()[6:].strip();counts[event]+=1
            if event in ('started','error','done'):print(event,flush=True)
save('stream-finished.json',{'finished_ms':time.time()*1000,'event_counts':dict(counts)})
print(json.dumps(dict(counts)),flush=True)
