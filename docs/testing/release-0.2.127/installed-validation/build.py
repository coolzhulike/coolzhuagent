from pathlib import Path
import subprocess,json,time,hashlib
root=Path.cwd();p=root/'tmp/2026-10-10-release-127'
assert not (p/'build-result.json').exists()
source=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
assert source=='a3ec4620c67e0bf2ae8e05811d5b198675b719a3'
assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip()
(p/'source-commit.txt').write_text(source+'\n',encoding='utf-8')
start=time.time()
with (p/'build-stdout.log').open('wb') as out,(p/'build-stderr.log').open('wb') as err:
 r=subprocess.run(['C:/Windows/System32/WindowsPowerShell/v1.0/powershell.exe','-NoProfile','-File',str(p/'child-build.ps1')],cwd=root,stdout=out,stderr=err)
result={'version':'0.2.127','source_commit':source,'actual_exit_code':r.returncode,
 'elapsed_seconds':round(time.time()-start,2),'normal_build_msi':True,'configuration':'release',
 'stdout':'build-stdout.log','stderr':'build-stderr.log'}
(p/'build-result.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(result),flush=True)
raise SystemExit(r.returncode)
