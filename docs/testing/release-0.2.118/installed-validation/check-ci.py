from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;root=p.parents[1];source=(p/'source-commit.txt').read_text(encoding='utf-8').strip()
r=subprocess.run(['python','tmp/2026-10-05-devin-host/with-gh.py','api','repos/coolzhulike/coolzhuagent/actions/runs?head_sha='+source+'&per_page=10'],cwd=root,capture_output=True)
assert r.returncode==0,r.stderr.decode(errors='replace')
(p/'source-ci.json').write_bytes(r.stdout)
d=json.loads(r.stdout);print(json.dumps([{'id':x['id'],'status':x['status'],'conclusion':x['conclusion'],'head_sha':x['head_sha']} for x in d['workflow_runs']],ensure_ascii=False))
