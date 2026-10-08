"""产品HTTP操作本轮自有ConPTY；不经终端UI执行命令。"""
import pathlib,json,urllib.request,urllib.parse,urllib.error,time,sys
folder=pathlib.Path(__file__).resolve().parent
owner=json.loads((folder/'owner.json').read_text(encoding='utf-8'))
scope=owner['scope']; handle=owner['handle']; marker=owner['marker']
def api(path,body=None,query=None,expected=200):
    url='http://127.0.0.1:8765'+path+('?' + urllib.parse.urlencode(query) if query else '')
    request=urllib.request.Request(url,json.dumps(body,ensure_ascii=False).encode() if body is not None else None,{'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request,timeout=25) as response: status=response.status; value=json.load(response)
    except urllib.error.HTTPError as e: status=e.code; value=json.loads(e.read())
    assert status==expected,(path,status,value)
    return value
current=api('/api/terminal',query=scope)
assert current['handle']==handle and current['active'],'非本轮句柄，保留'
if sys.argv[1]=='display':
    api('/api/terminal/'+handle+'/input',{**scope,'text':"[Console]::WriteLine(('UI-'+'UNICODE:竹林𠮷😀 / ')+$coolzhuPtyMarker)\r"})
    deadline=time.monotonic()+10
    cursor=current['next_cursor']; collected=''
    while time.monotonic()<deadline:
        r=api('/api/terminal/'+handle+'/output',query={**scope,'cursor':cursor})
        cursor=r['next_cursor']; collected+=r['text']
        if 'UI-UNICODE:竹林𠮷😀 / '+marker in collected:
            (folder/'ui-unicode-result.json').write_text(json.dumps({'passed':True,'handle':handle,'marker':'UI-UNICODE:竹林𠮷😀 / '+marker,'generated_model_requests':0},ensure_ascii=False,indent=2),encoding='utf-8')
            print('真实终端Unicode显示标记已执行');break
        time.sleep(.1)
    else: raise RuntimeError('显示标记未执行')
else:
    api('/api/terminal/'+handle+'/input',{**scope,'text':'[Environment]::Exit(23)\r'})
    deadline=time.monotonic()+15
    while time.monotonic()<deadline:
        r=api('/api/terminal/'+handle+'/output',query=scope)
        if r['closed']: break
        time.sleep(.1)
    else: raise RuntimeError('真实进程退出未收束')
    (folder/'exit-result.json').write_text(json.dumps({'stage':'097之后源码候选','old_handle':handle,'exit_requested':23,'closed_response':{k:v for k,v in r.items() if k!='text'},'generated_model_requests':0},ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps({'closed':r['closed'],'message':r['message']},ensure_ascii=False))
