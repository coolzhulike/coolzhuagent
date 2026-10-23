"""MSI 权限阻塞后的独立完整包正式端口验证；不冒充安装版覆盖成功。"""
import datetime,hashlib,json,os,pathlib,sqlite3,subprocess,time,urllib.request
root=pathlib.Path(r'D:\coolzhuagent');stage=root/'tmp/goal-32wbs-acceptance-20261013'
package=stage/'release-0.2.135/package';logdir=stage/'release-0.2.135/live-default';logdir.mkdir(exist_ok=True)
receipt=stage/'controller-package135-live-20261016.json'
data={'at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'state':'starting','scope':'完整0.2.135发布包在正式8765端口，默认随包配置和可见Tauri；不代表MSI覆盖成功','installed_version':'0.2.134','msi_blocker':'1730/1603：非提升进程无法删除旧机器级安装','package':str(package)}
def save():receipt.write_text(json.dumps(data,ensure_ascii=False,indent=2),encoding='utf-8')
def read(p):return json.loads(pathlib.Path(p).read_text(encoding='utf-8-sig'))
def sha(p):
 with pathlib.Path(p).open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()
try:
 prior=read(stage/'controller-switch135-20261016.json')
 assert prior['msi_exit_code']==1603 and prior['old_processes_exited']
 assert sha(pathlib.Path(r'C:\Program Files\CoolzhuAgent\bin\coolzhu-web-console.exe'))=='7e678c39883a6fcaabc61cad080d7e9073931a25b4ce4a77e3e3048a941d08f8'
 assert read(package/'config/package-launcher.json')['health_timeout_secs']==120
 data['default_health_timeout_secs']=120
 data['backup']=prior['backup'];data['history_before']=prior['history_before']
 inventory=read(package/'payload-inventory.json')
 for f in inventory['files']:assert sha(package/f['path'])==f['sha256'],f['path']
 data['payload_matched']=len(inventory['files'])
 data['source_snapshot_digest']=inventory['build_identity']['source_snapshot_digest']
 checkpath=pathlib.Path(os.environ['LOCALAPPDATA'])/'CoolzhuAgent/logs/package-launcher/package-selfcheck-last.json'
 start_wall=int(time.time()*1000);start=time.monotonic();save()
 with (logdir/'launcher.log').open('wb') as log:
  p=subprocess.Popen([str(package/'COOLZHU-AGENT.exe'),'--no-error-dialog'],cwd=root,stdout=log,stderr=subprocess.STDOUT)
 data['launcher_pid']=p.pid;data['arguments']=['--no-error-dialog'];save()
 for _ in range(130):
  if checkpath.exists():
   check=read(checkpath)
   if check.get('timestamp_ms',0)>=start_wall:
    data['selfcheck']=check
    if check.get('ok'):break
    raise RuntimeError(check.get('error'))
  try:
   with urllib.request.urlopen('http://127.0.0.1:8765/api/diagnostics/health',timeout=1) as response:
    if response.status==200 and 'http_ready_seconds' not in data:data['http_ready_seconds']=round(time.monotonic()-start,3)
  except (OSError,TimeoutError):pass
  time.sleep(1)
 else:raise RuntimeError('未取得本次启动自检')
 data['startup_seconds']=round(time.monotonic()-start,3)
 assert data['selfcheck']['resolved_launch_paths']['workspace_id']=='ws-8d17063fefe5c23c'
 data['history_after']={}
 dbpath=pathlib.Path(r'D:\tmp\.coolzhu\web-sessions.sqlite3')
 with sqlite3.connect(dbpath.as_uri()+'?mode=ro',uri=True) as db:
  for table in prior['history_before']:
   if table=='binding':
    data['history_after']['binding']=db.execute("SELECT workspace_id,agent_id,remote_session_id,locked_attempt FROM devin_acp_bindings WHERE agent_id='session-1791131217833'").fetchall();continue
   hashes=[]
   for row in db.execute('SELECT * FROM "'+table+'"'):
    hashes.append(hashlib.sha256(json.dumps(list(row),ensure_ascii=False,separators=(',',':'),default=lambda x:x.hex() if isinstance(x,bytes) else str(x)).encode()).digest())
   data['history_after'][table]={'rows':len(hashes),'sha256':hashlib.sha256(b''.join(sorted(hashes))).hexdigest()}
 # JSON回读将tuple变成list，先规范化再比较。
 data['history_unchanged']=json.loads(json.dumps(data['history_after']))==data['history_before']
 assert data['history_unchanged'],'历史变化须核诊，不能计通过'
 data['state']='completed'
except Exception as e:
 data['state']='failed';data['error']=repr(e);save();raise
finally:
 data['finished_at_utc']=datetime.datetime.now(datetime.timezone.utc).isoformat();save()
 print(json.dumps({k:v for k,v in data.items() if k not in ['history_before','history_after','selfcheck']},ensure_ascii=False))
