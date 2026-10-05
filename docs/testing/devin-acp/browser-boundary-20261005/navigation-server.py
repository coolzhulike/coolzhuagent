"""普通网页自然导航；只记录真实浏览器事件，不派发输入或构造模型响应。"""
import json, time
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock
root = Path(__file__).resolve().parent
events, lock = [], Lock()
common = '''const doc=crypto.randomUUID();function log(kind,e,sync=false){const raw=JSON.stringify({kind,document_id:doc,path:location.pathname,page_ms:Date.now(),trusted:e?.isTrusted});if(sync){const r=new XMLHttpRequest();r.open('POST','/event',false);r.setRequestHeader('Content-Type','application/json');r.send(raw)}else navigator.sendBeacon('/event',new Blob([raw],{type:'application/json'}))}for(const k of ['pointerdown','pointerup','click','keydown','keyup'])document.addEventListener(k,e=>log(k,e));window.addEventListener('pagehide',e=>log('pagehide',e));log('load');'''
source = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>在途导航验收来源</title><style>body{font:22px sans-serif;padding:24px;background:#eff8f5}button{font:24px sans-serif;padding:24px}</style><h1>在途导航验收来源</h1><p>来源标记 SOURCE-NAV-20261005</p><button id="go">进入目标页</button><script>'''+common+'''go.addEventListener('pointerdown',e=>{log('navigation_requested',e,true);location.assign('/target.html')});</script></html>'''
target = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>在途导航验收目标</title><style>body{font:22px sans-serif;padding:24px;background:#eff8f5}</style><h1>在途导航验收目标</h1><p>目标标记 TARGET-NAV-20261005</p><p id="received">目标页输入事件：0</p><input aria-label="目标输入框"><script>'''+common+'''let n=0;for(const k of ['pointerdown','pointerup','click','keydown','keyup','input'])document.addEventListener(k,e=>{received.textContent='目标页输入事件：'+(++n);if(k==='input')log(k,e)});</script></html>'''
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path=self.path.split('?')[0]
        if path=='/events':
            with lock: body=json.dumps(events,ensure_ascii=False).encode()
            mime='application/json; charset=utf-8'
        elif path in ['/source.html','/target.html']:
            body=(source if path=='/source.html' else target).encode();mime='text/html; charset=utf-8'
        else: self.send_error(404);return
        self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(body)
    def do_POST(self):
        if self.path!='/event': self.send_error(404);return
        item=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length',0)),4096)));item['server_ms']=time.time_ns()//1000000
        with lock:
            events.append(item)
            with (root/'navigation-events.jsonl').open('a',encoding='utf-8') as f: f.write(json.dumps(item,ensure_ascii=False)+'\n')
        self.send_response(204);self.end_headers()
    def log_message(self,*args): pass
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(root/'navigation-server.json').write_text(json.dumps({'port':server.server_port,'source_url':f'http://127.0.0.1:{server.server_port}/source.html','target_url':f'http://127.0.0.1:{server.server_port}/target.html'},indent=2),encoding='utf-8')
print('普通导航网页端口',server.server_port,flush=True)
server.serve_forever()
