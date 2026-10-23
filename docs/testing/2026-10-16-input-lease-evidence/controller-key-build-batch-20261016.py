"""串行编译新增DSH精简用例，再实际构建；每阶段独立保留回执。"""
import datetime, hashlib, json, os, pathlib, re, shutil, subprocess, sys, time
ROOT=pathlib.Path(r'D:\coolzhuagent'); R=ROOT/'tmp/goal-32wbs-acceptance-20261013'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
names=['modules/gui-web/packages/web-console/src/input_safety_store.rs','modules/gui-web/packages/web-console/src/input_lease_process_acceptance_tests.rs','modules/gui-web/packages/web-console/src/dsh_web_tests.rs']
env=os.environ.copy(); scratch=R/'key-build-batch';scratch.mkdir(exist_ok=True)
env.update(TEMP=str(scratch),TMP=str(scratch),COOLZHU_LOG_DIR=str(scratch/'logs'))
for action in ['test','build']:
 before={n:sha(ROOT/n) for n in names}
 cmd=[shutil.which('cargo'),action,'-p','coolzhu-web-console','--offline','-j','1','--target-dir',str(R/'cargo-target')]
 if action=='test':cmd+=['--no-run','--bin','coolzhu-web-console']
 log=R/f'controller-key-batch-{action}-20261016.log';receipt=log.with_suffix('.json')
 record=dict(state='running',command=cmd,source_before=before,startedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat(),log=str(log))
 receipt.write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8');started=time.monotonic()
 with log.open('wb') as out:
  code=subprocess.call(cmd,cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
 after={n:sha(ROOT/n) for n in names}
 record.update(state='completed' if code==0 else 'failed',exit_code=code,source_after=after,source_unchanged=before==after,elapsedSeconds=round(time.monotonic()-started,3),log_sha256=sha(log),finishedAtUtc=datetime.datetime.now(datetime.timezone.utc).isoformat())
 if action=='test' and code==0:
  match=re.search(r'Executable unittests src[\\/]main.rs \((.+?\.exe)\)',log.read_text('utf-8',errors='replace'))
  assert match
  exe=ROOT/match[1];record.update(executable=str(exe),executableSha256=sha(exe))
 receipt.write_text(json.dumps(record,ensure_ascii=False,indent=2),encoding='utf-8')
 print(json.dumps(record,ensure_ascii=False),flush=True)
 if code:raise SystemExit(code)
