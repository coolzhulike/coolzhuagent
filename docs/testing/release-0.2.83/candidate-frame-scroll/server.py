"""真实Frame滚动HTML及独立事件；普通UI准备与模型wheel分别记录。"""
from http.server import ThreadingHTTPServer,BaseHTTPRequestHandler
from pathlib import Path
from urllib.parse import urlsplit
import json,threading,time
folder=Path(__file__).resolve().parent;events=[];lock=threading.Lock()
common='<meta charset="utf-8"><style>html{scroll-behavior:auto}body{margin:12px;font:17px sans-serif;background:#edf5ed;color:#17291e}h1{font-size:21px;margin:10px 0}button{font:16px sans-serif;padding:7px}p{margin:10px 0}</style>'
report="""function report(type,detail={}){fetch('/event',{method:'POST',body:JSON.stringify({case:caseName,type,at_ms:performance.timeOrigin+performance.now(),...detail}),keepalive:true});}
for(const type of ['pointerdown','pointerup','click','wheel'])document.addEventListener(type,e=>report(type,{target:e.target.id||e.target.tagName,isTrusted:e.isTrusted,deltaY:e.deltaY||0}),{capture:true,passive:true});
window.addEventListener('scroll',e=>report('scroll',{y:scrollY,isTrusted:e.isTrusted}),{passive:true});report('ready',{y:scrollY});"""
parent=f'''<!doctype html><html lang="zh">{common}<title>083 Frame滚动父页面</title>
<div style="position:sticky;top:0;background:#edf5ed;z-index:2"><h1>083 Frame滚动</h1><p id="status">子滚动位置：0；父滚动位置：0</p></div>
<iframe id="child" title="SCROLL-CHILD" src="/child.html" style="width:100%;height:150px;border:2px solid #22563e"></iframe>
<p><button id="bottom">准备子页面底部</button> <button id="replace">切换子页面</button></p>
<p>父页面独立内容，仅父页面滚动时位置改变。</p><div style="height:1300px;background:linear-gradient(#d5ead7,#789e81)">父页面滚动区域</div>
<script>const caseName='SCROLL-PARENT';{report}let childY=0;
function render(){{status.textContent='子滚动位置：'+childY+'；父滚动位置：'+scrollY;}}
window.addEventListener('message',e=>{{if(e.source===child.contentWindow&&e.origin===location.origin&&e.data?.kind==='scroll'){{childY=e.data.y;document.getElementById('status').textContent='子滚动位置：'+childY+'；父滚动位置：'+scrollY;}}}});
window.addEventListener('scroll',()=>{{document.getElementById('status').textContent='子滚动位置：'+childY+'；父滚动位置：'+scrollY;}},{{passive:true}});
bottom.addEventListener('click',()=>{{child.contentWindow.postMessage({{kind:'prepare-bottom'}},location.origin);report('prepare-bottom');}});
replace.addEventListener('click',()=>{{child.src='/child-next.html';report('child-navigation');}});</script></html>'''
def child(name):
 return f'''<!doctype html><html lang="zh">{common}<title>{name}</title><h1 style="position:sticky;top:0;background:#edf5ed">{name}</h1>
 <p id="position" style="position:sticky;top:35px;background:#edf5ed">子位置：0</p>
 <div style="height:1100px;background:linear-gradient(#c5e2c9,#63816a)">子页面滚动区域</div><p>子页面底部</p>
 <script>const caseName={json.dumps(name)};{report}function publish(){{position.textContent='子位置：'+scrollY;parent.postMessage({{kind:'scroll',y:scrollY}},location.origin);}}
 window.addEventListener('scroll',publish,{{passive:true}});window.addEventListener('message',e=>{{if(e.source===parent&&e.origin===location.origin&&e.data?.kind==='prepare-bottom'){{scrollTo(0,document.scrollingElement.scrollHeight);report('prepared-bottom',{{y:scrollY}});publish();}}}});publish();</script></html>'''
pages={'/scroll.html':parent,'/child.html':child('SCROLL-CHILD'),'/child-next.html':child('SCROLL-REPLACED')}
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  path=urlsplit(self.path).path
  if path=='/events':
   with lock:data=json.dumps(events,ensure_ascii=False).encode()
   mime='application/json'
  elif path in pages:data=pages[path].encode();mime='text/html'
  else:self.send_error(404);return
  self.send_response(200);self.send_header('Content-Type',mime+'; charset=utf-8');self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  row=json.loads(self.rfile.read(int(self.headers['Content-Length'])));row['received_ms']=time.time_ns()/1e6
  with lock:
   events.append(row);(folder/'web-events.json').write_text(json.dumps(events,ensure_ascii=False,indent=2),encoding='utf-8')
  self.send_response(204);self.end_headers()
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'server-address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}','started_ms':time.time_ns()/1e6}),encoding='utf-8')
server.serve_forever()
