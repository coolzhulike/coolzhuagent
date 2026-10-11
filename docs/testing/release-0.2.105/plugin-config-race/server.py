"""只记录真实插件请求的本机服务；不替代模型、工具或宿主。"""
from pathlib import Path
from http.server import ThreadingHTTPServer,BaseHTTPRequestHandler
import threading,time,json,hashlib
folder=Path(__file__).resolve().parent
lock=threading.Lock()
def record(event,**fields):
    with lock:
        with (folder/'network-events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps({'event':event,'unix_ms':time.time()*1000,'monotonic_ns':time.perf_counter_ns(),**fields})+'\n')
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        record('real_plugin_get',path=self.path)
        body=b'OWNED_NETWORK_ENDPOINT_CONFIG_RACE'
        self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
plugin='dsh-aa909f79795799f288de8a67@external'
tool='dsh__'+hashlib.sha256(json.dumps([plugin,'net_fetch'],separators=(',',':')).encode()).hexdigest()[:32]
address={'port':server.server_port,'url':f'http://127.0.0.1:{server.server_port}/config-race-20261008','plugin_id':plugin,'tool_name':tool}
(folder/'address.json').write_text(json.dumps(address,indent=2),encoding='utf-8')
record('server_started',**address)
def stop():
    deadline=time.monotonic()+1200
    while not (folder/'stop-server').exists() and time.monotonic()<deadline:time.sleep(.2)
    server.shutdown()
threading.Thread(target=stop,daemon=True).start()
print(json.dumps(address),flush=True);server.serve_forever();server.server_close();record('server_stopped')
