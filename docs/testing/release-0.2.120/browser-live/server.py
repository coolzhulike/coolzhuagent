"""普通动态表单：非目标区域持续刷新，真实输入事件独立记录；不伪造模型/宿主响应。"""
from pathlib import Path
import http.server,json,secrets,threading,time
p=Path(__file__).resolve().parent;p.mkdir(exist_ok=True);assert not (p/'stop-server').exists()
codes=['BAMBOO-'+str(secrets.randbelow(9000)+1000),'JADE-'+str(secrets.randbelow(9000)+1000),'SCROLL-'+str(secrets.randbelow(9000)+1000)]
lock=threading.Lock()
html='''<!doctype html><meta charset="utf-8"><title>竹林动态订单验收</title>
<style>body{font:17px system-ui;margin:12px;background:#edf6ee;color:#143d2b}input,button{font:inherit;padding:12px;max-width:95%;box-sizing:border-box}#live{height:65px;overflow:hidden;background:#d4e6d7;padding:8px;margin-bottom:12px}output{display:block;margin:12px 0}button{margin-top:16px}</style>
<h1>竹林动态订单</h1><aside id="live" aria-label="无须操作的行情区"><span id="ticker"></span><span id="news"></span></aside>
<main id="work"></main><output id="final">整单未完成</output>
<script>
const codes=CODES;let stage=0,revision=0,accepted=false;
function log(kind,e={},extra={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,trusted:e.isTrusted===true,page_ms:performance.timeOrigin+performance.now(),target:e.target?.id||e.target?.tagName,stage,revision,...extra})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])document.addEventListener(kind,e=>log(kind,e,{key:e.key,value:e.target.value}),true);
function render(){accepted=false;document.querySelector('#work').innerHTML=`<h2>第${stage+1}步 / 共3步</h2><p>本步订单码：<strong>${codes[stage]}</strong></p><p>填写“订单码”后按Enter，验证通过再点“下一步”。最后一步按Enter完成。</p><form id="verify"><label>订单码<input id="code" aria-label="订单码" autocomplete="off"></label></form><output id="status">等待验证</output>${stage<2?'<button id="next" disabled>下一步</button>':''}`;
document.querySelector('#verify').onsubmit=e=>{e.preventDefault();accepted=document.querySelector('#code').value===codes[stage];document.querySelector('#status').textContent=accepted?'本步验证通过':'订单码错误';log('verified',e,{accepted});if(stage<2)document.querySelector('#next').disabled=!accepted;else if(accepted){document.querySelector('#final').textContent='整单已完成，三步均通过';log('final-submitted',e,{accepted:true});}};
if(stage<2)document.querySelector('#next').onclick=e=>{if(!accepted)return;log('advance',e);stage++;render();};}
render();log('loaded');setInterval(()=>{revision++;document.querySelector('#ticker').textContent='行情刷新 #'+revision+' 价格 '+(200+revision%11);const node=document.createElement('small');node.textContent=' / 公告 '+revision;document.querySelector('#news').replaceChildren(node);if(revision%100===0)log('non-target-repaint');},100);
</script>'''.replace('CODES',json.dumps(codes))
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  if self.path!='/orders.html':self.send_error(404);return
  raw=html.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  data=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));data['observed_ms']=time.time()*1000
  with lock:
   with (p/'events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(data,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(p/'address.json').write_text(json.dumps({'url':f'http://127.0.0.1:{server.server_port}/orders.html','codes':codes}),encoding='utf-8')
def stop():
 while not (p/'stop-server').exists():time.sleep(.3)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start();server.serve_forever()
