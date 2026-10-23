"""独立核对6.1定点用例、产物及构建后更新验收账。"""
import datetime,hashlib,json,pathlib,re,shutil,subprocess,sys
ROOT=pathlib.Path(r'D:\coolzhuagent');R=ROOT/'tmp/goal-32wbs-acceptance-20261013'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
receipts=[json.loads((R/f'controller-quota-key-{a}-20261016.json').read_text('utf-8')) for a in ['test','build']]
for r in receipts:
 assert r['state']=='completed' and r['exit_code']==0 and r['source_unchanged']
 assert sha(pathlib.Path(r['log']))==r['log_sha256']
 assert all(sha(ROOT/n)==h for n,h in r['source_after'].items())
assert receipts[0]['test_results']==[['1','0','0']]
log=pathlib.Path(receipts[0]['log']).read_text('utf-8');match=re.search(r'DIAG_QUOTA_KEY first_pid=(\d+) second_pid=(\d+).* root=(.+)',log)
assert match and match[1]!=match[2]
fixture=pathlib.Path(match[3].strip());assert fixture.resolve().is_relative_to(R.resolve())
assert not (fixture/'coolzhu-web-console.jsonl').exists()
assert (fixture/'coolzhu-tauri-shell.jsonl').stat().st_size==3000
assert (fixture/'tool-audit.jsonl').read_bytes()==b'A'*100
export=fixture/'redacted-bundle/logs/sample.txt';text=export.read_text('utf-8')
assert 'FIXTURE_SECRET_6_1_ONLY' not in text and '[REDACTED]' in text
manifest=fixture/'redacted-bundle/manifest.json'
assert (fixture/'redacted-bundle/manifest.sha256').read_text('utf-8').startswith(sha(manifest))
stamp=datetime.datetime.now(datetime.timezone.utc).isoformat()
conclusion='诊断清单登记去重、1MiB/4096成员上限及有界读取已修；两个真实产品写进程串行退出/接续后，旧产品日志回收，新日志与伪造登记的权威文件保留。100次重复登记零增长，超限拒读拒登记，真实导出清除假密钥。定点1过0败0忽略及实际离线build通过。'
review=dict(item='6.1',atUtc=stamp,decision='key_functional_pass',conclusion=conclusion,
 firstPid=int(match[1]),secondPid=int(match[2]),fixture=str(fixture),
 scope='自动接管仅限源码明确声明的三个产品结构化日志族；其他外部登记只能计量，不能扩权删除。旧超限清单拒绝读写，不擅自截断丢失账目。正式安装包并入最终交付批次。')
name='controller-quota-key-review-20261016.json';(R/name).write_text(json.dumps(review,ensure_ascii=False,indent=2),encoding='utf-8')
p=R/'acceptance-checklist.json';data=json.loads(p.read_text('utf-8'));item=next(i for i in data['items'] if i['id']=='6.1')
item.update(status='accepted',updatedAtUtc=stamp,latestEvidence=conclusion)
item['keyAcceptance'].update(status='accepted',conclusion=conclusion,next='关键功能通过，不重复测试。')
if name not in item['keyAcceptance']['receipts']:item['keyAcceptance']['receipts'].append(name)
data['currentReview']['activePlan']['current']=[i for i in data['currentReview']['activePlan']['current'] if i!='6.1']
p.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
archive=ROOT/'docs/testing/2026-10-16-diagnostics-quota-evidence';archive.mkdir(exist_ok=True)
sources=[R/name,R/'controller-quota-key-20261016.py',R/'controller-quota-key-review-20261016.py']
sources += [R/f'controller-quota-key-{a}-20261016.{e}' for a in ['test','build'] for e in ['json','log']]
sources += [ROOT/n for n in receipts[0]['source_after']]
sources += list((fixture/'redacted-bundle').rglob('*'))+[fixture/'coolzhu-web-console.jsonl.child-result.txt',fixture/'coolzhu-tauri-shell.jsonl.child-result.txt']
entries=[]
for source in sources:
 if not source.is_file():continue
 relative=pathlib.Path('export')/source.relative_to(fixture/'redacted-bundle') if source.is_relative_to(fixture/'redacted-bundle') else pathlib.Path(source.name)
 target=archive/relative;target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(source,target);assert sha(source)==sha(target)
 entries.append(dict(source=str(source),file=relative.as_posix(),sha256=sha(target),bytes=target.stat().st_size))
(archive/'manifest.json').write_text(json.dumps(entries,ensure_ascii=False,indent=2),encoding='utf-8')
with (ROOT/'docs/work-logs/2026-10-16-acceptance-takeover.md').open('a',encoding='utf-8') as f:f.write('\n### 6.1 诊断配额关键功能关闭\n\n'+conclusion+'\n\n'+review['scope']+'\n')
subprocess.run([sys.executable,str(R/'controller-key-ledger-render-20261016.py')],check=True)
print(json.dumps(review,ensure_ascii=False))
