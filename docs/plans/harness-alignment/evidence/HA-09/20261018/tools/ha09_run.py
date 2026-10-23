"""保存真实执行回执；不修改台账或历史证据。"""
from pathlib import Path
from datetime import datetime, timezone
import subprocess, sys, os, json, hashlib, time

root=Path(r'D:\coolzhuagent')
label=sys.argv[1]; command=sys.argv[2:]
out=root/'tmp'/'ha09-20261018-runs'/label
out.mkdir(parents=True, exist_ok=False)
def now(): return datetime.now(timezone.utc).isoformat()
paths=list((root/'modules/gui-web/packages/web-console/src').rglob('*.rs'))+list((root/'modules/llm-adapter/packages/llm-adapter').rglob('*.rs'))
paths+=list((root/'modules/cli/packages/command-line/src').rglob('*.rs'))
paths += [root/'Cargo.lock',root/'modules/llm-adapter/packages/llm-adapter/Cargo.toml']
sources=[{'path':p.relative_to(root).as_posix(),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths]
env=os.environ.copy(); env['CARGO_BUILD_JOBS']='1'; env['CARGO_PROFILE_TEST_DEBUG']='0'
env['COOLZHU_HARNESS_EVIDENCE_DIR']=str(out)
receipt={'command':command,'cwd':str(root),'started_at':now(),'environment':{'CARGO_BUILD_JOBS':'1','CARGO_PROFILE_TEST_DEBUG':'0'},'sources':sources}
if 'CARGO_PROFILE_DEV_DEBUG' in env: receipt['environment']['CARGO_PROFILE_DEV_DEBUG']=env['CARGO_PROFILE_DEV_DEBUG']
(out/'receipt.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2),encoding='utf-8')
start=time.monotonic()
with (out/'output.log').open('wb') as log:
    proc=subprocess.Popen(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT)
    receipt['pid']=proc.pid
    (out/'receipt.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2),encoding='utf-8')
    print(f'pid={proc.pid} log={out / "output.log"}',flush=True)
    code=proc.wait()
receipt.update(exit_code=code,finished_at=now(),duration_seconds=round(time.monotonic()-start,3))
receipt['source_changed_during_run']=[s['path'] for s in sources if hashlib.sha256((root/s['path']).read_bytes()).hexdigest()!=s['sha256']]
(out/'receipt.json').write_text(json.dumps(receipt,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in receipt.items() if k!='sources'},ensure_ascii=False),flush=True)
print((out/'output.log').read_bytes().decode('utf-8',errors='replace')[-6000:])
sys.exit(code)
