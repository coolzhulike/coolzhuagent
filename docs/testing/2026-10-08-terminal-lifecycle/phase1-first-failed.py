"""正式终端HTTP入口驱动真实ConPTY；常量命令只产生本轮标记及有界输出。"""
import pathlib,urllib.request,urllib.parse,urllib.error,json,time,hashlib,secrets
folder=pathlib.Path(__file__).resolve().parent
scope={'session_id':'session-1791131217833','room_id':'room-1791131523339'}
base='http://127.0.0.1:8765'
def api(path,body=None,query=None,expected=200):
    url=base+path+('?' +urllib.parse.urlencode(query) if query is not None else '')
    request=urllib.request.Request(url,json.dumps(body,ensure_ascii=False).encode() if body is not None else None,{'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request,timeout=25) as response:
            status=response.status; value=json.load(response)
    except urllib.error.HTTPError as error:
        status=error.code; value=json.loads(error.read())
    assert status==expected,(path,status,value)
    return value
before=api('/api/terminal',query=scope)
assert before['active'] is False,'已有用户终端，保留，不继续'
marker='PTY097-'+secrets.token_hex(4).upper()
started=api('/api/terminal/start',{**scope,'cols':100,'rows':30})
handle=started['handle']; assert handle and started['active'] and not started['closed']
(folder/'owner.json').write_text(json.dumps({'scope':scope,'handle':handle,'marker':marker},ensure_ascii=False,indent=2),encoding='utf-8')
def write(text):
    return api('/api/terminal/'+handle+'/input',{**scope,'text':text+'\r'})
def output(cursor=0):
    return api('/api/terminal/'+handle+'/output',query={**scope,'cursor':cursor})
def wait_for(marker,cursor=0):
    chunks=[]; sizes=[]; truncated=False; deadline=time.monotonic()+30
    while time.monotonic()<deadline:
        r=output(cursor); cursor=r['next_cursor']; chunks.append(r['text']); sizes.append(len(r['text'].encode())); truncated |= r['truncated']
        if marker in ''.join(chunks):
            return {'cursor':cursor,'text':''.join(chunks),'pages':len(chunks),'page_bytes':sizes,'truncated':truncated}
        time.sleep(.15)
    raise RuntimeError('未收到真实终端标记：'+marker)
time.sleep(1)
initial=output(); cursor=initial['next_cursor']
write("$coolzhuPtyMarker='"+marker+"'; [Console]::WriteLine('INITIAL-'+$coolzhuPtyMarker+' / 竹林𠮷😀')")
initial_marker=wait_for('INITIAL-'+marker+' / 竹林𠮷😀',cursor)
assert '\ufffd' not in initial_marker['text']
# 写入超过1MiB的真实控制台流，服务端环形缓冲必须明确截断而不是无限累积。
write("[Console]::Write((('甲竹𠮷😀-'+('x'*64))+\"`r`n\")*18000); [Console]::WriteLine('FLOOD-"+marker+"-DONE')")
time.sleep(3)
flood=wait_for('FLOOD-'+marker+'-DONE',0)
assert flood['truncated'],'大输出未覆盖环形缓冲；不能冒称容量验收'
assert len(flood['text'].encode())<=1024*1024
assert max(flood['page_bytes'])<=80*1024
assert '\ufffd' not in flood['text']
restored=api('/api/terminal',query=scope)
assert restored['handle']==handle and restored['active'] and not restored['closed']
resized=api('/api/terminal/'+handle+'/resize',{**scope,'cols':120,'rows':40})
write("[Console]::WriteLine('RESTORED-'+$coolzhuPtyMarker)")
restore_marker=wait_for('RESTORED-'+marker,flood['cursor'])
foreign=api('/api/terminal/'+handle+'/input',{**scope,'room_id':'main-room','text':"[Console]::WriteLine('FOREIGN-SHOULD-NOT-RUN')\r"},expected=409)
invalid=api('/api/terminal/not-the-owner-handle/output',query=scope,expected=409)
last=output(restore_marker['cursor'])
assert 'FOREIGN-SHOULD-NOT-RUN' not in last['text']
result={'installed_version':'0.2.97','marker':marker,'handle':handle,'before':before,'started':{k:v for k,v in started.items() if k!='text'},'initial_unicode_marker':initial_marker['text'],'flood':{k:v for k,v in flood.items() if k!='text'},'flood_sha256':hashlib.sha256(flood['text'].encode()).hexdigest(),'flood_returned_bytes':len(flood['text'].encode()),'restored_same_handle':restored['handle']==handle,'resize_accepted':bool(resized['handle']==handle),'restore_state_marker':restore_marker['text'],'foreign_room_rejected':foreign,'invalid_handle_rejected':invalid,'generated_model_requests':0,'close_exit_cleanup':'next_phase'}
(folder/'phase1-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'real_conpty':True,'unicode':'passed','flood_bytes':result['flood_returned_bytes'],'flood_pages':flood['pages'],'truncated':flood['truncated'],'same_handle_restore':True,'foreign_room':409},ensure_ascii=False))
