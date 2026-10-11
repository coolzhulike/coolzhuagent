"""保存真实退出和完整日志；低内存配置不改变产品语义。"""
from pathlib import Path
import os,sys,json,subprocess,time,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1];p.mkdir(exist_ok=True)
commands={'build':['cargo','build','-p','coolzhu-core-runtime','-p','coolzhu-web-console','--offline'],
          'core':['cargo','test','-p','coolzhu-core-runtime','--offline'],
          'web':['cargo','test','-p','coolzhu-web-console','--offline'],
          'linkage':['cargo','test','--test','module_linkage_smoke','--offline']}
kind=sys.argv[1];command=commands[kind]
env=os.environ.copy();overrides={'CARGO_PROFILE_DEV_DEBUG':'0','CARGO_PROFILE_TEST_DEBUG':'0','CARGO_BUILD_JOBS':'1'};env.update(overrides)
started=time.time()
with (p/(kind+'-stdout.log')).open('wb') as out,(p/(kind+'-stderr.log')).open('wb') as err:
    completed=subprocess.run(command,cwd=root,env=env,stdout=out,stderr=err)
result={'command':command,'actual_exit_code':completed.returncode,'elapsed_seconds':round(time.time()-started,2),'environment_overrides':overrides}
if kind=='build' and completed.returncode==0:
    result['binary_sha256']=hashlib.file_digest((root/'target/debug/coolzhu-web-console.exe').open('rb'),'sha256').hexdigest()
    result['shell_sha256']=hashlib.file_digest(Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-tauri-shell.exe').open('rb'),'sha256').hexdigest()
(p/(kind+'-result.json')).write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(result),flush=True);raise SystemExit(completed.returncode)
