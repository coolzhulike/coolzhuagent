from pathlib import Path
import hashlib,json,urllib.request,urllib.error,sqlite3,time,threading
p=Path(__file__).resolve().parent
prior=json.loads((p/'external-edit.json').read_text(encoding='utf-8'));file=Path(prior['file']);root=file.parent
def api(method,route,data=None):
 request=urllib.request.Request('http://127.0.0.1:8765'+route,method=method,data=json.dumps(data).encode() if data else None,headers={'Content-Type':'application/json'})
 try:response=urllib.request.urlopen(request,timeout=30)
 except urllib.error.HTTPError as error:response=error
 with response:return {'http':response.status,'body':json.load(response)}
def ledger():
 with sqlite3.connect((root/'.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
  return {'counts':{name:c.execute('select count(*) from '+name).fetchone()[0] for name in ['devin_acp_attempts','chat_room_messages','chat_usage_events']},'bindings':[list(row) for row in c.execute('select lane,remote_session_id,locked_attempt from devin_acp_bindings where agent_id=? order by lane',('session-1791131217833',))]}
r={'stage':'独立并发补采；首轮ledger列表/tuple比较错误，不追认首轮回执','file':str(file),'ledger_before':ledger()}
results=[];threads=[];origin=time.monotonic_ns();barrier=threading.Barrier(3)
def elapsed():return (time.monotonic_ns()-origin)/1e6
try:
 read=api('GET','/api/project/file?path=00-file-conflict-101.txt');r['read_before']=read;assert read['http']==200
 revision=read['body']['meta']['revision'];assert revision==hashlib.sha256(file.read_bytes()).hexdigest()
 def save(label):
  try:
   content='竹林文件版本验收：独立并发写者'+label+'提交。\n'
   barrier.wait(5);started=elapsed()
   result=api('PUT','/api/project/file',{'expected_workspace':str(root),'path':file.name,'content':content,'revision':revision})
   result.update(label=label,content=content,start_ms=started,end_ms=elapsed());results.append(result)
  except Exception as error:results.append({'label':label,'error':str(error)})
 threads=[threading.Thread(target=save,args=(label,)) for label in ['F','G']]
 for thread in threads:thread.start()
 barrier.wait(5)
 for thread in threads:thread.join(35);assert not thread.is_alive()
 r['concurrent_requests']=results
 assert sorted(result['http'] for result in results)==[200,409],results
 winner=next(result for result in results if result['http']==200)
 r['final_content']=file.read_text(encoding='utf-8');r['final_sha256']=hashlib.sha256(file.read_bytes()).hexdigest()
 assert r['final_content']==winner['content']
 r['final_read']=api('GET','/api/project/file?path=00-file-conflict-101.txt')
 assert r['final_read']['body']['meta']['revision']==r['final_sha256']
 r['ledger_after']=ledger();assert r['ledger_before']==r['ledger_after']==prior['ledger_before']
 r['one_winner_other_409']=True
except Exception as error:
 r['error']=str(error);raise
finally:
 r['concurrent_requests']=results
 (p/'concurrent-result.json').write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({key:r[key] for key in ['one_winner_other_409','final_content','concurrent_requests']},ensure_ascii=False))
