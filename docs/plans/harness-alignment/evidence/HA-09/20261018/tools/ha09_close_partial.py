"""有真实回归和build后只关闭已有证据的条目；HA09-04保持未完成。"""
import sys,json,re
sys.path.insert(0,'tmp')
from ha04_checkpoint import *
DEST=PLAN/'evidence/HA-09/20261018';label=sys.argv[1]
def run(name):
    r=json.loads((DEST/'runs'/name/'receipt.json').read_bytes());assert r['exit_code']==0 and not r['source_changed_during_run']
    for src in r['sources']:assert sha(ROOT/src['path'])==src['sha256'],src['path']
    return r
web=run(label);build=run('build-01');log=(DEST/'runs'/label/'output.log').read_text(encoding='utf8')
count=int(re.search(r'test result: ok\. (\d+) passed; 0 failed',log).group(1));assert count>100
for test in ['ha09_phase_consumption_requires_actual_command_receipts','ha09_missing_corrupt_contract_and_finalizer_storage_fault_are_not_passed','ha09_actual_cancel_and_timeout_have_receipts_and_no_late_effects','output_overflow_cannot_be_reported_as_successful_goal_verification','real_receipts_and_contract_changes_control_consumption']:
    assert test in log and f'{test} ... ok' in log,test
cases={}
for name in ['Passed','Failed','Disabled','Empty','Legacy','Absent','FilesExist','faults','cancel-timeout']:
    cases[name]=json.loads((DEST/'runs'/label/f'ha09-{name}.json').read_bytes())
for name,state in [('Passed','passed'),('Failed','failed'),('Disabled','skipped'),('Empty','invalid'),('Legacy','unable'),('Absent','not_required'),('FilesExist','not_required')]:
    q=cases[name];assert q['gate']['state']==state
    assert q['evidence']['command_verification']==q['gate'],name
    assert (q['status']=='completed')==(name in ['Passed','Absent','FilesExist'])
    if name not in ['Passed','Absent','FilesExist']:assert q['verdict']!='pass'
assert cases['Passed']['gate']['receipts'][0]['exit_code']==0
assert cases['Failed']['gate']['receipts'][0]['exit_code']==7
assert cases['cancel-timeout']['child_drained'] and not cases['cancel-timeout']['late_effect']
assert cases['cancel-timeout']['cancelled']['status']=='cancelled' and cases['cancel-timeout']['timeout']['status']=='timed_out'
assert cases['faults']['persisted_status']=='running' and cases['faults']['persisted_verdict']!='pass'
assert all(r['state']=='unable' for r in cases['faults']['unable_results'])
base=PLAN/'baseline/HA-00-20261018'
assert sha(base/'manifest.json')=='9771d8a52de8056d1b59bf6b0c75120a14cf2720de18dc532c90067a369326ae'
assert sha(base/'source.patch')=='4bec04dc89e403cf2dc4806a0c44d65cf91a777adaab8c5dd939155b3d14774e'
now=stamp()
(DEST/'audit.json').write_text(json.dumps({'at_utc':now,'test_count':count,'run':label,'cases':cases,'baseline_unchanged':True},ensure_ascii=False,indent=2),encoding='utf8')
(DEST/'acceptance.md').write_text(f'''# HA09部分逐条验收

当前输入实际{count}项通过、0失败，生产offline build退出0，输入未变化。原始日志和哈希见runs/对应receipt；判别性事实见audit.json。HA00～06当前输入回归/差异复核接续原33条；本包只新增4条，HA09仍in_progress。

| 条目 | 结果与证据 |
|---|---|
| HA09-01 | Passed真实exit0、Failed真实exit7；真实PowerShell到期/在PID写入屏障取消后均非成功，实际子进程已退出且无迟到效果。完整命令、cwd、时间、exit、原因、宿主回执ID已持久化。 |
| HA09-02 | Absent/FilesExist为not_required且没有Command回执；Disabled为skipped、Empty为invalid，真实token-aware消费者均拒绝后两者完成。畸形命令、空白项、错误类型另有实际模块测试。 |
| HA09-03 | Goal/phase缺失、损坏verification JSON为unable；不存在cwd启动失败与输出采集溢出实际测试通过；SQLite完成写入触发故障时事务失败、原phase保持running且无pass。 |
| HA09-04 | 未完成：当前提示词指定runtime执行，尚未接入模型诊断可信宿主回执的唯一消费路径；不得关闭。 |
| HA09-05 | 旧空Vec在Command合同中为unable，不能升级pass；FilesExist只证明文件存在/内容基础形态，不提供语义正确证明；普通聊天相关回归保持原路由，未引入verifier角色。 |

本次没有验证HA10输入依赖缓存，不宣称无需重跑已有模型诊断命令。第一轮编译失败保留，修正测试引用后才运行新一轮。原始日志含DIAG路径不存在告警和HA06刻意注入的worker panic；最终测试汇总0失败，日志未删减，本次不据此声称诊断日志持久性已通过独立验证。没有真实模型、安装或发布。HA07/08、HA10～12仍需后续按合同完成。
''',encoding='utf8')
(DEST/'prior-evidence-review.md').write_text('''# HA00～06当前适用性

本次差异仅Goal Command结果类型、受控命令回执、Goal消费/合同读取与Goal专属提示，以及测试。聊天模型工具循环、adapter序列化、授权/根预算/工具身份/HA06恢复规则未修改。原92项合并实际复跑，使用本次输入的新证据；CLI/adapter输入未变，保留原回执。基线manifest/source.patch完全不变，复用历史102项真实恢复；未重新声称恢复当前候选。

HA09-04和HA10未完成，不因本次类型迁移顺带关闭。普通任务没有新增Goal路由或验证角色。结构校验只检查台账，不能代替上述真实行为回执。
''',encoding='utf8')
def item(p):return {'path':p.relative_to(ROOT).as_posix(),'sha256':sha(p)}
def files(p):return [item(f) for f in sorted(p.rglob('*')) if f.is_file()]
p=PLAN/'state.json';original=p.read_bytes();d=json.loads(original);old={x['id']:x for x in d['evidence']}
def record(id,name,kind,observed,n=None):
    r=run(name)
    return {'id':id,'kind':kind,'result':'pass','observed':observed,'method':{'command':' '.join(r['command']),'procedure':'核对真实命令退出、Goal claim消费及持久结果、取消PID屏障/子进程退出和SQLite故障；映射见acceptance.md。','cwd':str(ROOT),'actor':'primary','started_at_utc':r['started_at'],'ended_at_utc':r['finished_at'],'exit_code':0,'tests_run':n},'artifacts':files(DEST/'runs'/name)+[item(DEST/'acceptance.md'),item(DEST/'audit.json')],'inputs':r['sources'],'limitations':['HA09-04模型诊断回执消费及HA10缓存未完成；非真实模型质量/性能实验，未安装发布。']}
records=[record('E-HA09-WEB-20261018',label,'test',f'{count}项实际通过，新增HA09-01/02/03/05事实；HA09-04未完成。',count),record('E-HA09-BUILD-20261018','build-01','build','当前源码实际生产offline build退出0。')]
review={'id':'E-HA00-HA09-REVIEW-20261018','kind':'review','result':'pass','observed':'冻结基线不变，历史恢复/隔离仍适用；Goal验证差异与当前回归已核对。','method':{'command':None,'procedure':'比较HA06验收输入及当前差异、复核冻结基线及当前合并回归。','cwd':str(ROOT),'actor':'primary','started_at_utc':web['started_at'],'ended_at_utc':now,'exit_code':None,'tests_run':None},'artifacts':[item(DEST/f) for f in ['audit.json','implementation.patch','implementation-inputs.json','prior-evidence-review.md']]+old['E-HA00-HA06-REVIEW-20261018']['artifacts'],'inputs':[item(ROOT/'modules/gui-web/packages/web-console/src'/f) for f in ['main.rs','goal_command.rs','goal_verification.rs']]+[item(base/'manifest.json'),item(base/'source.patch'),item(base/'restore.md')],'limitations':['沿用原基线实际恢复，未重跑恢复；未验证真实模型。']}
records.append(review);assert all(r['id'] not in old for r in records)
for task in d['tasks']:
    if task['id'] in [f'HA-0{i}' for i in range(7)]:
        task.update(status='accepted',note='当前输入合并回归/差异复核通过；历史证据不改写。')
        for c in task['criteria']:
            if task['id']=='HA-00':refs=[review['id']]
            elif task['id']=='HA-04':refs=[{'E-HA06-WEB-20261018':'E-HA09-WEB-20261018','E-HA06-BUILD-20261018':'E-HA09-BUILD-20261018'}.get(x,x) for x in c['evidence_ids']]
            else:refs=['E-HA09-WEB-20261018','E-HA09-BUILD-20261018']
            c.update(status='passed',evidence_ids=refs,note='本次实际回归/差异复核接续，见HA09报告。')
    elif task['id']=='HA-09':
        task.update(status='in_progress',note='01/02/03/05真实验收；04可信诊断回执单消费未完成。')
        for c in task['criteria']:
            if c['id']!='HA09-04':c.update(status='passed',evidence_ids=['E-HA09-WEB-20261018','E-HA09-BUILD-20261018'],note='实际命令/持久消费者/故障回执通过，见HA09逐项报告。')
d['evidence'].extend(records);d.update(active_task='HA-09',next_task='HA-09',updated_at_utc=stamp())
d['handoff'].update(checkpoint='ha09_partial_37_of_60',last_result=f'HA00～06恢复accepted；HA09四条实际通过，共37/60。{count}项合并回归和生产build通过；04尚未完成。',next_action='继续HA09-04：真实模型诊断工具产生的可信宿主回执，严格匹配合同/命令/cwd/实际执行范围并由单消费路径接纳；无回执runtime执行。然后HA10；HA07/08仍pending。',running_commands=[],unresolved_effects=[])
save(d,original);(DEST/'partial-state.json').write_bytes(p.read_bytes());print(json.dumps({'passed':37,'tests':count,'HA09':'in_progress','pending_criterion':'HA09-04'}))
