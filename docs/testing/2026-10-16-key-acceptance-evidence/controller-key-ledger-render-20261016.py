"""只渲染已审核的关键功能清单，不重新判定状态。"""
import collections
import json
from pathlib import Path

ROOT = Path(r'D:\coolzhuagent')
R = ROOT / 'tmp/goal-32wbs-acceptance-20261013'
data = json.loads((R / 'acceptance-checklist.json').read_text(encoding='utf-8'))
assert data['acceptanceMode'] == 'key_functional'
items = [x for x in data['items'] if x['status'] != 'waived']
counts = collections.Counter(x['status'] for x in items)
labels = {'accepted': '关键功能通过', 'pending_evidence': '待修复/补关键证据', 'blocked': '条件阻塞，搁置'}
def esc(v):
    return str(v).replace('|', '／').replace('\n', ' ')
def links(names):
    return '、'.join(f'[{Path(n).name}](../../tmp/goal-32wbs-acceptance-20261013/{n})' for n in names)

lines = ['# 统一验收清单：关键功能口径', '',
 '2026-10-16 按用户要求收窄验收。当前范围34项；豁免项不进入本页及执行计划。', '',
 f"关键功能通过 **{counts['accepted']}** 项，待修复或补关键证据 **{counts['pending_evidence']}** 项，条件阻塞搁置 **{counts['blocked']}** 项。", '',
 '原先“2项通过”统计的是全矩阵关闭。本次将已有证据重新按关键功能审核，新增的是关项判定，不是声称刚刚实现了这些功能。未执行的扩展测试仍为未验证；历史失败、忽略、旧版本证据全部保留。', '',
 '正式8765已验证完整0.2.135包；C盘机器级安装仍0.2.134。新增hook修复已通过源码定点测试和实际构建，尚未打入正式安装包。功能验收、最终打包安装和远端CI分别记录，交付未全部完成。', '',
 '执行规则：主会话执行；不拆分会话、不创建监督任务。每项默认一个真实关键正例及一个必要安全负例；已有证据复用。只有新修改、新失败或明确安全缺口才追加测试。已通过不重跑。', '',
 '| 编号 | 项目 | 结论 | 关键功能标准 | 已有结论/主要缺口 |',
 '| --- | --- | --- | --- | --- |']
for x in items:
    k = x['keyAcceptance']
    lines.append('| ' + ' | '.join(esc(v) for v in [x['id'], x['item'], labels[x['status']], k['criterion'], k['conclusion']]) + ' |')
lines += ['', '## 关键通过的范围与证据', '', '| 项目 | 可复用证据 | 保留的扩展限制 |', '| --- | --- | --- |']
for x in items:
    if x['status'] == 'accepted':
        k = x['keyAcceptance']
        lines.append(f"| {x['id']} {x['item']} | {links(k['receipts'])} | {esc(k['deferred'])} |")
lines += ['', '## 未完成项：只做下一条关键闭环', '', '| 项目 | 下一步与通过条件 | 证据/定位入口 |', '| --- | --- | --- |']
for x in items:
    if x['status'] != 'accepted':
        k=x['keyAcceptance']
        lines.append(f"| {x['id']} {x['item']} | {esc(k['next'])} | {links(k['receipts'])} |")
lines += ['', '## 留到后续加强验收的内容', '',
 '多平台/全部控件/全部工具入口的组合穷举、真实模型并发与所有迟到时序、物理断电/真实盘满/慢机器启动长尾、启动资源损坏注入，退出当前关键功能执行队列。权限绕过、误输入、数据误删、结果伪成功和已知502等真实缺陷不随之免除。', '',
 '多屏为3.1/6.3的主要目标，仍保留未验结论；其他功能不再重复被多屏条件卡住。MSI覆盖只卡6.2交付，不再连带拖住4.2回复定位。', '',
 '严格版清单和原生成器快照：`docs/testing/2026-10-16-key-acceptance-evidence/`。逐项新判定及证据SHA：`controller-key-acceptance-review-20261016.json`。']
(ROOT/'docs/testing/2026-10-13-goal-32wbs-acceptance-checklist.md').write_text('\n'.join(lines)+'\n',encoding='utf-8')

plan = ['# 当前关键功能验收计划', '',
 '主会话直接执行，不派发Devin任务、不拆分执行/监督会话、不创建监督定时任务。确需产品会话对象时可按既有授权使用Devin SWE-2。', '',
 f"当前：{counts['accepted']}项关键通过，{counts['pending_evidence']}项待修复/补证，{counts['blocked']}项条件阻塞。通过项不重复测试；豁免项不列。", '',
 '一次只推进一项关键闭环；默认复用一个关键正例及必要安全负例。相关代码改完后按仓库约定实际编译，随后只跑受影响用例；没有新修改不重建。打包/安装在修复批次完成后合并处理。', '',
 '## 可直接推进', '', '| 顺序 | 项目 | 单轮目标/退出条件 |', '| --- | --- | --- |']
by_id={x['id']:x for x in items}
order=data['currentReview']['activePlan']['current']
for n,id in enumerate(order,1):
    x=by_id[id]
    assert x['status']=='pending_evidence'
    plan.append(f"| {n} | {id} {x['item']} | {esc(x['keyAcceptance']['next'])} |")
plan += ['', '## 条件阻塞，暂不重试', '', '| 项目 | 主要阻塞 | 恢复条件 |', '| --- | --- | --- |']
for x in items:
    if x['status']=='blocked':
        k=x['keyAcceptance']
        plan.append(f"| {x['id']} {x['item']} | {esc(k['conclusion'])} | {esc(k['next'])} |")
plan += ['', '关键通过的13项及证据见[统一清单](2026-10-13-goal-32wbs-acceptance-checklist.md)。扩大矩阵不自动重新开启通过项；若发现实际反例，按具体缺陷重新打开。']
(ROOT/'docs/testing/2026-10-16-active-acceptance-plan.md').write_text('\n'.join(plan)+'\n',encoding='utf-8')
print(json.dumps({'mode':data['acceptanceMode'],'active_total':len(items),'counts':dict(counts),'plan_count':len(order)},ensure_ascii=False))
