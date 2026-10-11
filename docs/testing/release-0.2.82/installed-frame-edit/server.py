"""真实同源Frame表单与事件记录，仅记录实际浏览器输入，不代造模型结果。"""
from http.server import ThreadingHTTPServer,BaseHTTPRequestHandler
from pathlib import Path
from urllib.parse import urlsplit
import json,threading,time
folder=Path(__file__).resolve().parent
events=[]
lock=threading.Lock()
common='<meta charset="utf-8"><style>body{margin:12px;font:18px sans-serif;background:#edf5ed;color:#17291e}h1{font-size:22px}button,input{font:18px sans-serif;padding:8px;border:2px solid #22563e;border-radius:6px;box-sizing:border-box}input{width:100%}p{margin:10px 0}</style>'
report="""function report(type,detail={}){fetch('/event',{method:'POST',body:JSON.stringify({case:caseName,type,at_ms:performance.timeOrigin+performance.now(),...detail}),keepalive:true});}
for(const type of ['pointerdown','pointerup','click','input','keydown','keyup','focusin'])document.addEventListener(type,e=>report(type,{target:e.target.id||e.target.tagName,isTrusted:e.isTrusted,key:e.key||null,value:e.type==='input'?e.target.value:null}),true);report('ready');"""
parent=f'''<!doctype html><html lang="zh">{common}<title>082 Frame文本与键盘</title><h1>082 Frame文本与键盘</h1>
<p id="status">子输入：空；子提交次数：0；父输入：空</p>
<iframe id="child" title="同源Frame表单" src="/child.html" style="width:100%;height:200px;border:2px solid #22563e"></iframe>
<p><label>父页面输入框<input id="parent-input" aria-label="父页面输入框"></label></p>
<p><button id="move-focus">移走子页面焦点</button> <button id="replace">切换子页面</button></p>
<script>const caseName='EDIT-PARENT';{report}const parentInput=document.getElementById('parent-input');let childValue='',submitted=0;
function render(){{document.getElementById('status').textContent='子输入：'+(childValue||'空')+'；子提交次数：'+submitted+'；父输入：'+(parentInput.value||'空');}}
window.addEventListener('message',e=>{{if(e.source===child.contentWindow&&e.origin===location.origin&&e.data?.kind==='child-value'){{childValue=e.data.value;submitted=e.data.submitted;render();}}}});
parentInput.addEventListener('input',render);document.getElementById('move-focus').addEventListener('click',()=>{{parentInput.focus();report('parent-focused');}});
replace.addEventListener('click',()=>{{child.src='/child-next.html';report('child-navigation');}});</script></html>'''
def child(name):
 return f'''<!doctype html><html lang="zh">{common}<title>{name}</title><form id="form">
 <p><label>Frame输入框<input id="frame-text" aria-label="Frame输入框" autocomplete="off"></label></p>
 <button type="submit" id="submit">提交子表单</button><p id="result">子提交次数：0</p></form>
 <script>const caseName={json.dumps(name)};{report}let submitted=0;const field=document.getElementById('frame-text');
 function publish(){{parent.postMessage({{kind:'child-value',value:field.value,submitted}},location.origin);}}
 field.addEventListener('input',publish);form.addEventListener('submit',e=>{{e.preventDefault();submitted++;result.textContent='子提交次数：'+submitted;report('child-submit',{{value:field.value,count:submitted,isTrusted:e.isTrusted}});publish();}});</script></html>'''
pages={'/edit.html':parent,'/child.html':child('EDIT-CHILD'),'/child-next.html':child('EDIT-REPLACED')}
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
