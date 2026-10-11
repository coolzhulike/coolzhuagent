from pathlib import Path
import json,subprocess
p=Path(__file__).resolve().parent;root=p.parents[1]
source=(p/'source-commit.txt').read_text(encoding='utf-8-sig').strip()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()==source
assert not subprocess.check_output(['git','diff','--name-only'],cwd=root,text=True).strip()
with (p/'build-stdout-replay.log').open('wb') as out,(p/'build-stderr-replay.log').open('wb') as err:
    result=subprocess.run(['C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe','-NoProfile','-File','tmp/2026-10-09-release-118/child-build.ps1'],cwd=root,stdout=out,stderr=err)
facts={'version':'0.2.118','source_commit':source,'exit_code':result.returncode,'stdout':'build-stdout-replay.log','stderr':'build-stderr-replay.log'}
(p/'build-result.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(facts,ensure_ascii=False),flush=True)
raise SystemExit(result.returncode)
