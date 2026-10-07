"""真实iframe页面与独立浏览器事件台账；不生成模型回复或代理输入。"""
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from urllib.parse import urlsplit
import json, threading, time

dest = Path(__file__).resolve().parent
events = []
lock = threading.Lock()
common = '''<meta charset="utf-8"><style>body{margin:16px;background:#edf5ed;color:#17291e;font:20px sans-serif}h1{font-size:24px}button{font-size:22px;background:#d1e6d8;border:3px solid #22563e;border-radius:8px;padding:12px}pre{font:16px monospace;white-space:pre-wrap}</style>'''
report = '''function report(type,detail={}){fetch('/event',{method:'POST',body:JSON.stringify({case:caseName,type,at_ms:performance.timeOrigin+performance.now(),...detail}),keepalive:true});}
for(const type of ['pointerdown','pointerup','click'])document.addEventListener(type,e=>report(type,{target:e.target.id||e.target.tagName,isTrusted:e.isTrusted,button:e.button}),true);report('ready');'''
parent = f'''<!doctype html><html lang="zh">{common}<title>081 iframe子文档点击</title>
<h1>081 iframe子文档点击</h1><p id="result">父操作次数：0；Frame子操作次数：0</p>
<div id="outer" role="button" tabindex="0" aria-label="父操作按钮" style="width:360px;height:190px;border:3px solid #22563e;position:relative">
<iframe id="child" title="独立内嵌子文档" src="/child.html" style="position:absolute;inset:10px;width:340px;height:170px;border:0"></iframe></div>
<p><button id="replace">切换子页面</button></p><p>仅子按钮点击应增加子次数，不能增加父次数。</p>
<script>const caseName='IFRAME-PARENT';{report}let p=0,c=0;
outer.addEventListener('click',()=>{{p++;result.textContent='父操作次数：'+p+'；Frame子操作次数：'+c;report('parent-count',{{count:p}});}});
window.addEventListener('message',e=>{{if(e.source===child.contentWindow&&e.origin===location.origin&&e.data?.kind==='child-count'){{c=e.data.count;result.textContent='父操作次数：'+p+'；Frame子操作次数：'+c;}}}});
replace.addEventListener('click',()=>{{child.src='/child-next.html';report('child-navigation');}});</script></html>'''

def child(name):
    return f'''<!doctype html><html lang="zh">{common}<title>{name}</title><p style="margin:0 0 8px">{name}</p>
    <button id="frame-child" style="width:100%;height:70px;padding:0">Frame子操作按钮</button><p id="result" style="margin:8px 0">Frame子操作次数：0</p>
    <script>const caseName={json.dumps(name)};{report}let c=0;document.getElementById('frame-child').addEventListener('click',()=>{{c++;result.textContent='Frame子操作次数：'+c;report('child-count',{{count:c}});parent.postMessage({{kind:'child-count',count:c}},location.origin);}});</script></html>'''

pages = {'/frame.html': parent, '/child.html': child('IFRAME-CHILD'), '/child-next.html': child('IFRAME-REPLACED')}
class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
    def do_GET(self):
        path = urlsplit(self.path).path
        if path == '/events':
            with lock: data = json.dumps(events, ensure_ascii=False).encode()
            mime = 'application/json'
        elif path in pages: data = pages[path].encode(); mime = 'text/html'
        else: self.send_error(404); return
        self.send_response(200); self.send_header('Content-Type', mime+'; charset=utf-8'); self.send_header('Content-Length',str(len(data))); self.send_header('Cache-Control','no-store'); self.end_headers(); self.wfile.write(data)
    def do_POST(self):
        if self.path != '/event': self.send_error(404); return
        row = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        row['received_ms'] = time.time_ns()/1e6
        with lock:
            events.append(row)
            (dest/'web-events.json').write_text(json.dumps(events,ensure_ascii=False,indent=2),encoding='utf-8')
        self.send_response(204); self.end_headers()

server = ThreadingHTTPServer(('127.0.0.1',0),Handler)
(dest/'server-address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}','started_ms':time.time_ns()/1e6}),encoding='utf-8')
server.serve_forever()
