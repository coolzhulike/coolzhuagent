from pathlib import Path
p=Path(__file__).resolve().parent
text=(p/'scope/save-scope.py').read_text(encoding='utf-8').replace("folder=p/'save-scope'","folder=p/'save-scope2'")
old="                config=tomllib.loads((spaces[0]/'coolzhu.toml').read_text(encoding='utf-8-sig'))"
assert old in text
text=text.replace(old,"                try:config=tomllib.loads((spaces[0]/'coolzhu.toml').read_text(encoding='utf-8-sig'))\n                except PermissionError:\n                    time.sleep(.003);continue")
(p/'scope/save-scope2-driver.py').write_text(text,encoding='utf-8')
text=(p/'scope/restart.py').read_text(encoding='utf-8').replace("f=p/'save-scope'","f=p/'save-scope2'")
(p/'scope/restart2.py').write_text(text,encoding='utf-8')
for name in ['start-isolated-shell.ps1','stop-isolated-shell.ps1']:
    text=(p/name).read_text(encoding='utf-8').replace('scope/save-scope','scope/save-scope2')
    (p/name.replace('.ps1','2.ps1')).write_text(text,encoding='utf-8')
old=root=None
text=(p.parent/'2026-10-08-release-115/stop-original-shell.ps1').read_text(encoding='utf-8').replace('2026-10-08-release-115','2026-10-09-release-116').replace('115','116')
(p/'stop-original-shell.ps1').write_text(text,encoding='utf-8')
print('只读观察器临时OS共享占用在原2秒期限内重读；保存和切换均不重发。首失败保留。')
