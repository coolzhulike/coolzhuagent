"""独立真实裁剪网页：目标子控件在DOM中存在，但被父容器遮蔽。"""
import http.server,json,pathlib,time
folder=pathlib.Path(__file__).resolve().parent
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  port=self.server.server_port
  if self.path=='/clipped.html':
   body=f'''<title>CLIPPED-PARENT</title><h1>CLIPPED-PARENT</h1><p>右侧子按钮被父容器裁剪，不能点击。</p><p id="stats">父页点击次数：0</p><div style="width:180px;height:230px;overflow:hidden;border:3px solid #236"><iframe title="被裁剪子页" style="width:430px;height:220px;border:0" src="http://localhost:{port}/child.html"></iframe></div><script>let n=0;addEventListener('click',e=>{{stats.textContent='父页点击次数：'+(++n);navigator.sendBeacon('/event',JSON.stringify({{role:'PARENT',kind:'click',count:n,trusted:e.isTrusted}}))}})</script>'''
  elif self.path=='/child.html':
   body='''<title>CLIPPED-CHILD</title><h2>CLIPPED-CHILD</h2><p id="stats">被裁剪按钮点击次数：0</p><button style="margin-left:240px;width:160px;padding:16px">被裁剪确认</button><script>let n=0;for(const k of ['pointerdown','pointerup','click'])addEventListener(k,e=>{if(k==='click')stats.textContent='被裁剪按钮点击次数：'+(++n);navigator.sendBeacon('/event',JSON.stringify({role:'CHILD',kind:k,count:n,trusted:e.isTrusted,page_ms:performance.timeOrigin+performance.now()}))})</script>'''
  elif self.path=='/events':
   raw=json.dumps([json.loads(s) for s in (folder/'clip-events.jsonl').read_text(encoding='utf-8').splitlines()] if (folder/'clip-events.jsonl').exists() else [],ensure_ascii=False).encode();self.respond(raw,'application/json');return
  else:self.send_error(404);return
  raw=('<!doctype html><meta charset="utf-8"><style>body{font:18px system-ui;margin:12px;background:#edf6ed;color:#183e2b}h1,h2{font-size:21px}</style>'+body).encode();self.respond(raw,'text/html; charset=utf-8')
 def respond(self,raw,kind):
  self.send_response(200);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  event=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));event.update(observed_ms=time.time()*1000,host=self.headers.get('Host'))
  with (folder/'clip-events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(event,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'clip-address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8');server.serve_forever()
