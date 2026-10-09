"""单次正式SWE调用，只计SSE事件名，不保存思考。"""
from pathlib import Path
import json,sqlite3,urllib.request,collections,time
folder=Path(__file__).resolve().parent;repo=Path('C:\\Users\\zhupu\\.codex\\worktrees\\input-recovery-20261004\\coolzhuagent')
assert not (folder/'submitted-request.json').exists()
installed=json.loads((repo/'tmp/2026-10-08-release-112/installed-112-verification.json').read_text(encoding='utf-8'))
assert installed['version']=='0.2.112' and installed['package_safe'] is True
assert installed['source_commit']=='72be2b53f2c712ba7e7dced6686b5399f23fe3aa'
from hashlib import sha256
assert sha256(Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe').read_bytes()).hexdigest()==installed['web_sha256']
assert sha256(Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-tauri-shell.exe').read_bytes()).hexdigest()==installed['shell_sha256']
db=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:
    assert c.execute("SELECT COUNT(*) FROM runtime_runs WHERE state NOT IN ('completed','failed','interrupted','cancelled','canceled')").fetchone()[0]==0
    bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',('session-1791131217833',)).fetchall()
    assert sum(b[1]=='island-kayak' for b in bindings)==1 and not any(b[2] for b in bindings)
    before={'attempts':c.execute('SELECT COUNT(*) FROM devin_acp_attempts').fetchone()[0],'messages':c.execute('SELECT COUNT(*) FROM chat_room_messages').fetchone()[0],'bindings':bindings}
config=json.load(urllib.request.urlopen('http://127.0.0.1:8765/api/sessions/session-1791131217833/model-settings'))
assert config['session']['model']=='swe-2-medium' and config['configuration_revision']==51
assert 'computer_use_perform' in config['parameters']['tool_allowlist']
(folder/'before.json').write_text(json.dumps(before,ensure_ascii=False,indent=2),encoding='utf-8')
prompt='BU-PENDING-CLOSE-112-20261008：同版正式观察等待期间面板资源撤销验收。右栏稳定旧页已载入，只调用一次computer_use_perform(surface="browser",省略target,max_actions=1)，按新鲜节点点击“交接观察验收”一次。网页本身不导航，测试者会在输入释放、下一底层观察请求登记后正常关闭右栏浏览器。不要自行导航、关闭、重试、补发、调用其它工具或创建云端会话。完整read_request分页后respond，wait取得终态。最终读取input_steps分别如实报告投递、释放、效果，观察停止不能说成未投递；未知保持未知。'
payload={'expected_workspace_id':'ws-23f646a969206cb4','native_browser_panel':True,'session_id':'session-1791131217833','target_agent_ids':['session-1791131217833'],'chat_room_id':'room-1791131523339','text':prompt,'attachments':[]}
(folder/'submitted-request.json').write_text(json.dumps(payload,ensure_ascii=False,indent=2),encoding='utf-8')
request=urllib.request.Request('http://127.0.0.1:8765/api/chat/send/stream',json.dumps(payload).encode(),{'Content-Type':'application/json'})
counts=collections.Counter()
with urllib.request.urlopen(request,timeout=1000) as response:
    for raw in response:
        if raw.startswith(b'event:'):
            event=raw.decode().strip()[6:].strip();counts[event]+=1
            if event in ('started','error','done'):print(event,flush=True)
(folder/'stream-finished.json').write_text(json.dumps({'finished_ms':time.time()*1000,'events':dict(counts)},indent=2),encoding='utf-8')
print(json.dumps({'stream':'EOF','events':dict(counts)}),flush=True)
