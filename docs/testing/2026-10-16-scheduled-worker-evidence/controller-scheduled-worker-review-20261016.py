"""核真实调度与worker数据库事实，分别关闭4.5和2.5关键验收。"""
import datetime,hashlib,json,pathlib,shutil,sqlite3,subprocess,sys
ROOT=pathlib.Path(r'D:\coolzhuagent');R=ROOT/'tmp/goal-32wbs-acceptance-20261013';W=R/'scheduled-worker-key-retry-20261016'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
name='controller-scheduled-worker-key-retry-20261016.json';fact=json.loads((R/name).read_text('utf-8'))
assert fact['state']=='completed' and len(fact['steps'])==2 and all(x['state']=='passed' for x in fact['steps'])
assert len(fact['model_requests'])==2 and all(x['exited'] for x in fact['processes']) and not fact['productionTouched']
c=sqlite3.connect('file:'+str(W/'sessions.sqlite3').replace('\\','/')+'?mode=ro',uri=True);c.row_factory=sqlite3.Row
rows=[dict(x) for x in c.execute('SELECT id,state,run_id,result,error FROM execution_index ORDER BY started_ms')]
assert len(rows)==2 and rows[0]['state']=='completed' and 'ISOLATED_SCHEDULE_WORKER_OK' in rows[0]['result']
assert rows[1]['state']=='interrupted' and rows[1]['result'] is None
assert all(row['id']==step['evidence']['executionId'] and row['run_id']==step['evidence']['runId'] for row,step in zip(rows,fact['steps']))
late=c.execute('SELECT count(*) FROM chat_room_messages WHERE content LIKE ?',('%LATE_OUTPUT_MUST_NOT_COMMIT%',)).fetchone()[0];assert late==0
c.close();stamp=datetime.datetime.now(datetime.timezone.utc).isoformat()
conclusions={'4.5':'复用已通过Goal失败→修复→复验链；新增隔离产品HTTP一次性调度：保存后重启，模型实际请求一次并完成，终态重启后同run_id查询且run-due不重放。',
 '2.5':'隔离真实共享调度worker已经进入模型请求；通过执行API取消，迟到回包未提交到执行结果或聊天室，状态interrupted；重启后同execution_id/run_id且不自动重放。'}
review=dict(atUtc=stamp,decision='key_functional_pass',items=conclusions,receipt=name,receiptSha256=sha(R/name),executions=rows,lateMessageRows=late,boundary=fact['boundary'],modelRequests=fact['model_requests'],processes=fact['processes'],priorFailure='controller-scheduled-worker-key-20261016.json',priorFailureInterpretation='首次70秒等待在真实上下文装配中结束；本次有界240秒等待完成。未改变生产超时或调度语义。')
rn='controller-scheduled-worker-review-20261016.json';(R/rn).write_text(json.dumps(review,ensure_ascii=False,indent=2),encoding='utf-8')
p=R/'acceptance-checklist.json';data=json.loads(p.read_text('utf-8'))
for ident,conclusion in conclusions.items():
 item=next(x for x in data['items'] if x['id']==ident);item.update(status='accepted',updatedAtUtc=stamp,latestEvidence=conclusion)
 item['keyAcceptance'].update(status='accepted',conclusion=conclusion,next='关键功能通过，不重复测试。')
 if rn not in item['keyAcceptance']['receipts']:item['keyAcceptance']['receipts'].append(rn)
data['currentReview']['activePlan']['current']=[i for i in data['currentReview']['activePlan']['current'] if i not in conclusions]
p.write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
archive=ROOT/'docs/testing/2026-10-16-scheduled-worker-evidence';archive.mkdir(exist_ok=True)
files=[R/rn,R/name,R/'controller-scheduled-worker-review-20261016.py',R/'controller-scheduled-worker-key-retry-20261016.py',R/'controller-scheduled-worker-key-20261016.py',R/'controller-scheduled-worker-key-20261016.json',W/'sessions.sqlite3',W/'.coolzhu/scheduled-jobs.sqlite3',W/'coolzhu.toml']+list(W.glob('server-*.log'))+list((W/'logs').glob('*.jsonl'))
manifest=[]
for source in files:
 target=archive/source.name;shutil.copy2(source,target);assert sha(source)==sha(target)
 manifest.append(dict(source=str(source),file=target.name,sha256=sha(target),bytes=target.stat().st_size))
(archive/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
with (ROOT/'docs/work-logs/2026-10-16-acceptance-takeover.md').open('a',encoding='utf-8') as f:
 f.write('\n### 4.5 与2.5 调度及共享执行关键功能关闭\n\n'+ '\n\n'.join(conclusions.values())+'\n\n本机模型协议夹具明确返回固定结果；服务/HTTP/调度/取消/SQLite均为真实产品路径，不声称外部模型自主规划。四个隔离服务进程均退出，没有创建主会话监督任务，也没有发送测试通知。\n')
subprocess.run([sys.executable,str(R/'controller-key-ledger-render-20261016.py')],check=True)
print(json.dumps({'accepted':list(conclusions),'review':rn},ensure_ascii=False))
