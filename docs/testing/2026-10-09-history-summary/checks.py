"""保存真实子进程退出；复用现有低内存编译环境。"""
from pathlib import Path
import os,sys,json,subprocess,time,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1]
kind=sys.argv[1]
commands={'build':['cargo','build','-p','coolzhu-web-console','--offline'], 'web':['cargo','test','-p','coolzhu-web-console','--offline'], 'registry':['cargo','check','-p','coolzhu-tool-registry','--offline'], 'linkage':['cargo','test','--test','module_linkage_smoke','--offline']}
command=commands[kind]
env=os.environ.copy();env.update(CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_BUILD_JOBS='1')
started=time.time()
with (p/(kind+'-stdout.log')).open('wb') as out,(p/(kind+'-stderr.log')).open('wb') as err:
 completed=subprocess.run(command,cwd=root,env=env,stdout=out,stderr=err)
result={'command':command,'actual_exit_code':completed.returncode,'elapsed_seconds':round(time.time()-started,2),'environment_overrides':{k:env[k] for k in ('CARGO_PROFILE_TEST_DEBUG','CARGO_PROFILE_DEV_DEBUG','CARGO_BUILD_JOBS')}}
if kind=='build' and completed.returncode==0:
 result['binary_sha256']=hashlib.file_digest((root/'target/debug/coolzhu-web-console.exe').open('rb'),'sha256').hexdigest()
(p/(kind+'-result.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(result),flush=True);raise SystemExit(completed.returncode)
