from pathlib import Path
import json,subprocess,sys,time
p=Path(__file__).resolve().parent;root=p.parents[1];kind=sys.argv[1]
commands={'build':['cargo','build','-p','coolzhu-web-console','--offline'],'web':['cargo','test','-p','coolzhu-web-console','--offline'],'linkage':['cargo','test','--test','module_linkage_smoke','--offline']}
cmd=commands[kind];started=time.time()
with (p/(kind+'-stdout.log')).open('wb') as out,(p/(kind+'-stderr.log')).open('wb') as err:
 result=subprocess.run(cmd,cwd=root,stdout=out,stderr=err)
(p/(kind+'-result.json')).write_text(json.dumps({'command':cmd,'exit_code':result.returncode,'seconds':time.time()-started},indent=2)+'\n',encoding='utf-8')
print(json.dumps({'kind':kind,'exit_code':result.returncode,'seconds':time.time()-started}),flush=True)
raise SystemExit(result.returncode)
