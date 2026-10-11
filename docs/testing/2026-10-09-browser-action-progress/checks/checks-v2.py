from pathlib import Path
import json,os,subprocess,sys,time
p=Path(__file__).resolve().parent;root=p.parents[1];kind=sys.argv[1]; command_kind=kind.removesuffix('-v2')
commands={
 'web-build':['cargo','build','-p','coolzhu-web-console','--offline','-j','1'],
 'shell-build':['cargo','build','--manifest-path','modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml','--offline','-j','1'],
 'protocol':['cargo','test','-p','coolzhu-native-browser-protocol','--offline','-j','1','--','--test-threads=1'],
 'web':['cargo','test','-p','coolzhu-web-console','--offline','-j','1','--','--test-threads=1'],
 'shell':['cargo','test','--manifest-path','modules/gui-desktop/packages/tauri-shell/src-tauri/Cargo.toml','--offline','-j','1','--','--test-threads=1'],
 'linkage':['cargo','test','--test','module_linkage_smoke','--offline','-j','1','--','--test-threads=1']}
env=os.environ.copy();env.update(CARGO_PROFILE_TEST_DEBUG='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_BUILD_JOBS='1')
start=time.time()
with (p/(kind+'-stdout.log')).open('wb') as out,(p/(kind+'-stderr.log')).open('wb') as err:
 result=subprocess.run(commands[command_kind],cwd=root,env=env,stdout=out,stderr=err)
facts={'command':commands[command_kind],'exit_code':result.returncode,'seconds':time.time()-start,
 'environment_overrides':{k:env[k] for k in ('CARGO_PROFILE_TEST_DEBUG','CARGO_PROFILE_DEV_DEBUG','CARGO_BUILD_JOBS')}}
(p/(kind+'-result.json')).write_text(json.dumps(facts,indent=2)+'\n',encoding='utf-8')
print(json.dumps(facts),flush=True);raise SystemExit(result.returncode)
