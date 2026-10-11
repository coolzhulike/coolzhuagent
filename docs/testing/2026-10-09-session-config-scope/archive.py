from pathlib import Path
import hashlib,json,shutil,sqlite3,subprocess
p=Path(__file__).resolve().parent;root=p.parents[1];relative='docs/testing/2026-10-09-session-config-scope';dest=root/relative;dest.mkdir(exist_ok=False)
def load(f):return json.loads(f.read_text(encoding='utf-8-sig'))
assert all(x['actual_exit_code']==0 for x in load(p/'build-test-result.json'))
before=load(p/'before115b/facts.json');single=load(p/'candidate-interleaved-single/facts.json');saved=load(p/'save-scope/facts.json')
assert before['passed'] is False and before['mismatch_samples']==48 and before['total_reads']==2041
assert single['passed'] and single['successful_switches']==80 and single['total_reads']==77 and single['mismatch_samples']==0
assert saved['all_passed'] and saved['switch_during_sqlite_wait']['status']==409 and saved['other_workspace_untouched']
assert load(p/'save-scope/restart-result.json')['same_parameters_revision_name']
counts={}
for group in ['before115','before115b','candidate','candidate-interleaved','candidate-interleaved-single','save-scope']:
    target=dest/group;target.mkdir()
    assert load(p/group/'cleanup.json')['gui_hint_restored']
    for file in (p/group).iterdir():
        if file.is_file() and file.suffix in ('.json','.txt','.jpg','.py'):shutil.copyfile(file,target/file.name)
    for label in ['a','b']:
        db=p/group/('workspace-'+label)/'.coolzhu/web-sessions.sqlite3'
        if db.exists():
            with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True) as c:counts[group+'/'+label]=c.execute('SELECT count(*) FROM runtime_runs').fetchone()[0]
assert not any(counts.values())
assert load(p/'save-scope/restart-cleanup.json')['gui_hint_restored']
for name in ['race.py','save-scope.py','restart.py','start-isolated-shell.ps1','stop-isolated-shell.ps1','build-test.py','build-test-result.json','build.log','test.log','archive.py']:
    shutil.copyfile(p/name,dest/name)
shutil.copyfile(root/'tmp/2026-10-08-release-115/original-idle.json',dest/'original-idle.json')
(dest/'runtime-counts.json').write_text(json.dumps(counts,indent=2)+'\n',encoding='utf-8')
report='''# 模型配置工程作用域候选验收（2026-10-09）

## 问题和修复

正式115在工程选择与模型配置读取并发时，名称可能来自工程A，参数、容量与revision来自工程B。使用相同会话ID、不同工程名称/温度/本地容量/revision的真实独立HTTP服务复现：2041次读取/180次成功切换，每个读者最多保留8个混读样本，六读者合计48样本。这是样本保存上限，不是声称只有48次实际混读。首次准备把必填api_key_ref写null，读到404、有效读取0；它属于测试准备失败，完整事实单列before115，不能当产品复现。

SessionConfigService创建时复用既有WorkspacePin，覆盖同步读写及handler响应派生；工程正在切换则配置409，配置进行中则工程选择409。保持原参数规则、存储锁序及失败恢复，不添加缓存、队列或锁。详见[方案与风险](../../analysis/2026-09-21-integration-review/session-config-workspace-pin-2026-10-09.md)。

## 真实服务与原生验收

候选offline build实际0、完整Web回归1423通过/0失败/6既有忽略，另lib8、native-host1通过。候选EXE SHA256为9bd41bea066e678d330bb0771c4260b4c8a4a776bb7e3325ca0925b65cb16f71。

1. 六读者持续忙期：180次切换均409、有效读取137、混读0；读者停止后正常切换200。这只证明繁忙拒绝，不充当实际交错切换证据。
2. 单读者20ms间隔与180次切换真实交错：80次切换200、100次切换409、77次有效读取、68次读取409、混读0；停止读者后正常切换200。另六读者间隔case同样保留。
3. 真SQLite BEGIN IMMEDIATE写锁：正常参数保存已发布温度0.35但尚未完成会话落库期间，正常工程选择返回409；释放写锁后保存200，工程A名称、参数和revision正确。切到B后B原值完全保留；400非法温度和409旧版本返回后均能正常切换，pin没有泄漏。
4. 正式115原生壳连接新候选后台，正常设置/内部滚动显示[工程B 0.75/16384](save-scope/native-b.jpg)，正常工程选择切到[工程A 0.35/8192](save-scope/native-a.jpg)。同候选EXE结束后重启，名称、所有参数及revision逐项一致，[重启原生截图](save-scope/native-restarted.jpg)通过。重启时首次AX点击因缺少输入几何失败，fresh截图后正常点击成功；这是驱动状态错误，未算产品故障。
5. 所有隔离数据库runtime_runs为0、模型请求0、没有新云端会话，不使用模型回复夹具。全部有限驱动实际退出0；Windows terminate子退出1如实保留。原115后台和原SWE-2-medium/revision51/唯一island-kayak已恢复，活动轮次0，原安全库不写。

## 范围与后续测试设计

本候选关闭操作进行期参数混读/跨工程保存窗口，尚未包含正式115；待新正式包复验。HTTP未发送模型，不能以本项替代真实模型连通验收。迟到客户端请求expected_workspace/ABA、全部Service边界、单写者/outbox和跨资源崩溃仍开放。

后续模型须沿用SWE与唯一绑定；Browser严格在途撤销、跨来源commit严格down/up和新原生Target等继续开放。本项不改变Browser时限或注入暂停，Paint免测、微信不动、Opus暂停，整体Goal继续。
'''
(dest/'report.md').write_text(report,encoding='utf-8')
line='模型配置工程作用域候选：正式115真实复现2041读/180切换的跨工程混合；候选复用WorkspacePin覆盖读写和完整响应派生。实际build0、完整Web1423/0/6（另lib8/native-host1）；交错80次成功切换/77有效读取零混合，真实SQLite保存窗口切换409、另一工程不变、400/409退出释放pin，正常原生A/B参数和同EXE重启实拍通过。模型0、原SWE/revision51/唯一island恢复；115不含，迟到请求/ABA和完整领域迁移仍开放。[原失败、候选与实拍](../../testing/2026-10-09-session-config-scope/report.md)。Goal继续。'
for name in ['docs/analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md','docs/analysis/2026-09-21-integration-review/current-acceptance-queue.md','docs/analysis/2026-09-21-integration-review/implementation-status-and-change-inventory.md','docs/analysis/2026-09-27-four-task-status-and-navigation-addendum.md']:
    f=root/name;text=f.read_text(encoding='utf-8');first,rest=text.split('\n',1)
    entry=line if '2026-09-21-integration-review/' in name else line.replace('../../testing/','../testing/')
    f.write_text(first+'\n\n'+entry+'\n'+rest,encoding='utf-8')
attrs=root/'.gitattributes';rule='/'+relative+'/** -text whitespace=-blank-at-eof,-blank-at-eol,-space-before-tab,cr-at-eol'
if rule not in attrs.read_text():
    with attrs.open('a',encoding='utf-8') as f:f.write('\n'+rule+'\n')
manifest=[]
for f in sorted(dest.rglob('*')):
    if f.is_file():manifest.append({'path':f.relative_to(dest).as_posix(),'bytes':f.stat().st_size,'sha256':hashlib.file_digest(f.open('rb'),'sha256').hexdigest()})
(dest/'evidence-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'evidence_files':len(manifest),'model_requests':0,'passed':True}))
