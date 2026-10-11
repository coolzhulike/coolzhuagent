"""普通慢页面记录真实输入事件，不提供模型回复或输入替身。"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Lock
from urllib.parse import urlsplit
import json
import time

root = Path(__file__).resolve().parent
events = []
lock = Lock()
page = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>浏览器边界实操</title>
<style>body{font:20px sans-serif;padding:24px;background:#eff8f5;color:#17392c}button,input{font:22px sans-serif;padding:16px;margin:16px 0}output{display:block;margin:16px 0}</style>
<h1>浏览器边界实操</h1><p>来源页面：SOURCE。按下处理耗时1.6秒。</p>
<button id="counter">增加次数</button><output id="count">次数：0</output>
<label for="entry">输入验收</label><input id="entry"><output id="typed">已输入：</output>
<a href="/target.html">目标页面</a><script>
const doc=crypto.randomUUID();let n=0;
function log(kind,value,sync=false){const item={kind,value,document_id:doc,path:location.pathname,page_ms:Date.now(),visibility:document.visibilityState};const raw=JSON.stringify(item);if(sync){const r=new XMLHttpRequest();r.open('POST','/event',false);r.setRequestHeader('Content-Type','application/json');r.send(raw)}else{navigator.sendBeacon('/event',new Blob([raw],{type:'application/json'}))}}
counter.addEventListener('pointerdown',e=>{log('pointerdown',{trusted:e.isTrusted},true);const end=performance.now()+1600;while(performance.now()<end){};log('handler_finished')});
counter.addEventListener('pointerup',e=>log('pointerup',{trusted:e.isTrusted}));
counter.addEventListener('click',e=>{count.textContent='次数：'+(++n);log('click',{count:n,trusted:e.isTrusted})});
entry.addEventListener('input',()=>{typed.textContent='已输入：'+entry.value;log('input',entry.value)});
window.addEventListener('pagehide',()=>log('pagehide'));document.addEventListener('visibilitychange',()=>log('visibilitychange'));log('load');
</script></html>'''
target = '''<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>浏览器边界目标</title><style>body{font:22px sans-serif;background:#eff8f5;padding:24px}</style><h1>浏览器边界目标</h1><p>目标标记：TARGET-20261005</p><output id="events">目标页收到输入：0</output><script>
const doc=crypto.randomUUID(); let n=0; function log(kind,e){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,document_id:doc,path:location.pathname,page_ms:Date.now(),visibility:document.visibilityState,trusted:e?.isTrusted})],{type:'application/json'}))}for(const k of ['pointerdown','pointerup','keydown','keyup'])document.addEventListener(k,e=>{events.textContent='目标页收到输入：'+(++n);log(k,e)});log('load');</script></html>'''

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path = urlsplit(self.path).path
        if path == '/events':
            with lock:
                data = json.dumps(events, ensure_ascii=False).encode('utf-8')
            mime = 'application/json; charset=utf-8'
        elif path == '/boundary.html':
            data, mime = page.encode('utf-8'), 'text/html; charset=utf-8'
        elif path == '/target.html':
            data, mime = target.encode('utf-8'), 'text/html; charset=utf-8'
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header('Content-Type', mime)
        self.send_header('Cache-Control', 'no-store')
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        if self.path != '/event':
            self.send_error(404)
            return
        item = json.loads(self.rfile.read(min(int(self.headers.get('Content-Length', 0)), 4096)))
        item['server_ms'] = time.time_ns() // 1000000
        with lock:
            events.append(item)
            with (root / 'page-events.jsonl').open('a', encoding='utf-8') as f:
                f.write(json.dumps(item, ensure_ascii=False) + '\n')
        self.send_response(204)
        self.end_headers()

    def log_message(self, *args):
        pass

server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
(root / 'page-server.json').write_text(json.dumps({'port': server.server_port, 'url': f'http://127.0.0.1:{server.server_port}/boundary.html', 'target_url': f'http://127.0.0.1:{server.server_port}/target.html'}, indent=2), encoding='utf-8')
server.serve_forever()
