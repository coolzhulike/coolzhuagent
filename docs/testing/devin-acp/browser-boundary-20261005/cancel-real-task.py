import json,time,pathlib,sqlite3,threading,urllib.request
root=pathlib.Path('tmp/2026-10-05-browser-boundary'); db='tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
base='http://127.0.0.1:8767';page='http://127.0.0.1:62200/events'
def get(url):
 with urllib.request.urlopen(url,timeout=5) as r:return json.load(r)
def post(url,data,timeout=900):
 req=urllib.request.Request(url,data=json.dumps(data,ensure_ascii=False).encode(),headers={'Content-Type':'application/json'})
 with urllib.request.urlopen(req,timeout=timeout) as r:return json.load(r)
start=int(time.time()*1000); old=list(get(page)); last=max([x['server_ms'] for x in old if x['kind']=='pointerdown']+[0]);response={}
payload={'session_id':'session-1791131217833','chat_room_id':'room-1791131523339','target_agent_ids':['session-1791131217833'],'selected_message_ids':[],'attachments':[],'text':'BU-BOUNDARY-20261005-D：新的独立真实停止测试，仅当前右栏原生浏览器。只调用一次computer_use_perform，正式六项：objective=单击增加次数使0变1，随后聚焦输入验收框并输入SWE2-BOUNDARY-D；surface=browser；target只含url=http://127.0.0.1:62200/boundary.html；success_criteria=[网页实际次数1,实际已输入SWE2-BOUNDARY-D]；constraints=[仅本轮新鲜网页观察,不导航不滚动不用其它工具,收到停止立即结束,不得补发重试,失败未知立即停止,释放不代表目标成功]；max_actions=3。忽略历史任务，如实报告实际结果。'}
(root/'D-cancel-request.json').write_text(json.dumps(payload,ensure_ascii=False,indent=2),encoding='utf-8')
def send():
 try: response['chat']=post(base+'/api/chat/send',payload)
 except Exception as e:response['error']=str(e)
 (root/'D-cancel-chat-response.json').write_text(json.dumps(response,ensure_ascii=False,indent=2),encoding='utf-8')
t=threading.Thread(target=send);t.start();hit=None;interrupt=None
while time.time()*1000-start<180000 and t.is_alive():
 ev=get(page);hit=next((x for x in ev if x['kind']=='pointerdown' and x['server_ms']>last),None)
 if hit:
  with sqlite3.connect(db) as c:
   row=c.execute('select legacy_turn_id,id from runtime_runs where session_id=? and chat_room_id=? and created_at>=? order by created_at desc limit 1',(payload['session_id'],payload['chat_room_id'],start)).fetchone()
  if not row:raise RuntimeError('新任务身份尚未登记，不猜turn_id')
  interrupt={'requested_at_ms':int(time.time()*1000),'turn_id':row[0],'run_id':row[1],'pointerdown':hit}
  try:interrupt['response']=post(base+'/api/chat/turn/interrupt',{k:payload[k] for k in ['session_id','chat_room_id']}|{'turn_id':row[0]},timeout=30)
  except Exception as e:interrupt['error']=str(e)
  interrupt['returned_at_ms']=int(time.time()*1000)
  (root/'D-interrupt-receipt.json').write_text(json.dumps(interrupt,ensure_ascii=False,indent=2),encoding='utf-8'); print(json.dumps(interrupt,ensure_ascii=False),flush=True);break
 time.sleep(.04)
if not interrupt: print('未命中实际按下，不提交取消、不宣称通过',flush=True)
t.join(900)
print(json.dumps({'chat_done':not t.is_alive(),'elapsed_sec':round(time.time()-start/1000,1)}))
