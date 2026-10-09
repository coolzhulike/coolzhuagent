from pathlib import Path
import json,os,subprocess,sys,time
p=Path(__file__).resolve().parent;root=p.parents[1];kind=sys.argv[1]
commands={'build-lowmem':['cargo','build','-p','coolzhu-web-console','--offline','-j','1'],
 'web-lowmem':['cargo','test','-p','coolzhu-web-console','--offline','-j','1','--','--test-threads=1'],
 'linkage-lowmem':['cargo','test','--test','module_linkage_smoke','--offline','-j','1']}
env=os.environ.copy();env['CARGO_PROFILE_TEST_DEBUG']='0';env['CARGO_PROFILE_DEV_DEBUG']='0'
env['CARGO_BUILD_JOBS']='1';cmd=commands[kind];started=time.time()
with (p/(kind+'-stdout.log')).open('wb') as out,(p/(kind+'-stderr.log')).open('wb') as err:
 r=subprocess.run(cmd,cwd=root,env=env,stdout=out,stderr=err)
facts={'command':cmd,'exit_code':r.returncode,'seconds':time.time()-started,
 'environment_overrides':{k:env[k] for k in ['CARGO_PROFILE_TEST_DEBUG','CARGO_PROFILE_DEV_DEBUG','CARGO_BUILD_JOBS']}}
(p/(kind+'-result.json')).write_text(json.dumps(facts,indent=2)+'\n',encoding='utf-8')
print(json.dumps(facts),flush=True);raise SystemExit(r.returncode)
