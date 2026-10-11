from pathlib import Path
import shutil,subprocess,json
p=Path(__file__).resolve().parent;root=p.parents[1]
bin=p/'bin';bin.mkdir(exist_ok=True)
assert not (bin/'dsh-runtime').exists()
shutil.copytree(Path('C:/Program Files/CoolzhuAgent/bin/dsh-runtime'),bin/'dsh-runtime')
for name in ['package.json','package-lock.json','src/host.mjs','src/process.mjs','src/source_imports.mjs']:
    shutil.copyfile(root/'modules/tooling/packages/dsh-plugin-host'/name,bin/'dsh-runtime/host'/name)
r=subprocess.run(['python','scripts/prepare-dsh-runtime.py','--verify',str(bin/'dsh-runtime')],cwd=root,capture_output=True,text=True)
(p/'runtime-verification.json').write_text(r.stdout,encoding='utf-8')
assert r.returncode==0,r.stderr
shutil.copyfile(root/'target/debug/coolzhu-web-console.exe',bin/'coolzhu-web-console.exe')
print(r.stdout)
