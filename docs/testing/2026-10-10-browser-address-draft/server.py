"""普通HTML同文档地址变化，用真实原生界面验证用户草稿不会被网页事件覆盖。"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.parse import urlsplit
import json,threading,time,os
p=Path(__file__).resolve().parent;lock=threading.Lock()
def record(data):
 data.update(received_ms=time.time()*1000,server_ns=time.perf_counter_ns())
 with lock:
  with (p/'events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(data,ensure_ascii=False)+'\n')
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  path=urlsplit(self.path).path;record({'kind':'request','path':self.path})
  if path not in ('/pulse.html','/target.html'):self.send_error(404);return
  title='同文档地址更新' if path=='/pulse.html' else '用户草稿目标页已打开'
  html=f'<!doctype html><meta charset="utf-8"><title>{title}</title><style>body{{font:21px system-ui;margin:26px;background:#eff7ef;color:#17442e}}p{{line-height:1.7}}</style><body><h1>{title}</h1><p>网页正常更新自身地址。用户正在编辑的新地址应保留，提交后导航到目标页面。</p><p id="pulse">等待更新</p>'
  if path=='/pulse.html':html+='''<script>let tick=0;setInterval(()=>{history.replaceState(null,'','/pulse.html?tick='+ ++tick);pulse.textContent='实际地址变化 '+tick; navigator.sendBeacon('/event',new Blob([JSON.stringify({kind:'same-document-url',tick,page_ms:performance.timeOrigin+performance.now()})],{type:'application/json'}));},500);</script>'''
  raw=(html+'</body>').encode();self.send_response(200);self.send_header('Content-Type','text/html;charset=utf-8');self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  n=int(self.headers.get('Content-Length',0))
  if self.path!='/event' or not 0<n<=2048:self.send_error(400);return
  record(json.loads(self.rfile.read(n)));self.send_response(204);self.end_headers()
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(p/'address.json').write_text(json.dumps({'base':f'http://127.0.0.1:{server.server_port}','pid':os.getpid()}),encoding='utf-8')
def stop():
 while not (p/'stop-server').exists():time.sleep(.2)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start();print('ready',server.server_port,flush=True);server.serve_forever()
