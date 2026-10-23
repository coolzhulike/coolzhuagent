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
audit={'at_utc':now,'scope':'35项逐项核查；已豁免项保留历史但完全不进入执行计划','executor':'当前主会话直接执行，不派发Devin，不启动旧监督','items':[]}
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
data['execution']='2026-10-16起由当前主会话直接执行、监督、取证和审核；不再派发Devin或监督会话。'
review=data.setdefault('currentReview',{})
review.update(date='2026-10-16',workLog='docs/work-logs/2026-10-16-acceptance-takeover.md',itemAudit='controller-35-item-review-20261016.json',activeBuildReceipt='controller-current-full-20261016.json',takeoverRule='豁免项不进入执行计划；阻塞项留证后转其他可执行项；不改旧失败为成功。')
if complete:
    review.update(fullTestReview='controller-current-full-20261016.json',fullTestResult='当前逐回复定位源码debug/test完整套件：主1489+子3通过、0失败6忽略、微信26过滤；前端6通过，源码前后未变。',activeBuildReceipt=None,candidateBoundary='0.2.134仍为正式安装版；逐回复定位候选已build/前后端/HTTP/完整套件通过，正式部署UI验收仍开放。')
by_id={x['id']:x for x in data['items']}
by_id['0.2']['acceptance']='由当前支持的执行链补长输出8192→69632连续分页和完整读取证据；保留旧失败，不以恢复Devin为前置。'
by_id['1.1']['latestEvidence']='controller-runtime-history-20261016.json只读全行多重集核验：冷启动前后11表逐行一致，2336消息/6060工具/828运行/1352 ACP attempts等均保留，quick_check ok。当前工作区ws-8d17063fefe5c23c中原agent_id的remote_session_id实际为buttercup-pink，已不同于早期island-kayak；差异在本轮启动前已有，不能宣称本轮仍绑定island-kayak。用户现要求主会话独立执行，本轮不改绑、不派发；完整入口/终态/未知矩阵仍缺。'
by_id['4.2']['latestEvidence']='controller-message-run-build.json终态completed：前端6过、离线build exit0、chat_insights后端8过0败。主会话新增controller-reply-http-20261016.json：候选独立HTTP合成夹具10项全过（唯一旧回复、跨房间/工程、缺失/无归属/已删除404、冲突/分页400、歧义409），隔离进程已退出；未改生产库、未调用模型。尚未出包，正式0.2.134逐回复UI直达及后代汇总/迟到并发等整项矩阵仍开放。'
by_id['6.2']['latestEvidence']='0.2.134正式安装历史保留。2026-10-16冷启动首次30秒健康检查超时、后台被启动器停止；主会话先备份原库，用隔离诊断配置将等待上限延至180秒后约23秒就绪。安装目录EXE/桌面壳、D工作区/原库/公开回复已可见；controller-runtime-history-20261016.json证实11张历史表前后逐行一致。实际既有远端绑定是buttercup-pink，不能宣称仍是island-kayak；本轮未改绑。安装配置文件未改，默认冷启动超时风险及完整旧库/磁盘满/断电/回退矩阵仍待解决。'
by_id['3.2']['latestEvidence']='旧279轮input_safety_root_not_injected为历史失败，后续343已补接线、344原生输入有界纠正及345严格nativeTarget正负例通过；不再把旧输入根注入故障列为当前阻塞。完整Windows控件pattern正负实机能力表仍缺，整项保留blocked。'
by_id['3.3']['latestEvidence']='controller-344-audit.json真实隔离桌面两步纠正通过：第1步已投递/已释放但验证失败，第2步已投递/已释放且3/3视觉通过。仅该链通过；完整次数上限、分页、取消迟到、子目标重规划及自然导航误判矩阵仍待验。'
by_id['3.4']['latestEvidence']='controller-345-audit.json：严格原生目标正负例通过；未命中时0输入，正例已投递/释放且视觉2/2。真实跨来源commit、在途撤销/HRESULT及误拒率仍缺，旧BODY keydown负例失败保留。'
for key in ['1.1','4.2','6.2','3.2','3.3','3.4']: by_id[key]['updatedAtUtc']=now
review['activePlan']={'current':['4.2','6.2'],'excludeStatuses':['waived','accepted'],'blockedPolicy':'阻塞留证，不阻止其他项；需硬件/外部凭据/签名的项目不伪造环境。'}
tmp=ledger.with_suffix('.next.json')
tmp.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
tmp.replace(ledger)

labels={'accepted':'已验收','blocked':'阻塞搁置','in_progress':'进行中','pending_evidence':'待补证','waived':'豁免'}
doc=root/'docs/testing/2026-10-13-goal-32wbs-acceptance-checklist.md'
lines=['# 统一验收清单：当前执行范围','',f'核查日期：2026-10-16。历史总计35项；已豁免1项仅保留原JSON历史，本页及后续计划不列该项。当前主会话独立执行、监督和审核，不派发Devin或旧监督会话。','', '临时产物及收据统一保存在 `D:\\coolzhuagent\\tmp\\goal-32wbs-acceptance-20261013`。JSON保留历史失败与本轮新增证据；不将单元测试、合成夹具或旧版测试外推为整项通过。','', '本轮EX-01当前源码完整回归已通过。当前优先顺序：4.2逐回复运行定位 → 6.2冷启动期限；动画原生资源故障等阻塞项先留证，继续其他可执行项。','', '| 项目 | 验收项 | 状态 | 整项门禁范围 | 最新证据 |','| --- | --- | --- | --- | --- |']
for x in data['items']:
    if x['status']=='waived': continue
    row=[x['id'],x['item'],labels[x['status']],x['acceptance'],x.get('latestEvidence',x.get('evidence',''))]
    lines.append('| '+' | '.join(str(t).replace('|','／').replace('\n',' ') for t in row)+' |')
lines+=['','证据审计：`controller-35-item-review-20261016.json`；已完成/豁免状态不进入后续执行队列。']
doc.write_text('\n'.join(lines)+'\n',encoding='utf-8')
plan=root/'docs/testing/2026-10-16-active-acceptance-plan.md'
lines=['# 当前验收执行计划','', '执行者：当前主会话；派发、监督、取证与审核合并在当前会话。每项按真实退出码、统计、源码身份及适用UI证据判定。已验收项不重复派发。','', '## 当前推进','', '| 优先 | 项目 | 下一步 |','| --- | --- | --- |','| 1 | 4.2 | 候选HTTP与前后端、完整回归已过，继续部署后的逐回复真实运行定位；其余整项矩阵独立验收。 |','| 2 | 6.2 | 复核并处理默认启动期限不足；保留失败日志、原数据库及绑定。 |','', '## 其余可执行项','', '| 项目 | 仍需完成 |','| --- | --- |']
current=set(review['activePlan']['current'])
for x in data['items']:
    if x['status'] in ('pending_evidence','in_progress') and x['id'] not in current: lines.append(f"| {x['id']} {x['item']} | {x['acceptance']} |")
lines+=['','## 留证搁置项','', '遇到已有阻塞先处理其他可执行项；只有环境或证据条件变化时重新打开。','', '| 项目 | 解阻条件 |','| --- | --- |']
for x in data['items']:
    if x['status']=='blocked':
        condition=x['acceptance']
        if x['id']=='0.2': condition='在当前支持的执行链中补长输出连续分页及完整读取证据；不以恢复Devin为前置条件。'
        lines.append(f"| {x['id']} {x['item']} | {condition} |")
plan.write_text('\n'.join(lines)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in audit.items() if k not in ('items',)},ensure_ascii=False))
