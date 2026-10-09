"""真实同进程iframe滚动；中间步骤只改变自身视口，末端才更新页面文字。"""
import http.server,json,pathlib,threading,time
folder=pathlib.Path(__file__).resolve().parent
lock=threading.Lock()
parent='<!doctype html><meta charset="utf-8"><title>SAMEPROCESS-PARENT</title><style>body{font:18px system-ui;margin:18px;background:#edf6ee;color:#16432e}iframe{width:96%;height:230px;border:2px solid #365c42}</style><h1>同进程子文档滚动验收</h1><p>仅滚动下方子文档，父页保持原位。</p><iframe src="/child.html" title="SAMEPROCESS-CHILD"></iframe>'
child='<!doctype html><meta charset="utf-8"><title>SAMEPROCESS-CHILD</title><style>body{font:17px system-ui;margin:12px;background:#f8faf0;color:#16432e}section{height:960px;background:linear-gradient(#d9eddb,#bfd5bb)}</style><h2>子文档滚动任务</h2><p id="status">子滚动验收未完成</p><section aria-label="长文档区域"></section><p>子文档末端</p><script>\nfunction report(kind,data={}){navigator.sendBeacon(\'/event\',new Blob([JSON.stringify({kind,page_ms:performance.timeOrigin+performance.now(),left:scrollX,top:scrollY,height:innerHeight,...data})],{type:\'application/json\'}));}\naddEventListener(\'scroll\',e=>{report(\'child-scroll\',{trusted:e.isTrusted});if(scrollY+innerHeight>=document.scrollingElement.scrollHeight-3){document.querySelector(\'#status\').textContent=\'子滚动验收完成\';report(\'child-completed\',{trusted:e.isTrusted});}});\naddEventListener(\'wheel\',e=>report(\'child-wheel\',{trusted:e.isTrusted}));\n</script>'
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  body=parent if self.path=='/parent.html' else child if self.path=='/child.html' else None
  if body is None:self.send_error(404);return
  raw=body.encode();self.send_response(200);self.send_header('Content-Type','text/html;charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  e=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));e['observed_ms']=time.time()*1000
  with lock:
   with (folder/'events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(e,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8')
print(server.server_port,flush=True)
def stop():
 while not (folder/'stop-server').exists():time.sleep(.3)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start();server.serve_forever()
