"""仅在真实接口用例与实际构建均成功后关闭0.2，保留所有失败收据。"""
import collections, datetime, hashlib, json, pathlib, re, shutil, subprocess, sys
ROOT=pathlib.Path(r'D:\coolzhuagent')
R=ROOT/'tmp/goal-32wbs-acceptance-20261013'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
test=json.loads((R/'controller-long-result-test-corrected-20261016.json').read_text('utf-8'))
build=json.loads((R/'controller-long-result-build-20261016.json').read_text('utf-8'))
for receipt in [test,build]:
 assert receipt['state']=='completed' and receipt['exit_code']==0 and receipt['source_unchanged']
 assert sha(pathlib.Path(receipt['log']))==receipt['log_sha256']
 assert all(sha(ROOT/name)==digest for name,digest in receipt['source_after'].items())
assert test['test_results']==[['1','0','0']]
log=pathlib.Path(test['log']).read_text('utf-8')
match=re.search(r'LONG_RESULT_ACCEPTANCE pages=(\d+) bytes=(\d+) sha256=([a-f0-9]{64}) tool_executions=1 cancelled_read_denied=true',log)
assert match and int(match[2])>96000
stamp=datetime.datetime.now(datetime.timezone.utc).isoformat()
review=dict(atUtc=stamp,item='0.2',decision='key_functional_pass',
 pages=int(match[1]),bytes=int(match[2]),resultSha256=match[3],toolExecutions=1,
 cancelledReadDenied=True,testReceipt='controller-long-result-test-corrected-20261016.json',
 buildReceipt='controller-long-result-build-20261016.json',
 boundary='真实本地MCP HTTP桥与read_file执行器；没有调用远端模型。旧plugin外壳30秒超时未据此宣称修复。',
 priorFailures=['controller-long-result-test-20261016.json','controller-long-result-test-retry1-20261016.json'])
(R/'controller-long-result-review-20261016.json').write_text(json.dumps(review,ensure_ascii=False,indent=2),encoding='utf-8')
path=R/'acceptance-checklist.json';data=json.loads(path.read_text('utf-8'))
item=next(x for x in data['items'] if x['id']=='0.2')
conclusion=f"真实MCP HTTP入口单次read_file，{review['bytes']}字节回执分{review['pages']}页续读、全文SHA一致、工具台账仅一次执行；取消后续读被拒。新增用例1过0败0忽略，实际离线构建exit0。"
item.update(status='accepted',updatedAtUtc=stamp,latestEvidence=conclusion)
item['keyAcceptance'].update(status='accepted',conclusion=conclusion,next='关键功能通过，不重复测试。',
 deferred='覆盖当前受支持MCP桥read_file长回执；其他工具组合与旧插件进程30秒外壳限制保留范围说明，不外推为全部入口通过。')
item['keyAcceptance']['receipts'].append('controller-long-result-review-20261016.json')
data['currentReview']['activePlan']['current']=[x for x in data['currentReview']['activePlan']['current'] if x!='0.2']
data['currentReview']['longResultReview']='controller-long-result-review-20261016.json'
data['currentReview']['activeBuildReceipt']='controller-long-result-build-20261016.json'
path.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
archive=ROOT/'docs/testing/2026-10-16-long-result-evidence';archive.mkdir(exist_ok=True)
names=['controller-long-result-review-20261016.json','controller-long-result-checks-20261016.py','controller-long-result-review-20261016.py']
for suffix in ['test','test-retry1','test-corrected','build']:
 names += [f'controller-long-result-{suffix}-20261016.json',f'controller-long-result-{suffix}-20261016.log']
files=[R/name for name in names]+[ROOT/name for name in test['source_after']]
manifest=[]
for source in files:
 target=archive/source.name;shutil.copy2(source,target)
 assert sha(source)==sha(target)
 manifest.append(dict(source=str(source),file=target.name,sha256=sha(target),bytes=target.stat().st_size))
(archive/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
with (ROOT/'docs/work-logs/2026-10-16-acceptance-takeover.md').open('a',encoding='utf-8') as f:
 f.write('\n### 0.2 关键长回执接口关闭\n\n'+conclusion+'\n\n首次续读超时，原因未独立证实；第二次全文已恢复，新增测试误把字符串output当对象，修正测试解包后通过。旧失败原样归档，不改写历史。没有扩大全量测试或创建执行/监督会话。\n')
subprocess.run([sys.executable,str(R/'controller-key-ledger-render-20261016.py')],check=True)
print(json.dumps(review,ensure_ascii=False))
