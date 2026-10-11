"""原SWE/唯一island-kayak的一次真实工具调用；不提供伪响应。"""
from pathlib import Path
import json,time,urllib.request
p=Path(__file__).resolve().parent
server=json.loads((p/'slow-server.json').read_text())
arguments={'url':f"http://127.0.0.1:{server['port']}/revoke-20261008",'allowPrivate':True,'timeoutMs':60000,'maxBytes':1024}
prompt='PLUGIN-INSTALLED099-HELD-PROCESS-20261008：正式0.2.99真实插件清理专项。只调用一次工具 '+server['tool_name']+'，参数 '+json.dumps(arguments,ensure_ascii=False)+'。调用之后测试端会正常停用插件。仅报告原始宿主投递、取消及cleanup_confirmed事实，不推断Windows进程是否退出。禁止重试、补发、其它工具、其它网址、shell、CU、新云端会话；旧任务已结束，沿用现有唯一远端。收到取消后结束回复。'
payload={'expected_workspace_id':'ws-23f646a969206cb4','session_id':'session-1791131217833','target_agent_ids':['session-1791131217833'],'chat_room_id':'room-1791131523339','text':prompt,'attachments':[]}
(p/'submitted-request.json').write_text(json.dumps(payload,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
req=urllib.request.Request('http://127.0.0.1:8765/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
with urllib.request.urlopen(req,timeout=1000) as response,(p/'stream-events.txt').open('w',encoding='utf-8') as output:
    for raw in response:
        line=raw.decode('utf-8');output.write(line);output.flush()
        if line.startswith('event:'):print(line.strip(),flush=True)
(p/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'status':'EOF，仍需核真实终态'}),encoding='utf-8')
