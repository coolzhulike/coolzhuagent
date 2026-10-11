"""普通双来源订单页：真实页面和输入事件，非模型/工具响应夹具。"""
from pathlib import Path
import http.server,json,secrets,threading,time
p=Path(__file__).resolve().parent;assert not (p/'stop-server').exists()
orders=[{'item':item,'code':prefix+'-'+str(secrets.randbelow(9000)+1000),'price':secrets.randbelow(800)+101,'quantity':secrets.randbelow(6)+2} for item,prefix in [('竹剑','BAMBOO'),('玉佩','JADE'),('卷轴','SCROLL')]]
lock=threading.Lock()
child='''<!doctype html><meta charset="utf-8"><title>玉石订单操作</title><style>body{font:17px system-ui;background:#ecf6ee;color:#163c2b;margin:12px}input,button{font:inherit;padding:11px;max-width:95%;box-sizing:border-box}output{display:block;margin:12px 0}button{margin-top:12px}</style><main id="work"></main><output id="final">整单未完成</output><script>
const orders=ORDERS;let stage=0,accepted=false;
function log(kind,e={},extra={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,trusted:e.isTrusted===true,page_ms:performance.timeOrigin+performance.now(),target:e.target?.id||e.target?.tagName,stage,...extra})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])document.addEventListener(kind,e=>log(kind,e,{key:e.key,value:e.target.value}),true);
function render(){accepted=false;let o=orders[stage];document.querySelector('#work').innerHTML=`<h2>第${stage+1}项 / 共3项：${o.item}</h2><p>本项单价 ${o.price}，数量 ${o.quantity}</p><p>本步订单码：<strong>${o.code}</strong></p><p>填写“订单码”后按Enter，通过再点下一步。</p><form id="verify"><label>订单码<input id="code" aria-label="订单码" autocomplete="off"></label></form><output id="status">等待验证</output>${stage<2?'<button id="next" disabled>下一步</button>':''}`;
document.querySelector('#verify').onsubmit=e=>{e.preventDefault();accepted=document.querySelector('#code').value===o.code;document.querySelector('#status').textContent=accepted?'本步验证通过':'订单码错误';log('verified',e,{accepted});if(accepted)parent.postMessage({receipt:stage,item:o.item,price:o.price,quantity:o.quantity},'*');if(stage<2)document.querySelector('#next').disabled=!accepted;else if(accepted){document.querySelector('#final').textContent='整单已完成，三项均通过';log('final-submitted',e,{accepted:true});}};
if(stage<2)document.querySelector('#next').onclick=e=>{if(!accepted)return;log('advance',e);stage++;render();};}
render();log('loaded');</script>'''.replace('ORDERS',json.dumps(orders,ensure_ascii=False))
outer='''<!doctype html><meta charset="utf-8"><title>竹林订单与账本</title><style>body{font:16px system-ui;background:#f0f6ef;color:#163c2b;margin:10px}h1{font-size:22px;margin:8px}aside{height:42px;overflow:hidden;background:#d2e6d4;padding:5px}iframe{width:calc(100% - 12px);height:445px;border:2px solid #60866d;transform:skewY(1deg)}#ledger{padding:8px;background:#dcebdd;min-height:62px}p{margin:4px}</style><h1>竹林订单与账本</h1><aside id="live">行情正在刷新</aside><section id="ledger"><b>已验证订单回执</b><div id="rows">尚无回执</div></section><iframe src="CHILD" title="跨来源订单表单"></iframe><script>
const source='ORIGIN';let revision=0,receipts=new Map();window.addEventListener('message',e=>{if(e.origin!==source||e.source!==document.querySelector('iframe').contentWindow)return;let r=e.data;if(!Number.isInteger(r.receipt)||r.receipt<0||r.receipt>2)return;receipts.set(r.receipt,r);document.querySelector('#rows').innerHTML=[...receipts].sort((a,b)=>a[0]-b[0]).map(([k,v])=>`<p>第${k+1}项已验证：${v.item}；单价 ${v.price}；数量 ${v.quantity}</p>`).join('');});setInterval(()=>{revision++;document.querySelector('#live').textContent='无须操作行情 #'+revision+' / 公告 '+(revision%11);if(revision%100===0)navigator.sendBeacon('/event',new Blob([JSON.stringify({kind:'non-target-repaint',revision})],{type:'application/json'}));},100);</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  if self.server.is_child and self.path=='/order.html':raw=child.encode()
  elif not self.server.is_child and self.path=='/ledger.html':raw=outer.replace('CHILD',child_origin+'/order.html').replace('ORIGIN',child_origin).encode()
  else:self.send_error(404);return
  self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  data=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));data['observed_ms']=time.time()*1000;data['origin_port']=self.server.server_port
  with lock:
   with (p/'events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(data,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);server.is_child=False
secondary=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);secondary.is_child=True;child_origin=f'http://127.0.0.1:{secondary.server_port}'
(p/'address.json').write_text(json.dumps({'url':f'http://127.0.0.1:{server.server_port}/ledger.html','child_origin':child_origin,'orders':orders,'total':sum(o['price']*o['quantity'] for o in orders)},ensure_ascii=False),encoding='utf-8')
def stop():
 while not (p/'stop-server').exists():time.sleep(.3)
 server.shutdown();secondary.shutdown()
threading.Thread(target=stop,daemon=True).start();threading.Thread(target=secondary.serve_forever,daemon=True).start();server.serve_forever()
