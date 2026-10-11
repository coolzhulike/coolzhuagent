from pathlib import Path
root=Path(__file__).resolve().parents[2];p=Path(__file__).resolve().parent;old=root/'tmp/2026-10-08-release-115';scope=root/'tmp/2026-10-09-session-config-scope'
for name in ['verify-115.py','install-115.ps1','start-standard115.ps1','restore-original-shell.ps1','check-original.py']:
    text=(old/name).read_text(encoding='utf-8').replace('0.2.115','0.2.116').replace('2026-10-08-release-115','2026-10-09-release-116').replace('115','116')
    (p/name.replace('115','116')).write_text(text,encoding='utf-8')
text=(old/'stop-standard114.ps1').read_text(encoding='utf-8').replace('tmp/2026-10-08-session-config-service/original-restored-receipt.json','tmp/2026-10-08-release-115/original-restored-receipt.json').replace('tmp/2026-10-08-release-115/stopped114-receipt.json','tmp/2026-10-09-release-116/stopped115-receipt.json').replace('114','115')
(p/'stop-standard115.ps1').write_text(text,encoding='utf-8')
formal=p/'scope';formal.mkdir(exist_ok=False)
for name in ['race.py','save-scope.py','restart.py']:
    text=(scope/name).read_text(encoding='utf-8')
    # scope目录比原驱动深一层；真实Program Files二进制必须显式替换。
    text=text.replace('root=Path(__file__).resolve().parents[2]','root=Path(__file__).resolve().parents[3]').replace('root=p.parents[1]','root=p.parents[2]')
    text=text.replace("root/'target/debug/coolzhu-web-console.exe'","Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe')")
    (formal/name).write_text(text,encoding='utf-8')
for name in ['start-isolated-shell.ps1','stop-isolated-shell.ps1']:
    text=(scope/name).read_text(encoding='utf-8').replace('tmp/2026-10-09-session-config-scope/save-scope','tmp/2026-10-09-release-116/scope/save-scope').replace('tmp/2026-10-08-release-115/installed-115-verification.json','tmp/2026-10-09-release-116/installed-116-verification.json').replace('正式115壳已打开新候选后台隔离设置页，不计正式115源码修复','正式116壳与正式116后台打开隔离配置页')
    (p/name).write_text(text,encoding='utf-8')
print('116正常安装/启动与正式EXE专项驱动已准备')
