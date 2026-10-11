from pathlib import Path
import subprocess,json
p=Path(__file__).resolve().parent;root=p.parents[1];cwd=root/'modules/gui-desktop/packages/tauri-shell/src-tauri'
rows=[]
for stage,cmd in [('build',['cargo','build','-p','coolzhu-tauri-shell','--offline']),('test',['cargo','test','-p','coolzhu-tauri-shell','--offline'])]:
 with (p/(stage+'.log')).open('wb') as log:r=subprocess.run(cmd,cwd=cwd,stdout=log,stderr=subprocess.STDOUT)
 rows.append({'stage':stage,'command':cmd,'actual_exit_code':r.returncode})
 (p/'checks.json').write_text(json.dumps(rows,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
 print(stage,r.returncode,flush=True)
 if r.returncode:raise SystemExit(r.returncode)
