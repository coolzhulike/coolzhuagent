"""真实双进程用例和实际构建均通过才关闭1.3。"""
import datetime,hashlib,json,pathlib,re,shutil,subprocess,sys
ROOT=pathlib.Path(r'D:\coolzhuagent');R=ROOT/'tmp/goal-32wbs-acceptance-20261013'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
test=json.loads((R/'controller-input-lease-test-fixed-20261016.json').read_text('utf-8'))
build=json.loads((R/'controller-key-batch-build-20261016.json').read_text('utf-8'))
for receipt in (test,build):
 assert receipt['state']=='completed' and receipt['exit_code']==0 and receipt['source_unchanged']
 assert sha(pathlib.Path(receipt['log']))==receipt['log_sha256']
 assert all(sha(ROOT/n)==h for n,h in receipt['source_after'].items())
assert test['test_results']==[['1','0','0']]
match=re.search(r'INPUT_LEASE_ACCEPTANCE parent_pid=(\d+) child_pid=(\d+) old_epoch=(\d+) new_epoch=(\d+) contender_denied=true old_unknown_preserved=true intake_closed=true',pathlib.Path(test['log']).read_text('utf-8'))
assert match and int(match[4])>int(match[3]) and match[1]!=match[2]
stamp=datetime.datetime.now(datetime.timezone.utc).isoformat()
conclusion='真实双进程竞争同一输入安全库：第二持有者被拒，杀死本夹具持有者后epoch上升并安全接管；旧unknown与恢复记录保留、输入入口仍关闭。修正Windows退出进程句柄仍可查身份导致的误判，未知/拒绝访问仍保守不回收。新增定点1过0败0忽略，实际离线build通过。'
review=dict(item='1.3',atUtc=stamp,decision='key_functional_pass',conclusion=conclusion,parentPid=int(match[1]),childPid=int(match[2]),oldEpoch=int(match[3]),newEpoch=int(match[4]),testReceipt='controller-input-lease-test-fixed-20261016.json',buildReceipt='controller-key-batch-build-20261016.json',priorFailure='controller-input-lease-test-20261016.json',boundary='真实测试子进程与隔离输入安全库；没有对用户桌面发送输入，未修改正式输入安全库。')
name='controller-input-lease-review-20261016.json';(R/name).write_text(json.dumps(review,ensure_ascii=False,indent=2),encoding='utf-8')
p=R/'acceptance-checklist.json';data=json.loads(p.read_text('utf-8'));item=next(i for i in data['items'] if i['id']=='1.3')
item.update(status='accepted',updatedAtUtc=stamp,latestEvidence=conclusion)
item['keyAcceptance'].update(status='accepted',conclusion=conclusion,next='关键功能通过，不重复测试。')
if name not in item['keyAcceptance']['receipts']:item['keyAcceptance']['receipts'].append(name)
data['currentReview']['activePlan']['current']=[i for i in data['currentReview']['activePlan']['current'] if i!='1.3']
data['currentReview']['activeBuildReceipt']=review['buildReceipt'];p.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
archive=ROOT/'docs/testing/2026-10-16-input-lease-evidence';archive.mkdir(exist_ok=True)
names=[name,'controller-input-lease-review-20261016.py','controller-input-lease-checks-20261016.py','controller-key-build-batch-20261016.py']
names += [f'controller-input-lease-test{s}-20261016.{ext}' for s in ('','-fixed') for ext in ('json','log')]
names += [f'controller-key-batch-{action}-20261016.{ext}' for action in ('test','build') for ext in ('json','log')]
manifest=[]
for source in [R/n for n in names]+[ROOT/n for n in test['source_after']]:
 target=archive/source.name;shutil.copy2(source,target);assert sha(source)==sha(target)
 manifest.append(dict(source=str(source),file=target.name,sha256=sha(target),bytes=target.stat().st_size))
(archive/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
with (ROOT/'docs/work-logs/2026-10-16-acceptance-takeover.md').open('a',encoding='utf-8') as f:f.write('\n### 1.3 输入租约关键功能关闭\n\n'+conclusion+'\n\n首次真实进程退出后仍EpochConflict，失败回执完整归档。修复复用既有capture_live_process_identity，同一进程句柄上的存活探测替代仅比身份，未放宽unknown和访问错误的保护。\n')
subprocess.run([sys.executable,str(R/'controller-key-ledger-render-20261016.py')],check=True)
print(json.dumps(review,ensure_ascii=False))
