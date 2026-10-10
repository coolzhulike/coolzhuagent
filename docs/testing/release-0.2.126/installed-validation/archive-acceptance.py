"""只在本批正式实操完成后归档原字节，所有通过范围来自独立核验。"""
from pathlib import Path
import json,hashlib,shutil,datetime
p=Path(__file__).resolve().parent;root=p.parents[1];dest=root/'docs/testing/release-0.2.126'
def read(path):return json.loads(path.read_text(encoding='utf-8-sig'))
v=read(p/'installed-126-verification.json');source=(p/'source-commit.txt').read_text().strip()
assert v['source_commit']==source and read(p/'build-result.json')['actual_exit_code']==0
assert read(p/'install-126-result.json')['exit_code'] in (0,3010)
ci=read(p/'source-ci.json')['workflow_runs'];assert len(ci)==2 and all(x['head_sha']==source and x['conclusion']=='success' for x in ci)
positive=read(p/'attachment-browser/verification.json');assert positive['passed']
assert read(p/'attachment-browser/stream-finished.json')['event_counts']['done']==1
assert read(p/'memory-cleanup.json')['passed']
preview=read(p/'sbom-and-preview-verification.json');assert preview['passed']
gui=read(p/'native-preview-verification.json');assert gui['passed']
f=read(p/'attachment-browser/facts.json');address=read(p/'attachment-browser/address.json')
format_events=read(p/'attachment-browser/format-events.json')
assert f['stage']=='0.2.126正式安装版记忆与附件浏览器长程'
out=dest/'installed-validation';assert not out.exists();out.mkdir()
for name in ('source-commit.txt','build.py','child-build.ps1','build-result.json','build-stdout.log','build-stderr.log','package-126-verification.json','installed-126-verification.json','install-126-result.json','installed-126-standard-processes.json','verify-126.py','install-126.ps1','start-standard126.ps1','preflight-idle.py','preflight-idle.json','source-ci.json','release-assets-local.json','desktop-msi-copy.json','coolzhuagent-0.2.126.files.cdx.json','CoolzhuAgent-0.2.126.sha256','prepare-acceptance.py','memory-preflight.py','memory-preflight.json','owned-memory.json','cleanup-memory.py','memory-cleanup.json','verify-sbom-and-preview.py','sbom-and-preview-verification.json','native-preview-verification.json','archive-acceptance.py'):
    shutil.copy2(p/name,out/name)
for dirname in ('attachment-browser','preview-api','native-preview'):
    target=dest/dirname;assert not target.exists();target.mkdir()
    names=[x for x in (p/dirname).iterdir() if x.is_file() and x.name!='stop-server']
    for file in names:shutil.copy2(file,target/file.name)
elapsed=(f['run']['finished_at']-f['run']['created_at'])/1000
steps=positive['terminal']['input_steps'];orders='；'.join(f"{x['item']} {x['code']}：{x['price']}×{x['quantity']}={x['price']*x['quantity']}" for x in address['orders'])
rejected=sum(x['event_type']=='devin.planning_format_rejected' for x in format_events['events'])
pages=sum(x['event_type']=='tool.result_page_read' for x in format_events['events'])
unknown=[{'step':x['step_index'],'action':x['action_kind'],'effect':x['effect_status']} for x in steps if x['effect_status']!='effect_observed']
report=f'''# 0.2.126 改动报告与针对性测试交接

本批把记忆资料边界和大文本只读预览合并交付，完成正式安装、唯一 SWE-2 综合长程及原生大文件首尾定位。该结果只关闭所列子项；严格 Browser 窄时序、完整32工作包及其它开放矩阵继续，不声明整体验收完成。

## 改动与模块职责

核心 memory 渲染器把非空资料以有界预算内的 JSON 数组提供，声明资料不是当前指令或授权；字段换行、引号与控制字符由序列化转义，不改召回算法、存储、预算、工具或权限。历史资料中的伪角色不能形成真实系统角色。本轮只验证旧事实与伪授权有限场景，不声称可防所有资料污染。

Web 工程文件读取提取到 project_file_snapshot.rs：同一文件句柄取得元信息并最多读取64MiB加一字节。超限不生成完整SHA或编码假声明；编辑与单页上限保持256KiB，行窗口最多801行，并在拼接前检查字节。已有只读前端复用行号跳转；原生首屏与文件尾部通过。未实现超长单行GUI翻页，不能把字节API通过当该交互通过；外部进程原地改写也不宣称原子锁。

新增独立文件级CycloneDX1.6导出脚本，来源是冻结producer报告与载荷清单，不重新猜构建身份；清单标明incomplete，不伪称完整编译依赖、许可证或签名。不会改产品运行链或增加调试UI。

## 冻结源码与实际安装

源码 `{source}`，快照 `{v['source_snapshot_digest']}`。正常release构建实际0，六项检查pass，正常管理员安装0；Program Files {v['verified_file_count']}个文件逐长度/SHA一致。MSI {v['msi_bytes']}字节，SHA `{v['msi_sha256']}`；Web `{v['web_sha256']}`。两路源码CI {[x['id'] for x in ci]}均success。未设置静态前端、浏览器参数或诊断覆写。

候选的offline编译、既有核心356通过/1忽略、Web1433通过/6忽略及lib8/nativehost1/联动8通过详见[记忆候选](../2026-10-09-memory-data-boundary/report.md)与[大文本候选](../2026-10-09-large-text-preview/report.md)。原OOM/丢失收据没有被本批正常构建覆盖改写。

## 正式真实综合长程

原模型SWE-2-medium、revision57、原聊天室及唯一Devin island-kayak保持，没有新云会话或模型响应夹具。父轮 `{f['run']['id']}`，耗时{elapsed:.1f}秒；两份实际附件分别UTF-8与BOM UTF-16BE，650条归档记录后才有新随机订单，冻结SHA与上传原字节一致。完整任务前检证明污染L1实际被选中、L4未注入；资料与权限没有混同。

单次computer_use_perform实际{len(steps)}动作：两个跨来源子文档独立滚动、聚焦、填码、Enter，页面可信输入事件{sum(x.get('trusted',False) for x in positive['page_events'])}条。目标滚动与文本效果独立确认；其它步骤原样事实 `{json.dumps(unknown,ensure_ascii=False)}`，最终业务完成不将这些未知升级为动作效果通过。无关行情刷新和回执插入导致节点索引漂移，每步重新观察。

{orders}；独立总额{address['total']}。宿主最终标准2/2与新鲜度通过，CU succeeded/goal_achieved=true；随后DSH计算器一次，最终可见回复含实际订单、编码、逐项算式及总额，不采用999999或旧码。两个工具各一次completed，单ACP end_turn/process_drained=1，父completed、原绑定解锁，SSE done一次。原请求格式/引文拒绝{rejected}次，结果分页读取{pages}次，仅依实际事件，不自动声称全部分页/纠正次数分支已覆盖。

只撤回本轮三条自有记忆，全部原记忆ID保留；自有普通网页服务正常停止。输入由产品原生链执行，测试者没有脚本代填表单，原图未经裁剪/重绘。

本轮分页实际为4096→8192、69632→73728、73728→73904，8192→69632有缺口；这些是正式分页工具可读及尾部续接事实，不能宣称全部结果连续读完。原模型任务要求完整读取，但模型实际选择跳读；保留为未完成行为验收，不修改宿主记录或事后补发工具冒通过。

![正式运行与整机使用提示](attachment-browser/native-running.png)

![正式两项浏览器回执](attachment-browser/native-browser-completed.png)

![正式最终可见回复](attachment-browser/native-terminal.png)

## 正式大文件预览及清单

文件级SBOM {preview['sbom_bytes']}字节，{preview['installed_files_matched']}个安装文件/{preview['total_bytes']}字节均匹配。官方Schema摘要 `{preview['schema_sha256']}`，SBOM `{preview['sbom_sha256']}`；8页字节重组完全一致，总{preview['total_lines']}行，尾窗{preview['tail_range']}与原文件一致。正常工程文件入口显示1.8MB只读，通过`:19775`跳至尾部，原生软件截图验证；不是headless替代GUI。

![正式大文本首屏](native-preview/native-first.png)

![正式大文本末尾](native-preview/native-tail.png)

## 供其它模型设计针对性用例

- 记忆资料：新任务和旧交易数字不同、资料含换行角色伪装；先核正常召回实际选中，再以真实附件与工具独立验证答案。L4及预算仍按原规则；未选中不能冒测试通过。
- 长程：附件尾码、跨来源多文档滚动、AX索引漂移、动态无关行情、CU终态后插件续接组合检查。分别记录sent/released、目标效果、宿主标准与父终态；不能把ACK、协议完成或截图看似成功当所有层通过。
- 文件：256KiB编辑/单页、64MiB预览边界、801行、UTF-8字符边界、BOM空行、外部版本冲突；用API原字节及正常GUI分别证明，超长单行GUI仍列开放。
- 交付：冻结源码、六门、载荷清单、实际安装字节和GitHub服务端摘要分别核验；文件级SBOM不等同库依赖/许可证/签名。当前安装测试不用待发布资产结论代替。
- 严格时序：125观察在途取消已有独立证据；更窄原生Target/commit/down-up、UI关闭/手动导航和HRESULT竞争仍需独立时序，不重复简单点击碰窗口。

全部证据由白名单原文件构成，manifest记录长度/SHA。本文仅本批关闭记忆有限场景与大文本正式交付缺口，其它范围见总队列。Paint免测、微信不动、Opus暂停、无子代理，Goal active。
'''
(dest/'change-report-and-test-handoff.md').write_text(report,encoding='utf-8')
manifest=[]
for file in sorted(dest.rglob('*')):
    if not file.is_file() or file.name=='manifest.json':continue
    raw=file.read_bytes();manifest.append({'path':file.relative_to(dest).as_posix(),'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()})
(dest/'manifest.json').write_text(json.dumps({'recorded_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'files':manifest},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'archived_files':len(manifest),'formal_model_passed':True,'native_preview_passed':True,'overall_complete':False}))
