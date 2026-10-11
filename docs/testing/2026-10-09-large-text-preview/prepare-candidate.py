from pathlib import Path
import json,sqlite3,shutil,hashlib,re
root=Path.cwd();p=root/'tmp/2026-10-09-large-text-preview'
source=root/'tmp/2026-10-04-devin-models/workspace'
candidate=p/'workspace';assert not any(x.is_file() for x in candidate.rglob('*'));(candidate/'.coolzhu').mkdir(parents=True,exist_ok=True)
facts=json.loads((root/'docs/testing/2026-10-09-memory-data-boundary/attachment-browser/facts.json').read_text())
sid=facts['run']['session_id']
with sqlite3.connect((source/'.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:
    active=c.execute("SELECT id,state FROM runtime_runs WHERE session_id=? AND finished_at IS NULL AND state IN ('queued','running')",(sid,)).fetchall()
    assert not active, active
    counts={t:c.execute('SELECT count(*) FROM '+t).fetchone()[0] for t in ['runtime_runs','devin_acp_attempts','chat_room_messages']}
    bindings=c.execute('SELECT lane,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id=?',(sid,)).fetchall()
    remote=[b for b in bindings if b[1] is not None]
    assert len(remote)==1 and remote[0][1]=='island-kayak' and all(b[2] is None for b in bindings)
    with sqlite3.connect(candidate/'.coolzhu/web-sessions.sqlite3') as dest:c.backup(dest)
config=(source/'coolzhu.toml').read_text(encoding='utf-8-sig')
assert re.search(r'bind_addr = "127\.0\.0\.1:876[57]"',config)
config=re.sub(r'bind_addr = "127\.0\.0\.1:876[57]"','bind_addr = "127.0.0.1:8767"',config)
(candidate/'coolzhu.toml').write_text(config,encoding='utf-8')
shutil.copy2(source/'.coolzhu/web-sessions.json',candidate/'.coolzhu/web-sessions.json')
shutil.copy2(source/'sbom-0.2.125-files.cdx.json',candidate/'sbom-0.2.125-files.cdx.json')
with (candidate/'oversize-sparse.txt').open('wb') as f:f.truncate(64*1024*1024+1)
(candidate/'long-single-line.txt').write_text('竹'*(256*1024//3+1),encoding='utf-8')
(candidate/'blank-lines-utf8-bom.txt').write_text('\ufeff\n\n竹林\n\n玉石\n',encoding='utf-8')
record={'original_workspace':str(source),'candidate_workspace':str(candidate),
 'session_id':sid,'room_id':facts['run']['chat_room_id'],'original_counts':counts,
 'sole_remote_session':'island-kayak','active_session_runs':0,'model_calls':0,
 'source_config_sha256':hashlib.sha256((source/'coolzhu.toml').read_bytes()).hexdigest(),
 'original_bindings':bindings,'passed':True}
(p/'candidate-preflight.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in record.items() if k not in ['session_id','room_id','original_bindings']},ensure_ascii=False))
