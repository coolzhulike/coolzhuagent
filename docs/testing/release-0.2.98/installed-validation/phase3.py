"""重建及关闭本轮自有终端；真实状态不替换为夹具。"""
import pathlib,json,urllib.request,urllib.parse,urllib.error,time
folder=pathlib.Path(__file__).resolve().parent
owner=json.loads((folder/'owner.json').read_text(encoding='utf-8')); scope=owner['scope']
def api(path,body=None,query=None,expected=200):
    url='http://127.0.0.1:8765'+path+('?' +urllib.parse.urlencode(query) if query else '')
    request=urllib.request.Request(url,json.dumps(body,ensure_ascii=False).encode() if body is not None else None,{'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request,timeout=25) as response: status=response.status; value=json.load(response)
    except urllib.error.HTTPError as e: status=e.code; value=json.loads(e.read())
    assert status==expected,(path,status,value)
    return value
old=api('/api/terminal',query=scope)
assert old['handle']==owner['handle'] and old['closed'],'旧进程尚未退出或归属变化，保留'
new=api('/api/terminal/start',{**scope,'cols':100,'rows':30})
assert new['handle']!=old['handle'] and new['active'] and not new['closed']
(folder/'recreated-owner.json').write_text(json.dumps({'scope':scope,'handle':new['handle']},ensure_ascii=False,indent=2),encoding='utf-8')
stale=api('/api/terminal/'+old['handle']+'/output',query=scope,expected=409)
time.sleep(1)
cursor=api('/api/terminal/'+new['handle']+'/output',query=scope)['next_cursor']
api('/api/terminal/'+new['handle']+'/input',{**scope,'text':"[Console]::WriteLine(('NEW-'+'SCOPE:')+($null -eq (Get-Variable coolzhuPtyMarker -ErrorAction SilentlyContinue)))\r"})
deadline=time.monotonic()+10; text=''
while time.monotonic()<deadline:
    response=api('/api/terminal/'+new['handle']+'/output',query={**scope,'cursor':cursor})
    cursor=response['next_cursor']; text+=response['text']
    if 'NEW-SCOPE:True' in text: break
    time.sleep(.1)
else: raise RuntimeError('新终端变量隔离未通过')
closed=api('/api/terminal/'+new['handle']+'/close',scope)
after=api('/api/terminal',query=scope)
after_old=api('/api/terminal/'+new['handle']+'/output',query=scope,expected=409)
assert not closed['active'] and not after['active'] and after['handle'] is None
result={'stage':'0.2.98正式安装版','old_handle':old['handle'],'new_handle':new['handle'],'replaced':True,'old_output_http':409,'old_error':stale,'old_variable_absent':True,'close_response':closed,'after_close':after,'new_output_after_close_http':409,'new_error':after_old,'generated_model_requests':0}
(folder/'phase3-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'recreated_new_handle':True,'old_variable_absent':True,'old_handle':409,'close_absent':True,'closed_handle':409},ensure_ascii=False))
