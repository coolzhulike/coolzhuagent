from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;root=p.parents[1];results=[]
for stage,args in [('build',['cargo','build','-p','coolzhu-web-console','--offline']),('test',['cargo','test','-p','coolzhu-web-console','--offline'])]:
    with (p/(stage+'.log')).open('wb') as out:
        result=subprocess.run(args,cwd=root,stdout=out,stderr=subprocess.STDOUT)
    results.append({'stage':stage,'command':args,'actual_exit_code':result.returncode})
    (p/'build-test-result.json').write_text(json.dumps(results,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(stage,result.returncode,flush=True)
    if result.returncode:raise SystemExit(result.returncode)
