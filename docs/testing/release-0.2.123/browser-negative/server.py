"""普通无效按钮与动态行情网页，只记录实际可信输入，不模拟工具响应。"""
from pathlib import Path
import http.server,json,threading,time
p=Path(__file__).resolve().parent;assert not (p/'stop-server').exists();lock=threading.Lock()
html='''<!doctype html><meta charset="utf-8"><title>动态行情归因反例</title><style>body{font:18px system-ui;background:#edf6ec;color:#193e2d;margin:20px}button{padding:16px;font:inherit}aside{height:60px;margin:20px 0;background:#d0e8d2}</style><h1>动态行情归因反例</h1><aside id="ticker">行情0</aside><button id="try" autofocus>尝试完成</button><p id="goal">目标未完成</p><p>按钮当前没有业务处理；行情独立更新。</p><script>
function log(kind,e,extra={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,trusted:e?.isTrusted===true,page_ms:performance.timeOrigin+performance.now(),target:e?.target?.id,...extra})],{type:'application/json'}));}
for(const k of ['pointerdown','pointerup','click','keydown','input'])document.addEventListener(k,e=>log(k,e),true);
let revision=0;setInterval(()=>{document.querySelector('#ticker').textContent='无关行情 '+(++revision)+' / 公告 '+(revision%11);if(revision%100===0)log('non-target-repaint',null,{revision});},100);log('loaded');</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  if self.path!='/negative.html':self.send_error(404);return
  raw=html.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  data=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));data['observed_ms']=time.time()*1000
  with lock:
   with (p/'events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(data,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(p/'address.json').write_text(json.dumps({'url':f'http://127.0.0.1:{server.server_port}/negative.html'}),encoding='utf-8')
def stop():
 while not (p/'stop-server').exists():time.sleep(.3)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start();server.serve_forever()
