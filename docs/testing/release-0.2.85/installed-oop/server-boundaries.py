"""普通跨站点边界测试页服务器；文件只用于网页，不能代替真实模型输入。"""
import http.server,json,pathlib,time
folder=pathlib.Path(__file__).resolve().parent
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        name=self.path.split('?')[0].removeprefix('/')
        if name not in {'cover.html','scale.html','switch.html','child.html','replacement.html'}:
            self.send_error(404);return
        raw=(folder/'pages'/name).read_text(encoding='utf-8').replace('PORT',str(self.server.server_port)).encode('utf-8')
        self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
    def do_POST(self):
        if self.path!='/event':self.send_error(404);return
        event=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)))
        event.update(observed_ms=time.time()*1000,host=self.headers.get('Host'))
        with (folder/'boundary-events.jsonl').open('a',encoding='utf-8') as output:output.write(json.dumps(event,ensure_ascii=False)+'\n')
        self.send_response(204);self.end_headers()
    def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'boundary-address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8')
server.serve_forever()
