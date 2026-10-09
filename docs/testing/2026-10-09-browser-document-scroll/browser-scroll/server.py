"""普通双文档网页：滚动事件插入外层回执，真实输入随机订单码；无模型响应夹具。"""
from pathlib import Path
import http.server,json,secrets,threading,time
p=Path(__file__).resolve().parent;assert not (p/'stop-server').exists()
orders=[{'item':item,'code':prefix+'-'+str(secrets.randbelow(9000)+1000),'price':secrets.randbelow(600)+101,'quantity':secrets.randbelow(4)+2} for item,prefix in [('竹剑','BAMBOO'),('玉佩','JADE')]]
lock=threading.Lock()
child='''<!doctype html><meta charset="utf-8"><title>ITEM订单文档</title><style>body{font:16px system-ui;background:#ecf6ee;color:#163c2b;margin:10px}h2{font-size:18px;margin:4px}#space{height:245px}input{font:inherit;padding:9px;width:92%;box-sizing:border-box}p{margin:4px}output{display:block;margin:5px}</style><h2>ITEM订单</h2><p>请在本订单文档向下滚动，填写底部订单码后按Enter。</p><div id="space"></div><p>单价 PRICE，数量 QUANTITY</p><p>订单码：<strong>CODE</strong></p><form id="verify"><label>ITEM订单码<input id="code" aria-label="ITEM订单码" autocomplete="off"></label></form><output id="status">订单尚未验证</output><script>
const order=ORDER;function log(kind,e={},extra={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,trusted:e.isTrusted===true,page_ms:performance.timeOrigin+performance.now(),target:e.target?.id||e.target?.tagName,item:order.item,...extra})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown','wheel'])document.addEventListener(kind,e=>log(kind,e,{key:e.key,value:e.target.value}),true);
let noted=false;window.addEventListener('scroll',e=>{log('document-scroll',e,{page_y:scrollY});if(!noted&&scrollY>0){noted=true;parent.postMessage({kind:'scroll-receipt',item:order.item},'*');}});
document.querySelector('#verify').onsubmit=e=>{e.preventDefault();let accepted=document.querySelector('#code').value===order.code;document.querySelector('#status').textContent=accepted?order.item+'订单验证通过':'订单码错误';log('verified',e,{accepted});if(accepted)parent.postMessage({kind:'order-receipt',item:order.item,price:order.price,quantity:order.quantity},'*');};log('loaded');</script>'''
outer='''<!doctype html><meta charset="utf-8"><title>竹林双文档验收</title><style>body{font:15px system-ui;background:#f0f6ef;color:#163c2b;margin:8px}h1{font-size:19px;margin:3px}aside{height:20px;overflow:hidden}#scrolls{height:26px;overflow:hidden}#rows{min-height:42px}iframe{width:calc(100% - 5px);height:171px;border:1px solid #60866d;margin-top:3px}p{margin:2px}output{display:block;font-weight:bold}</style><h1>双文档订单与滚动回执</h1><aside id="live">行情正在刷新</aside><div id="scrolls"></div><section id="rows">尚无订单回执</section><output id="final">双文档订单尚未完成</output><iframe src="ORIGIN/one.html" title="竹剑订单"></iframe><iframe src="ORIGIN/two.html" title="玉佩订单"></iframe><script>
const source='ORIGIN';let revision=0,receipts=new Map(),scrolls=new Set();window.addEventListener('message',e=>{let frames=[...document.querySelectorAll('iframe')];if(e.origin!==source||!frames.some(f=>f.contentWindow===e.source))return;let r=e.data;if(!['竹剑','玉佩'].includes(r.item))return;if(r.kind==='scroll-receipt'&&!scrolls.has(r.item)){scrolls.add(r.item);let p=document.createElement('p');p.textContent=r.item+'文档已滚动，新增独立回执';document.querySelector('#scrolls').appendChild(p);navigator.sendBeacon('/event',new Blob([JSON.stringify({kind:'ax-prefix-inserted',item:r.item})],{type:'application/json'}));}if(r.kind==='order-receipt'){receipts.set(r.item,r);document.querySelector('#rows').innerHTML=[...receipts.values()].map(v=>`<p>${v.item}已验证：单价 ${v.price}；数量 ${v.quantity}</p>`).join('');if(receipts.size===2)document.querySelector('#final').textContent='双文档订单已完成，两项均通过';}});setInterval(()=>{revision++;document.querySelector('#live').textContent='无关行情 #'+revision;if(revision%100===0)navigator.sendBeacon('/event',new Blob([JSON.stringify({kind:'non-target-repaint',revision})],{type:'application/json'}));},100);</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  if self.server.is_child and self.path in ('/one.html','/two.html'):
   order=orders[0 if self.path=='/one.html' else 1];text=child.replace('ORDER',json.dumps(order,ensure_ascii=False))
   for key in ('item','code','price','quantity'):text=text.replace(key.upper(),str(order[key]))
   raw=text.encode()
  elif not self.server.is_child and self.path=='/scroll.html':raw=outer.replace('ORIGIN',child_origin).encode()
  else:self.send_error(404);return
  self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  data=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));data.update(observed_ms=time.time()*1000,origin_port=self.server.server_port)
  with lock:
   with (p/'events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(data,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);server.is_child=False
secondary=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler);secondary.is_child=True;child_origin=f'http://127.0.0.1:{secondary.server_port}'
(p/'address.json').write_text(json.dumps({'url':f'http://127.0.0.1:{server.server_port}/scroll.html','child_origin':child_origin,'orders':orders,'total':sum(o['price']*o['quantity'] for o in orders)},ensure_ascii=False),encoding='utf-8')
def stop():
 while not (p/'stop-server').exists():time.sleep(.3)
 server.shutdown();secondary.shutdown()
threading.Thread(target=stop,daemon=True).start();threading.Thread(target=secondary.serve_forever,daemon=True).start();server.serve_forever()
