"""仅运行本轮新增的 hook 服务矩阵及所需实际构建，产物集中在验收根。"""
import datetime, hashlib, json, os, pathlib, re, shutil, subprocess, sys, time

repo = pathlib.Path(r'D:\coolzhuagent')
out = repo / 'tmp/goal-32wbs-acceptance-20261013'
scratch = out / 'hook-budget-20261016'
scratch.mkdir(exist_ok=True)
action = sys.argv[1]
assert action in ('test', 'build', 'core')
names = ['modules/gui-web/packages/web-console/src/tool_dispatch_service.rs',
         'modules/gui-web/packages/web-console/src/dispatch_hook_budget_tests.rs',
         'modules/gui-web/packages/web-console/src/main.rs',
         'modules/core-runtime/packages/core-runtime/src/hooks.rs']
hashes = lambda: {name: hashlib.sha256((repo/name).read_bytes()).hexdigest() for name in names}
before = hashes()
command = [shutil.which('cargo'), 'test' if action == 'core' else action, '-p',
           'coolzhu-core-runtime' if action == 'core' else 'coolzhu-web-console', '--offline', '-j', '1',
           '--target-dir', str(out/'cargo-target')]
if action == 'test':
    command += ['--bin', 'coolzhu-web-console', 'hook_budget_acceptance::dispatch_hook_budget_', '--', '--test-threads=1', '--nocapture']
elif action == 'core':
    command += ['--lib', 'quoted_hook_path_with_spaces_keeps_shell_semantics', '--', '--test-threads=1', '--nocapture']
elif action == 'build':
    command += ['-p', 'coolzhu-core-runtime']
log = out/f'controller-hook-budget-{action}-20261016.log'
receipt = out/f'controller-hook-budget-{action}-20261016.json'
env = os.environ.copy()
env.update(TEMP=str(scratch), TMP=str(scratch), COOLZHU_LOG_DIR=str(scratch/'logs'),
           COOLZHU_RUNTIME_DIR=str(scratch/'runtime'),
           COOLZHU_WEB_SESSION_DB=str(scratch/'global-sessions.sqlite3'),
           COOLZHU_WEB_SESSION_STORE=str(scratch/'global-sessions.json'),
           COOLZHU_WEB_ATTACHMENT_STORE=str(scratch/'attachments'),
           COOLZHU_INPUT_SAFETY_STATE_ROOT=str(scratch/'input-safety'), RUST_BACKTRACE='0')
record = dict(state='running', action=action, command=command, source_before=before,
              started_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), log=str(log))
receipt.write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
began = time.monotonic()
with log.open('wb') as capture:
    process = subprocess.Popen(command, cwd=repo, env=env, stdout=capture, stderr=subprocess.STDOUT,
                               creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
    code = process.wait()
record.update(state='completed' if code == 0 else 'failed', exit_code=code,
              elapsed_seconds=round(time.monotonic()-began, 3), source_after=hashes())
record['source_unchanged'] = record['source_before'] == record['source_after']
record['log_sha256'] = hashlib.sha256(log.read_bytes()).hexdigest()
contents = log.read_text(encoding='utf-8', errors='replace')
record['test_results'] = re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', contents)
record['finished_at_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
receipt.write_text(json.dumps(record, ensure_ascii=False, indent=2), encoding='utf-8')
print(json.dumps({key:value for key,value in record.items() if not key.startswith('source_')}, ensure_ascii=False))
print('\n'.join(contents.splitlines()[-24:]))
sys.exit(code)
