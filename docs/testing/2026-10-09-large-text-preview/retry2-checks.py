from pathlib import Path
import os,subprocess,time,json,hashlib
root=Path.cwd();p=root/'tmp/2026-10-09-large-text-preview'
env=os.environ.copy();env.update(CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0',CARGO_BUILD_JOBS='1')
sources={str(f).replace('\\','/'):hashlib.sha256(f.read_bytes()).hexdigest() for f in [
 Path('modules/gui-web/packages/web-console/src/main.rs'),
 Path('modules/gui-web/packages/web-console/src/app.js'),
 Path('modules/gui-web/packages/web-console/src/project_file_snapshot.rs')]}
for name,command in [('web-retry2',['cargo','test','-p','coolzhu-web-console','--offline']),
                     ('linkage',['cargo','test','--test','module_linkage_smoke','--offline'])]:
    assert not (p/(name+'-result.json')).exists()
    start=time.time()
    with (p/(name+'-stdout.log')).open('wb') as out,(p/(name+'-stderr.log')).open('wb') as err:
        r=subprocess.run(command,cwd=root,env=env,stdout=out,stderr=err)
    unchanged=all(hashlib.sha256(Path(f).read_bytes()).hexdigest()==h for f,h in sources.items())
    receipt={'command':command,'actual_exit_code':r.returncode,'elapsed_seconds':round(time.time()-start,2),
             'own_formal_preview_processes_stopped':True,'source_sha256':sources,'source_unchanged':unchanged}
    (p/(name+'-result.json')).write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    print(name,json.dumps(receipt),flush=True)
    if r.returncode or not unchanged:raise SystemExit(r.returncode or 1)
