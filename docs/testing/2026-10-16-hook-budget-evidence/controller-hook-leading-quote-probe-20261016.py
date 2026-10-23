"""验证cmd外层引号契约，仅调用复制的where.exe查找本地cmd.exe。"""
import json, os, pathlib, shutil, subprocess
root=pathlib.Path(r'D:\coolzhuagent\tmp\goal-32wbs-acceptance-20261013')
scratch=root/'hook-budget-20261016'
exe=scratch/'quoted helper.exe'
shutil.copy2(pathlib.Path(os.environ['SystemRoot'])/'System32/where.exe',exe)
shell=str(pathlib.Path(os.environ['SystemRoot'])/'System32/cmd.exe')
command=f'"{exe}" "cmd.exe"'
results=[]
for name,line in [('raw',f'"{shell}" /D /C {command}'),('wrapped',f'"{shell}" /D /S /C "{command}"')]:
    p=subprocess.run(line,cwd=scratch,stdout=subprocess.PIPE,stderr=subprocess.PIPE,creationflags=subprocess.CREATE_NO_WINDOW)
    results.append(dict(name=name,line=line,exit_code=p.returncode,stdout=p.stdout.decode('gb18030',errors='replace'),stderr=p.stderr.decode('gb18030',errors='replace')))
path=root/'controller-hook-leading-quote-probe-20261016.json'
path.write_text(json.dumps(results,ensure_ascii=False,indent=2),encoding='utf-8')
print(path.read_text(encoding='utf-8'))
