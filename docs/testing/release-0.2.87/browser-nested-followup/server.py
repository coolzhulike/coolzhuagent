"""真实嵌套网页与同名兄弟控件；仅记录浏览器可信事件，不生成模型响应。"""
import http.server,json,pathlib,time,urllib.parse
folder=pathlib.Path(__file__).resolve().parent
style='<style>body{font:18px system-ui;margin:12px;background:#edf6ed;color:#183e2b}h1,h2{font-size:21px;margin:8px 0}button{font:inherit;padding:14px}iframe{border:2px solid #467763;box-sizing:border-box}p{margin:8px 0}</style>'
script='''<script>const role=ROLE;let count=0;function log(kind,e){navigator.sendBeacon('/event',new Blob([JSON.stringify({role,kind,trusted:e.isTrusted,page_ms:performance.timeOrigin+performance.now(),count})],{type:'application/json'}));}for(const kind of ['pointerdown','pointerup','click'])document.addEventListener(kind,e=>log(kind,e));document.querySelector('button').addEventListener('click',e=>{count++;document.querySelector('output').textContent=role+' 点击次数：'+count;log('accepted',e);parent.postMessage({role,count},'*')});</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  parsed=urllib.parse.urlparse(self.path);port=self.server.server_port
  if parsed.path=='/parent.html':
   body=f'''<title>NESTED-PARENT</title><h1>NESTED-PARENT</h1><button>确认</button><output>父页点击次数：0</output><p id="result">LEFT-LEAF：0；RIGHT-LEAF：0</p><iframe title="嵌套中间页" style="width:100%;height:330px" src="http://localhost:{port}/middle.html"></iframe>'''+script.replace('ROLE',json.dumps('PARENT'))+'''<script>const seen={'LEFT-LEAF':0,'RIGHT-LEAF':0};addEventListener('message',e=>{if(e.data.role in seen){seen[e.data.role]=e.data.count;result.textContent='LEFT-LEAF：'+seen['LEFT-LEAF']+'；RIGHT-LEAF：'+seen['RIGHT-LEAF']}})</script>'''
  elif parsed.path=='/middle.html':
   body=f'''<title>NESTED-MIDDLE</title><h2>NESTED-MIDDLE</h2><button>确认</button><output>中间页点击次数：0</output><div style="display:flex;gap:10px;margin-top:10px"><iframe title="重复子页" style="width:49%;height:215px" src="http://127.0.0.1:{port}/leaf.html?side=LEFT"></iframe><iframe title="重复子页" style="width:49%;height:215px" src="http://127.0.0.1:{port}/leaf.html?side=RIGHT"></iframe></div>'''+script.replace('ROLE',json.dumps('MIDDLE'))+'''<script>addEventListener('message',e=>parent.postMessage(e.data,'*'))</script>'''
  elif parsed.path=='/leaf.html':
   role='RIGHT-LEAF' if urllib.parse.parse_qs(parsed.query).get('side')==['RIGHT'] else 'LEFT-LEAF'
   body=f'<title>{role}</title><h2>{role}</h2><button>确认</button><p><output>{role} 点击次数：0</output></p>'+script.replace('ROLE',json.dumps(role))
  elif parsed.path=='/events':
   raw=json.dumps([json.loads(s) for s in (folder/'events.jsonl').read_text(encoding='utf-8').splitlines()] if (folder/'events.jsonl').exists() else [],ensure_ascii=False).encode();self.respond(raw,'application/json');return
  else:self.send_error(404);return
  self.respond(('<!doctype html><meta charset="utf-8">'+style+body).encode(),'text/html; charset=utf-8')
 def respond(self,raw,kind):
  self.send_response(200);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  event=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));event.update(observed_ms=time.time()*1000,host=self.headers.get('Host'))
  with (folder/'events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(event,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8')
server.serve_forever()
