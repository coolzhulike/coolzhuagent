"""完整候选包正式8765的逐回复归属只读复验；UI点击另行留证。"""
import datetime, hashlib, json, pathlib, urllib.request, urllib.error
root=pathlib.Path(r'D:\coolzhuagent')
stage=root/'tmp/goal-32wbs-acceptance-20261013'
switch=json.loads((stage/'controller-package135-live-20261016.json').read_text(encoding='utf-8'))
assert switch['state']=='completed'
expected=json.loads((stage/'controller-reply-ui-expectations-20261016.json').read_text(encoding='utf-8'))
data={'at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'version':'0.2.135完整候选包，正式8765；MSI覆盖仍受阻','method':'正式端口只读HTTP，与实际持久化归属对照；没有模型调用或生产库写入','cases':[],'static_resources':[]}
def get(path):
    try:
        with urllib.request.urlopen('http://127.0.0.1:8765'+path,timeout=20) as response:return response.status,response.read()
    except urllib.error.HTTPError as e:return e.code,e.read()
for x in expected['expectations']:
    path='/api/chat/rooms/'+expected['room_id']+'/trace?limit=30&message_id='+x['message_id']
    status,raw=get(path);body=json.loads(raw)
    ids=[r['run_id'] for r in body.get('runs',[])]
    data['cases'].append({'case':'真实回复精确归属','path':path,'status':status,'message_id':x['message_id'],'expected_run_id':x['run_id'],'actual_run_ids':ids,'requested_message_id':body.get('requested_message_id'),'pass':status==200 and ids==[x['run_id']] and body.get('requested_message_id')==x['message_id'] and not body.get('has_more')})
base='/api/chat/rooms/'+expected['room_id']+'/trace'
msg=expected['expectations'][0]['message_id']
for name,path,want in [('不存在回复',base+'?message_id=missing-formal-135',404),('跨聊天室','/api/chat/rooms/main-room/trace?message_id='+msg,404),('冲突run筛选',base+'?message_id='+msg+'&run_id='+expected['expectations'][1]['run_id'],400),('禁止回复定位混用分页',base+'?message_id='+msg+'&before=1',400)]:
    status,raw=get(path)
    data['cases'].append({'case':name,'path':path,'expected_status':want,'status':status,'pass':status==want,'response':json.loads(raw)})
for route,file in [('/src/app.js','src/app.js'),('/src/workspace_panels.js','src/workspace_panels.js')]:
    status,raw=get(route);source=root/'modules/gui-web/packages/web-console'/file
    data['static_resources'].append({'route':route,'status':status,'actual_sha256':hashlib.sha256(raw).hexdigest(),'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'match':status==200 and raw==source.read_bytes()})
data['passed']=sum(x['pass'] for x in data['cases'])
data['failed']=sum(not x['pass'] for x in data['cases'])
data['state']='completed' if data['failed']==0 and all(x['match'] for x in data['static_resources']) else 'failed'
(stage/'controller-reply-formal135-20261016.json').write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(data,ensure_ascii=False))
if data['state']!='completed':raise SystemExit(1)
