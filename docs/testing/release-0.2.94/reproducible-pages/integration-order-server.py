"""真实浏览器订单流程：计算结果输入、业务反馈和确认后提交；不伪造工具或模型回执。"""
import http.server,json,pathlib,threading,time
import argparse
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output-dir',type=pathlib.Path,required=True)
folder=parser.parse_args().output_dir.resolve()
folder.mkdir(parents=True,exist_ok=True)
if (folder/'stop-server').exists():raise SystemExit('输出目录含旧停止标记，请使用新的tmp目录')
lock=threading.Lock()
page='''<!doctype html><meta charset="utf-8"><title>竹林采购核验</title><style>body{font:18px system-ui;margin:22px;background:#edf6ee;color:#123e2c}input,button{font:inherit;padding:10px;margin:8px 0;max-width:100%;box-sizing:border-box}section{border:1px solid #497864;padding:12px;margin-top:15px}button{display:block}button[hidden]{display:none}</style><h1>竹林采购核验</h1><p>数量：42；单价：19；附加费用：17。</p><p>使用工作流的计算器结果填写总价，完成结算。</p><label for="total">计算器总价</label><input id="total" autocomplete="off"><button id="submit">核验并提交订单</button><section id="feedback" aria-live="polite">订单未完成</section><button id="delivery" hidden>确认包邮配送</button><script>
let confirmed=false;
function record(kind,e,data={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,trusted:e.isTrusted,target:e.target.id||e.target.tagName,page_ms:performance.timeOrigin+performance.now(),...data})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])document.addEventListener(kind,e=>record(kind,e),true);
document.querySelector('#submit').onclick=e=>{const total=document.querySelector('#total').value.trim();const feedback=document.querySelector('#feedback');if(total!==String(42*19+17)){feedback.textContent='金额不正确，请核对实际计算器结果';record('business-rejected',e,{reason:'total'});return;}if(!confirmed){feedback.textContent='金额正确；请先确认包邮配送，已保留总价。';document.querySelector('#delivery').hidden=false;record('business-rejected',e,{reason:'delivery',total});return;}feedback.textContent='订单核验通过；结算单：BAMBOO-ORDER-094；总价：'+total+'；配送：包邮';record('order-completed',e,{total,delivery:'free',receipt:'BAMBOO-ORDER-094'});};
document.querySelector('#delivery').onclick=e=>{confirmed=true;e.target.disabled=true;document.querySelector('#feedback').textContent='配送已确认，请再次提交订单。';record('delivery-confirmed',e);};
</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  if self.path!='/order.html':self.send_error(404);return
  raw=page.encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
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
threading.Thread(target=stop,daemon=True).start()
server.serve_forever()
