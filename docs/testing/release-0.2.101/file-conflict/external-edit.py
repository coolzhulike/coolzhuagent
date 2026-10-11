from pathlib import Path
import hashlib,json,urllib.request,sqlite3
p=Path(__file__).resolve().parent
owned=json.loads((p/'owned.json').read_text(encoding='utf-8'))
file=Path(owned['path']);root=file.parent
assert hashlib.sha256(file.read_bytes()).hexdigest()==owned['initial_sha256']
def ledger():
 with sqlite3.connect((root/'.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
  return {'counts':{name:c.execute('select count(*) from '+name).fetchone()[0] for name in ['devin_acp_attempts','chat_room_messages','chat_usage_events']},'bindings':c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=? order by lane',('session-1791131217833',)).fetchall()}
with urllib.request.urlopen('http://127.0.0.1:8765/api/project/file?path=00-file-conflict-101.txt') as response:
 prior=json.load(response)
content='竹林文件版本验收：外部已提交版本C，不能被旧编辑覆盖。\n'.encode()
file.write_bytes(content)
result={'file':str(file),'initial_sha256':owned['initial_sha256'],'read_before_edit':prior,'external_content':content.decode(),'external_sha256':hashlib.sha256(content).hexdigest(),'ledger_before':ledger()}
(p/'external-edit.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'initial_sha256':owned['initial_sha256'],'external_sha256':result['external_sha256']},ensure_ascii=False))
