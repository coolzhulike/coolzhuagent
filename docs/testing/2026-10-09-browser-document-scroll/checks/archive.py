"""归档真实候选终态和原图；不得用候选替代正式安装验收。"""
from pathlib import Path
import hashlib,json,shutil
p=Path(__file__).resolve().parent;root=p.parents[1];dest=root/'docs/testing/2026-10-09-browser-document-scroll'
def read(path):return json.loads(path.read_text(encoding='utf-8-sig'))
v=read(p/'browser-scroll/verification.json');assert v['passed']
facts=read(p/'browser-scroll/facts.json');assert facts['run']['state']=='completed'
assert read(p/'browser-scroll/diagnostic-verification.json')['passed']
for kind in ('web-build','web','shell-build','shell','protocol','linkage','tooling'):
 assert read(p/(kind+'-result.json'))['exit_code']==0
for entry in read(p/'candidate-source.json'):
 raw=(root/entry['path']).read_bytes();assert len(raw)==entry['bytes'] and hashlib.sha256(raw).hexdigest()==entry['sha256']
out=dest/'browser-scroll';out.mkdir(parents=True,exist_ok=True)
for f in (p/'browser-scroll').iterdir():
 if f.is_file() and f.suffix in ('.py','.json','.jsonl','.png'):shutil.copy2(f,out/f.name)
out=dest/'checks';out.mkdir(exist_ok=True)
for f in p.iterdir():
 if f.is_file() and (f.suffix in ('.py','.ps1') or f.name.startswith(('web-','shell-','protocol-','linkage-','tooling-','candidate-source','candidate-processes','startup-failure'))):shutil.copy2(f,out/f.name)
steps=v['terminal']['input_steps'];scrolls=[x for x in steps if x['action_kind']=='scroll'];unknown=sum(x['effect_status']=='inconclusive' for x in steps)
report=f'''# Browser 稳定文档滚动与真实长程候选验收

候选通过：原 SWE-2-medium / 唯一 Devin island-kayak、revision57，两个子文档真实滚动、读取随机订单码、输入并按 Enter 提交，{len(steps)}次动作。{len(scrolls)}次滚动均 sent/not_needed/effect_observed，两次文本改变由宿主读回确认；{unknown}步效果仍为 inconclusive。最终宿主2/2目标成功，随后仅一次实际 DSH 计算器，总额{v['independent_total']}。根 completed，两个工具各一次 completed，单 end_turn/drained，唯一绑定解锁。

run：{v['run_id']}。普通双来源 HTML 页面使用两个独立内嵌文档，表单位于各文档底部。首次滚动通过普通 scroll/message 事件在外层插入新的文字节点，同时保持外层回执容器高度；行情每100ms更新。两项输入码及价格随机生成，任务提交未预泄露，必须从当前页面读取。独立网页日志核对两文档各自 trusted wheel、scroll、input 和实际 submit 通过，以及回执插入先于后续输入。全程正常右栏导航后由模型工具操作，没有模型/工具响应夹具，没有脚本代填表单。

原 NodeCache 按 DocumentSnapshot 的文档范围保存随机 document_scope_id，同文档 AX 重新排序和动作后旧节点引用撤销时保留，资源/文档替换即更新。只给 RootWebArea 投影，真实 CDP frame/loader/backend/session 身份不外传。原 PageProvenance 绑定当前 Scroll 规划观察和所选身份；原目标索引仅用于定位原观察，后观察按同一个唯一文档身份比较视口。缺身份、重复身份、换文档或非法视口保持未知，不退回序号匹配。

离线双端 build 实际0，Web1429/0/6既有忽略（另lib8/native-host1），壳78/0，模块联动8/0，tool-registry check0；协议crate目前0测试，未将其描述为有测试覆盖。必要回归覆盖同文档序号漂移、另文档占原序号、重复/缺身份、资源/文档替换和非法非根身份。软件实拍和网页事件证明本次真实操作；未新增原始宿主 AX 身份诊断，不能把截图索引当作原生 CDP 身份审计。

候选第一次启动脚本复制时错误替换收据名称，Get-Process在旧PID19680失败exit1，未停止任何进程。错误脚本/收据说明及初次目录保留；修正后以EXE路径、SHA、启动UTC ticks三项核对最新原候选，正常切换启动exit0。原候选订单/负例失败证据见[本步效果报告](../2026-10-09-browser-action-progress/report.md)，未追认失败轮通过。

![真实软件初态](browser-scroll/native-initial.png)

![首个文档滚动并插入回执](browser-scroll/native-first-scroll.png)

![第二文档滚动和第一订单通过](browser-scroll/native-second-scroll.png)

![聊天室与浏览器终态](browser-scroll/native-terminal.png)

这是源码候选，正式0.2.122仍不含本步效果和滚动身份修补。下一批正式包需独立新随机订单复验与行情负例，不能直接复用候选结论。严格新原生Target、跨来源commit/down-up、在途撤销/HRESULT窄竞争及其余32工作包继续开放。文档身份事实用于效果归因，不扩展输入权限，也不完整证明脚本自发滚动/转场与本步输入的严格因果。Paint免测、微信不动、Opus暂停，Goal active。
'''
(dest/'report.md').write_text(report,encoding='utf-8')
attrs=root/'.gitattributes';rule='/docs/testing/2026-10-09-browser-document-scroll/** -text whitespace=-blank-at-eof,-blank-at-eol,-space-before-tab,cr-at-eol'
if rule not in attrs.read_text(encoding='utf-8'):
 with attrs.open('a',encoding='utf-8') as f:f.write('\n'+rule+'\n')
note=f'2026-10-09滚动文档身份候选：原NodeCache按文档范围保存稳定随机身份，前后视口不再依赖AX序号。真实SWE-2/唯一island双子文档长程{len(steps)}动作、{len(scrolls)}次滚动effect_observed、两项随机码提交，宿主2/2后DSH总额{v["independent_total"]}；{unknown}步效果保持未知，根completed、单end_turn/drained并解锁。双端build0、Web1429/0/6、壳78/0、联动8/0。只计候选，下一批正式包与独立复验、严格原生窄时序及其它32WBS仍开放。[候选实拍与交接](../../testing/2026-10-09-browser-document-scroll/report.md)。'
for name in ('acceptance-summary-2026-10-08.md','current-acceptance-queue.md','implementation-status-and-change-inventory.md','wbs-implementation-audit-2026-10-07.md'):
 doc=root/'docs/analysis/2026-09-21-integration-review'/name;text=doc.read_text(encoding='utf-8')
 if note not in text:
  head,sep,body=text.partition('\n');doc.write_text(head+'\n\n'+note+'\n'+sep+body,encoding='utf-8')
worklog=root/'docs/work-logs/2026-10-09-browser-document-scroll.md'
with worklog.open('a',encoding='utf-8') as f:f.write('\n'+note+'\n')
rows=[]
for f in sorted(dest.rglob('*')):
 if f.is_file() and f.name!='evidence-manifest.json':
  raw=f.read_bytes();rows.append({'path':f.relative_to(dest).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(dest/'evidence-manifest.json').write_text(json.dumps(rows,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'candidate_passed':True,'archived_files':len(rows),'formal_package_verified':False,'scrolls':len(scrolls),'unknown_effects':unknown},ensure_ascii=False))
