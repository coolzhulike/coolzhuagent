from pathlib import Path
import subprocess,json,sys
p=Path(__file__).resolve().parent;p.mkdir(parents=True,exist_ok=True)
mode=sys.argv[1];assert mode in ['build','test']
command=['cargo',mode,'-p','coolzhu-web-console','--offline']
with (p/(mode+'.log')).open('wb') as log:result=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT)
(p/(mode+'-result.json')).write_text(json.dumps({'command':command,'exit_code':result.returncode})+'\n')
print((p/(mode+'.log')).read_text(encoding='utf-8',errors='replace')[-1800:]);sys.exit(result.returncode)
