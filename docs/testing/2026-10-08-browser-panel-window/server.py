"""真实按下事件期间延长页面处理，供正常UI关闭右栏的时序采证。"""
import http.server,json,pathlib,threading,time
folder=pathlib.Path(__file__).resolve().parent
lock=threading.Lock()
page='''<!doctype html><meta charset="utf-8"><title>PANEL-INPUT-WINDOW</title>
<style>body{font:20px system-ui;background:#f3f7ee;color:#17452b;margin:28px}button{font:24px system-ui;padding:24px}</style>
<h1>面板关闭时序验收</h1><p>仅一次真实点击。页面按下处理持续1800毫秒，供正常关闭右栏。</p>
<button id="target">一次点击时序目标</button><p id="status">尚未点击</p>
<script>
function report(kind,e){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,page_ms:performance.timeOrigin+performance.now(),trusted:e?.isTrusted,target:e?.target?.id||e?.target?.tagName})],{type:'application/json'}));}
target.addEventListener('pointerdown',e=>{report('pointerdown',e);const end=performance.now()+1800;while(performance.now()<end){};report('down-handler-finished',e);});
addEventListener('pointerup',e=>report('pointerup',e));
addEventListener('click',e=>{report('click',e);document.querySelector('#status').textContent='真实点击完成';});
addEventListener('pagehide',e=>report('pagehide',e));
report('loaded');
</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  if self.path!='/window.html':self.send_error(404);return
  raw=page.encode();self.send_response(200);self.send_header('Content-Type','text/html;charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  event=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length',0)),4096)));event['observed_ms']=time.time()*1000
  with lock:
   with (folder/'events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(event,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8')
print(server.server_port,flush=True)
def stop():
 while not (folder/'stop-server').exists():time.sleep(.3)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start();server.serve_forever()
