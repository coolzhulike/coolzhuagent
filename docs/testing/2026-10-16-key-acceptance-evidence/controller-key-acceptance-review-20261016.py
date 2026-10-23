"""按2026-10-16用户的新指示复核关键功能，不伪造未做的测试。"""
import collections
import datetime
import hashlib
import json
import runpy
import shutil
from pathlib import Path

ROOT=Path(r'D:\coolzhuagent')
R=ROOT/'tmp/goal-32wbs-acceptance-20261013'
ledger=R/'acceptance-checklist.json'
data=json.loads(ledger.read_text(encoding='utf-8-sig'))
archive=ROOT/'docs/testing/2026-10-16-key-acceptance-evidence'
archive.mkdir(exist_ok=True)
for source,name in [(ledger,'strict-checklist-before-rescope.json'),(R/'controller-ledger-review-20261016.py','strict-generator-before-rescope.py')]:
    if not (archive/name).exists(): shutil.copy2(source,archive/name)
now=datetime.datetime.now(datetime.timezone.utc).isoformat()
rows={}
def add(id,status,criterion,conclusion,next_action,receipts,deferred='全面组合矩阵保留为后续加强项，不以未执行代替通过。'):
    rows[id]={'status':status,'criterion':criterion,'conclusion':conclusion,'next':next_action,'receipts':receipts.split(),'deferred':deferred}
A='accepted';P='pending_evidence';B='blocked'
add('0.1',A,'能追溯本次候选源码、完整载荷和实际运行程序身份。',
    '134/135完整出包身份和1222个载荷核验已有证据；MSI覆盖与CI分别归6.2/0.3，不再重复阻断身份项。','已关关键功能，不重跑。',
    'controller-release135-20261016.json controller-package135-live-20261016.json',
    '只证明已核候选；不把后续dirty源码、尚未打包的hook修复或远端CI算进旧包。')
add('0.2',P,'长工具输出可以按result_id连续续读，读取完整内容且不重复执行命令。',
    '82KB输出旧轮被约30秒宿主时限中断，没有result_id、实际读取0字节；连续分页仍缺关键证明。',
    '用当前支持执行链生成一次有首尾哨兵的长输出，连续读到尾并核总量/哈希；失败仅修续读或时限接线。',
    'controller-159-audit.json','取消重跑长程原Devin的前置条件；不做不同模型/所有输出大小穷举。')
add('0.3',B,'最终交付源码有对应CI结果，失败不显示为通过。',
    '已有远端绿灯对应旧提交；最终本地未提交修改没有对应CI，不能沿用旧绿灯。',
    '到最终交付时，按届时有效发布授权提交最终变更并取得该版本CI；当前不擅自push，也不重复本地编译冒充CI。',
    'wbs-0-3-ci-current-head-audit.json')
add('1.1',A,'真实执行的输入、工具、回复归属同一轮；失败/未知不冒充成功。',
    '正式成功/失败链、公开回复与工具记录，及恢复后unknown保留均已有证据。',
    '已关关键功能，不重跑。',
    'controller-283-installed-ui-audit.json controller-289-dom-audit.json controller-runtime-history-20261016.json controller-recovery-contract-matrix-20261016.json',
    '恢复矩阵为人工索引隔离HTTP；未外推全部模型和全部入口。当前已观察绑定为buttercup-pink，不再误称island-kayak。')
add('1.2',A,'输入回执区分未投递、已投递和已释放；执行失败不能抹掉已发生输入。',
    '原生生命周期49项与实机344/345正负例可复用：未命中0输入，实际点击有投递/释放事实，验证失败不篡改输入事实。',
    '已关当前关键链，不重跑。',
    'controller-300-audit.json controller-344-audit.json controller-345-audit.json',
    '多来源真实丢/迟回执组合留作加强；相关负向库测试有效，不宣称已完成实机丢包注入。')
add('1.3',P,'输入租约的第二持有者被拒绝，异常持有者退出后安全接管且unknown不擦除。',
    '会话数据库单写者已验证，输入broker/lease尚缺同一输入安全库的真实双进程接管；二者不能混为一项证据。',
    '复用input_safety_store测试，只补一个双进程租约竞争→持有者退出→接管的闭环，核旧unknown记录不变。',
    'controller-183-audit.json controller-184-audit.json controller-writer-live-matrix-20261016.json')
add('1.4',P,'已支持执行入口继承最小权限，撤权后旧子任务不能继续借旧授权执行。',
    '普通工具允许/拒绝/FullAccess撤销/自然过期已过；DSH或子任务冻结授权关键链仍有ignored而未实际执行。',
    '只补一个已支持子任务入口：父授权冻结→撤权→下一有副作用工具被拒，核零副作用；不再跑普通读取和TTL矩阵。',
    'controller-311-audit.json controller-318-audit.json controller-permission-expiry-matrix-20261016.json')
add('1.5',A,'聊天/Goal在途取消停止后续动作；受控工具超时回收且迟到结果不变成成功。',
    '聊天与Goal真实HTTP在途取消已过；hook根预算、前后取消、回收与迟到零写入新增8项和实际构建通过。',
    '关键链通过；hook新源码随下一批正式交付，不为本项单独打包。',
    'controller-312-audit.json controller-313-audit.json controller-hook-budget-review-20261016.json',
    'hook新增修复尚未部署；外部Provider和所有工具后代组合留后续，授权在途变化仍由1.4跟进。')
add('2.1',A,'配置提交有版本冲突保护，重启后会话、模型与参数一致。',
    '完整服务HTTP提交revision0→1、旧revision拒绝和重启读回已过；提交前后中断恢复也有证据。',
    '已关当前关键配置，不重跑。',
    'controller-341-audit.json controller-342-audit.json',
    '新平台真实凭据接入和物理断电不是本次配置保存的前置；未对未连接平台宣称可用。')
add('2.2',A,'历史可恢复；已知用量归属正确，未知用量显示未知而非0。',
    '11历史表逐行对账、实际模型协议用量41/7/13/2与insights一致、unknown为null；迟到归属隔离HTTP有证据。',
    '已关关键历史/计量，不重跑。',
    'controller-installed-133-history-review.json controller-315-audit.json controller-320-audit.json controller-remaining-matrix-review-20261016.json',
    '真实外部账单及真实多后代并发全链未验；不将mock数值说成提供商实际计费。')
add('2.3',P,'一次工具只派发一次，权限闸有效；声明支持的hook有可用配置和独立授权入口。',
    '普通派发及hook取消/预算已测；默认生产HookRunner仍缺配置和独立授权材料来源，测试override不等于用户可用。',
    '补最小正式hook配置与独立授权接线，再用一个前置hook→目标→后置hook闭环及未授权拒绝验收；不重跑8项预算用例。',
    'controller-245-audit.json controller-hook-budget-review-20261016.json')
add('2.4',P,'单写者和epoch拒绝旧写入；同轮通知崩溃恢复后只投递一次。',
    '135/134同库竞争和epoch八项已过；通知outbox的崩溃窗口与已出现的UI积压尚未形成最后闭环。',
    '仅用隔离接收端核一条通知落盘→发送中断→恢复去重，保留同轮唯一送达证据；不向主会话反复发测试消息。',
    'controller-writer-live-matrix-20261016.json controller-246-audit.json controller-294-audit.json')
add('2.5',P,'已支持共享运行入口创建、取消、恢复使用同一执行身份，不自动重放未知副作用。',
    'execution_service局部17项及索引unknown恢复已过；真实运行worker到恢复的共享入口闭环仍缺。',
    '选一个已支持的隔离共享入口运行→取消/重启→查询，核同ID和无自动重放；不启Devin接力或另建执行会话。',
    'controller-299-audit.json controller-recovery-contract-matrix-20261016.json')
add('3.1',B,'多屏/混合DPI下截图坐标与实际目标一致。',
    '当前证据只含单显示器，缺真实多屏、混合DPI和负坐标硬件条件。',
    '有相应显示环境后合并3.1/6.3做一次截图→定位→输入；当前不再反复测试单屏。',
    'controller-236-audit.json')
add('3.2',P,'声明支持的原生UIA语义动作能操作实际控件，缺目标时不误操作。',
    '旧输入安全根漏接线已修；现有桌面点击/目标正负证据不充分证明UIA pattern动作本身。',
    '只补一个实际UIA控件的Invoke或Value动作及缺目标拒绝，核控件值/效果；取消完整Windows控件能力表穷举。',
    'controller-279-audit.json controller-344-audit.json controller-345-audit.json')
add('3.3',A,'动作验证失败可有界纠正，达到上限停止，不以点击已发送判成功。',
    '真实桌面首次验证失败→第二次纠正3/3通过，次数上限和验证失败终态9项已有回归证据。',
    '已关关键纠正功能，不重跑。',
    'controller-316-audit.json controller-344-audit.json',
    '复杂分页/并发子目标重规划和自然导航误判泛化不在本次关项范围；未宣称所有任务均能自动纠正。')
add('3.4',P,'目标或页面失配时不投递输入，过期目标不能借用新页面。',
    '原生目标正负例通过；旧WebView未知BODY收到keydown的反例只有假DOM修复证据，缺真实负例复验。',
    '只复验原BODY keydown负例及一个明确目标正例；失败定位实际页面提交/目标判定，不穷举所有HRESULT。',
    'controller-317-audit.json controller-345-audit.json')
add('4.1',A,'正式控制台能显示会话/公开回复/头像/运行面板，重开后历史保留。',
    '安装入口和完整发布包正式页面均已有可见证据，图标/头像资源及冷启动历史保留已核。',
    '已关当前单屏关键界面，不重跑。',
    'controller-283-installed-ui-audit.json controller-installed-133-http-files.json controller-installed-133-cold-start-d-workspace.json controller-reply-ui-20261016.json',
    '混合DPI并到3.1/6.3；全部自定义身份组合移加强。C安装134与正式D135差异明确保留。')
add('4.2',A,'每条公开回复能定位自身唯一运行及原提问；搜索可定位，未知统计不伪填。',
    '正式135可见两条回复各自run和原提问4项、HTTP6项、新页/重载2项通过；FTS及并发/隔离补证可复用。',
    '已关逐回复定位，不重跑；MSI覆盖归6.2。',
    'controller-300-installed-search-audit.json controller-reply-ui-20261016.json controller-reply-formal135-20261016.json controller-reply-browser-reload-20261016.json controller-remaining-matrix-review-20261016.json',
    '正式生产库没有真实后代对象，真实后代树UI/外部模型时序留后续，不称已实测。')
add('4.3',P,'长文件/附件可完整读取；清理不会删除仍被会话或任务引用的内容。',
    'spill读回和18项引用检查已过，但GC只有dry-run/deleted=0，Goal/导出/待发送/文本引用保护不完整。',
    '与0.2共用长内容读回证据；补最小有效引用保护后在隔离目录只删一份无引用对象、保留一份有引用对象。',
    'controller-252-audit.json controller-253-audit.json','微信不改不测；任何尚未识别的引用类别采取保守保留，不据此开启全目录GC。')
add('4.4',P,'记忆/SKILL能实际召回供模型使用；清理不破坏仍在使用的资料。',
    'memory36/images9局部测试通过，实际模型召回/使用闭环未证；共享附件GC缺口归4.3修一次。',
    '选一个无敏感信息的记忆样例，保存后由产品会话召回并核引用/实际读取；必要时只用Devin SWE-2作测试对象。',
    'controller-251-audit.json controller-253-audit.json','多模态压缩/全排序矩阵后续加强；不重复4.3引用保护测试，不把外部材料当系统指令。')
add('4.5',P,'Goal失败回退后能修复并复验成功；已支持调度任务重启后不会丢失或重发。',
    '安装134实际工具执行的失败→修复→复验闭环已过；durable调度任务的恢复仍缺一条运行证据。',
    '复用Goal修复证据，只补隔离一次性job落盘→重启→执行一次→终态；不创建主会话监督定时任务。',
    'controller-goal-release-134-installed.json controller-313-audit.json controller-299-audit.json','模型决策使用受控协议夹具，真实HTTP/工具/确定性门控有效；外部模型自主规划与并行DAG复杂矩阵后续加强。')
add('5.1',A,'本机凭据加密保存可读回，坏密文被拒绝，诊断不泄漏秘密。',
    '本机DPAPI、旧明文迁移、损坏拒绝和诊断脱敏13项通过，均用假凭据且未读取真实密钥。',
    '已关本机关键凭据存储，不重跑。',
    'controller-234-audit.json','换设备需重新认证，不能搬用DPAPI密文；跨设备恢复与全来源迁移组合留专项。')
add('5.2',P,'执行进程受生命周期约束，并有实际可证的OS权限隔离边界。',
    'Job杀子树和socket不继承已有证据；受限令牌/低完整性/AppContainer隔离没有实证，独立进程不等于沙箱。',
    '先定位实际启用的隔离机制；若不存在则记实现缺口并修，若存在只做一次受限资源访问拒绝和正常工作区正例。',
    'controller-235-audit.json','取消全逃逸手法穷举，但不能删掉OS边界本身的验收标准。')
add('5.3',A,'既定stdio MCP能连接、调用和关闭，命名冲突/权限有保护。',
    '官方Everything Server真实stdio互通及既有生命周期/权限证据已通过，保持原已验收结论。',
    '已关，不重跑。','controller-173-audit.json controller-198-audit.json','只含stdio；延期transport/OAuth不列入本次计划，不冒充已交付。')
add('5.4',P,'代码导航能返回实际位置，终端能启动/输出/结束。',
    'PTY已有退出/Unicode证据；正式LSP导航502没有复验关闭，隔离unlinked-file空结果不能证明成功。',
    '只用当前仓库一个真实Rust符号走正式导航取文件行号；失败定位LSP项目初始化/路由，已有终端测试不重跑。',
    'controller-202-audit.json controller-203-audit.json')
add('5.5',P,'适用语音连接器能接收真实音频，停止后不再产生旧轮输出。',
    '只有start/stop/status协议成功和session ID隔离，尚无真实音频处理证据；这属于未补关键测试，不冒称已证环境不可用。',
    '先核可用语音后端，选一段本地无敏感测试音频做开始→处理→停止；若缺模型/设备再记录确切条件并转其他项。',
    'controller-325-audit.json','微信按既有要求不改不测；不做所有连接器组合穷举。')
add('6.1',P,'日志有总量上限、不写穿目录、可脱敏导出，旧进程退出不会永久耗尽额度。',
    '跨进程总额和硬链接拒绝已过；配额成员清单仍可无限追加，旧属主退出后可能长期占额，另缺完整脱敏导出。',
    '只修成员清单上限和安全回收，共用一条双进程退出/接管用例，再核一个含假密钥的导出脱敏结果。',
    'controller-327-audit.json','保留已通过预算/硬链接例，不重复全量诊断测试；符号链接环境限制单列。')
add('6.2',B,'默认启动不超时，升级保留原数据并实际替换机器安装版本。',
    '启动120秒期限、正式135新进程12.837秒和迁移恢复已过；机器MSI覆盖被1730/1603管理员权限拦下，C仍134。',
    '操作系统安装权限条件具备时做一次最终包覆盖→确认注册版本/实际路径→原历史保留；不反复同权限重试，也不要求用户现在审批。',
    'controller-package135-live-20261016.json controller-remaining-matrix-review-20261016.json controller-switch135-20261016.json',
    '物理断电/盘满、所有慢机器长尾退出当前计划；SQLite FULL和强杀恢复已有证据。')
add('6.3',B,'Windows当前声明支持的多屏/DPI窗口与输入行为一致。',
    '与3.1共用同一硬件缺口；已有单屏144DPI和减少动态证据，不重复测试。',
    '真实多屏环境具备后与3.1合并一轮；其余单屏界面/启动结论分别复用4.1和EX-03。',
    'controller-236-audit.json controller-installed-133-native.json','启动损坏资源注入属于加强项，不再额外卡住已正常的开机动画。')
add('6.4',P,'交付包组件与许可证/NOTICE一致，版本和交付说明可追溯。',
    '最近许可证审计556组件中82缺本地原文，实际组件映射未确认；需要重核实际缺项，不是继续跑业务功能测试。',
    '按最终载荷去重核许可证缺项并补NOTICE/映射，签名与远端发布作为交付条件单列；不对未核组件声明已合规。',
    'controller-281-wbs-6-4-audit.json','缺签名证书不阻断本地功能验收，但保留最终发布限制；不未经授权对外发布。')
add('EX-01',A,'适用控制台完整套件有真实完成结果，后续改动按影响范围补验。',
    '已核源码完整测试1492过、0败、6忽略；本轮后续hook八项及实际build通过，保持原关项。',
    '不再重跑已过全量；只有后续修改影响未覆盖行为时补定点测试。',
    'controller-current-full-20261016.json controller-hook-budget-review-20261016.json','旧全量证据只对其源码有效，不称当前hook修改已重跑完整套件；忽略不计通过。')
add('EX-02',B,'只清理已确认可恢复且无占用的C盘旧数据，并核实际释放字节。',
    '按用户后续要求留待手动处理；16目录约276.7MiB已核可清，实际释放0。另9.28GB工作树含D归档缺失文件，不能直接删。',
    '收到实际手工清理结果后核空间；独有文件先归档再另判，junction指向D的数据不算C回收，不自动删除。',
    'controller-c-readonly-20261015.json controller-c-reference-readonly-20261015.json controller-gotdot-compare.json')
add('EX-03',A,'去旧印章/舞剑，COOLZHU下方居中打坐；动画可跳过并正常进入控制台。',
    '安装版关键帧、首次/日常交接、减少动态与Esc/按钮/演出区跳过均已有实际证据；基本视觉需求通过。',
    '已关关键动画，不重跑。',
    'controller-installed-133-native.json controller-installed-133-cold-start-d-workspace.json controller-installed-133-click-button.json controller-installed-133-click-area.json',
    '资源损坏注入此前没有成功，仍未验证；本次移到故障加强，不把它伪写为通过。')

active={x['id'] for x in data['items'] if x['status']!='waived'}
assert active==set(rows),(active-set(rows),set(rows)-active)
receipt_checks={}
for row in rows.values():
    for name in row['receipts']:
        p=R/name
        assert p.is_file(),name
        raw=p.read_bytes();record=json.loads(raw.decode('utf-8-sig'))
        state=record.get('state',record.get('verdict',record.get('decision'))) if isinstance(record,dict) else '结构化数组，按各条原始结论保留'
        receipt_checks[name]={'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'historical_state':state}
prior_counts=dict(collections.Counter(x['status'] for x in data['items']))
for x in data['items']:
    if x['status']=='waived':continue
    if 'strictReviewBeforeRescope' not in x:
        x['strictReviewBeforeRescope']={k:x.get(k) for k in ['status','acceptance','latestEvidence','updatedAtUtc']}
    k=rows[x['id']]
    x.update(status=k['status'],acceptance=k['criterion'],keyAcceptance=k,updatedAtUtc=now,
        latestEvidence=k['conclusion']+' 证据：'+'、'.join(k['receipts'])+'。扩展限制：'+k['deferred'])
data['acceptanceMode']='key_functional'
data['schemaVersion']=2
data['blockedRule']='只有已确认的外部条件或用户明确手工处置才列blocked；代码缺口/关键证据不足列pending_evidence。既有安全反例不因缩减矩阵销项。'
plan_order=['0.2','3.4','5.4','1.4','2.3','4.3','6.1','1.3','2.4','2.5','3.2','4.4','4.5','5.2','5.5','6.4']
assert set(plan_order)=={id for id,k in rows.items() if k['status']==P}
review=data['currentReview']
review.update(keyAcceptanceReview='controller-key-acceptance-review-20261016.json',acceptanceMode='key_functional',
    installedRuntime='最近已验证：正式8765来自D盘完整0.2.135包；C机器安装仍0.2.134。见controller-package135-live-20261016.json；本次仅重新审核证据，没有重启或切换。',
    activePlan={'current':plan_order,'excludeStatuses':['accepted','waived','blocked'],'rule':'默认一个关键正例及必要安全负例；复用历史证据，已过不重跑；仅具体新缺陷或代码变更追加测试。','externalBlocked':[id for id,k in rows.items() if k['status']==B]},
    scopeChange='用户要求改为关键功能验收，缩小冗余测试；原严格状态保存在每项strictReviewBeforeRescope及独立快照。')
report={'at_utc':now,'mode':'key_functional','action':'仅复用实际证据重新判定并收窄计划，本脚本不执行产品测试/安装/模型调用','prior_strict_counts':prior_counts,
 'counts':dict(collections.Counter(k['status'] for k in rows.values())),
 'items':[{'id':id,**k} for id,k in rows.items()], 'receipt_checks':receipt_checks,
 'limits':['旧严格版2项与新关键版13项口径不同，不能宣称本轮新实现11项功能。','仍有21项关键缺口或条件阻塞，未宣称35项完整交付。','当前hook新源码尚未进入正式安装包。','旧失败/忽略/未执行证据保持原文件与SHA，不作修改。']}
(R/'controller-key-acceptance-review-20261016.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
temp=ledger.with_suffix('.next.json');temp.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');temp.replace(ledger)
for p in [R/'controller-key-acceptance-review-20261016.json',ledger,R/'controller-key-acceptance-review-20261016.py',R/'controller-key-ledger-render-20261016.py']:
    shutil.copy2(p,archive/p.name)
manifest={'at_utc':now,'files':[]}
for p in sorted(archive.iterdir()):
    if p.name=='manifest.json':continue
    b=p.read_bytes();manifest['files'].append({'path':p.name,'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()})
(archive/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
runpy.run_path(str(R/'controller-key-ledger-render-20261016.py'))
print(json.dumps({'evidence_receipts':len(receipt_checks),'scope_archive':str(archive),'counts':report['counts']},ensure_ascii=False))
