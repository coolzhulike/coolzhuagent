"""普通HTML测试软件及真实网页事件；不模拟模型、派发或授权回执。"""
import pathlib,json,time,threading
from http.server import ThreadingHTTPServer,BaseHTTPRequestHandler
r=pathlib.Path(__file__).resolve().parent;lock=threading.Lock()
def log(data):
 data['server_ms']=time.time_ns()//1000000
 with lock:
  with (r/'page-events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(data,ensure_ascii=False)+'\n')
common='''const doc=crypto.randomUUID();let held=false,orphanDown=0,orphanUp=0,firstMove=null;
function post(data){navigator.sendBeacon('/events',new Blob([JSON.stringify({...data,doc,url:location.href,time:performance.timeOrigin+performance.now(),dpr:devicePixelRatio,scale:visualViewport.scale,pageY:scrollY,screen:[screen.width,screen.height,screen.availLeft,screen.availTop]})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','pointermove','input','change'])document.addEventListener(kind,e=>{
if(kind==='pointerdown'){if(held)orphanDown++;held=true}if(kind==='pointerup'){if(!held)orphanUp++;held=false}
if(kind==='pointermove'){if(firstMove!==null)return;firstMove=e.buttons}
let box=e.target.getBoundingClientRect();post({kind,trusted:e.isTrusted,buttons:e.buttons,id:e.target.id,x:e.clientX,y:e.clientY,rect:[box.x,box.y,box.width,box.height],firstMove,orphanDown,orphanUp});},true);
window.addEventListener('pagehide',()=>post({kind:'pagehide',held,orphanDown,orphanUp}));post({kind:'load'});
function report(){post({kind:'report',dpr:devicePixelRatio,scale:visualViewport.scale});let s=document.querySelector('#metrics');if(s)s.textContent='DPR='+devicePixelRatio+' SCALE='+visualViewport.scale+' pageY='+scrollY;}
window.addEventListener('resize',report);report();'''
def page(title,content,script=''):
 return '<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>'+title+'</title><style>body{font:22px sans-serif;padding:24px;background:#edf7f3;color:#123a28}button,input{font:22px sans-serif;padding:15px;margin:12px}label{display:block}#metrics{font:17px monospace}</style><h1>'+title+'</h1><p id="metrics"></p>'+content+'<script>'+common+script+'</script></html>'
class Handler(BaseHTTPRequestHandler):
 def do_GET(self):
  log({'kind':'request','path':self.path})
  path=self.path.split('?')[0]
  if path=='/source.html':body=page('慢弹窗源页-074','<button id="popup" onclick="window.open(\'/popup.html\',\'_blank\')">打开慢弹窗</button>')
  elif path=='/popup.html':
   time.sleep(120);log({'kind':'response_ready','path':path});body=page('旧弹窗晚响应-074','<p>LATE-074</p>')
  elif path=='/slow.html':
   body=page('加载中目标-074','<p>LOADING-074</p>');raw=body.encode();self.send_response(200);self.send_header('Content-Type','text/html;charset=utf-8');self.send_header('Content-Length',str(len(raw)+1));self.end_headers();self.wfile.write(raw);self.wfile.flush();log({'kind':'partial_sent','path':path});time.sleep(120)
   try:self.wfile.write(b' ');self.wfile.flush();log({'kind':'response_sent','path':path})
   except (BrokenPipeError,ConnectionResetError,ConnectionAbortedError):log({'kind':'disconnected','path':path})
   return
  elif path=='/explicit.html':body=page('模型接管目标-074','<button id="confirm" onclick="document.querySelector(\'#result\').textContent=\'EXPLICIT-PASSED-074\'">确认新页面</button><p id="result">尚未确认</p>')
  elif path=='/form.html':body=page('缩放表单-074','<label>验收姓名<input id="name" aria-label="验收姓名"></label><div style="height:700px"></div><label><input id="agree" type="checkbox">确认验收</label><button id="submit">提交本地表单</button><p id="result">尚未提交</p>',"submit.onclick=()=>result.textContent=document.querySelector('#name').value==='SWE2-074'&&agree.checked?'FORM-PASSED-074':'FORM-FAILED-074'")
  elif path=='/replace.html':body=page('同步节点替换-074','<button id="replace">按下时替换节点</button><p id="result">尚未替换</p>',"replace.onpointerdown=()=>{replace.outerHTML='<p id=\"replacement\">NODE-REPLACED-074</p>';result.textContent='替换已完成';post({kind:'node_replaced',held,orphanDown,orphanUp})}")
  elif path=='/micro.html':body=page('自然文档替换-074','<button id="navigate">按下时导航</button>',"navigate.onpointerdown=()=>location.assign('/micro-target.html')")
  elif path=='/micro-target.html':body=page('文档替换目标-074','<p>MICRO-TARGET-074</p>')
  else:self.send_error(404);return
  self.send_response(200);self.send_header('Content-Type','text/html;charset=utf-8');self.send_header('Cache-Control','no-store');self.end_headers()
  try:self.wfile.write(body.encode());self.wfile.flush();log({'kind':'response_sent','path':path})
  except (BrokenPipeError,ConnectionResetError,ConnectionAbortedError):log({'kind':'disconnected','path':path})
 def do_POST(self):
  if self.path!='/events':self.send_error(404);return
  log(json.loads(self.rfile.read(min(int(self.headers.get('Content-Length',0)),8192))))
  self.send_response(204);self.end_headers()
 def log_message(self,*args):pass
s=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(r/'server.json').write_text(json.dumps({'port':s.server_port,'base':f'http://127.0.0.1:{s.server_port}'}));print(s.server_port,flush=True);s.serve_forever()
