"""归档实际安装、截图及失败验收，不将有污染的负例改写为通过。"""
from pathlib import Path
from PIL import Image
import hashlib,json,shutil
p=Path(__file__).resolve().parent;root=p.parents[1]
def read(file):return json.loads(file.read_text(encoding='utf-8-sig'))
def sha(file):
 with file.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
installed=read(p/'installed-127-verification.json');build=read(p/'build-result.json')
assert installed['version']=='0.2.127' and build['actual_exit_code']==0
assert read(p/'install-127-result.json')['exit_code']==0
assert all(g['result']=='pass' for g in installed['release_gates'])
images={}
for name,file in [('before',root/'docs/testing/2026-10-10-upload-layout/formal-before.jpg'),('formal127',p/'native-upload-formal127.jpg')]:
 im=Image.open(file).convert('RGB');assert im.size==(1443,897)
 points=[(x,y) for y in range(823,846) for x in range(124,150) if sum(im.getpixel((x,y)))>=520]
 assert points
 bounds=[min(x for x,y in points),min(y for x,y in points),max(x for x,y in points),max(y for x,y in points)]
 images[name]={'bounds':bounds,'center_x':(bounds[0]+bounds[2])/2,'sha256':sha(file)}
assert images['formal127']['center_x']>images['before']['center_x']
images['note']='原始JPEG金色主笔画近似测量；不是精确零误差或裁剪后的效果图。'
(p/'upload-formal-measurement.json').write_text(json.dumps(images,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
a=root/'tmp/2026-10-10-browser-address-draft-installed127'
first=read(a/'draft-initial.json');held=read(a/'draft-held.json');back=read(a/'back-settled.json');address=read(a/'address.json')['base']
assert first['address'].endswith(address+'/target.html') and held['address'].endswith(address+'/target.html')
assert address+'/pulse.html?tick=' in back['address']
events=[json.loads(x) for x in (a/'events.jsonl').read_text().splitlines()]
updates=[x for x in events if x['kind']=='same-document-url' and first['captured_ms']<=x['received_ms']<=held['captured_ms']]
target=[x for x in events if x['kind']=='request' and x['path']=='/target.html']
assert held['captured_ms']-first['captured_ms']>=8000 and len(updates)>=16 and len(target)==1
address_result={'passed':True,'version':'0.2.127','draft_hold_ms':held['captured_ms']-first['captured_ms'],'independent_url_updates_during_hold':len(updates),'target_http_requests':len(target),'input_method':'原生UIA编辑控件set_value及打开按钮；不是脚本修改DOM','scope':'地址草稿保持、打开按钮导航及后退地址同步；本轮未复验Enter','model_calls':0,'new_cloud_sessions':0}
(a/'verification.json').write_text(json.dumps(address_result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
d=root/'tmp/2026-10-10-browser-document-change-installed127';inspection=read(d/'inspection.json')
terminal=json.loads(inspection['cu_runs'][0]['terminal_result_json'])
assert terminal['error']['code']=='stale_observation' and terminal['stage']=='execution'
assert terminal['error']['retryable'] is False and terminal['error']['retry_owner']=='none'
assert len(inspection['steps'])==1 and inspection['steps'][0]['input_delivery']=='not_sent'
assert len([x for x in inspection['events'] if x['event_type']=='browser.observation_requested'])==2
trusted=[x for x in inspection['page_events'] if x.get('trusted') and x['kind'] in ('keydown','pointerdown','pointerup','click','input')]
assert len(trusted)==1 and trusted[0]['kind']=='keydown' and trusted[0]['target']=='BODY'
negative={'passed':False,'formal_version':'0.2.127','guard_receipt':'旧click/not_sent；stale_observation原因保留；两次观察','failure':'严格零可信输入断言失败：新文档BODY出现一次来源不明keydown，页面未记录键值，不能归因或删除','unattributed_event':trusted[0],'target_pointer_or_click_events':0,'model':'swe-2-medium','binding':'island-kayak','configuration_revision':57,'limits':'不是原生commit/down-up/UI关闭/HRESULT竞争验收；不得将本轮整体负例计为通过'}
(d/'verification-failure.json').write_text(json.dumps(negative,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
out=root/'docs/testing/release-0.2.127/installed-validation';assert not out.exists();out.mkdir(parents=True)
files=['source-commit.txt','build.py','child-build.ps1','build-result.json','build-stdout.log','build-stderr.log','package-127-verification.json','installed-127-verification.json','install-127-result.json','installed-127-standard-processes.json','verify-127.py','install-127.ps1','start-standard127.ps1','stop-standard126.ps1','stopped-formal126.json','preflight-idle.py','preflight-idle.json','source-ci.json','release-assets-local.json','desktop-msi-copy.json','coolzhuagent-0.2.127.files.cdx.json','CoolzhuAgent-0.2.127.sha256','native-upload-formal127.jpg','native-upload-final127.jpg','upload-formal-measurement.json','archive-formal.py']
for name in files:shutil.copy2(p/name,out/name)
for src,name in [(a,'browser-address-draft'),(d,'browser-document-change')]:
 dest=out/name;dest.mkdir()
 for f in src.iterdir():
  if f.is_file() and f.suffix in ('.py','.json','.jsonl','.jpg'):shutil.copy2(f,dest/f.name)
report=f'''# 0.2.127正式安装复验与针对性测试交接

上传按钮的隐藏“附件”span在字号为0时仍留下6px flex空隙，导致可见箭头左偏约3CSS像素。删除该span并标记纯图标，保留原SVG、玉石外框、title/aria-label和上传处理函数。本次不是重新裁剪图案；纠正之前仅凭SVG对称就判断控件已居中的结论。

正常release构建实际返回0，用时{build['elapsed_seconds']}秒；安装实际0；六项发布门通过，Program Files的{installed['verified_file_count']}文件逐SHA匹配。正式1443×897完整原生实拍通过，主笔画近似中心从{images['before']['center_x']}移至{images['formal127']['center_x']}，JPEG抗锯齿不用于声称精确零误差。

![正式版上传按钮](installed-validation/native-upload-formal127.jpg)

同批交付浏览器地址草稿保护与CU失败原因保留。正式地址草稿在{address_result['draft_hold_ms']}ms、{len(updates)}次真实网页地址更新中保持；正常打开按钮使目标页只收到一次GET，后退恢复网页地址。原生UIA编辑控件输入，不是DOM脚本；Enter本轮未复验，前候选Enter证据仍独立保留。

原SWE-2-medium/revision57、唯一island-kayak的一次真实文档切换调用（可见#812/#813、38.8秒）拒绝旧click，input_delivery=not_sent，终态execution/stale_observation、retry_owner=none，两次观察、无重新规划；父failed为预期负例，end_turn/drained/解锁。**整体负例未通过**：严格零可信输入断言发现新文档BODY一次来源不明keydown；键值未记录，不能确认来源，也不删除或宽松化原断言。两个目标没有指针/点击事件，不能据此宣称所有输入为零。

冻结源码：`{installed['source_commit']}`，快照`{installed['source_snapshot_digest']}`。两路源码CI38056523918/38056520650均success。MSI SHA256：`{installed['msi_sha256']}`；文件级CycloneDX1.6清单1159文件，completeness=incomplete，不代表完整库依赖或签名。桌面dist复制摘要匹配，未修改自动更新索引。

后续测试重点：上传键盘/鼠标入口与不同窗口宽度；地址编辑遇到同文档与完整导航的清理边界、Enter提交；文档负例增加键值/时间/目标取证以区分外来输入；严格原生nativeTarget/commit/down-up/关闭/HRESULT竞争仍开放。结果分页完整连续读取、超长单行GUI翻页、完整记忆/多模态/GC及其余32WBS仍开放，Goal active，Paint免测、微信不动、Opus暂停、无子代理。
'''
(out.parent/'change-report-and-test-handoff.md').write_text(report,encoding='utf-8')
manifest=[{'path':f.relative_to(out).as_posix(),'size':f.stat().st_size,'sha256':sha(f)} for f in sorted(out.rglob('*')) if f.is_file()]
(out/'evidence-manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
note='2026-10-10正式127：上传隐藏span产生的6px空隙已移除，原SVG/玉石皮肤保持，正式原生实拍居中；正常release构建301.68秒/actual0、安装0、六门pass、1159安装文件逐SHA、a3ec462两路CI success。正式地址草稿保持及打开/后退通过；原SWE-2/rev57/唯一island文档旧click被stale_observation/not_sent拒绝并保留终态原因，但一次新文档BODY来源不明keydown使严格零输入负例断言失败，本轮不计整体通过。原失败、模型回复及实拍均保留；严格原生竞态和32WBS继续，Goal active。[127正式交接与原图](../../testing/release-0.2.127/change-report-and-test-handoff.md)。'
for name in ('acceptance-summary-2026-10-08.md','current-acceptance-queue.md','implementation-status-and-change-inventory.md','wbs-implementation-audit-2026-10-07.md'):
 file=root/'docs/analysis/2026-09-21-integration-review'/name;text=file.read_text(encoding='utf-8');first,rest=text.split('\n',1);file.write_text(first+'\n\n'+note+'\n'+rest,encoding='utf-8')
with (root/'docs/work-logs/2026-10-10-upload-layout.md').open('a',encoding='utf-8') as f:f.write('\n\n'+note+'\n')
with (root/'.gitattributes').open('a',encoding='utf-8') as f:f.write('\n/docs/testing/release-0.2.127/** -text whitespace=-blank-at-eof,-blank-at-eol,-space-before-tab,cr-at-eol\n')
print(json.dumps({'formal_upload_passed':True,'address':address_result,'negative_passed':False,'archived_files':len(manifest)},ensure_ascii=False))
