"""真实HTTP请求计数，响应仅为普通页面，不代替插件。"""
from common import *
import http.server,threading
lock=threading.Lock()
def record(event,**fields):
    with lock:
        with (p/'network-events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps({'event':event,'epoch_ms':time.time()*1000,**fields})+'\n')
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        record('network_request',path=self.path)
        data=b'Unexpected old request execution'
        self.send_response(200);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);server.daemon_threads=True
threading.Thread(target=server.serve_forever,daemon=True).start()
save('server.json',{'port':server.server_port,'url':f'http://127.0.0.1:{server.server_port}/frozen-candidate'})
record('server_started',port=server.server_port);print('受控HTTP服务已就绪',flush=True)
deadline=time.monotonic()+600
while not (p/'stop-server').exists() and time.monotonic()<deadline:time.sleep(.2)
server.shutdown();server.server_close();record('server_stopped')
