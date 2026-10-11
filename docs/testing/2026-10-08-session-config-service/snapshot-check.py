"""真实服务并发读取与配置保存，无模型响应夹具，不写原库。"""
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
import hashlib, json, os, socket, sqlite3, subprocess, threading, time, urllib.request, urllib.error

root = Path(__file__).resolve().parents[2]
p = Path(__file__).resolve().parent
case = 'snapshot'
folder = p / case
assert not folder.exists(), '禁止覆盖旧证据'
workspace = folder / 'workspace'
workspace.mkdir(parents=True)
(folder/'driver.py').write_bytes(Path(__file__).read_bytes())
port = 8768
with socket.socket() as sock:
    assert sock.connect_ex(('127.0.0.1', port)) != 0
(workspace / 'coolzhu.toml').write_text(f'[web]\nbind_addr="127.0.0.1:{port}"\n[pet]\nenabled=false\n[model]\nenable_real_llm=false\nlocal_chat_port=8082\nlocal_chat_context_window=8192\nlocal_chat_max_output_tokens=4096\n', encoding='utf-8')
binary = Path('C:/Program Files/CoolzhuAgent/bin/coolzhu-web-console.exe') if case == 'before112' else root / 'target/debug/coolzhu-web-console.exe'
env = os.environ.copy()
env.update(COOLZHU_RUNTIME_DIR=str(workspace), COOLZHU_LOG_DIR=str(folder/'logs'), COOLZHU_INPUT_SAFETY_STATE_ROOT=str(folder/'isolated-input-safety'))
for key in ['COOLZHU_WEB_STATIC_ROOT', 'COOLZHU_BROWSER_NAV_DIAGNOSTICS']:
    env.pop(key, None)
gui = Path(os.environ['TEMP']) / 'coolzhu-gui-web-url.txt'
oldgui = gui.read_bytes() if gui.exists() else None
out = (folder/'stdout.txt').open('wb'); err = (folder/'stderr.txt').open('wb')
process = subprocess.Popen([str(binary)], cwd=workspace, env=env, stdout=out, stderr=err, creationflags=subprocess.CREATE_NO_WINDOW)
facts = {'case':case, 'binary':str(binary), 'binary_sha256':hashlib.file_digest(binary.open('rb'),'sha256').hexdigest(), 'pid':process.pid, 'port':port, 'model_requests':0, 'new_cloud_sessions':0, 'original_runtime_writes':0}
stop = threading.Event()

def api(path, data=None):
    request = urllib.request.Request(f'http://127.0.0.1:{port}'+path, data=None if data is None else json.dumps(data).encode(), headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return response.status, json.load(response)
    except urllib.error.HTTPError as error:
        return error.code, json.load(error)

try:
    for _ in range(150):
        assert process.poll() is None
        try:
            if api('/api/sessions')[0] == 200: break
        except OSError: pass
        time.sleep(.1)
    else: raise RuntimeError('服务未就绪')
    status, created = api('/api/sessions', {'name':'快照-a', 'provider':'custom', 'model':'qwen3.8-flash', 'base_url':'https://example.com/v1'})
    assert status == 200, (status, created)
    sid = created['session']['id']; path = '/api/sessions/'+sid+'/model-settings'
    facts['session_id'] = sid
    def save(label):
        a = label == 'a'
        parameters = {'temperature':.25 if a else .75, 'context_window':8192 if a else 16384, 'max_output_tokens':512 if a else 1024, 'protocol':'openai_chat_completions', 'base_url':'https://snapshot-'+label+'.example/v1', 'endpoint':'/chat/completions?variant='+label}
        return api(path, {'session':{'name':'快照-'+label}, 'parameters':parameters})
    assert save('a')[0] == 200
    def reader(_):
        reads = 0; mismatches = []; failures = []
        while not stop.is_set():
            status, loaded = api(path)
            if status != 200:
                failures.append({'status':status,'result':loaded}); continue
            reads += 1
            label = loaded['session']['name'][-1]
            expected = (0.25,8192,512) if label == 'a' else (0.75,16384,1024)
            actual = (loaded['parameters']['temperature'],loaded['parameters']['context_window'],loaded['parameters']['max_output_tokens'])
            expected_url = 'https://snapshot-'+label+'.example/v1'
            expected_endpoint = '/chat/completions?variant='+label
            if actual != expected or loaded['effective_context_window'] != actual[1] or loaded['effective_max_output_tokens'] != actual[2] or loaded['base_url'] != expected_url or loaded['session']['base_url'] != expected_url or loaded['endpoint'] != expected_endpoint or loaded['session']['endpoint'] != expected_endpoint:
                if len(mismatches) < 10: mismatches.append(loaded)
        return {'reads':reads, 'mismatches':mismatches, 'failures':failures}
    with ThreadPoolExecutor(max_workers=7) as pool:
        readers = [pool.submit(reader,i) for i in range(6)]
        writes = []
        try:
            for i in range(180):
                status, result = save('b' if i%2 == 0 else 'a')
                assert status == 200, (status, result)
                writes.append({'sequence':i,'revision':result['configuration_revision'],'name':result['session']['name']})
        finally: stop.set()
        observations = [f.result() for f in readers]
    facts.update(writes=writes, readers=observations, total_reads=sum(r['reads'] for r in observations), mismatch_samples=sum(len(r['mismatches']) for r in observations), race_passed=not any(r['mismatches'] or r['failures'] for r in observations))
    # 正常本地容量规则仍生效，不启动本地模型或发送请求。
    baseline = api(path)[1]
    status, loaded = api(path, {'session':{'name':'快照-本地容量复验','base_url':'http://127.0.0.1:8082/v1'}, 'parameters':{'context_window':32768,'max_output_tokens':1024,'temperature':.25}, 'expected_revision':baseline['configuration_revision']})
    assert status == 200 and loaded['effective_context_window'] == 8192 and loaded['effective_max_output_tokens'] == 1024, (status,loaded)
    facts['local_cap_passed'] = True
    (folder/'facts.json').write_text(json.dumps(facts,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    (folder/'saved-settings.json').write_text(json.dumps(loaded,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'stage':'race-completed','case':case,'reads':facts['total_reads'],'writes':len(writes),'mismatch_samples':facts['mismatch_samples'],'race_passed':facts['race_passed'],'local_cap_passed':True,'session_id':sid},ensure_ascii=False),flush=True)
    if case != 'before112':
        assert facts['race_passed'], '候选快照读取不一致'
        print('同步块提取后的真实读取竞争完成，无需复用旧截图',flush=True)
finally:
    if process.poll() is None: process.terminate()
    process.wait(timeout=15); out.close(); err.close()
    if oldgui is None:
        if gui.exists(): gui.unlink()
    else: gui.write_bytes(oldgui)
    (folder/'cleanup.json').write_text(json.dumps({'pid':process.pid,'exit_code':process.returncode,'gui_hint_restored':True})+'\n',encoding='utf-8')
