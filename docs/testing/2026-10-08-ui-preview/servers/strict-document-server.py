"""真实网页在可信pointerdown内同步替换文档；不控制宿主、不模拟模型或输入回执。"""
import http.server, json, pathlib, threading, time

import argparse
args=argparse.ArgumentParser(description="真实页面验收服务，不模拟模型或宿主回执")
args.add_argument("--output-dir",default="tmp/browser-strict-acceptance")
folder=pathlib.Path(args.parse_args().output_dir).resolve()
folder.mkdir(parents=True,exist_ok=True)
if (folder/"stop-server").exists():raise SystemExit("输出目录已有停止标记，请选择新的tmp目录")
lock = threading.Lock()
style = '<style>body{font:18px system-ui;margin:24px;background:#eef6ef;color:#163b2b}button{font:inherit;padding:24px;margin-top:30px}</style>'
logger = '''<script>
function record(kind,e,data={}) {navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,role:document.body.dataset.role,trusted:e.isTrusted,url:location.href,page_ms:performance.timeOrigin+performance.now(),target:e.target?.id||e.target?.tagName,...data})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])document.addEventListener(kind,e=>record(kind,e),true);
addEventListener('load',e=>record('loaded',e));
</script>'''
after = '<!doctype html><meta charset="utf-8"><title>STRICT-AFTER</title>'+style+'<body data-role="AFTER"><h1>新文档：原按钮已撤销</h1><p>只允许原控制器释放已按下的鼠标；禁止继续点击、输入或重放。</p><button id="after-button">新文档按钮，不应被旧动作点击</button>'+logger+'</body>'
before = '<!doctype html><meta charset="utf-8"><title>STRICT-BEFORE</title>'+style+'<body data-role="BEFORE"><h1>按下期间替换文档边界验收</h1><p>点击下方按钮一次；页面会在pointerdown处理期间同步替换文档，并变更同源地址。</p><button id="replace-button">按下一次并替换文档</button>'+logger+'''<script>
document.querySelector('#replace-button').addEventListener('pointerdown',e=>{
 if(!e.isTrusted)return;
 record('replacement-started',e);
 document.open(); document.write('''+json.dumps(after).replace('</','<\\/')+'''); document.close();
 history.replaceState(null,'','/after.html?source=pointerdown');
 record('replacement-finished',e,{replacement:'synchronous-document-open'});
});
</script></body>'''

class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  if self.path.startswith('/before.html'): body=before
  elif self.path.startswith('/after.html'): body=after
  else:self.send_error(404);return
  raw=body.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  raw=self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096))
  event=json.loads(raw);event['observed_ms']=time.time()*1000
  with lock:
   with (folder/'events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(event,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
 def log_message(self,*args):pass

server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8')
def stop():
 while not (folder/'stop-server').exists():time.sleep(.3)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start()
server.serve_forever()
