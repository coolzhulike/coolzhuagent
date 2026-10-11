from pathlib import Path
import json,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1]
out=root/'tmp/2026-10-09-plugin-frozen-permission-candidate';out.mkdir(exist_ok=True)
assert not (out/'before.json').exists()
for name in ['common.py','prepare.py','restore.py','send.py','watch.py','collect.py','server.py']:
    text=(p/name).read_text(encoding='utf-8')
    text=text.replace('PLUGIN-FROZEN-PERMISSION-117-20261009','PLUGIN-FROZEN-PERMISSION-CANDIDATE-20261009').replace('正式117','源码候选').replace('/frozen-117','/frozen-candidate')
    if name=='common.py':
        text=text.replace("for binary,key in [('coolzhu-web-console.exe','web_sha256'),('coolzhu-tauri-shell.exe','shell_sha256')]:", "for binary,key in [('coolzhu-tauri-shell.exe','shell_sha256')]:")
        text=text.replace('    with connection() as c:\n', "    receipt=json.loads((p/'candidate-process.json').read_text(encoding='utf-8-sig'))\n    assert hashlib.sha256(Path(receipt['binary']).read_bytes()).hexdigest()==receipt['sha256']\n    v={**v,'source_commit':'d094c7a + ACP不可续接审批收尾候选','candidate_backend':receipt}\n    with connection() as c:\n")
        text=text.replace('        if require_unlocked:assert not any(x[2] for x in b)', "        if require_unlocked and any(x[2] for x in b):\n            assert [x[2] for x in b if x[2]]==['af87b8cedd2e560bd23826dd5a70813a86c07df0993bfec7']\n            old=c.execute(\"SELECT state,protocol_stop,process_drained FROM devin_acp_attempts WHERE attempt_id=?\",(b[0][2],)).fetchone()\n            assert old==('unknown','end_turn',1)")
    if name=='collect.py':
        text=text.replace("'version':'0.2.117','source_commit':'d094c7ae6e9def40bb4c0bfb6ed81d33bbb3f0ae'", "'version':'源码候选/正式117壳','backend':json.loads((p/'candidate-process.json').read_text(encoding='utf-8-sig'))")
    (out/name).write_text(text,encoding='utf-8')
print(out)
