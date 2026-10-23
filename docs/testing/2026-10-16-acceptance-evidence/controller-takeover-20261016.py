import collections, datetime, hashlib, json, os, pathlib, re, subprocess, time

ROOT = pathlib.Path(r'D:\coolzhuagent')
OUT = ROOT / 'tmp/goal-32wbs-acceptance-20261013'
RECEIPT = OUT / 'controller-current-full-20261016.json'

def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def sources():
    names = subprocess.check_output(['git', 'ls-files', '-c', '-o', '--exclude-standard'], cwd=ROOT).decode('utf-8').splitlines()
    extensions = {'.rs', '.toml', '.lock', '.js', '.cjs', '.mjs', '.css', '.html', '.json', '.ps1'}
    return {n: sha(ROOT / n) for n in sorted(set(names)) if (ROOT / n).is_file() and pathlib.Path(n).suffix in extensions and (n.startswith(('modules/', 'packages/', 'config/', 'scripts/')) or n in ('Cargo.toml', 'Cargo.lock'))}

def save(data):
    tmp = RECEIPT.with_suffix('.new.json')
    tmp.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding='utf-8')
    os.replace(tmp, RECEIPT)

def main():
    if RECEIPT.exists():
        raise SystemExit('本轮收据已存在，拒绝重复提交。')
    data = {'started_at_utc': now(), 'state': 'running', 'pid': os.getpid(), 'source_before': sources(), 'steps': []}
    save(data)
    commands = [
        ('frontend', ['node', '--test', str(ROOT / 'modules/gui-web/packages/web-console/tests/chat-trace-locate-sync.test.cjs')]),
        ('console-full', [str(pathlib.Path.home() / '.cargo/bin/cargo.exe'), 'test', '-p', 'coolzhu-web-console', '--offline', '-j', '1', '--target-dir', str(OUT / 'cargo-target'), '--', '--test-threads=1', '--skip', 'wechat'])
    ]
    for name, cmd in commands:
        log = OUT / f'controller-current-{name}-20261016.log'
        step = {'name': name, 'command': cmd, 'log': str(log), 'state': 'running', 'started_at_utc': now()}
        data['steps'].append(step)
        start = time.monotonic()
        with log.open('wb') as stream:
            proc = subprocess.Popen(cmd, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
            step['pid'] = proc.pid
            save(data)
            code = proc.wait()
        content = log.read_text(encoding='utf-8', errors='replace')
        step.update(exit_code=code, state='completed' if code == 0 else 'failed', seconds=round(time.monotonic()-start, 3), log_sha256=sha(log), finished_at_utc=now())
        step['results'] = [{'passed': int(a), 'failed': int(b), 'ignored': int(c), 'filtered': int(d)} for a,b,c,d in re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored; \d+ measured; (\d+) filtered out', content)]
        step['failed_lines'] = [line for line in content.splitlines() if 'FAILED' in line or line.startswith('error:')]
        save(data)
        print(json.dumps({'name': name, 'state': step['state'], 'exit_code': code, 'seconds': step['seconds'], 'results': step['results']}, ensure_ascii=False), flush=True)
        if code:
            break
    data['source_after'] = sources()
    data['source_unchanged'] = data['source_before'] == data['source_after']
    data['state'] = 'completed' if all(s['exit_code'] == 0 for s in data['steps']) and data['source_unchanged'] else 'failed'
    data['finished_at_utc'] = now()
    save(data)
    print(json.dumps({'state': data['state'], 'source_unchanged': data['source_unchanged']}, ensure_ascii=False), flush=True)

if __name__ == '__main__':
    main()
