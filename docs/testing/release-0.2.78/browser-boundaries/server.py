"""真实HTML边界页面及事件台账；不生成或替代模型响应。"""
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from urllib.parse import urlsplit
import json, threading, time

root=Path(__file__).resolve().parent
events=[]
lock=threading.Lock()
style='body{background:#edf5ed;color:#17291e;font:20px sans-serif;padding:24px}h1{font-size:26px}button,.outer{width:340px;height:140px;border:3px solid #22563e;border-radius:10px;background:#d1e6d8;position:relative;padding:0;margin:16px 0;color:#123b28;font-size:23px}.inner{position:absolute;inset:12px;display:flex;align-items:center;justify-content:center;background:#91b8dd}iframe{position:absolute;inset:12px;width:310px;height:110px;border:0}pre{font:17px monospace;white-space:pre-wrap}'

def page(case, body, extra=''):
    return f'''<!doctype html><html lang="zh"><meta charset="utf-8"><title>078 {case}边界</title><style>{style}</style><h1>078 {case}边界</h1><p>当前用例：{case}</p>{body}<pre id="ledger">本页输入事件：0</pre><script>
    const caseName={json.dumps(case)};let seen=[];
    function report(type,detail={{}}){{let row={{case:caseName,type,at_ms:performance.timeOrigin+performance.now(),...detail}};fetch('/event',{{method:'POST',body:JSON.stringify(row),keepalive:true}});}}
    for(const type of ['pointerdown','pointerup','click'])document.addEventListener(type,e=>{{const row={{target:e.target.id||e.target.tagName,button:e.button,isTrusted:e.isTrusted}};seen.push({{type,...row}});document.getElementById('ledger').textContent='本页输入事件：'+seen.length+'\\n'+JSON.stringify(seen);report(type,row);}},true);
    report('ready');{extra}
    </script></html>'''

pages={
 '/nested.html':page('NESTED','<button id="outer" aria-label="父操作按钮"><span id="nested-control" role="button" tabindex="0" class="inner">独立子操作按钮</span></button><p id="result">父操作次数：0；子操作次数：0</p>',"let p=0,c=0;outer.addEventListener('click',e=>{if(e.target===outer)p++;else c++;result.textContent='父操作次数：'+p+'；子操作次数：'+c;});"),
 '/shadow.html':page('SHADOW','<button id="outer" aria-label="父操作按钮"><span id="shadow-host" class="inner"></span></button><p id="result">父操作次数：0；Shadow子操作次数：0</p>',"let p=0,c=0;const s=document.getElementById('shadow-host').attachShadow({mode:'open'});s.innerHTML='<style>button{width:100%;height:100%;font-size:22px;background:#e7c287;border:0}</style><button id=shadow-child>Shadow子操作按钮</button>';s.firstElementChild.nextElementSibling.addEventListener('click',()=>{c++;result.textContent='父操作次数：'+p+'；Shadow子操作次数：'+c;report('shadow-click',{count:c});});outer.addEventListener('click',e=>{if(e.target===outer){p++;result.textContent='父操作次数：'+p+'；Shadow子操作次数：'+c;}});"),
 '/frame.html':page('IFRAME','<div id="outer" role="button" tabindex="0" aria-label="父操作按钮" class="outer"><iframe title="独立内嵌子文档" src="/child.html"></iframe></div><p id="result">父操作次数：0；Frame子操作次数：0</p>',"let p=0;outer.addEventListener('click',()=>{p++;result.textContent='父操作次数：'+p+'；Frame子操作次数：0';});"),
 '/child.html':page('IFRAME-CHILD','<button id="frame-child" style="width:100%;height:70px;margin:0">Frame子操作按钮</button>'),
 '/navigation.html':page('NAVIGATION','<button id="navigate" aria-label="跳转到新文档">跳转到新文档</button><p>当前文档：SOURCE</p>',"navigate.addEventListener('pointerdown',()=>{report('navigation-request');location.replace('/landing.html');});"),
 '/landing.html':page('LANDING','<p id="result">当前文档：LANDING-078</p>'),
 '/close.html':page('PANEL-CLOSE','<button id="increment" aria-label="增加次数">增加次数</button><p id="result">次数：0</p>',"let n=0;increment.addEventListener('click',()=>{result.textContent='次数：'+(++n);});"),
}
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  path=urlsplit(self.path).path
  if path=='/events':
   with lock:data=json.dumps(events,ensure_ascii=False).encode()
   mime='application/json'
  elif path in pages:data=pages[path].encode();mime='text/html'
  else:self.send_error(404);return
  self.send_response(200);self.send_header('Content-Type',mime+'; charset=utf-8');self.send_header('Content-Length',str(len(data)));self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(data)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  row=json.loads(self.rfile.read(int(self.headers['Content-Length'])));row['received_ms']=time.time_ns()/1e6
  with lock:
   events.append(row)
   (root/'web-events.json').write_text(json.dumps(events,ensure_ascii=False,indent=2),encoding='utf-8')
  self.send_response(204);self.end_headers()

server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(root/'server-address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}','started_ms':time.time_ns()/1e6}),encoding='utf-8')
server.serve_forever()
