"""归档结束回执和最终差异；不会修改通过状态。仅在当前源码最终稳定后运行。"""
from pathlib import Path
import hashlib,json,shutil,difflib
ROOT=Path(r'D:\coolzhuagent');DEST=ROOT/'docs/plans/harness-alignment/evidence/HA-09/20261018';DEST.mkdir(parents=True,exist_ok=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def copy(src,dst):
    dst.parent.mkdir(parents=True,exist_ok=True)
    if dst.exists():assert sha(src)==sha(dst),str(dst)
    else:shutil.copy2(src,dst)
for run in (ROOT/'tmp/ha09-20261018-runs').iterdir():
    r=json.loads((run/'receipt.json').read_bytes())
    if 'finished_at' in r:
        for src in run.iterdir():
            if src.is_file():copy(src,DEST/'runs'/run.name/src.name)
base=ROOT/'tmp/ha09-20261018-before';w='modules/gui-web/packages/web-console/src/';rows=[];diff=[]
for name in ['main.rs','goal_command.rs','goal_verification.rs','ha09_tests.rs']:
    a=base/name;b=ROOT/w/name;before=a.read_text(encoding='utf8') if a.exists() else '';after=b.read_text(encoding='utf8')
    rows.append({'path':w+name,'before_sha256':sha(a) if a.exists() else None,'after_sha256':sha(b)})
    diff.extend(difflib.unified_diff(before.splitlines(True),after.splitlines(True),fromfile='before/'+w+name if a.exists() else '/dev/null',tofile='after/'+w+name))
if (DEST/'implementation-inputs.json').exists():assert json.loads((DEST/'implementation-inputs.json').read_bytes())==rows,'禁止覆盖已归档的最终差异'
else:
    (DEST/'implementation-inputs.json').write_text(json.dumps(rows,indent=2),encoding='utf8');(DEST/'implementation.patch').write_text(''.join(diff),encoding='utf8')
for name in ['ha09_run.py','ha09_regression.py','ha09_archive.py','ha09_close_partial.py']:copy(ROOT/'tmp'/name,DEST/'tools'/name)
print(json.dumps({'runs':sorted(p.name for p in (DEST/'runs').iterdir()),'changed_files':len(rows)}))
