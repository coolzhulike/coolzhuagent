"""最后仅排版后再构建，保留实测候选原构建日志与二进制摘要。"""
from pathlib import Path
import os,time,subprocess,json,hashlib
p=Path(__file__).resolve().parent;root=p.parents[1]
env=os.environ.copy();env.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_BUILD_JOBS='1')
command=['cargo','build','-p','coolzhu-core-runtime','-p','coolzhu-web-console','--offline']
started=time.time()
with (p/'final-build-stdout.log').open('wb') as out,(p/'final-build-stderr.log').open('wb') as err:
    result=subprocess.run(command,cwd=root,env=env,stdout=out,stderr=err)
record={'command':command,'actual_exit_code':result.returncode,'elapsed_seconds':round(time.time()-started,2),'source_memory_sha256':hashlib.sha256((root/'modules/core-runtime/packages/core-runtime/src/memory.rs').read_bytes()).hexdigest()}
if result.returncode==0:record['binary_sha256']=hashlib.file_digest((root/'target/debug/coolzhu-web-console.exe').open('rb'),'sha256').hexdigest()
(p/'final-build-result.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8')
print(json.dumps(record),flush=True);raise SystemExit(result.returncode)
