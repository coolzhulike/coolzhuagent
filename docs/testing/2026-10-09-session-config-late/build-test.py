from pathlib import Path
import subprocess,json,shutil
p=Path(__file__).resolve().parent;root=p.parents[1];results=[]
node=shutil.which('node')
assert node,'需真实Node运行既有前端回归'
stages=[('build',['cargo','build','-p','coolzhu-web-console','--offline']),
        ('test',['cargo','test','-p','coolzhu-web-console','--offline']),
        ('frontend',[node,'--test',*[str(f.relative_to(root)) for f in (root/'modules/gui-web/packages/web-console/tests').glob('*.cjs')]])]
for stage,args in stages:
    with (p/(stage+'.log')).open('wb') as out:
        result=subprocess.run(args,cwd=root,stdout=out,stderr=subprocess.STDOUT)
    results.append({'stage':stage,'command':args,'actual_exit_code':result.returncode})
    (p/'build-test-result.json').write_text(json.dumps(results,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(stage,result.returncode,flush=True)
    if result.returncode:raise SystemExit(result.returncode)
