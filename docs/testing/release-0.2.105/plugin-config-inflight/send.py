"""一次真实SWE，原房间/原唯一远端；只记录SSE事件计数。"""
from pathlib import Path
import json,sqlite3,urllib.request,collections,time
folder=Path(__file__).resolve().parent;root=folder.parents[1]
address=json.loads((folder/'address.json').read_text(encoding='utf-8'))
assert not (folder/'submitted-request.json').exists()
tool=address['tool_name'];url=address['url']
prompt=f'''PLUGIN-CONFIG-INFLIGHT-105-20261008：正式0.2.105插件函数执行期间重新配置与退出验收。仅尝试一次工具 {tool}，参数 {json.dumps({'url':url,'allowPrivate':True,'timeoutMs':60000,'maxBytes':1024})}。本机资料端点会等待；验收端只在真实网络函数进入后正常重新配置既有插件，添加无操作效应的验收标签。旧执行应被取消或拒绝，报告实际状态与宿主返回的清理事实，不假报成功，不重试、不补发、不调用其它工具、不改文件、不创建新云端会话。收到结果后立即完成最终回复。'''
payload={'expected_workspace_id':'ws-23f646a969206cb4','native_browser_panel':True,'session_id':'session-1791131217833','target_agent_ids':['session-1791131217833'],'chat_room_id':'room-1791131523339','text':prompt,'attachments':[]}
(folder/'submitted-request.json').write_text(json.dumps(payload,ensure_ascii=False,indent=2),encoding='utf-8')
req=urllib.request.Request('http://127.0.0.1:8765/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
counts=collections.Counter()
with urllib.request.urlopen(req,timeout=1000) as response:
    for raw in response:
        if raw.startswith(b'event:'):
            event=raw.decode().strip()[6:].strip();counts[event]+=1
            if event in ('started','error','done'):print(event,flush=True)
(folder/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'events':dict(counts)},indent=2),encoding='utf-8')
print(json.dumps({'stream':'EOF','events':dict(counts)}),flush=True)
