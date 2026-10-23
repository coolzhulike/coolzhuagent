"""只验证6.1新增关键路径，串行执行定点用例及实际离线构建。"""
import datetime,hashlib,json,os,pathlib,re,shutil,subprocess,time
ROOT=pathlib.Path(r'D:\coolzhuagent');R=ROOT/'tmp/goal-32wbs-acceptance-20261013'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
names=['modules/diagnostics/packages/diagnostics/src/quota.rs','modules/diagnostics/packages/diagnostics/src/quota_key_acceptance.rs']
scratch=R/'quota-key-20261016';scratch.mkdir(exist_ok=True)
env=os.environ.copy();env.update(TEMP=str(scratch),TMP=str(scratch),QUOTA_KEY_EVIDENCE_ROOT=str(scratch),COOLZHU_LOG_DIR=str(scratch/'logs'))
for action in ['test','build']:
 before={n:sha(ROOT/n) for n in names}
 cmd=[shutil.which('cargo'),action,'-p','coolzhu-diagnostics','--offline','-j','1','--target-dir',str(R/'cargo-target')]
 if action=='test':cmd+=['production_restart_keeps_registry_bounded_and_authoritative_files_safe','--','--nocapture','--test-threads=1']
 log=R/f'controller-quota-key-{action}-20261016.log';receipt=log.with_suffix('.json')
 record=dict(state='running',command=cmd,source_before=before,startedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),log=str(log))
 receipt.write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8');start=time.monotonic()
 with log.open('wb') as out:code=subprocess.call(cmd,cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
 after={n:sha(ROOT/n) for n in names};text=log.read_text('utf-8',errors='replace')
 record.update(state='completed' if code==0 else 'failed',exit_code=code,source_after=after,source_unchanged=before==after,
  elapsedSeconds=round(time.monotonic()-start,3),log_sha256=sha(log),finishedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
  test_results=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',text))
 receipt.write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps(record,ensure_ascii=False),flush=True)
 if code:print(text[-9000:]);raise SystemExit(code)
