import datetime, hashlib, json, pathlib, sqlite3, subprocess, time, urllib.request

root = pathlib.Path(r'D:\coolzhuagent')
out = root / 'tmp/goal-32wbs-acceptance-20261013'
stage = out / 'cold-start-20261016'
stage.mkdir(exist_ok=True)
receipt = out / 'controller-cold-start-20261016.json'
def now(): return datetime.datetime.now(datetime.timezone.utc).isoformat()
def save(data): receipt.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding='utf-8')
db = pathlib.Path(r'D:\tmp\.coolzhu\web-sessions.sqlite3')
src = sqlite3.connect(db.as_uri()+'?mode=ro', uri=True)
dst = sqlite3.connect(stage / 'session-before.sqlite3')
src.backup(dst)
dst.close()
src.close()
original = pathlib.Path(r'C:\Program Files\CoolzhuAgent\config\package-launcher.json')
config = json.loads(original.read_text(encoding='utf-8-sig'))
config['web_console']['executable'] = r'C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe'
config['tauri']['executable'] = r'C:\Program Files\CoolzhuAgent\bin\coolzhu-tauri-shell.exe'
config['health_timeout_secs'] = 180
config['log_dir'] = str(stage / 'launcher-logs')
cfg = stage / 'package-launcher.json'
cfg.write_text(json.dumps(config, ensure_ascii=False, indent=2), encoding='utf-8')
data = {'started_at_utc':now(), 'state':'running', 'method':'安装版正式启动器，隔离诊断配置仅将就绪期限30秒延长180秒并单独保存日志', 'backup':str(stage/'session-before.sqlite3'), 'changes_to_installed_files':False, 'probes':[]}
save(data)
with (stage / 'launcher.log').open('wb') as log:
    proc = subprocess.Popen([r'C:\Program Files\CoolzhuAgent\COOLZHU-AGENT.exe',str(cfg),'--workspace',str(root),'--no-error-dialog'], cwd=root, stdout=log,stderr=subprocess.STDOUT)
    data['launcher_pid'] = proc.pid
    start = time.monotonic()
    save(data)
    for i in range(38):
        try:
            with urllib.request.urlopen('http://127.0.0.1:8765/api/diagnostics/health',timeout=2) as response:
                data['health'] = json.load(response)
            data['ready_after_seconds'] = round(time.monotonic()-start,2)
            data['state']='ready'
            save(data)
            break
        except Exception as error:
            data['probes'].append({'seconds':round(time.monotonic()-start,2),'error':type(error).__name__})
        if proc.poll() is not None:
            break
        time.sleep(5)
    if data['state'] != 'ready':
        data['state']='failed'
    data['launcher_exit_code']=proc.poll()
    data['finished_at_utc']=now()
    save(data)
print(json.dumps({k:v for k,v in data.items() if k!='health'},ensure_ascii=False))
