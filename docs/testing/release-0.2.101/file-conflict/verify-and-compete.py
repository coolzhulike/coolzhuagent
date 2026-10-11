from pathlib import Path
import hashlib,json,urllib.request,urllib.error,sqlite3,time,threading
p=Path(__file__).resolve().parent
r=json.loads((p/'external-edit.json').read_text(encoding='utf-8'));file=Path(r['file']);root=file.parent
def api(method,route,data=None):
 request=urllib.request.Request('http://127.0.0.1:8765'+route,method=method,data=json.dumps(data).encode() if data else None,headers={'Content-Type':'application/json'})
 try:response=urllib.request.urlopen(request,timeout=30)
 except urllib.error.HTTPError as error:response=error
 with response:return {'http':response.status,'body':json.load(response)}
r['disk_after_gui_conflict']={'content':file.read_text(encoding='utf-8'),'sha256':hashlib.sha256(file.read_bytes()).hexdigest()}
assert r['disk_after_gui_conflict']['sha256']==r['external_sha256'],'旧编辑覆盖了外部文件'
read=api('GET','/api/project/file?path=00-file-conflict-101.txt');assert read['http']==200
revision=read['body']['meta']['revision'];assert revision==r['external_sha256']
barrier=threading.Barrier(3);results=[];origin=time.monotonic_ns()
def elapsed():return (time.monotonic_ns()-origin)/1e6
def save(label):
 content='竹林文件版本验收：并发写者'+label+'提交。\n'
 barrier.wait();started=elapsed()
 result=api('PUT','/api/project/file',{'expected_workspace':str(root),'path':file.name,'content':content,'revision':revision})
 result.update(label=label,content=content,start_ms=started,end_ms=elapsed());results.append(result)
threads=[threading.Thread(target=save,args=(label,)) for label in ['D','E']]
for thread in threads:thread.start()
barrier.wait()
for thread in threads:thread.join(35);assert not thread.is_alive()
r['concurrent_requests']=results
assert sorted(result['http'] for result in results)==[200,409],results
winner=next(result for result in results if result['http']==200)
assert file.read_text(encoding='utf-8')==winner['content']
r['final_content']=file.read_text(encoding='utf-8');r['final_sha256']=hashlib.sha256(file.read_bytes()).hexdigest()
r['final_read']=api('GET','/api/project/file?path=00-file-conflict-101.txt')
assert r['final_read']['body']['meta']['revision']==r['final_sha256']
with sqlite3.connect((root/'.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
 r['ledger_after']={'counts':{name:c.execute('select count(*) from '+name).fetchone()[0] for name in ['devin_acp_attempts','chat_room_messages','chat_usage_events']},'bindings':c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=? order by lane',('session-1791131217833',)).fetchall()}
assert r['ledger_before']==r['ledger_after']
r['gui_conflict_preserves_external']=True;r['concurrent_same_revision_one_winner']=True
(p/'result.json').write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({key:r[key] for key in ['gui_conflict_preserves_external','concurrent_same_revision_one_winner','final_content','concurrent_requests']},ensure_ascii=False))
