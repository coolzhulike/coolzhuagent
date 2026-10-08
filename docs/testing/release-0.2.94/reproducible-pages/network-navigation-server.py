"""真实网页在pointerdown触发跨来源HTTP导航，记录实际事件时序；不伪造模型或宿主回执。"""
import http.server,json,pathlib,threading,time
import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output-dir',type=pathlib.Path,required=True)
folder=parser.parse_args().output_dir.resolve()
folder.mkdir(parents=True,exist_ok=True)
if (folder/'stop-server').exists():raise SystemExit('输出目录含旧停止标记，请使用新的tmp目录')
lock=threading.Lock()
style='<style>body{font:18px system-ui;margin:24px;background:#edf6ee;color:#163b2b}button{font:inherit;padding:24px;margin-top:30px}</style>'
def page(role):
 logger="""<script>
let held=false;
function record(kind,e,data={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,role:document.body.dataset.role,trusted:e.isTrusted,held,url:location.href,page_ms:performance.timeOrigin+performance.now(),target:e.target?.id||e.target?.tagName,...data})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])document.addEventListener(kind,e=>{if(kind==='pointerdown')held=true;if(kind==='pointerup')held=false;record(kind,e);},true);
addEventListener('load',e=>record('loaded',e));addEventListener('beforeunload',e=>record('beforeunload',e));
</script>"""
 if role=='AFTER':return '<!doctype html><meta charset="utf-8"><title>NETWORK-AFTER</title>'+style+'<body data-role="AFTER"><h1>跨来源新文档已加载</h1><p>只观察，不继续点击或输入。原动作应已释放。</p><button id="new-button">新文档按钮，禁止补点</button>'+logger+'</body>'
 destination='http://localhost:'+str(server.server_port)+'/after.html'
 return '<!doctype html><meta charset="utf-8"><title>NETWORK-BEFORE</title>'+style+'<body data-role="BEFORE"><h1>跨来源按下导航验收</h1><p>点击一次，pointerdown触发跨来源HTTP导航；导航后不得追加点击、输入或重放。</p><button id="navigate-button">按下一次并跨来源导航</button>'+logger+"<script>document.querySelector('#navigate-button').addEventListener('pointerdown',e=>{if(!e.isTrusted)return;record('navigation-started',e,{destination:"+json.dumps(destination)+"});location.replace("+json.dumps(destination)+");});</script></body>"
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  if self.path=='/before.html':body=page('BEFORE')
  elif self.path=='/after.html':body=page('AFTER')
  else:self.send_error(404);return
  event={'kind':'http-get','path':self.path,'host':self.headers.get('Host'),'observed_ms':time.time()*1000}
  with lock:
   with (folder/'events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(event)+'\n')
  raw=body.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  event=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));event['observed_ms']=time.time()*1000
  with lock:
   with (folder/'events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(event,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8')
def stop():
 while not (folder/'stop-server').exists():time.sleep(.3)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start();server.serve_forever()
