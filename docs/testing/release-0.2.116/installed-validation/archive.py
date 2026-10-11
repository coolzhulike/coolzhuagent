from pathlib import Path
import hashlib,json,shutil,sqlite3,subprocess
p=Path(__file__).resolve().parent;root=p.parents[1];dest=root/'docs/testing/release-0.2.116';dest.mkdir(exist_ok=True);validation=dest/'installed-validation';validation.mkdir(exist_ok=True)
def load(f):return json.loads(f.read_text(encoding='utf-8-sig'))
v=load(p/'installed-116-verification.json');source=v['source_commit'];sha=v['web_sha256']
assert load(p/'build-result.json')['exit_code']==0 and load(p/'install-116-result.json')['exit_code']==0
assert source=='aa72c2545d3178ec993a01bc8281c7bba1c0d847'
counts={}
for case in ['candidate','candidate-interleaved-single','save-scope','save-scope2']:
    f=p/'scope'/case;target=dest/'scope'/case;target.mkdir(parents=True,exist_ok=True)
    assert load(f/'cleanup.json')['gui_hint_restored']
    if case!='save-scope':assert load(f/'facts.json')['binary_sha256']==sha
    for file in f.iterdir():
        if file.is_file() and file.suffix in ('.json','.txt','.jpg','.py'):shutil.copyfile(file,target/file.name)
    for label in ['a','b']:
        db=f/('workspace-'+label)/'.coolzhu/web-sessions.sqlite3'
        with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:counts[case+'/'+label]=c.execute('SELECT count(*) FROM runtime_runs').fetchone()[0]
assert not any(counts.values())
busy=load(p/'scope/candidate/facts.json');inter=load(p/'scope/candidate-interleaved-single/facts.json');save=load(p/'scope/save-scope2/facts.json')
assert busy['passed'] and inter['passed'] and inter['successful_switches']>0 and save['all_passed']
assert inter['mismatch_samples']==0 and save['other_workspace_untouched'] and save['pins_released_after_400_and_409']
assert load(p/'scope/save-scope2/restart-result.json')['binary_sha256']==sha
assert load(p/'scope/save-scope2/restart-cleanup.json')['gui_hint_restored']
for name in ['race.py','save-scope.py','save-scope2-driver.py','restart.py','restart2.py']:shutil.copyfile(p/'scope'/name,validation/name)
for name in ['build-result.json','build-stdout-replay.log','build-stderr-replay.log','build-direct.py','child-build.ps1','source-commit.txt','package-116-verification.json','installed-116-verification.json','install-116-result.json','installed-116-standard-processes.json','original-idle.json','original-restored-receipt.json','verify-116.py','prepare-validation.py','prepare-save-retry.py','start-isolated-shell2.ps1','stop-isolated-shell2.ps1','archive.py']:
    shutil.copyfile(p/name,validation/name)
shutil.copyfile(p/'installed-native-main.jpg',dest/'installed-native-main.jpg')
(validation/'runtime-counts.json').write_text(json.dumps(counts,indent=2)+'\n',encoding='utf-8')
(validation/'driver-first-error.json').write_text(json.dumps({'actual_exit_code':1,'error':'PermissionError只读观察器撞上配置原子写入的Windows文件共享窗口；原驱动finally释放SQLite锁、停止子进程并恢复GUI提示；不计保存/工程切换测试通过','retry_change':'仅只读观察器在原2秒窗口重读，业务保存/切换均不重发'},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
for label,run in [('a','37902125115'),('b','37902120763')]:
    raw=subprocess.check_output(['python','tmp/2026-10-05-devin-host/with-gh.py','run','view',run,'--json','status,conclusion,headSha,url,name'],cwd=root)
    data=json.loads(raw);assert data['status']=='completed' and data['conclusion']=='success' and data['headSha']==source
    (validation/('ci-source-'+label+'.json')).write_bytes(raw)
report=f'''# 0.2.116 改动与定向验收交接

## 修改与风险边界

修复模型配置与工程切换并发时，名称来自旧工程、参数/容量/revision来自新工程的真实混读。SessionConfigService持有既有WorkspacePin至整个HTTP操作及响应派生结束；切换期间配置409，配置进行中切换409，完成或错误返回后释放。保持原锁序、参数规则、权限及SQLite失败恢复，无新队列或缓存。[正式115原失败、候选及方案](../2026-10-09-session-config-scope/report.md)有2041读/180成功切换与六读者48限量混读样本，首准备错误404独立保留。

源码 `{source}`，冻结快照 `{v['source_snapshot_digest']}`，dirty=false。正常release构建实际0、六门pass、正常管理员MSI安装0；Program Files {v['verified_file_count']}项逐长度/SHA一致，CLI116/源码一致；同源两路CI37902125115/37902120763均success。MSI {v['msi_bytes']}字节，SHA256 `{v['msi_sha256']}`。候选完整Web1423/0/6既有忽略，另lib8/native-host1通过，不新增镜像实现的单元测试。

## 正式安装程序复验与用例依据

所有专项使用真实Program Files EXE，SHA `{sha}`；配置竞争零模型，不作为模型连通验证或模型回复夹具。

1. 六路读取持续忙期：{busy['total_reads']}有效读、180切换均409、混读0；停止读者后正常切换200。此组只证明忙期拒绝。
2. 单读者20ms间隔真实交错：{inter['successful_switches']}次切换200、{180-inter['successful_switches']}次409、{inter['total_reads']}次有效读取、{inter['conflicts']}次读取409、混读0；读者停止后切换200。工程A/B相同会话ID但名称、温度、revision及本地容量不同，按整组核对。
3. 真SQLite写锁阻住会话更新，参数已发布期间切换409；释放后保存200。正常切到B，其名称/温度0.75/revision20均未变。非法温度400及旧版本409退出后都能正常切换，pin无泄漏。首只读观察撞Windows共享占用退出1，原cleanup保留；第二轮只改观察器，不改产品期限，不重发业务请求，完整通过。
4. 正常原生设置和滚动，显示[工程B温度0.75/容量16384](scope/save-scope2/native-b.jpg)；正常工程目录输入切换，显示[工程A温度0.35/容量8192](scope/save-scope2/native-a.jpg)。同已安装EXE结束后重启，名称、参数、revision逐项恢复，[重启原生实拍](scope/save-scope2/native-restarted.jpg)通过。复用候选驱动的输出文字仍称“候选”，以实际Program Files路径和EXE SHA为准，未拿debug结果充当正式。
5. 八个隔离SQLite库runtime_runs均0，无模型请求或新云端会话。所有成功有限驱动实际0；Windows terminate子退出1独立记录。[正式原主界面](installed-native-main.jpg)恢复SWE-2-medium/revision51/原聊天室/唯一island-kayak解锁，活动轮次0，原安全库保持。

构建协调收据first_outer_wait文字继承115模板，仅解释115历史，不表示116发生第一次失败；116只有本次正常直接子等待构建、实际0。

## 未完成项

只关闭模型配置操作进行期的工程一致性修复及正式交付。客户端迟到请求expected_workspace/ABA、完整SessionConfig/ToolDispatch/SharedRunner、跨进程单写者/outbox及跨资源崩溃继续开放。Browser复杂九步已有正式证据，但新nativeTarget、跨来源commit严格down/up、在途观察撤销及HRESULT/资源竞争仍未完整实机通过；115最新正常关闭晚于登记16721ms且父轮已完成，不计严格通过。不改时限或注入暂停，不重复简单点击。

其余供应商凭据、附件/许可/记忆/调度/多屏等以[总清单](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)为准。Paint免测、微信不动、Opus暂停。未签名、预发布、不标latest、不发自动升级清单。整体Goal继续。
'''
(dest/'change-report-and-test-handoff.md').write_text(report,encoding='utf-8')
entry=f'116正式增量：模型配置操作和工程选择复用WorkspacePin，覆盖完整响应；修复正式115跨工程参数混读。正常release构建/安装0、六门pass、1159文件逐SHA一致，冻结aa72c25两路CI success；正式EXE忙期180切换409，真实交错{inter["successful_switches"]}成功切换/{inter["total_reads"]}有效读取零混合，SQLite保存窗口409、另一工程不变、400/409后释放、正常原生A/B参数及同EXE重启三实拍通过。首观察器共享占用失败保留；模型0、原SWE/revision51/唯一island恢复。迟到请求/ABA及完整领域迁移和严格Browser仍开放。[116正式交接与实拍](../../testing/release-0.2.116/change-report-and-test-handoff.md)。Goal继续。'
for name in ['docs/analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md','docs/analysis/2026-09-21-integration-review/current-acceptance-queue.md','docs/analysis/2026-09-21-integration-review/implementation-status-and-change-inventory.md','docs/analysis/2026-09-27-four-task-status-and-navigation-addendum.md']:
    f=root/name;first,rest=f.read_text(encoding='utf-8').split('\n',1);row=entry if '2026-09-21-integration-review/' in name else entry.replace('../../testing/','../testing/')
    f.write_text(first+'\n\n'+row+'\n'+rest,encoding='utf-8')
msi=root/v['msi_relative_path'];checksum=msi.with_suffix('.msi.sha256');checksum.write_text(v['msi_sha256']+'  '+msi.name+'\n',encoding='utf-8')
assets=[]
for name in [msi.name,'CoolzhuAgent-0.2.116-installer-report.json','CoolzhuAgent-0.2.116-package-safety.json',checksum.name]:
    f=root/'dist'/name;assets.append({'name':name,'size':f.stat().st_size,'sha256':hashlib.file_digest(f.open('rb'),'sha256').hexdigest()})
(p/'release-assets-local.json').write_text(json.dumps(assets,indent=2)+'\n',encoding='utf-8')
(p/'release-notes.md').write_text('0.2.116预发布：修复配置读取/保存与工程切换并发混读，复用WorkspacePin保持完整操作作用域。正常构建/安装0、六门pass、1159文件逐SHA及两路CI通过；正式EXE交错切换/保存窗口/错误释放、原生A/B参数和同EXE重启实拍通过。原SWE与唯一绑定保持，模型0。严格Browser和总体矩阵仍开放。源码 '+source+'，MSI SHA256 '+v['msi_sha256']+'。未签名、不标latest；交接：docs/testing/release-0.2.116/change-report-and-test-handoff.md。\n',encoding='utf-8')
text=(root/'tmp/2026-10-08-release-115/verify-release.py').read_text(encoding='utf-8').replace('0.2.115','0.2.116').replace('f6f8729953a1ddb6351a4d5a9c73daa9076e10f1',source)
(p/'verify-release.py').write_text(text,encoding='utf-8')
for name in ['release-assets-local.json','release-notes.md','verify-release.py']:shutil.copyfile(p/name,validation/name)
attrs=root/'.gitattributes';rule='/docs/testing/release-0.2.116/** -text whitespace=-blank-at-eof,-blank-at-eol,-space-before-tab,cr-at-eol'
if rule not in attrs.read_text():
    with attrs.open('a',encoding='utf-8') as f:f.write('\n'+rule+'\n')
print(json.dumps({'formal_passed':True,'successful_switches':inter['successful_switches'],'reads':inter['total_reads'],'model_requests':0}))
