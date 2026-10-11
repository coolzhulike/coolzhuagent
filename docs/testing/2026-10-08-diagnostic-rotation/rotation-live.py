"""真实候选EXE在独立日志/工作目录的OS锁竞争与轮转验证；不发模型请求。"""
from pathlib import Path
import hashlib,json,msvcrt,os,sqlite3,socket,subprocess,time,urllib.request
root=Path(__file__).resolve().parent;repo=root.parents[1];folder=root/'rotation-live'
folder.mkdir(exist_ok=True);workspace=folder/'workspace';assert not workspace.exists()
(workspace/'.coolzhu').mkdir(parents=True)
with socket.socket() as s:assert s.connect_ex(('127.0.0.1',8767))!=0
source=root/'wal-drill3/restored/.coolzhu/web-sessions.sqlite3';db=workspace/'.coolzhu/web-sessions.sqlite3'
src=sqlite3.connect(source.resolve().as_uri()+'?mode=ro',uri=True);dst=sqlite3.connect(db);src.backup(dst);src.close();dst.close()
(workspace/'coolzhu.toml').write_text('[web]\nbind_addr="127.0.0.1:8767"\n[pet]\nenabled=false\n[model]\nenable_real_llm=true\n',encoding='utf-8')
binary=repo/'target/debug/coolzhu-web-console.exe';binary_sha=hashlib.file_digest(binary.open('rb'),'sha256').hexdigest()
logs=folder/'logs';logs.mkdir();structured=logs/'coolzhu-web-console.jsonl';crumb=workspace/'err.log'
limit=8*1024*1024
json_line=(json.dumps({'event':'rotation_seed','message':'x'*975},separators=(',',':'))+'\n').encode()
seed=json_line*(limit//len(json_line));seed+=b' '*(limit-len(seed)-1)+b'\n' if len(seed)<limit else b''
assert len(seed)==limit
structured.write_bytes(seed);crumb.write_bytes(b'old diagnostic breadcrumb\n'*(limit//26)+b'\n'*(limit%26))
# seed大小按真实长度核验；不以伪JSON模型响应驱动产品。
seed_hash=hashlib.sha256(seed).hexdigest();crumb_before=crumb.read_bytes();crumb_hash=hashlib.sha256(crumb_before).hexdigest()
env=os.environ.copy();env['COOLZHU_RUNTIME_DIR']=str(workspace);env['COOLZHU_LOG_DIR']=str(logs)
env['COOLZHU_INPUT_SAFETY_STATE_ROOT']=str(folder/'isolated-input-safety');env.pop('COOLZHU_WEB_STATIC_ROOT',None)
env.pop('COOLZHU_BROWSER_NAV_DIAGNOSTICS',None)
gui=Path(os.environ['TEMP'])/'coolzhu-gui-web-url.txt';old_gui=gui.read_bytes() if gui.exists() else None
processes=[];locks=[]
def start(name):
    out=(folder/(name+'-stdout.txt')).open('wb');err=(folder/(name+'-stderr.txt')).open('wb')
    p=subprocess.Popen([str(binary)],cwd=workspace,env=env,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
    processes.append((name,p,out,err))
    begin=time.monotonic()
    for _ in range(100):
        assert p.poll() is None,'候选启动失败'
        try:
            with urllib.request.urlopen('http://127.0.0.1:8767/api/chat/rooms',timeout=1) as r:rooms=json.load(r)
            return p,round((time.monotonic()-begin)*1000)
        except OSError:time.sleep(.1)
    raise RuntimeError('候选服务未就绪')
try:
    # 用独立Python进程的Windows文件锁占住真实产品锁，验证不等待且不改文件。
    lock_path=Path(str(structured)+'.lock');lock_path.write_bytes(b'0')
    handle=lock_path.open('r+b');msvcrt.locking(handle.fileno(),msvcrt.LK_NBLCK,1);locks.append(handle)
    first,contended_ms=start('contended')
    assert hashlib.file_digest(structured.open('rb'),'sha256').hexdigest()==seed_hash
    assert not Path(str(structured)+'.1').exists()
    handle.seek(0);msvcrt.locking(handle.fileno(),msvcrt.LK_UNLCK,1);handle.close();locks.clear()
    first.terminate();first.wait(timeout=10)
    processes[0][2].close();processes[0][3].close()
    stderr=(folder/'contended-stderr.txt').read_text(encoding='utf-8',errors='replace')
    assert stderr.count('结构化诊断文件写入失败')==1
    second,normal_ms=start('normal')
    assert hashlib.file_digest(Path(str(structured)+'.1').open('rb'),'sha256').hexdigest()==seed_hash
    active=[json.loads(x) for x in structured.read_text(encoding='utf-8').splitlines() if x.strip()]
    assert any(x.get('event')=='diagnostics.ready' for x in active)
    assert structured.stat().st_size<=limit
    assert Path(str(crumb)+'.1').exists()
    assert hashlib.file_digest(Path(str(crumb)+'.1').open('rb'),'sha256').hexdigest()==crumb_hash
    assert crumb.stat().st_size<=limit
    result={'stage':'0.2.109之后源码候选，未追认为109正式','binary_sha256':binary_sha,'contended_start_ready_ms':contended_ms,
      'contended_file_unchanged':True,'contended_stderr_warning_count':1,'normal_start_ready_ms':normal_ms,
      'structured_backup_seed_sha256':seed_hash,'structured_active_bytes':structured.stat().st_size,'structured_ready_event':True,
      'breadcrumb_backup_seed_sha256':crumb_hash,'breadcrumb_active_bytes':crumb.stat().st_size,
      'original_runtime_database_writes':0,'model_requests':0,'cloud_sessions_created':0,'live_pid':second.pid,'status':'候选真实轮转事实通过，待正常UI实拍'}
    (folder/'result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');print(json.dumps(result,ensure_ascii=False),flush=True)
    while not (folder/'stop').exists():time.sleep(.2)
finally:
    for handle in locks:handle.close()
    cleanup=[]
    for name,p,out,err in processes:
        if p.poll() is None:p.terminate()
        p.wait(timeout=10);out.close();err.close();cleanup.append({'name':name,'pid':p.pid,'exit_code':p.returncode,'termination':'自有候选测试服务，非业务取消证明'})
    if old_gui is not None:gui.write_bytes(old_gui)
    elif gui.exists():gui.unlink()
    (folder/'cleanup.json').write_text(json.dumps(cleanup,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
