import collections, datetime, hashlib, json, pathlib, re, shutil

root=pathlib.Path(r'D:\coolzhuagent')
out=root/'tmp/goal-32wbs-acceptance-20261013'
ledger=out/'acceptance-checklist.json'
data=json.loads(ledger.read_text(encoding='utf-8-sig'))
backup=out/'acceptance-checklist-before-20261016.json'
if not backup.exists(): shutil.copy2(ledger,backup)
now=datetime.datetime.now(datetime.timezone.utc).isoformat()
full_path=out/'controller-current-full-20261016.json'
full=json.loads(full_path.read_text(encoding='utf-8')) if full_path.exists() else {}
complete=full.get('state')=='completed' and full.get('source_unchanged') is True and len(full.get('steps',[]))==2 and all(step.get('exit_code')==0 for step in full['steps'])
if complete:
    item=next(x for x in data['items'] if x['id']=='EX-01')
    item['status']='accepted'
    item['latestEvidence']='controller-current-full-20261016.json：当前源码离线debug/test完整串行测试真实结束，Cargo exit0，主套件1489过、子探针3过，共1492过/0败/6忽略，微信26过滤；前端6项通过。源码全面哈希前后一致。仅关闭该源码的适用完整测试，不外推发布版、安装版逐回复UI或35项整体验收。'
    item['updatedAtUtc']=now
item=next(x for x in data['items'] if x['id']=='EX-03')
item['status']='blocked'
item['latestEvidence']='安装版首次/日常代表帧、正常交接、恢复不重播、减少动态及Esc/跳过按钮/演出区域退出已有通过证据。补核native-resource-fault-134/cdp.json：instrumentation_not_verified、exit1、attached=[]、blocked_requests=[]；未开放诊断端口，实际未注入人物图片失败，不能把最终窗口可见计作资源故障链通过。保留失败并先搁置，不重复同一无效注入。'
item['updatedAtUtc']=now

data['execution']='当前主会话直接推进任务、执行测试并审核；仅测试用例需要CoolzhuAgent会话对象时使用Devin SWE-2。不拆分监督或执行会话，不创建监督定时任务。'
review=data.setdefault('currentReview',{})
review.update(date='2026-10-16',workLog='docs/work-logs/2026-10-16-acceptance-takeover.md',itemAudit='controller-35-item-review-20261016.json',activeBuildReceipt='controller-current-full-20261016.json',takeoverRule='豁免项不进入执行计划；阻塞项留证后转其他可执行项；不改旧失败为成功。')
if complete:
    review.update(fullTestReview='controller-current-full-20261016.json',fullTestResult='先前生产源码适用完整测试1492过/0败/6忽略保持历史通过；本轮仅新增测试，前端9/9、schema迁移4/4通过，不声称重跑了整个完整套件。',activeBuildReceipt=None,candidateBoundary='C机器级安装仍0.2.134；完整0.2.135包已在正式8765完成两条真实逐回复UI和默认启动验证，MSI覆盖权限受阻，整项矩阵仍开放。')
by_id={x['id']:x for x in data['items']}
by_id['0.1']['latestEvidence']='0.2.134正常release出包与MSI exit0，源码快照ec6c7c04…前后不变；1222载荷逐项匹配、注册安装及全新进程身份通过。当前web EXE SHA7e678c39…已包含Goal修复并从安装目录复验。358失败和0.2.133证据独立保留；该段为0.2.134历史，本轮0.2.135冻结补证见后，CI与工具链剩余门禁不关闭。 controller-release135-20261016.json：后续0.2.135已独立冻结、正常全量release/MSI出包，源码前后不变，1222载荷逐文件一致；controller-package135-live-20261016.json记录完整包正式8765实际进程、默认配置和原历史一致。MSI覆盖1730/1603权限阻塞，C安装仍134；不把D包运行冒充机器级升级。CI/全工具链剩余门禁未关闭。'
by_id['0.2']['acceptance']='由当前支持的执行链补长输出8192→69632连续分页和完整读取证据；保留旧失败，不以恢复Devin为前置。'
by_id['1.1']['latestEvidence']='controller-runtime-history-20261016.json只读全行多重集核验：冷启动前后11表逐行一致，2336消息/6060工具/828运行/1352 ACP attempts等均保留，quick_check ok。当前工作区ws-8d17063fefe5c23c中原agent_id的remote_session_id实际为buttercup-pink，已不同于早期island-kayak；差异在本轮启动前已有，不能宣称本轮仍绑定island-kayak。用户现要求主会话独立执行，本轮不改绑、不派发；完整入口/终态/未知矩阵仍缺。'
by_id['4.2']['latestEvidence']='controller-remaining-matrix-review-20261016.json：4.2隔离HTTP九项（含64并发读取与24次提交、后代环路/共享run去重、跨工程与房间隔离、迟到usage未知补全、断线重连正文/工具终态重读）通过；前端九项覆盖跨工程同ID、会话ABA与较早请求迟到、刷新去重。controller-reply-browser-reload-20261016.json：正式8765完整135包新浏览页/重载后两条既有回复各唯一run，历史CU失败及未知用量如实显示，2/2；此前Tauri逐回复/原提问4/4、正式HTTP6/6保留。首次夹具失败单独保留。生产execution_index=0，真实后代UI与真实模型并发/断线迟到全链仍缺；MSI管理员权限阻塞，C安装仍134，不计整项关闭。'
by_id['6.2']['latestEvidence']='controller-remaining-matrix-review-20261016.json：完整135发布服务隔离迁移八项全过：v12旧Goal复合同名阶段→v29中文记录与备份逐行保留、重启幂等、134兼容二进制回退再启135、最终metadata写入失败/备份目录被文件占用的拒绝及恢复、超前版本拒绝。新增Rust迁移故障四项全过（含SQLITE_FULL配额回滚与事务中强杀后恢复），cargo test及实际cargo build exit0。此前默认期限120秒、启动器75项及完整包正式新进程12.837秒、11历史表/绑定一致仍有效。合成历史DDL不是生产旧库，进程强杀不是断电，SQLITE_FULL不是物理盘满；真实OS冷启动长尾、备份盘满、物理断电、MSI升级/安装降级仍缺；整项保持进行中。'
by_id['3.2']['latestEvidence']='旧279轮input_safety_root_not_injected为历史失败，后续343已补接线、344原生输入有界纠正及345严格nativeTarget正负例通过；不再把旧输入根注入故障列为当前阻塞。完整Windows控件pattern正负实机能力表仍缺，整项保留blocked。'
by_id['3.3']['latestEvidence']='controller-344-audit.json真实隔离桌面两步纠正通过：第1步已投递/已释放但验证失败，第2步已投递/已释放且3/3视觉通过。仅该链通过；完整次数上限、分页、取消迟到、子目标重规划及自然导航误判矩阵仍待验。'
by_id['3.4']['latestEvidence']='controller-345-audit.json：严格原生目标正负例通过；未命中时0输入，正例已投递/释放且视觉2/2。真实跨来源commit、在途撤销/HRESULT及误拒率仍缺，旧BODY keydown负例失败保留。'
for key in ['1.1','4.2','6.2','3.2','3.3','3.4']: by_id[key]['updatedAtUtc']=now
review['activePlan']={'current':['4.2','6.2'],'excludeStatuses':['waived','accepted'],'blockedPolicy':'阻塞留证，不阻止其他项；需硬件/外部凭据/签名的项目不伪造环境。'}

# 后续独立矩阵只有真实结束且所有自有服务退出后才纳入，不覆盖旧失败。
runtime_matrices={}
for key,name,expected in [('writer','controller-writer-live-matrix-20261016.json',8),('recovery','controller-recovery-contract-matrix-20261016.json',13),('permission','controller-permission-expiry-matrix-20261016.json',7)]:
    path=out/name
    if not path.exists(): continue
    value=json.loads(path.read_text(encoding='utf-8'))
    if value.get('state')=='completed' and value.get('passed')==expected and value.get('failed')==0 and value.get('processes') and all(p.get('process_exited') for p in value['processes']):
        runtime_matrices[key]=name
if 'writer' in runtime_matrices and 'recovery' in runtime_matrices:
    by_id['2.4']['latestEvidence']='controller-writer-live-matrix-20261016.json：完整135/已安装134二进制、隔离共享库与真实HTTP八项全过：同版与新旧双向竞争均启动前拒绝，强杀后134接管再回135，epoch逐次增加；在线注入权威epoch后两次保存409、数据库与读取缓存无幽灵记录，重启恢复。补核范围发现微信不测约定，controller-recovery-contract-matrix-20261016.json内八项Clawbot本地outbox检查已标范围外，不计本轮验收、不再继续。主控通知outbox崩溃窗口、UI积压、正式生产双写/更旧不遵锁helper仍缺，整项保持进行中。'
    by_id['1.1']['latestEvidence']+=' controller-recovery-contract-matrix-20261016.json：完整发布服务隔离execution_index重启矩阵通过：running/cancel_requested无活句柄只投影outcome_unknown，既有completed/failed/interrupted/unknown保留；无句柄取消六类均409、不改库，未绑定run的trace409不猜房间，未知用量保持null。人工索引不是实际模型执行链，正式入口全矩阵仍开放。'
    by_id['2.5']['latestEvidence']='controller-recovery-contract-matrix-20261016.json：完整135服务真实HTTP、隔离人工execution_index补重启恢复五项通过：无句柄running/cancel_requested显示unknown而非完成、六类取消409且不伪造终态；缺身份/旧配置scope400或409、跨工程详情和trace404；无run轨迹409、未知用量null；再次强杀重启无自动重放且两库quick_check ok。没有创建调度任务或调用外部模型；真实在途worker取消、生产崩溃/正式调度及子Agent入口全链仍缺。'
    for key in ['2.4','1.1','2.5']:by_id[key]['updatedAtUtc']=now
if 'permission' in runtime_matrices:
    by_id['1.4']['latestEvidence']='controller-permission-expiry-matrix-20261016.json：完整135服务、隔离Devin SWE-2本地对象及人工文本七项全过；缺确认授权400不生效；FullAccess最短TTL60秒真实自然过期后越界read重新require-approval且output=null，工作区内read仍可；未知工具仍拒绝、另一会话不继承授权；pending真实五分钟自然失效，过期approve404零执行，跨会话消费404不删除原待办；重启不恢复临时grant或重放pending、本地对象保留。未改系统时钟、未调模型、未动正式权限；DSH/chat冻结grant、在途撤权及正式UI门禁仍开放。'
    by_id['1.4']['updatedAtUtc']=now
if runtime_matrices:
    review['runtimeContractMatrices']=runtime_matrices
    review['runtimeContractArchive']='docs/testing/2026-10-16-runtime-contract-evidence'
    review['activePlan']['current']=['2.4','1.4','2.5']
    review['activePlan']['deferredSubmatrices']=['4.2生产真实后代/模型并发迟到全链','6.2物理冷启动/盘满断电/MSI权限']
audit={'at_utc':now,'scope':'35项逐项核查；已豁免项保留历史但完全不进入执行计划','executor':'当前主会话直接推进任务、执行测试并审核；仅测试用例需要CoolzhuAgent会话对象时使用Devin SWE-2。不拆分监督或执行会话，不创建监督定时任务。','items':[]}
for item in data['items']:
    selected=[]
    for text in [item.get('latestEvidence',''),item.get('evidence','')]:
        refs=list(dict.fromkeys(re.findall(r'controller-[A-Za-z0-9_.-]+\.json',text)))
        selected.extend(refs[-2:])
    evidence=[]
    for name in dict.fromkeys(selected):
        path=out/name
        row={'name':name,'exists':path.is_file()}
        if path.is_file():
            raw=path.read_bytes()
            row['sha256']=hashlib.sha256(raw).hexdigest()
            try:
                record=json.loads(raw.decode('utf-8-sig'))
                row['valid_json']=True
                row['decision']=record.get('decision',record.get('state')) if isinstance(record,dict) else '结构化列表'
            except Exception as error: row.update(valid_json=False,error=type(error).__name__)
        evidence.append(row)
    audit['items'].append({'id':item['id'],'item':item['item'],'status':item['status'],'in_execution_plan':item['status'] not in ('accepted','waived'),'receipt_checks':evidence})
audit['status_counts']=dict(collections.Counter(x['status'] for x in data['items']))
audit['checked_receipts']=sum(len(x['receipt_checks']) for x in audit['items'])
audit['missing_receipts']=[r['name'] for x in audit['items'] for r in x['receipt_checks'] if not r['exists']]
audit['scope_note']='状态为整项完成门禁；已有大量子检查通过，不能将局部通过计作整项完成。'
(out/'controller-35-item-review-20261016.json').write_text(json.dumps(audit,ensure_ascii=False,indent=2),encoding='utf-8')
review.update(remainingMatrixReview='controller-remaining-matrix-review-20261016.json',remainingMatrixArchive='docs/testing/2026-10-16-reply-startup-matrix-evidence')
tmp=ledger.with_suffix('.next.json')
tmp.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
tmp.replace(ledger)

labels={'accepted':'已验收','blocked':'阻塞搁置','in_progress':'进行中','pending_evidence':'待补证','waived':'豁免'}
doc=root/'docs/testing/2026-10-13-goal-32wbs-acceptance-checklist.md'
lines=['# 统一验收清单：当前执行范围','',f'核查日期：2026-10-16。历史总计35项；已豁免1项仅保留原JSON历史，本页及后续计划不列该项。当前主会话直接推进任务、执行测试并审核；仅测试用例需要CoolzhuAgent会话对象时使用Devin SWE-2。不拆分监督或执行会话，不创建监督定时任务。','', '临时产物及收据统一保存在 `D:\\coolzhuagent\\tmp\\goal-32wbs-acceptance-20261013`。JSON保留历史失败与本轮新增证据；不将单元测试、合成夹具或旧版测试外推为整项通过。','', '本轮4.2新增隔离HTTP9项、前端9项、正式页新页/重载2项通过；6.2发布服务迁移8项、Rust故障4项通过且实际build exit0。C安装仍0.2.134；真实后代UI与真实模型全链、OS冷启动长尾、硬件故障及MSI门禁未关闭，两个整项继续进行。EX-01既有生产源码适用完整回归保留，本轮只新增测试；遇阻塞转其他可执行项。','', '| 项目 | 验收项 | 状态 | 整项门禁范围 | 最新证据 |','| --- | --- | --- | --- | --- |']
for x in data['items']:
    if x['status']=='waived': continue
    row=[x['id'],x['item'],labels[x['status']],x['acceptance'],x.get('latestEvidence',x.get('evidence',''))]
    lines.append('| '+' | '.join(str(t).replace('|','／').replace('\n',' ') for t in row)+' |')
lines+=['','证据审计：`controller-35-item-review-20261016.json`；已完成/豁免状态不进入后续执行队列。']
doc.write_text('\n'.join(lines)+'\n',encoding='utf-8')
if runtime_matrices:
    text=doc.read_text(encoding='utf-8')
    anchor='| 项目 | 验收项 | 状态 | 整项门禁范围 | 最新证据 |'
    note='后续补验：2.4完整服务单写者/跨版本/epoch八项，1.1/2.5恢复契约五项通过；1.4自然过期七项通过后纳入。八项误选的Clawbot本地outbox检查按微信不测约定标为范围外，不计验收。均为隔离真实HTTP，外部模型和正式生产故障全链未被替代；整项门禁继续保留。'
    if 'permission' in runtime_matrices:note=note.replace('1.4自然过期七项通过后纳入','1.4自然过期七项通过')
    doc.write_text(text.replace(anchor,note+'\n\n'+anchor),encoding='utf-8')
plan=root/'docs/testing/2026-10-16-active-acceptance-plan.md'
lines=['# 当前验收执行计划','', '当前主会话直接推进任务、执行测试并审核；仅测试用例需要CoolzhuAgent会话对象时使用Devin SWE-2。不拆分监督或执行会话，不创建监督定时任务。每项按真实退出码、统计、源码身份及适用UI证据判定。已验收项不重复派发。','', '## 当前推进','', '| 优先 | 项目 | 下一步 |','| --- | --- | --- |','| 1 | 4.2 | 已通过完整0.2.135包在正式8765的两条真实回复/原提问定位UI四项与HTTP六项；本轮隔离HTTP9/9、前端9/9与正式页重载2/2已过；仍缺生产真实后代可见UI与真实模型并发/迟到全链。正式库execution_index为0；不以合成夹具冒充。 |','| 2 | 6.2 | 默认120秒、启动器75项、完整包默认新进程12.837秒已过；隔离发布服务迁移8/8与Rust故障4/4已过、实际build exit0；MSI管理员权限仍阻塞。余OS冷启动长尾、物理盘满/断电及安装器升降级，先留证搁置。 |','', '## 其余可执行项','', '| 项目 | 仍需完成 |','| --- | --- |']
current=set(review['activePlan']['current'])
if runtime_matrices:
    start=lines.index('## 当前推进');end=lines.index('## 其余可执行项')
    lines[start:end]=['## 当前推进','', '| 项目 | 本轮通过 | 剩余门禁 |','| --- | --- | --- |',
      '| 2.4 | 完整服务同版/134混用/强杀接管/在线epoch 8/8 | 主控通知outbox崩溃窗口及UI积压、正式生产双写与更旧helper仍需独立证据；Clawbot八项已排除，不继续微信测试。 |',
      '| 1.4 | FullAccess自然60秒过期与pending自然五分钟失效矩阵7/7完成后记入收据 | DSH/chat冻结grant、在途撤权和正式UI仍缺；不重复已过的TTL/pending检查。 |',
      '| 2.5 / 1.1 | 完整发布服务隔离索引重启、unknown投影、六类无句柄取消、scope隔离和二次恢复通过 | 缺真实worker/模型在途与正式支持入口全链，人工索引不冒充实际执行；继续可执行项。 |',
      '', '4.2剩余真实后代UI/真实模型并发迟到、6.2物理冷启动/断电盘满与MSI权限先留证搁置；条件变化再打开。既有通过矩阵不重复执行。','']
    if 'permission' in runtime_matrices:lines=[line.replace('矩阵7/7完成后记入收据','矩阵7/7已通过') for line in lines]
for x in data['items']:
    if x['status'] in ('pending_evidence','in_progress') and x['id'] not in current and (not runtime_matrices or x['id'] not in ('4.2','6.2')): lines.append(f"| {x['id']} {x['item']} | {x['acceptance']} |")
lines+=['','## 留证搁置项','', '遇到已有阻塞先处理其他可执行项；只有环境或证据条件变化时重新打开。','', '| 项目 | 解阻条件 |','| --- | --- |']
for x in data['items']:
    if x['status']=='blocked':
        condition=x['acceptance']
        if x['id']=='0.2': condition='在当前支持的执行链中补长输出连续分页及完整读取证据；不以恢复Devin为前置条件。'
        lines.append(f"| {x['id']} {x['item']} | {condition} |")
plan.write_text('\n'.join(lines)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in audit.items() if k not in ('items',)},ensure_ascii=False))
