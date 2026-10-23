"""将4.2/6.2本轮真实收据归档并同步清单；不改写首次失败或外推整项完成。"""
import collections, datetime, hashlib, json, pathlib, shutil, subprocess

repo = pathlib.Path(r'D:\coolzhuagent')
out = repo / 'tmp/goal-32wbs-acceptance-20261013'
archive = repo / 'docs/testing/2026-10-16-reply-startup-matrix-evidence'
now = datetime.datetime.now(datetime.timezone.utc).isoformat()

def read(name):
    return json.loads((out/name).read_text(encoding='utf-8-sig'))

build = read('controller-remaining-build-20261016.json')
assert build['exit_code'] == 0 and build['state'] == 'completed', build
initial = read('controller-remaining-http-matrix-20261016.json')
reconnect = read('controller-reply-reconnect-matrix-20261016.json')
migration = read('controller-migration-retry-matrix-20261016.json')
browser = read('controller-reply-browser-reload-20261016.json')
assert (reconnect['passed'], reconnect['failed']) == (4,0)
assert (migration['passed'], migration['failed']) == (8,0)
assert (browser['passed'], browser['failed']) == (2,0)
fault_log = (out/'controller-schema-fault-matrix-20261016.log').read_text(encoding='utf-8-sig')
frontend_log = (out/'controller-reply-frontend-matrix-20261016.log').read_text(encoding='utf-8-sig')
assert 'test result: ok. 4 passed; 0 failed; 0 ignored' in fault_log
assert 'pass 9' in frontend_log and 'fail 0' in frontend_log
initial_reply = [x for x in initial['cases'] if x['name'].startswith('4.2') and x['state']=='passed']
assert len(initial_reply) == 5

review = {
    'at_utc': now, 'state': 'completed_with_remaining_gates',
    'executor': '当前主会话直接执行与审核，未派发Devin、未新建执行/监督会话或定时任务',
    'external_model_calls': 0,
    'passed_slices': {
        '4.2': {'isolated_http_cases': 9, 'initial_valid_cases': [x['name'] for x in initial_reply],
                'reconnect_cases': 4, 'frontend_tests': 9, 'formal_browser_new_reload_cases': 2,
                'parallel_reads': 64, 'concurrent_commits': 24},
        '6.2': {'release_service_migration_cases': 8, 'rust_migration_fault_tests': 4,
                'cargo_test_exit_code': 0, 'actual_build_exit_code': 0}
    },
    'initial_failures_preserved': {
        'controller-remaining-http-matrix-20261016.json': {
            'passed': 8, 'failed': 6,
            'diagnosis': '1项SSE测试漏传必需workspace查询参数；5项v12服务夹具只有Goal表，缺v7-v10历史记忆侧表，首次保存找不到表。修正测试输入后分别补测4/4与8/8，原失败文件不改写。'
        },
        'controller-reply-frontend-matrix-20261016-initial.log': {
            'passed': 8, 'failed': 1,
            'diagnosis': '测试错误期待英文completed且按整卡工具名计数；界面真实为中文已完成，修正为唯一工具事实行后9/9。未改生产前端。'
        }
    },
    'remaining_gates': {
        '4.2': ['正式真实后代执行树的可见UI和真实模型并发/断线迟到全链：当前生产execution_index=0，没有可只读复验的真实后代对象',
                'C机器级安装目录仍0.2.134，管理员权限阻塞，未把D完整135包正式8765运行冒充MSI覆盖'],
        '6.2': ['真实设备重启/OS冷缓存及慢机器长尾启动；当前12.837秒为新进程启动',
                '物理磁盘满、备份盘满及物理断电：本轮覆盖SQLite配额SQLITE_FULL和精确强杀迁移进程，不等同硬件故障',
                '机器级MSI覆盖及安装器降级：本轮只验证同schema29的134兼容二进制回退再启动135']
    },
    'production_boundary': '正式页面只读；隔离服务单独SQLite/config/port且均退出。新增Rust与前端测试，没有新增生产行为修复。生产应用默认日志位置未因夹具环境变量改变。',
    'temporary_root': str(out),
    'cleanup': '隔离服务均结束；小型故障夹具及首次失败日志作为复现证据保留，活动135发布包、正式库备份与后续需要的编译缓存不清除。此前scratch删除拒绝不绕过。',
    'build_receipt': 'controller-remaining-build-20261016.json'
}
(out/'controller-remaining-matrix-review-20261016.json').write_text(json.dumps(review,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

# 同步生成器，防止下次重生成清单将本轮结论还原。
generator = out/'controller-ledger-review-20261016.py'
source = generator.read_text(encoding='utf-8')
reply_evidence = ('controller-remaining-matrix-review-20261016.json：4.2隔离HTTP九项（含64并发读取与24次提交、后代环路/共享run去重、跨工程与房间隔离、迟到usage未知补全、断线重连正文/工具终态重读）通过；前端九项覆盖跨工程同ID、会话ABA与较早请求迟到、刷新去重。controller-reply-browser-reload-20261016.json：正式8765完整135包新浏览页/重载后两条既有回复各唯一run，历史CU失败及未知用量如实显示，2/2；此前Tauri逐回复/原提问4/4、正式HTTP6/6保留。首次夹具失败单独保留。生产execution_index=0，真实后代UI与真实模型并发/断线迟到全链仍缺；MSI管理员权限阻塞，C安装仍134，不计整项关闭。')
startup_evidence = ('controller-remaining-matrix-review-20261016.json：完整135发布服务隔离迁移八项全过：v12旧Goal复合同名阶段→v29中文记录与备份逐行保留、重启幂等、134兼容二进制回退再启135、最终metadata写入失败/备份目录被文件占用的拒绝及恢复、超前版本拒绝。新增Rust迁移故障四项全过（含SQLITE_FULL配额回滚与事务中强杀后恢复），cargo test及实际cargo build exit0。此前默认期限120秒、启动器75项及完整包正式新进程12.837秒、11历史表/绑定一致仍有效。合成历史DDL不是生产旧库，进程强杀不是断电，SQLITE_FULL不是物理盘满；真实OS冷启动长尾、备份盘满、物理断电、MSI升级/安装降级仍缺；整项保持进行中。')
replacements = {
    "by_id['4.2']['latestEvidence']=": "by_id['4.2']['latestEvidence']="+repr(reply_evidence),
    "by_id['6.2']['latestEvidence']=": "by_id['6.2']['latestEvidence']="+repr(startup_evidence)
}
for prefix,line in replacements.items():
    assert sum(x.startswith(prefix) for x in source.splitlines())==1,prefix
    source='\n'.join(line if x.startswith(prefix) else x for x in source.splitlines())+'\n'
source=source.replace('下一步后代汇总、迟到重连、跨工程与并发矩阵。','本轮隔离HTTP9/9、前端9/9与正式页重载2/2已过；仍缺生产真实后代可见UI与真实模型并发/迟到全链。正式库execution_index为0；不以合成夹具冒充。')
source=source.replace('MSI管理员权限阻塞搁置，继续隔离旧库/磁盘满/回退可执行项。','隔离发布服务迁移8/8与Rust故障4/4已过、实际build exit0；MSI管理员权限仍阻塞。余OS冷启动长尾、物理盘满/断电及安装器升降级，先留证搁置。')
source=source.replace('本轮4.2逐回复正式页面切片、6.2默认启动期限修复及完整包启动验证已通过；C安装仍0.2.134，MSI覆盖权限阻塞单列，两个整项的其余矩阵继续开放。EX-01当前web源码完整回归保持通过；遇阻塞转其他可执行项。','本轮4.2新增隔离HTTP9项、前端9项、正式页新页/重载2项通过；6.2发布服务迁移8项、Rust故障4项通过且实际build exit0。C安装仍0.2.134；真实后代UI与真实模型全链、OS冷启动长尾、硬件故障及MSI门禁未关闭，两个整项继续进行。EX-01既有生产源码适用完整回归保留，本轮只新增测试；遇阻塞转其他可执行项。')
# 审计必须在写入本轮latestEvidence之后，以免审计仍引用上轮收据。
start=source.index("audit={'at_utc':now")
anchor="tmp=ledger.with_suffix('.next.json')"
assert anchor in source
if start < source.index("data['execution']="):
    end=source.index("\ndata['execution']=",start)
    audit_block=source[start:end]
    source=source[:start]+source[end:]
    source=source.replace(anchor,audit_block+'\n'+anchor,1)
source=source.replace('当前逐回复定位源码debug/test完整套件：主1489+子3通过、0失败6忽略、微信26过滤；前端6通过，源码前后未变。','先前生产源码适用完整测试1492过/0败/6忽略保持历史通过；本轮仅新增测试，前端9/9、schema迁移4/4通过，不声称重跑了整个完整套件。')
matrix_fields="review.update(remainingMatrixReview='controller-remaining-matrix-review-20261016.json',remainingMatrixArchive='docs/testing/2026-10-16-reply-startup-matrix-evidence')"
if matrix_fields not in source:
    source=source.replace(anchor,matrix_fields+'\n'+anchor,1)
generator.write_text(source,encoding='utf-8')
subprocess.run([r'C:\Python314\python.exe',str(generator)],cwd=repo,check=True)

ledger=out/'acceptance-checklist.json'
data=read(ledger.name)
data['currentReview'].update(remainingMatrixReview='controller-remaining-matrix-review-20261016.json',
    remainingMatrixArchive='docs/testing/2026-10-16-reply-startup-matrix-evidence',
    fullTestResult='先前生产源码适用完整测试1492过/0败/6忽略保持历史通过；本轮仅新增测试，前端9/9、schema迁移4/4通过，不声称重跑了整个完整套件。',
    activeBuildReceipt=None)
ledger.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')

log=repo/'docs/work-logs/2026-10-16-acceptance-takeover.md'
heading='## 4.2与6.2剩余矩阵补验（本轮）'
if heading not in log.read_text(encoding='utf-8'):
    with log.open('a',encoding='utf-8') as f:
        f.write('\n'+heading+'\n\n')
        f.write('主会话独立完成，无Devin派发/外部模型调用，无新增监督。4.2隔离HTTP九项全过，包括64次并发读取与24次提交、后代环路及共享run去重、跨工程/房间隔离、迟到usage未知补全与SSE断线重连后正文/工具终态重读；前端九项全过，补跨工程同ID、会话A→B→A及迟到刷新防覆盖。正式8765完整135包新浏览页和重载后分别展开359/360回复，各唯一run，2/2，失败与未知用量保持真实。只读检查生产execution_index为0，真实后代UI对象不足，不能把隔离合成图冒充正式模型链。\n\n')
        f.write('6.2完整135发布服务隔离旧库/恢复八项全过：合成v12历史DDL中复合同名Goal阶段升级v29不丢中文、旧备份全行一致、重复启动不增备份；同schema29的134兼容二进制回退再启动135保留五张业务表；最终metadata写入失败和备份目录文件占用均关闭HTTP、旧库未改，移除隔离故障后恢复；超前版本拒绝且旧字节不变。新增Rust迁移故障四项过0败：SQLITE_FULL配额回滚、事务中精确强杀后重开回滚并再升级、首次锁等待及未checkpoint的WAL备份。Cargo定点test exit0；按仓库约定额外实际cargo build exit0。\n\n')
        f.write('首次HTTP矩阵8过6败文件原样保留：SSE漏传workspace参数1项；局部v12夹具缺历史记忆侧表导致5项失败。补齐真实历史DDL后定点重验8/8，不记为生产修复。前端首次8过1败是断言期待英文/按整卡重复工具名计数，按中文唯一工具事实行修正后9/9，旧日志保留。没有为通过而修改生产行为。\n\n')
        f.write('4.2和6.2整项继续进行：正式真实后代UI/真实模型并发与迟到全链、真实OS重启冷缓存和慢机器长尾、物理盘满/备份盘满及断电、机器级MSI升降级仍缺。C安装仍134；管理员阻塞不绕过。小型隔离夹具保留复现证据，所有临时脚本/构建/日志位于统一根，隔离服务均结束；活动135包、生产库备份与后续构建缓存继续保留。证据见本轮独立归档manifest。\n')

files=[
 'controller-remaining-matrix-review-20261016.json',
 'controller-remaining-http-matrix-20261016.py','controller-remaining-http-matrix-20261016.json','controller-remaining-http-matrix-20261016.log',
 'controller-reply-reconnect-matrix-20261016.py','controller-reply-reconnect-matrix-20261016.json','controller-reply-reconnect-matrix-20261016.log',
 'controller-migration-retry-matrix-20261016.py','controller-migration-retry-matrix-20261016.json','controller-migration-retry-matrix-20261016.log',
 'controller-reply-browser-reload-20261016.json','controller-reply-frontend-matrix-20261016.log','controller-reply-frontend-matrix-20261016-initial.log',
 'controller-schema-fault-matrix-20261016.log','controller-remaining-build-20261016.json','controller-remaining-build-20261016.log',
 'controller-complete-remaining-matrix-20261016.py']
archive.mkdir(exist_ok=True)
manifest={'at_utc':now,'boundary':review['production_boundary'],'files':[]}
for name in files:
    src=out/name; dest=archive/name; shutil.copy2(src,dest)
    digest=hashlib.sha256(src.read_bytes()).hexdigest()
    assert digest==hashlib.sha256(dest.read_bytes()).hexdigest()
    manifest['files'].append({'source':str(src),'archive':str(dest.relative_to(repo)), 'sha256':digest,'bytes':src.stat().st_size})
for rel in ['modules/gui-web/packages/web-console/src/schema_upgrade.rs','modules/gui-web/packages/web-console/tests/chat-trace-locate-sync.test.cjs']:
    src=repo/rel; dest=archive/src.name; shutil.copy2(src,dest)
    digest=hashlib.sha256(src.read_bytes()).hexdigest()
    assert digest==hashlib.sha256(dest.read_bytes()).hexdigest()
    manifest['files'].append({'source':rel,'archive':str(dest.relative_to(repo)),'sha256':digest,'bytes':src.stat().st_size})
(archive/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'review':review['state'],'status_counts':dict(collections.Counter(x['status'] for x in data['items'])),'archived_files':len(manifest['files']),'all_hashes_match':True},ensure_ascii=False))
