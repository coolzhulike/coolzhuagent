"""核验本轮三组独立矩阵后归档并同步清单；不删除历史结果。"""
import collections,datetime,hashlib,json,pathlib,socket,subprocess,urllib.request
REPO=pathlib.Path(r'D:\coolzhuagent')
OUT=REPO/'tmp/goal-32wbs-acceptance-20261013'
ARCHIVE=REPO/'docs/testing/2026-10-16-runtime-contract-evidence'
ARCHIVE.mkdir(exist_ok=True)
names={'writer':('controller-writer-live-matrix-20261016',8),'recovery':('controller-recovery-contract-matrix-20261016',13),'permission':('controller-permission-expiry-matrix-20261016',7)}
reports={}
for label,(name,count) in names.items():
 value=json.loads((OUT/(name+'.json')).read_text(encoding='utf-8'))
 assert value['state']=='completed' and value['passed']==count and value['failed']==0,value
 assert len(value['cases'])==count and all(c['state']=='passed' for c in value['cases'])
 assert all(p.get('process_exited') for p in value['processes']),value['processes']
 reports[label]=value
ports={**{str(p):False for p in range(29501,29508)},'29511':False,'29512':False}
for port in ports:
 with socket.socket() as sock:
  sock.settimeout(.5);ports[port]=sock.connect_ex(('127.0.0.1',int(port)))==0
assert not any(ports.values()),ports
with urllib.request.urlopen('http://127.0.0.1:8765/api/workspace',timeout=10) as response:
 formal={'http':response.status,'workspace':json.load(response)}
assert formal['http']==200
assert pathlib.Path(formal['workspace']['workspace']).resolve()==REPO.resolve(),formal
formal['workspace']={key:formal['workspace'][key] for key in ['workspace','workspace_id','status','writable']}
with urllib.request.urlopen('http://127.0.0.1:8765/api/system/info',timeout=10) as response:
 formal['system_info']=json.load(response)
identity_probe=subprocess.run(['powershell.exe','-NoProfile','-Command',"$runtimeOwnerIds = @(Get-NetTCPConnection -LocalPort 8765 -State Listen -ErrorAction Stop | Select-Object -ExpandProperty OwningProcess -Unique); $runtimeOwnerRecords = foreach ($runtimeOwnerId in $runtimeOwnerIds) { Get-CimInstance Win32_Process -Filter \"ProcessId=$runtimeOwnerId\" | Select-Object ProcessId, ExecutablePath }; $runtimeOwnerRecords | ConvertTo-Json -Compress"],capture_output=True,text=True,encoding='utf-8',errors='replace')
assert identity_probe.returncode==0,identity_probe.stderr
owners=json.loads(identity_probe.stdout);owners=[owners] if isinstance(owners,dict) else owners
assert len(owners)==1,owners
binary=pathlib.Path(owners[0]['ExecutablePath'])
assert binary.resolve()==pathlib.Path(reports['writer']['processes'][0]['exe']).resolve(),owners
formal['port_owner']={**owners[0],'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}
assert formal['port_owner']['sha256']==reports['permission']['binary_sha256']
review={'at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'decision':'partial-accepted','passed':20,'failed':0,'actual_checks_passed':28,'excluded_scope_checks':8,
 'groups':{k:{'receipt':names[k][0]+'.json','passed':v['passed'],'processes_exited':len(v['processes'])} for k,v in reports.items()},
 'formal_health':formal,'isolated_ports_open':ports,'source_modified':False,'external_model_calls':0,'external_message_sends':0,'supervision_created':False,
 'temporary_root':str(OUT),'remaining_gates':{
 '2.4':['生产双实例与更旧不遵锁helper','主控通知outbox崩溃窗口和Codex队列UI积压；Clawbot本地outbox不是该通知通道'],
 '1.1/2.5':['实际worker/模型在途取消和崩溃恢复','正式已支持入口全链；本轮人工execution_index不是真实会话执行'],
 '1.4':['DSH/chat父轮冻结grant','在途撤权和迟到工具结果','正式页面权限入口完整验收']},
 'scope_correction':'恢复脚本前八项误选Clawbot本地outbox。补核微信不改不测约定后停止该方向；历史实际结果原样保留，八项均标范围外，不计完成进度。恢复脚本后五项执行索引契约属于本轮范围。',
 'temporary_files_note':'本轮脚本/收据/数据库/stdout集中统一根；前两组测试服务沿用程序默认共享诊断日志，第三组已通过COOLZHU_LOG_DIR重定向至夹具logs。未删除共享日志、旧回退备份、活动发布包或编译缓存。只终止本轮自有进程，无文件删除。'}
(OUT/'controller-runtime-matrices-review-20261016.json').write_text(json.dumps(review,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
run=subprocess.run([r'C:\Python314\python.exe',str(OUT/'controller-ledger-review-20261016.py')],cwd=REPO,capture_output=True,text=True,encoding='utf-8',errors='replace')
(OUT/'controller-runtime-ledger-sync-20261016.log').write_text(run.stdout+run.stderr,encoding='utf-8')
assert run.returncode==0,run.stderr

doc=REPO/'docs/testing/2026-10-16-runtime-contract-matrices.md'
doc.write_text('''# 其他未完成项：发布服务恢复契约补验

2026-10-16。由主会话直接执行与审核，不创建执行/监督会话或监督任务。已阻塞的真实模型后代UI、物理故障及机器级MSI权限先搁置。本轮没有修改生产或测试源码，没有构建新安装包。

## 实际结果

| 矩阵 | 结果 | 实际执行边界 |
| --- | --- | --- |
| 2.4 单写者 / 混合版本 / epoch | 8过、0败，脚本exit0 | 完整0.2.135与已安装0.2.134服务，隔离共享会话库，独立进程和真实HTTP |
| 1.1/2.5索引恢复 | 5过、0败 | 完整135服务，隔离预置执行索引，实际HTTP及强杀重启；该脚本实际13过exit0，另八项Clawbot检查范围外 |
| 1.4 权限自然过期 | 7过、0败，脚本exit0 | 隔离Devin SWE-2本地对象与人工文本，实际60秒授权和五分钟pending自然到期，零模型调用 |

共20个范围内验收检查通过。恢复脚本误选八项Clawbot本地outbox检查；补核微信不改不测约定后停止该方向，结果原样保留但标为范围外，不计完成进度。它们补齐子矩阵，不将整项门禁直接改为完成。

## 2.4 单写者与epoch

同版竞争、新135持有旧134竞争、旧134持有新135竞争，均在开放HTTP前拒绝第二写者且不改库。强杀旧持有者并确认退出后，旧版接管再回新版，业务记录保留，epoch从1至2至3。只在隔离库把权威epoch改成103后，旧内存写者连续两次HTTP保存409，数据库和可见缓存均无幽灵记录；重启获得104，继续正常保存，quick_check为ok。

这是完整服务与两版程序证据，不再仅依赖预编译锁测试探针。正式生产双写、比134更旧且不遵锁的helper未测试。

## 范围纠正

八项Clawbot本地outbox检查虽实际通过，但属于微信不测范围，完全不纳入本轮验收。没有外部发送或微信界面操作。后续不继续该方向，不能据此宣称主控通知outbox已完成崩溃矩阵、Codex界面积压已清理，或外部投递exactly-once。

## 1.1 / 2.5 未知状态与恢复

人工预置六类执行索引，强杀完整服务后重启读取。无存活句柄的running/cancel_requested显示outcome_unknown，既有completed/failed/interrupted/outcome_unknown不被改写；unknown用量保留null。六类无句柄取消均409，原库不被写成“取消成功”。缺冻结身份或旧scope为400/409，跨工程详情/trace为404。无真实run绑定的轨迹409，不能按房间猜测。二次重启不自动重放，两个隔离库quick_check为ok。

人工索引不是在途模型/worker，不代替真实取消与正式支持入口全链；本轮没有创建产品调度任务。

## 1.4 授权与待审批自然到期

仅在隔离服务创建Devin SWE-2本地配置对象，不连接远端。最低权限可读工作区内人工文本；同一根内、工作区外的人工文本必须审批且output为null。缺风险确认或二次确认的grant为400，无授权副作用。请求1秒授权被正式实现钳制至60秒，活跃期间只授权本会话；其它会话不继承，未知工具仍拒绝。

未修改系统时钟，真实等待60秒后授权失效，越界读取重新require-approval且output=null，工作区内读取仍正常。跨会话消费pending为404且原待办保留；真实五分钟后待办从列表失效，旧approve为404且不执行。新授权后强杀重启，临时grant和pending均不重放，本地Devin配置保留。

DSH/chat父轮冻结grant、在途撤权及正式页面完整权限入口仍缺，不计整项关闭。

## 收尾与证据

全部自有隔离服务已退出，九个测试端口均关闭；正式8765仍200且工作区为D:\\coolzhuagent。没有重启正式服务，没有调用外部模型或发送外部消息。所有新脚本、数据库、收据和stdout在统一临时根；前两组服务沿用程序默认共享诊断日志，第三组已设置COOLZHU_LOG_DIR集中至夹具。未删除共享日志、活动135包、既有回退备份或构建缓存。

关键小型收据、脚本与服务日志归档于[证据清单](2026-10-16-runtime-contract-evidence/manifest.json)，包含逐文件SHA-256。统一[验收清单](2026-10-13-goal-32wbs-acceptance-checklist.md)与[执行计划](2026-10-16-active-acceptance-plan.md)已同步。
''',encoding='utf-8')
worklog=REPO/'docs/work-logs/2026-10-16-acceptance-takeover.md'
with worklog.open('a',encoding='utf-8') as file:
 file.write('\n## 其他未完成项恢复契约补验\n\n主会话直接执行，4.2/6.2已阻塞子矩阵先搁置；无独立执行者或监督任务。完整135/134服务单写者与epoch8/8，execution_index恢复HTTP五项通过，Devin SWE-2本地对象权限自然到期7/7；共20项范围内检查过0败。恢复脚本另八项误选Clawbot本地outbox，补核微信不改不测约定后停止，结果原样留存但完全排除验收计数；三个脚本实际均exit0。未改生产/测试源码，未构建新包，未调用外部模型。详见[本轮矩阵](../testing/2026-10-16-runtime-contract-matrices.md)。\n\n所有13个自有服务进程（写者7、恢复4、权限2）均确认退出，测试端口关闭；正式8765健康，端口持有者路径与135 EXE SHA吻合。首次收尾脚本误把system/info.build_version的提交号日期当作语义版本，断言失败；已保留失败日志，按实际端口PID/路径/SHA修正核验，没有修改服务。前两组程序默认追加共享诊断日志，第三组已改COOLZHU_LOG_DIR集中输出，未删除共享日志。主控通知outbox/队列UI积压、真实worker取消和正式权限全链仍缺，不把Clawbot或人工索引外推；原四项整项状态保留。清单与生成器同步，新证据独立归档。\n')

files=[OUT/'controller-runtime-matrices-review-20261016.json',OUT/'controller-runtime-ledger-sync-20261016.log',OUT/'controller-runtime-final-health-20261016.json',OUT/'controller-complete-runtime-matrices-first-failure-20261016.log',OUT/'controller-complete-runtime-matrices-20261016.py',OUT/'controller-ledger-review-20261016.py']
for label,(stem,count) in names.items():files.extend(OUT/(stem+suffix) for suffix in ['.py','.json','.log'])
for dirname in ['writer-live','recovery-contract','permission-expiry']:
 fixture=OUT/'other-matrix-20261016'/dirname
 files.extend(fixture.glob('server-*.log'))
 if dirname=='writer-live':files.extend(fixture.glob('*/server.log'))
manifest=[]
for source in files:
 rel=source.relative_to(OUT);dest=ARCHIVE/rel;dest.parent.mkdir(parents=True,exist_ok=True)
 raw=source.read_bytes();dest.write_bytes(raw);sha=hashlib.sha256(raw).hexdigest()
 assert hashlib.sha256(dest.read_bytes()).hexdigest()==sha
 manifest.append({'source':str(source),'file':rel.as_posix(),'bytes':len(raw),'sha256':sha})
(ARCHIVE/'manifest.json').write_text(json.dumps({'at_utc':review['at_utc'],'passed':20,'failed':0,'excluded_scope_checks':8,'files':manifest},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
ledger=json.loads((OUT/'acceptance-checklist.json').read_text(encoding='utf-8'))
assert all(x['status'] not in ('accepted','waived') for x in ledger['items'] if x['id'] in ['1.1','1.4','2.4','2.5'])
assert '3.5 Paint' not in (REPO/'docs/testing/2026-10-16-active-acceptance-plan.md').read_text(encoding='utf-8')
print(json.dumps({'passed':20,'failed':0,'excluded_scope_checks':8,'archived_files':len(manifest),'status_counts':dict(collections.Counter(x['status'] for x in ledger['items'])),'formal_http':formal['http'],'isolated_ports_closed':len(ports)},ensure_ascii=False))
