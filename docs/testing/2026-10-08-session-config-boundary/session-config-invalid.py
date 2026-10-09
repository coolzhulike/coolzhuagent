"""直连真实候选HTTP，确认服务端拒绝越界参数且未改配置。"""
from pathlib import Path
import json, urllib.request, urllib.error
folder=Path(__file__).resolve().parent/'session-config-live'
facts=json.loads((folder/'facts.json').read_text(encoding='utf-8'))
url='http://127.0.0.1:8767/api/sessions/'+facts['session_id']+'/model-settings'
def get():
    with urllib.request.urlopen(url,timeout=4) as response:return json.load(response)
before=get()
assert before['parameters']==facts['initial_parameters']
parameters=dict(before['parameters']);parameters['temperature']=2.1
request=urllib.request.Request(url,data=json.dumps({'parameters':parameters,'expected_revision':before['configuration_revision']}).encode(),headers={'Content-Type':'application/json'})
try:
    urllib.request.urlopen(request,timeout=4)
    raise AssertionError('非法参数未拒绝')
except urllib.error.HTTPError as error:
    assert error.code==400
    body=error.read().decode()
after=get()
assert before['parameters']==after['parameters']
assert before['configuration_revision']==after['configuration_revision']
result={'http_status':400,'error':body,'parameters_unchanged':True,'revision_unchanged':True,'gui_invalid_draft_not_persisted':True,'model_requests':0}
(folder/'evidence/invalid-server.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result,ensure_ascii=False))
