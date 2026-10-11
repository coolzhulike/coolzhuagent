"""只提供普通新窗口网页并记录真实事件，不生成输入或模拟模型。"""
import json,time,pathlib
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
root=pathlib.Path(__file__).resolve().parent
common="""const doc=crypto.randomUUID();function log(kind,e){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,doc,url:location.href,page_ms:Date.now(),trusted:e?.isTrusted})],{type:'application/json'}))}for(const k of ['pointerdown','pointerup','click','keydown','keyup'])document.addEventListener(k,e=>log(k,e));window.addEventListener('pagehide',e=>log('pagehide',e));log('load');"""
source='<meta charset="utf-8"><title>新窗口来源</title><style>body{font:22px sans-serif;padding:24px}button{padding:24px;font:24px sans-serif}</style><h1>POPUP-SOURCE</h1><button id="go">打开目标链接</button><script>'+common+"go.addEventListener('pointerdown',e=>{log('popup_requested',e);window.open('/popup-target.html','_blank')});</script>"
target='<meta charset="utf-8"><title>新窗口目标</title><style>body{font:22px sans-serif;padding:24px}</style><h1>POPUP-TARGET-20261005</h1><p id="received">目标页输入事件：0</p><script>'+common+"let n=0;for(const k of ['pointerdown','pointerup','click','keydown','keyup','input'])document.addEventListener(k,e=>received.textContent='目标页输入事件：'+(++n));</script>"
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path not in ['/popup-source.html','/popup-target.html']:self.send_error(404);return
        self.send_response(200);self.send_header('Content-Type','text/html;charset=utf-8');self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write((source if self.path=='/popup-source.html' else target).encode())
    def do_POST(self):
        if self.path!='/event':self.send_error(404);return
        item=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length',0)),4096)));item['server_ms']=time.time_ns()//1000000
        with (root/'popup-events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(item,ensure_ascii=False)+'\n')
        self.send_response(204);self.end_headers()
    def log_message(self,*args):pass
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(root/'popup-server.json').write_text(json.dumps({'port':server.server_port}),encoding='utf-8');print(server.server_port,flush=True);server.serve_forever()
