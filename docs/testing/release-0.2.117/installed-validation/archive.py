from pathlib import Path
import hashlib,json,shutil,sqlite3
p=Path(__file__).resolve().parent;root=p.parents[1];dest=root/'docs/testing/release-0.2.117';validation=dest/'installed-validation';validation.mkdir(parents=True,exist_ok=True)
def read(f):return json.loads(f.read_text(encoding='utf-8-sig'))
def sha(f):return hashlib.file_digest(f.open('rb'),'sha256').hexdigest()
v=read(p/'installed-117-verification.json');source=v['source_commit']
assert read(p/'build-result.json')['exit_code']==0 and read(p/'install-117-result.json')['exit_code']==0
assert source=='d094c7ae6e9def40bb4c0bfb6ed81d33bbb3f0ae'
runs=read(p/'source-ci.json')['workflow_runs'];assert len(runs)==2 and all(r['head_sha']==source and r['conclusion']=='success' for r in runs)
facts=read(p/'formal/facts.json');assert facts['all_api_checks_passed'] and facts['binary_sha256']==v['web_sha256']
assert read(p/'formal/restart-result.json')['same_parameters_revision_name']
assert read(p/'formal/restart-result.json')['binary_sha256']==v['web_sha256']
counts={}
for label in ['a','b']:
    db=p/'formal'/('workspace-'+label)/'.coolzhu/web-sessions.sqlite3'
    with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:counts[label]=c.execute('SELECT count(*) FROM runtime_runs').fetchone()[0]
assert not any(counts.values())
formal=dest/'scope';formal.mkdir(exist_ok=True)
for f in (p/'formal').iterdir():
    if f.is_file() and f.suffix in ('.json','.txt','.jpg','.py'):shutil.copyfile(f,formal/f.name)
for name in ['source-commit.txt','source-ci.json','build-direct.py','child-build.ps1','build-result.json','build-stdout-replay.log','build-stderr-replay.log','package-117-verification.json','installed-117-verification.json','install-117-result.json','installed-117-standard-processes.json','original-idle.json','verify-117.py','verify-late.py','prepare-validation.py','prepare-restart.py','restart.py','start-isolated-shell.ps1','stop-isolated-shell.ps1','start-standard117.ps1','stop-standard116.ps1','installed-native-main.jpg','archive.py']:
    shutil.copyfile(p/name,validation/name)
(validation/'runtime-counts.json').write_text(json.dumps(counts,indent=2)+'\n',encoding='utf-8')
report=f'''# 0.2.117 改动报告与测试交接

修复旧工程配置页迟到请求错写当前工程：正式116真实复现同会话ID、同revision的A草稿在正常切B后保存200并改B；117将统一配置页绑定完整工程路径、后端工程ID及非凭据配置加载标识。切换、同工程重载及重启后旧请求409，草稿未提交；失败切换不使当前页失效，正常编辑冲突仍由revision决定。无头旧HTTP客户端继续兼容，不宣称所有旧客户端均受保护。

源码冻结 `{source}`，快照 `{v['source_snapshot_digest']}`。正常发布构建实际退出0、六门pass、安装退出0，Program Files共{v['verified_file_count']}文件逐长度/SHA一致。两路同源CI 37910989964/37910984282均success。MSI SHA256 `{v['msi_sha256']}`，{v['msi_bytes']}字节；正式Web SHA `{v['web_sha256']}`，壳SHA `{v['shell_sha256']}`。候选既有Web1423通过/0失败/6既有忽略，另lib8/native-host1；前端22通过/0失败。实际安装/构建/CI原收据见installed-validation，候选及原失败见[专项报告](../2026-10-09-session-config-late/report.md)。

## 实现职责

workspace_activity沿用短时Mutex和RAII pin捕获标识；不持Guard跨await，不增队列/重试/权限。SessionConfigService入口持pin校验X-Coolzhu-Workspace-Id和X-Coolzhu-Configuration-Scope；配置GET/POST、旧容量GET/POST、新建会话覆盖。GET工程及配置响应给标识，完整工程装入后再发布新标识。model_settings仅绑定一次，不自动续期；chat_experience、Devin及免费Provider三个入口传顶栏完整工程路径，路径未就绪时不挂载，避免草稿缓存键误作工程ID。错误提示刷新页面后重新打开，保留草稿。

## 正式安装版验收

16组真实HTTP/SQLite检查全部通过：跨工程旧读取/保存/容量/新建、只带旧工程ID首次读取、A→B→A、同工程重载、进程重启、失败切换不作废标识、非法参数400/旧revision409后释放、无头兼容、有效新建/保存及同scope正常编辑。旧请求之后B配置字节和SQLite会话行保持一致。不是模型回复夹具，不把单元用例当GUI实操证据。

正常原生壳：B温度0.75/容量16384；顶栏正常切A，选择共享会话后0.35/8192；数值框改0.45并正常点击保存，界面已保存、独立读revision12；同正式EXE重启后0.45/8192/name/revision逐项保持。编辑配置的会话与发送对象独立，切A默认发送对象为API检查正常新建的会话，实拍通过正常下拉选回共享会话，不声称自动改发送对象。

![工程B](scope/native-b.jpg)

![工程A](scope/native-a.jpg)

![正式正常保存](scope/native-saved.jpg)

![同正式EXE重启恢复](scope/native-restarted.jpg)

隔离两个SQLite库runtime_runs均0，模型0、新云端0、原库不写。两条长期隔离驱动都正常收尾并取得actual exit0；自有Windows后台terminate子退出1独立保留，不伪写成0。正式117已恢复原SWE-2-medium/revision51/唯一island-kayak，活动轮次0、远端解锁，原生新界面见installed-validation/installed-native-main.jpg。

## 其它未完及下一验收边界

本版仅关闭配置页迟到/ABA/重载/重启子项，不代表完整SessionConfigService/ToolDispatch/SharedRunner/跨进程单写者/outbox/权威epoch完成；新建与参数保存仍两步，其它修改/发现接口另列。Browser复杂长程已有正式证据，但严格新nativeTarget/跨来源commit落在down与up之间、在途观察撤销/HRESULT资源竞争仍开放；116纯截图驱动正常关闭晚9502ms，stopped0，不计严格通过，不改五秒期限或重复简单点击。附件、许可、调度、免费模型独立凭据及多屏等见[总清单](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)。Paint免测、微信不动、Opus暂停。

未签名、预发布、不标latest、不发布自动升级清单；公开资产摘要与实际tag需另核，核验事实随后补记。整体Goal持续进行。
'''
(dest/'change-report-and-test-handoff.md').write_text(report,encoding='utf-8')
entry='117正式增量：旧配置页迟到请求作用域已出包复验。正常构建/安装0、六门pass、1159文件逐SHA一致，冻结d094c7a两路CI success；正式EXE的16组跨工程/ABA/重载/重启/错误释放/无头兼容真实HTTP与存储检查，正常原生B/A参数、保存0.45/revision12和同EXE重启四实拍通过。模型0、新云端0，原SWE/revision51/唯一island恢复。只关闭配置页此子项，完整领域及严格Browser仍开放。[117正式交接](../../testing/release-0.2.117/change-report-and-test-handoff.md)。Goal继续。'
for name in ['docs/analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md','docs/analysis/2026-09-21-integration-review/current-acceptance-queue.md','docs/analysis/2026-09-21-integration-review/implementation-status-and-change-inventory.md','docs/analysis/2026-09-27-four-task-status-and-navigation-addendum.md']:
    f=root/name;first,rest=f.read_text(encoding='utf-8').split('\n',1);row=entry if '2026-09-21-integration-review/' in name else entry.replace('../../testing/','../testing/')
    f.write_text(first+'\n\n'+row+'\n'+rest,encoding='utf-8')
msi=root/v['msi_relative_path'];checksum=msi.with_suffix('.msi.sha256');checksum.write_text(v['msi_sha256']+'  '+msi.name+'\n',encoding='utf-8')
assets=[]
for name in [msi.name,'CoolzhuAgent-0.2.117-installer-report.json','CoolzhuAgent-0.2.117-package-safety.json',checksum.name]:
    f=root/'dist'/name;assets.append({'name':name,'size':f.stat().st_size,'sha256':sha(f)})
(p/'release-assets-local.json').write_text(json.dumps(assets,indent=2)+'\n',encoding='utf-8')
(p/'release-notes.md').write_text('0.2.117预发布：修复模型配置旧页跨工程/ABA/重载/重启后迟到错写；统一页、Devin和免费Provider配置入口绑定工程。正常构建/安装0、六门pass、1159文件逐摘要及两路CI通过；正式16组真实HTTP/存储检查与原生保存/重启四实拍。源码 '+source+'，MSI SHA256 '+v['msi_sha256']+'。Browser严格时序及总矩阵未全闭环，未签名、不标latest。交接：docs/testing/release-0.2.117/change-report-and-test-handoff.md。\n',encoding='utf-8')
text=(root/'tmp/2026-10-09-release-116/verify-release.py').read_text(encoding='utf-8').replace('0.2.116','0.2.117').replace('aa72c2545d3178ec993a01bc8281c7bba1c0d847',source)
(p/'verify-release.py').write_text(text,encoding='utf-8')
for name in ['release-assets-local.json','release-notes.md','verify-release.py']:shutil.copyfile(p/name,validation/name)
attrs=root/'.gitattributes';rule='/docs/testing/release-0.2.117/** -text whitespace=-blank-at-eof,-blank-at-eol,-space-before-tab,cr-at-eol'
if rule not in attrs.read_text(encoding='utf-8'):
    with attrs.open('a',encoding='utf-8') as f:f.write('\n'+rule+'\n')
print(json.dumps({'formal_passed':True,'source_commit':source,'checks':len(facts['checks']),'model_requests':0}))
