from pathlib import Path
import json,urllib.request,urllib.parse,urllib.error,hashlib,sqlite3,datetime
root=Path.cwd();p=root/'tmp/2026-10-09-large-text-preview';w=p/'workspace'
pre=json.loads((p/'candidate-preflight.json').read_text());api='http://127.0.0.1:8767'
raw=p/'api';raw.mkdir(exist_ok=True)
def sha(data):return hashlib.sha256(data).hexdigest()
def call(name,path,method='GET',payload=None,expected_status=200):
    req=urllib.request.Request(api+path,data=None if payload is None else json.dumps(payload).encode(),
       method=method,headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(req,timeout=20) as r:status=r.status;data=r.read()
    except urllib.error.HTTPError as e:status=e.code;data=e.read()
    (raw/(name+'.json')).write_bytes(data)
    assert status==expected_status,(name,status,data[:500])
    return json.loads(data)
workspace=call('workspace','/api/workspace')
assert Path(workspace['workspace']).resolve()==w.resolve(),workspace
filename='sbom-0.2.125-files.cdx.json';original=(w/filename).read_bytes()
query='path='+urllib.parse.quote(filename)
meta=call('sbom-meta','/api/project/file/meta?'+query)
assert meta['previewable'] and not meta['editable'] and not meta['binary']
assert meta['file_size']==len(original) and meta['revision']==sha(original)
pages=[];offset=0;assembled=b''
while True:
    page=call(f'sbom-byte-page-{len(pages)+1:02d}','/api/project/file?'+query+f'&offset={offset}&limit=262144')
    data=page['content'].encode()
    assert page['meta']['revision']==meta['revision'] and page['bytes_read']==len(data)<=262144
    assert page['offset']==offset
    assembled+=data;pages.append({'offset':offset,'bytes':len(data),'next_offset':page['next_offset'],'sha256':sha(data)})
    if page['next_offset'] is None:break
    assert page['next_offset']>offset
    offset=page['next_offset']
assert assembled==original
lines=original.decode().splitlines()
tail=call('sbom-tail-lines','/api/project/file?'+query+f'&start_line={len(lines)-400}&end_line={len(lines)+400}')
assert tail['total_lines']==len(lines) and tail['content']=='\n'.join(lines[tail['start_line']-1:tail['end_line']])
assert tail['end_line']==len(lines)
bounded=call('sbom-line-range-capped','/api/project/file?'+query+'&start_line=1&end_line=999999')
assert bounded['end_line']==801 and bounded['content']=='\n'.join(lines[:801])
assert bounded['bytes_read']<=262144

blank='blank-lines-utf8-bom.txt'
blank_bytes=(w/blank).read_bytes()
blank_meta=call('blank-meta','/api/project/file/meta?path='+blank)
assert blank_meta['has_utf8_bom'] and blank_meta['revision']==sha(blank_bytes) and blank_meta['editable']
blank_lines=call('blank-lines','/api/project/file?path='+blank+'&start_line=1&end_line=4')
assert blank_lines['content']=='\n\n竹林\n'
oversize=call('oversize-meta','/api/project/file/meta?path=oversize-sparse.txt')
assert not oversize['previewable'] and not oversize['editable'] and oversize['revision']=='' and oversize['encoding']=='unknown'
assert oversize['file_size']==64*1024*1024+1
call('oversize-read','/api/project/file?path=oversize-sparse.txt',expected_status=413)
call('long-line-window','/api/project/file?path=long-single-line.txt&start_line=1&end_line=1',expected_status=413)
long=call('long-line-byte-page','/api/project/file?path=long-single-line.txt&offset=0&limit=262144')
assert long['bytes_read']<=262144 and long['next_offset'] is not None and long['content'].encode()==(w/'long-single-line.txt').read_bytes()[:long['bytes_read']]
call('large-write-rejected','/api/project/file',method='PUT',payload={
 'expected_workspace':workspace['workspace'],'path':filename,'content':'禁止覆盖只读大文件',
 'revision':meta['revision']},expected_status=413)
assert (w/filename).read_bytes()==original

# 普通小文件的版本冲突仍有效；只操作自有副本，外部改动不应被旧版本请求覆盖。
external=b'external change\n';(w/blank).write_bytes(external)
call('small-write-conflict','/api/project/file',method='PUT',payload={
 'expected_workspace':workspace['workspace'],'path':blank,'content':'stale write',
 'revision':blank_meta['revision']},expected_status=409)
assert (w/blank).read_bytes()==external
(w/blank).write_bytes(blank_bytes)

source=Path(pre['original_workspace'])
assert sha((source/'coolzhu.toml').read_bytes())==pre['source_config_sha256']
for location in [source,w]:
    with sqlite3.connect((location/'.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
        counts={t:c.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in pre['original_counts']}
        assert counts==pre['original_counts'],(str(location),counts)
        bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(pre['session_id'],)).fetchall()
        assert sorted(bindings)==sorted(tuple(b) for b in pre['original_bindings'])
receipt={'recorded_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'passed':True,
 'stage':'源码候选真实SQLite独立副本API专项；非原生GUI验收','full_sbom_bytes':len(original),
 'full_sbom_sha256':sha(original),'byte_pages':pages,'full_bytes_reconstructed':True,
 'total_lines':len(lines),'tail_range':[tail['start_line'],tail['end_line']],
 'row_window_cap':801,'row_response_max_bytes':262144,'preview_total_max_bytes':64*1024*1024,
 'large_file_editable':False,'bom_and_blank_lines_preserved':True,
 'oversize_read_status':413,'long_line_window_status':413,'large_write_status':413,
 'small_write_conflict_status':409,'source_config_and_runtime_counts_unchanged':True,
 'new_model_calls':0,'new_cloud_sessions':0,'native_gui_passed':False}
(p/'live-api-verification.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(receipt,ensure_ascii=False))
