"""正式105单次真实SWE；不保存思考或SSE正文。"""
from pathlib import Path
import json,sqlite3,urllib.request,collections,time
folder=Path(__file__).resolve().parent;root=folder.parents[1]
assert not (folder/'submitted-request.json').exists()
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
    assert c.execute("SELECT COUNT(*) FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchone()[0]==0
    bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',)).fetchall()
    assert sum(b[1]=='island-kayak' for b in bindings)==1 and not any(b[2] for b in bindings)
    before={'attempts':c.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0],'messages':c.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0],'bindings':bindings}
d=json.load(urllib.request.urlopen('http://127.0.0.1:8765/api/sessions/session-1791131217833/model-settings'))
assert d['session']['model']=='swe-2-medium' and d['configuration_revision']==51
assert 'computer_use_perform' in d['parameters']['tool_allowlist']
(folder/'before.json').write_text(json.dumps(before,ensure_ascii=False,indent=2),encoding='utf-8')
prompt='BU-RESOURCE-107-20261008：正式0.2.107观察期间资源变化原因验收。当前右栏旧网页已载入，只调用一次computer_use_perform(surface="browser",省略target,max_actions=1)，按新鲜节点点击“投递事实验收”一次。验收端只在收到可信click事件后通过正常地址栏打开另一网页，不是按下期间变化。不要自行导航、关闭、重试、补发、调用其它工具或创建云端会话。read_request完整分页后respond，wait取得真实终态。最终准确读取终态input_steps中的input_delivery/input_release_status/effect_status；分别报告动作投递、释放和效果观察。不能把观察失效说成没有投递；不能声称导航发生于按下期间；未取得事实保持未知。'
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
