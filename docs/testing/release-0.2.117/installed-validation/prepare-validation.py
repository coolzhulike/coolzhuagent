from pathlib import Path
p=Path(__file__).resolve().parent;root=p.parents[1];old=root/'tmp/2026-10-09-session-config-late'
(p/'verify-late.py').write_text((old/'verify.py').read_text(encoding='utf-8'),encoding='utf-8')
stop=(root/'tmp/2026-10-09-release-116/stop-standard115.ps1').read_text(encoding='utf-8-sig').replace('tmp/2026-10-08-release-115/original-restored-receipt.json','tmp/2026-10-09-release-116/original-restored-receipt.json').replace('tmp/2026-10-09-release-116/stopped115-receipt.json','tmp/2026-10-09-release-117/stopped116-receipt.json').replace('115','116')
(p/'stop-standard116.ps1').write_text(stop,encoding='utf-8')
start=(root/'tmp/2026-10-09-release-116/start-standard116.ps1').read_text(encoding='utf-8-sig').replace('release-116','release-117').replace('0.2.116','0.2.117').replace('installed-116','installed-117')
(p/'start-standard117.ps1').write_text(start,encoding='utf-8')
check=(root/'tmp/2026-10-09-release-116/check-original.py').read_text(encoding='utf-8')
(p/'check-original.py').write_text(check,encoding='utf-8')
print('117正式复验驱动与按身份停止/启动脚本已准备；尚未执行安装或改变正式运行环境')
