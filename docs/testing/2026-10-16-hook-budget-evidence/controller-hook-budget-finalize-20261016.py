"""仅在真实检查和构建完成后归档本轮证据并更新验收清单。"""
import datetime, hashlib, json, pathlib, shutil, subprocess, sys

repo = pathlib.Path(r'D:\coolzhuagent')
out = repo / 'tmp/goal-32wbs-acceptance-20261013'
archive = repo / 'docs/testing/2026-10-16-hook-budget-evidence'
def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()
receipts = {}
for action, expected in [('core', [['1', '0', '0']]), ('test', [['7', '0', '0']]), ('build', [])]:
    path = out / f'controller-hook-budget-{action}-20261016.json'
    value = json.loads(path.read_text(encoding='utf-8'))
    assert value['state'] == 'completed' and value['exit_code'] == 0, (action, value)
    assert value['source_unchanged'] and value['source_before'] == value['source_after'], action
    assert value['test_results'] == expected, (action, value['test_results'])
    assert sha(pathlib.Path(value['log'])) == value['log_sha256'], action
    assert all(sha(repo / name) == digest for name, digest in value['source_after'].items()), action
    receipts[action] = value
assert all(value['source_after'] == receipts['core']['source_after'] for value in receipts.values())

archive.mkdir(parents=True, exist_ok=True)
files = []
def preserve(source, name=None):
    source = pathlib.Path(source)
    dest = archive / (name or source.name)
    shutil.copy2(source, dest)
    assert sha(source) == sha(dest)
    files.append(dict(path=dest.name, source=str(source.relative_to(repo)), bytes=dest.stat().st_size, sha256=sha(dest)))

for action in receipts:
    for suffix in ('json', 'log'):
        preserve(out / f'controller-hook-budget-{action}-20261016.{suffix}')
for name in [
    'controller-hook-budget-baseline-20261016.log',
    'controller-hook-budget-baseline-isolated-20261016.log',
    'controller-hook-budget-first-test-20261016.json',
    'controller-hook-budget-first-test-20261016.log',
    'controller-hook-command-diagnostic-20261016.log',
    'controller-hook-leading-quote-probe-20261016.json',
    'controller-hook-leading-quote-probe-20261016.py',
    'controller-hook-budget-interrupted-test-20261016.json',
    'controller-hook-budget-interrupted-test-20261016.log',
    'controller-hook-budget-checks-20261016.py',
    'controller-hook-budget-finalize-20261016.py',
    'controller-ledger-review-20261016.py',
]:
    preserve(out / name)
for action in ('core', 'test'):
    for suffix in ('json', 'log'):
        previous = out / f'controller-hook-budget-before-outer-quote-{action}-20261016.{suffix}'
        if previous.exists(): preserve(previous)
preserve(out / 'hook-budget-20261016/command-diagnostic.rs')
preserve(out / 'hook-budget-20261016/tool_dispatch_service.before.rs')
for name in ['modules/gui-web/packages/web-console/src/tool_dispatch_service.rs',
             'modules/gui-web/packages/web-console/src/dispatch_hook_budget_tests.rs',
             'modules/core-runtime/packages/core-runtime/src/hooks.rs']:
    preserve(repo / name)

record = dict(
    state='completed_with_remaining_gates', passed=8, failed=0,
    at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    executor='当前主会话直接执行；没有Devin派发、独立会话或监督任务。',
    source_unchanged=True, source=receipts['core']['source_after'],
    receipts={action: f'controller-hook-budget-{action}-20261016.json' for action in receipts},
    build=dict(exit_code=0, profile='debug', offline=True,
               crates=['coolzhu-core-runtime', 'coolzhu-web-console'],
               executable_sha256=sha(out / 'cargo-target/debug/coolzhu-web-console.exe')),
    coverage=[
        '真实Windows shell引用：中文、空格、&路径，exit0放行/exit2拒绝，带引号可执行路径及参数，核实际文件和回执',
        '前置hook截止中止：真实进程退出，目标和后续hook均未执行',
        '后置hook截止中止：真实进程退出，保留目标已写入效果',
        '前置hook在途取消：同一异步reactor可响应取消，目标未执行',
        '后置hook在途取消：进程退出，目标既成效果保留',
        '等待future丢弃：hook进程被取消，延后观察仍无迟到写入',
        '较长冻结父预算不能延长当前较早截止',
        '正常正对照：真实pre/目标/post均执行并产生文件',
    ],
    earlier_failures=[
        '首个基线在会话库初始化os error5，未到hook，不作产品缺陷复现。',
        '显式隔离基线以及首次七项矩阵未看到hook入场；首次矩阵0过7败且编译间源码改变，不能作为冻结候选验收。原始结果完整保留。',
        '独立真实HookRunner诊断：原CRT转义下带引号路径失败，同路径不带引号成功。随后修改Windows shell命令传递并通过新增实证。',
        '独立前导引号诊断：不加外层引号时带空格的可执行路径被cmd拆开exit1；/D /S /C外层引号对照exit0。最后补此边界，仅重验受命令传递变化影响的本轮新增用例。',
        '最终复验首次接续时exec句柄已失效、CIM无对应运行器或cargo/rustc，日志无完成统计。保存interrupted_unknown收据，不计作测试失败或通过，随后接续同一冻结源码。',
    ],
    remaining=[
        '正式hook配置和独立授权材料来源尚未实现；当前默认零授权，不把测试override冒充正式产品配置。',
        '所有支持入口、授权在途变化、外部HTTP/真实模型和所有后代树仍需各自证据。',
        '生产当前仍完整0.2.135包，机器安装仍0.2.134；本轮未安装、切换、重启正式服务。',
        '旧1492完整测试仅保留旧源码历史；本次仅8个新增受影响链检查与实际build，不外推当前源码完整回归。',
    ],
    archive=str(archive.relative_to(repo)).replace('\\', '/'),
    cleanup='构建缓存供后续增量复用，诊断日志/收据/小型隔离数据保留；一次性诊断可执行文件及PDB另行安全清理。',
)
review_path = out / 'controller-hook-budget-review-20261016.json'
review_path.write_text(json.dumps(record, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
preserve(review_path)
(archive / 'manifest.json').write_text(json.dumps(dict(at_utc=record['at_utc'], files=files), ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
for item in files:
    assert sha(archive / item['path']) == item['sha256']

matrix = '''# 2.3 / 1.5：hook 根预算与取消补验

2026-10-16，主会话直接执行和审核。仅推进未完成的 hook 子矩阵；旧通过矩阵未重跑，已阻塞项未重试，没有模型请求或新建监督任务。

## 改动与实际检查

Windows hook 原先将完整 shell 命令交给普通参数转义，带引号路径被改写后执行失败；现在使用 /D /S /C 与完整外层引号保持已独立授权的 shell 命令语义，并禁用 cmd AutoRun。额外核对了以带空格可执行路径开头、后跟带引号参数的情况。控制台前后置 hook 改为受控阻塞 worker，同步命令不占住异步取消执行器；沿用已有取消与较早根截止，正常返回前收取真实进程结果，等待被丢弃时仍取消并接回 worker 收尾。

| 新增检查 | 结果 | 实证 |
| --- | --- | --- |
| Windows 引号命令 | 1过0败 | 中文、空格、&路径实际文件，exit0放行及exit2拒绝，前导带空格可执行路径和带引号参数 |
| 前置/后置截止 | 2过0败 | 子进程先存活后退出，后续hook不启动；前置阻止目标，后置保留既成效果 |
| 前置/后置在途取消 | 2过0败 | 同一异步执行器实时发送取消；真实子进程回收及目标效果核对 |
| 丢弃等待 | 1过0败 | 丢弃派发future后真实进程退出，等待超过原睡眠期限仍无迟到文件 |
| 冻结父预算较长 | 1过0败 | 较长父预算不能延长当前较早截止 |
| 正常正对照 | 1过0败 | pre、目标、post三个文件实际产生，回执成功 |

总计8过、0败、0忽略。core-runtime新增用例和web-console七项分别真实cargo exit0；随后两个crate实际离线cargo build exit0。四个受影响源码文件在三步检查前后SHA-256一致，构建收据保存候选EXE身份。

## 原始失败保留

首次基线在会话库初始化遇os error5，未到hook；后续隔离基线及首批七项矩阵由于带引号命令未启动，均不能当作通过。首批七项0过7败且源码编译间变化，收据明确source_unchanged=false，保留原貌。独立真实HookRunner诊断复现带引号失败、不带引号成功，再做上述定点修复和新矩阵。最终复验首次接续时exec句柄失效，CIM未见对应进程，日志无结束统计，保存为interrupted_unknown，不计作测试失败或通过；随后接续同一冻结源码。未使用删失败用例或减少断言的方式通过。

## 验收范围

这些结果只补齐2.3/1.5的hook链；两个整项仍开放。生产默认hook没有配置/独立授权来源，测试注入不等于正式入口；全入口一致性、授权在途变化、外部HTTP/真实模型和全部后代树仍待验。前置取消不执行目标，后置取消不会将已发生效果解释为未执行。旧1492项完整测试是旧生产源码历史，本轮未重复运行、也不外推当前候选全量通过。

没有修改正式8765程序、页面和会话库，没有安装新包；正式完整包仍0.2.135，机器安装仍0.2.134。小型夹具、日志、源码快照和收据集中统一临时根；可重用编译缓存保留。

可复现命令见[运行器](2026-10-16-hook-budget-evidence/controller-hook-budget-checks-20261016.py)，收据与原始失败见[SHA-256清单](2026-10-16-hook-budget-evidence/manifest.json)。统一[验收清单](2026-10-13-goal-32wbs-acceptance-checklist.md)与[执行计划](2026-10-16-active-acceptance-plan.md)同步更新。
'''
(repo / 'docs/testing/2026-10-16-hook-budget-matrix.md').write_text(matrix, encoding='utf-8')
subprocess.run([sys.executable, str(out / 'controller-ledger-review-20261016.py')], cwd=repo, check=True)
worklog = repo / 'docs/work-logs/2026-10-16-acceptance-takeover.md'
text = worklog.read_text(encoding='utf-8')
heading = '## 2.3 / 1.5 hook预算与取消补验'
if heading not in text:
    worklog.write_text(text + '\n' + heading + '\n\n主会话独立定位并修复Windows shell完整命令转义及控制台同步hook预算/取消缺口。新增命令引号1项、服务真实进程矩阵7项全部通过，两个crate实际离线build exit0，检查期间四份源码哈希一致。前置中止阻止目标，后置中止保留既成效果；等待丢弃仍回收真实hook。首次初始化拒绝及7项入场失败原样保留，独立诊断后修复，不伪造旧失败成功。\n\n未重复既有通过矩阵，未调用外部模型，未建立执行/监督会话，未切换正式135包或机器134安装。正式hook配置/授权来源和全入口、外部请求/后代矩阵仍开放，两个整项不冒进关闭；旧1492完整回归仅保留历史。详见[矩阵与范围](../testing/2026-10-16-hook-budget-matrix.md)及独立证据manifest；清单与生成脚本已同步。\n', encoding='utf-8')
print(json.dumps(dict(state=record['state'], passed=8, failed=0, archived=len(files), review=str(review_path)), ensure_ascii=False))
