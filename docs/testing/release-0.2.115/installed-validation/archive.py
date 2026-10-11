from pathlib import Path
import hashlib,json,shutil,sqlite3,subprocess
p=Path(__file__).resolve().parent;root=p.parents[1];out=root/'docs/testing/release-0.2.115'
out.mkdir(parents=True,exist_ok=True)
def load(f):return json.loads(f.read_text(encoding='utf-8-sig'))
v=load(p/'installed-115-verification.json')
assert load(p/'build-result.json')['exit_code']==0 and load(p/'install-115-result.json')['exit_code']==0
legacy=load(p/'legacy/facts.json');snap=load(p/'snapshot/facts.json');save=load(p/'config-save-live3/facts.json');ui=load(p/'ui/facts.json')
assert all(f['binary_sha256']==v['web_sha256'] for f in [legacy,snap,save,ui])
assert legacy['local_match'] and legacy['clear_match'] and legacy['sampling_preserved'] and legacy['mixed_samples']==0
assert snap['mismatch_samples']==0 and snap['race_passed'] and snap['local_cap_passed'] and save['all_passed']
assert sorted(save['concurrent_statuses'])==[200,409]
restart=load(p/'ui/restart-settings.json');assert restart['parameters']==ui['settings']['parameters'] and restart['configuration_revision']==ui['settings']['configuration_revision']
assert restart['effective_context_window']==8192 and restart['effective_max_output_tokens']==1024
counts={}
for group in ['legacy','snapshot','config-save-live3','ui']:
    with sqlite3.connect((p/group/'workspace/.coolzhu/web-sessions.sqlite3').resolve().as_uri()+'?mode=ro',uri=True) as c:counts[group]=c.execute('SELECT count(*) FROM runtime_runs').fetchone()[0]
    assert counts[group]==0
assert load(p/'original-idle.json')['active_runs']==0
validation=out/'installed-validation';validation.mkdir(exist_ok=True)
for name,run in [('ci-source-a.json','37895776447'),('ci-source-b.json','37895770145')]:
    raw=subprocess.check_output(['python','tmp/2026-10-05-devin-host/with-gh.py','run','view',run,'--json','status,conclusion,headSha,url,name'],cwd=root)
    row=json.loads(raw);assert row['status']=='completed' and row['conclusion']=='success' and row['headSha']==v['source_commit']
    (validation/name).write_bytes(raw)
for name in ['build-result.json','build-stdout.log','build-stderr.log','build-stdout-replay.log','build-stderr-replay.log','first-coordinator-wait.json','build-direct.py','source-commit.txt','package-115-verification.json','installed-115-verification.json','install-115-result.json','installed-115-standard-processes.json','original-idle.json','original-restored-receipt.json','verify-115.py','check-original.py']:
    shutil.copyfile(p/name,validation/name)
for group in ['legacy','snapshot','config-save-live3','ui']:
    target=out/group;target.mkdir(exist_ok=True)
    for f in (p/group).glob('*'):
        if f.is_file() and f.suffix in ['.json','.txt','.jpg']:shutil.copyfile(f,target/f.name)
for name in ['legacy-check.py','snapshot-check.py','save-check.py','native-live.py','native-restart.py','start-isolated-shell.ps1','stop-isolated-shell.ps1','archive.py']:
    shutil.copyfile(p/name,validation/name)
shutil.copyfile(p/'original-restored.jpg',out/'installed-native-main.jpg')
(validation/'isolated-runtime-counts.json').write_text(json.dumps(counts,indent=2)+'\n',encoding='utf-8')
report=f'''# 0.2.115 改动与验收交接

本版将四个模型参数配置入口的同步块提取至SessionConfigService，保留原TrackedSessionStore诊断、store→config锁序与失败恢复，HTTP入口只组织输入和响应。没有第二缓存、参数表、任务队列、权限/模型或预算变更。插件目录IO留在锁外；服务暂复用Web错误适配与发布基础设施，不代表core-runtime完整领域迁移。

冻结源码 `{v['source_commit']}`，快照 `{v['source_snapshot_digest']}`。正常release构建与正常管理员MSI安装均实际0，六发布门pass，Program Files {v['verified_file_count']}项逐长度/SHA一致。同源两路CI37895776447/37895770145均success。MSI {v['msi_bytes']}字节，SHA256 `{v['msi_sha256']}`。候选完整已有Web回归1423/0/6既有忽略，另lib8/native-host1通过，日志见[候选报告](../2026-10-08-session-config-service/report.md)。

## 正式安装复验与测试设计依据

第一次外层Start-Process -Wait在构建进程已结束后仍等待后代，未捕获实际子退出，不计成功。只停止路径/UTC ticks匹配的自有协调进程，未停止编译器助手；直接subprocess等待正常发布脚本重做后实际返回0。第二次同源码快照保持，dirty_against_commit=true来自第一次生成的未跟踪release证据目录，源码无修改；两次报告分别保留。此次使用最终带时间戳的MSI，不拿第一次同版本文件冒充最终包。

使用已安装EXE，四个隔离配置/安全库，无模型请求或模型回复fixture：

1. 旧容量六读者/180保存，实际{legacy['total_reads']}读、混合0；本地容量8192/4096正确应用，清零恢复规则与采样0.25保留。旧API仍返回容量上限，不把小上下文的本轮请求预算视为所有字段必须相等。
2. 统一配置六读者/180整组保存，实际{snap['total_reads']}读、混合0；名称、baseURL、endpoint、参数、revision与预算属于同一捕获快照。
3. 真SQLite触发器拒绝更新：原参数缺项及已有参数两例均返回500，原会话/SQLite/参数恢复，缺项仍缺项。正常撤销故障后保存成功；同expected_revision竞争返回200/409，不能覆盖他方成功值。
4. 正常壳点击设置/内部滚动，用户上下文32768/输出1024、生效8192/1024、温度0.25，[原生设置](ui/budget.jpg)通过。同已安装EXE结束后重启，全部参数、名称、revision和两接口预算恢复；正常Ctrl+R/重新打开/滚动，[重启原生截图](ui/restarted.jpg)通过。
5. 四个SQLite库runtime_runs均0；Qwen ID仅用于参数规则，未实际切换原SWE。驱动均已消费退出0；Windows子进程Terminate返回1如实保留。

[正式主界面](installed-native-main.jpg)已恢复原SWE-2-medium、revision51、原聊天室、唯一island-kayak解锁、活动轮次0；原安全库保持。

仅关闭Web配置服务提取的正式交付子项。LLM resolve、跨工作区作用域、单写者/outbox/跨资源崩溃、SharedRunner及其它WBS矩阵继续开放。Browser严格在途撤销、跨来源commit严格down/up及新Target未因此通过；不改期限、不注入暂停、不重复简单点击碰窗口。Paint免测、微信不动、Opus暂停。ChatGPT订阅最小真实连通仍为独立实验，非正式Provider。

发布状态另核；未签名、预发布、不标latest、不发自动升级清单。整体Goal继续。
'''
(out/'change-report-and-test-handoff.md').write_text(report,encoding='utf-8')
version='0.2.115';msi=root/v['msi_relative_path'];checksum=msi.with_suffix(msi.suffix+'.sha256');checksum.write_text(v['msi_sha256']+'  '+msi.name+'\n',encoding='utf-8')
assets=[]
for name in [msi.name,f'CoolzhuAgent-{version}-installer-report.json',f'CoolzhuAgent-{version}-package-safety.json',checksum.name]:
    f=root/'dist'/name;assets.append({'name':name,'size':f.stat().st_size,'sha256':hashlib.file_digest(f.open('rb'),'sha256').hexdigest()})
(p/'release-assets-local.json').write_text(json.dumps(assets,indent=2)+'\n',encoding='utf-8')
previous=root/'tmp/2026-10-08-release-114/verify-release.py'
text=previous.read_text().replace('0.2.114','0.2.115').replace('95c2704140f54db375fd1743c9fbdb1987ffda3c',v['source_commit'])
(p/'verify-release.py').write_text(text,encoding='utf-8')
(p/'release-notes.md').write_text('0.2.115预发布：收敛Web会话配置服务职责，复用已验证存储/发布/失败恢复。正式build/安装0、六门pass、1159安装文件逐SHA一致；旧容量与统一读取并发、SQLite失败恢复及200/409、正常原生设置与同EXE重启实拍通过。原SWE/唯一island保持，模型0。完整架构及严格Browser时序仍开放。交接：docs/testing/release-0.2.115/change-report-and-test-handoff.md。源码 '+v['source_commit']+'，MSI SHA256 '+v['msi_sha256']+'。未签名，不标latest。\n',encoding='utf-8')
for name in ['release-assets-local.json','release-notes.md','verify-release.py']:shutil.copyfile(p/name,validation/name)
attrs=root/'.gitattributes';rule='/docs/testing/release-0.2.115/** -text whitespace=-blank-at-eof,-blank-at-eol,-space-before-tab,cr-at-eol'
if rule not in attrs.read_text():
    with attrs.open('a',encoding='utf-8') as f:f.write('\n'+rule+'\n')
print(json.dumps({'version':version,'formal_passed':True,'legacy_reads':legacy['total_reads'],'snapshot_reads':snap['total_reads'],'model_requests':0},ensure_ascii=False))
