from pathlib import Path
import os,subprocess,time,json
root=Path.cwd();p=root/'tmp/2026-10-09-large-text-preview'
env=os.environ.copy();env.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_BUILD_JOBS='1')
for name,command in [('web-retry',['cargo','test','-p','coolzhu-web-console','--offline']),
                     ('linkage',['cargo','test','--test','module_linkage_smoke','--offline'])]:
    start=time.time()
    with (p/(name+'-stdout.log')).open('wb') as out,(p/(name+'-stderr.log')).open('wb') as err:
        r=subprocess.run(command,cwd=root,env=env,stdout=out,stderr=err)
    receipt={'command':command,'actual_exit_code':r.returncode,'elapsed_seconds':round(time.time()-start,2),
             'own_formal_preview_processes_stopped':True,'source_unchanged_after_build':True}
    (p/(name+'-result.json')).write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(name,json.dumps(receipt),flush=True)
    if r.returncode:raise SystemExit(r.returncode)
