"""运行真实双进程租约关键用例与实际构建，不复跑已验收矩阵。"""
import datetime, hashlib, json, os, pathlib, re, shutil, subprocess, sys, time
ROOT=pathlib.Path(r'D:\coolzhuagent'); R=ROOT/'tmp/goal-32wbs-acceptance-20261013'
action=sys.argv[1]; assert action in ('test','build')
names=['modules/gui-web/packages/web-console/src/input_safety_store.rs',
 'modules/gui-web/packages/web-console/src/input_lease_process_acceptance_tests.rs']
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
before={x:sha(ROOT/x) for x in names}
scratch=R/'input-lease-20261016';scratch.mkdir(exist_ok=True)
env=os.environ.copy();env.update(TEMP=str(scratch),TMP=str(scratch),COOLZHU_INPUT_SAFETY_STATE_ROOT=str(scratch/'global-input-safety'),
 COOLZHU_LOG_DIR=str(scratch/'logs'),COOLZHU_RUNTIME_DIR=str(scratch/'runtime'),
 COOLZHU_WEB_SESSION_DB=str(scratch/'sessions.sqlite3'),COOLZHU_WEB_SESSION_STORE=str(scratch/'sessions.json'))
cmd=[shutil.which('cargo'),action,'-p','coolzhu-web-console','--offline','-j','1','--target-dir',str(R/'cargo-target')]
if action=='test':cmd+=['--bin','coolzhu-web-console','real_process_lease_competition_reclaims_without_clearing_unknown','--','--test-threads=1','--nocapture']
suffix=sys.argv[2] if len(sys.argv)>2 else '';assert re.fullmatch(r'[a-z0-9-]*',suffix)
log=R/f'controller-input-lease-{action}{suffix}-20261016.log';receipt=log.with_suffix('.json')
record={'state':'running','action':action,'command':cmd,'source_before':before,'startedAtUtc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'log':str(log)}
receipt.write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8');began=time.monotonic()
with log.open('wb') as output:
 p=subprocess.Popen(cmd,cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
 code=p.wait()
after={x:sha(ROOT/x) for x in names};text=log.read_text('utf-8',errors='replace')
record.update(state='completed' if code==0 else 'failed',exit_code=code,source_after=after,source_unchanged=before==after,
 elapsedSeconds=round(time.monotonic()-began,3),log_sha256=sha(log),test_results=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',text),
 finishedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat())
receipt.write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({k:v for k,v in record.items() if not k.startswith('source_')},ensure_ascii=False))
print('\n'.join(x[:1000] for x in text.splitlines()[-18:]));sys.exit(code)
