"""原唯一 SWE 云端绑定，正式安装版的一次真实原生视图替换验证。"""
from pathlib import Path
import json, sqlite3, urllib.request, time
folder = Path(__file__).resolve().parent
root = folder.parents[1]
db = root / 'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro', uri=True) as conn:
    assert conn.execute("SELECT COUNT(*) FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchone()[0] == 0
    bindings = conn.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?', ('session-1791131217833',)).fetchall()
    assert sum(b[1]=='island-kayak' for b in bindings)==1 and not any(b[2] for b in bindings), bindings
    before = {'attempts':conn.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0], 'messages':conn.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0]}
with urllib.request.urlopen('http://127.0.0.1:8765/api/sessions/session-1791131217833/model-settings') as response:
    settings = json.load(response)
assert settings['session']['model']=='swe-2-medium' and settings['configuration_revision']==35
assert settings['parameters']['tool_allowlist']==['computer_use_perform','plugin__cli_anything_status','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab']
(folder/'before.json').write_text(json.dumps({'counts':before,'bindings':bindings,'configuration_revision':35},ensure_ascii=False,indent=2),encoding='utf-8')
prompt='''BU-NATIVE-REPLACEMENT-101-20261008：正式0.2.101原生视图替换窗口验证。当前右栏旧网页已加载，只调用一次computer_use_perform(surface="browser",省略target,max_actions=1)，根据新鲜节点点击“替换窗口验收”一次。验收端将尝试在网页真实按下处理期间通过正常界面关闭旧视图、打开新的原生视图；不保证命中窗口。你只做上述一次点击，不主动关闭、导航、重试、补发或输入，不用其它工具，不创建云端会话。允许read_request完整分页后respond与wait，收到未知输入状态立即停止。仅报告真实宿主回执与终态；未命中不得报通过。
以下链接仅供验收端正常点击打开，模型不得操作： http://localhost:59793/new.html'''
payload={'expected_workspace_id':'ws-23f646a969206cb4','native_browser_panel':True,'session_id':'session-1791131217833','target_agent_ids':['session-1791131217833'],'chat_room_id':'room-1791131523339','text':prompt,'attachments':[]}
(folder/'submitted-request.json').write_text(json.dumps(payload,ensure_ascii=False,indent=2),encoding='utf-8')
req=urllib.request.Request('http://127.0.0.1:8765/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
with urllib.request.urlopen(req,timeout=1000) as response:
    for raw in response:
        if raw.startswith(b'event:'):print(raw.decode('utf-8').strip(),flush=True)
(folder/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'status':'EOF；另核协议终态'}),encoding='utf-8')
