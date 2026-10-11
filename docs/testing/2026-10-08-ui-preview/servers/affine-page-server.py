"""多层真实跨来源仿射页面。只记录可信浏览器事件，不伪造模型响应。"""
import http.server,json,pathlib,secrets,threading,time,urllib.parse
import argparse
args=argparse.ArgumentParser(description="真实页面验收服务，不模拟模型或宿主回执")
args.add_argument("--output-dir",default="tmp/browser-affine-acceptance")
folder=pathlib.Path(args.parse_args().output_dir).resolve()
folder.mkdir(parents=True,exist_ok=True)
if (folder/"stop-server").exists():raise SystemExit("输出目录已有停止标记，请选择新的tmp目录")
code='JADE-'+str(secrets.randbelow(9000)+1000)
receipt='BAMBOO-'+str(secrets.randbelow(9000)+1000)
lock=threading.Lock()
style='<style>body{font:17px system-ui;margin:12px;background:#eef6ef;color:#163b2b}h1,h2{font-size:20px;margin:8px 0}p{margin:8px 0}input,button{font:inherit;padding:10px;max-width:90%;box-sizing:border-box}iframe{border:2px solid #487a61;box-sizing:border-box;background:white}output{display:block;padding:8px;background:#cfe7d5}</style>'
def script(role):
 return '<script>const role='+json.dumps(role)+''';function log(kind,e,data={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({role,kind,trusted:e.isTrusted,page_ms:performance.timeOrigin+performance.now(),...data})],{type:'application/json'}));}for(const kind of ['pointerdown','pointerup','click','keydown','input','wheel'])document.addEventListener(kind,e=>log(kind,e,{target:e.target.id||e.target.tagName,key:e.key,value:e.target.value,scrollX,scrollY}));addEventListener('scroll',e=>log('scroll',e,{scrollX,scrollY}));</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  parsed=urllib.parse.urlparse(self.path);port=self.server.server_port
  query=urllib.parse.parse_qs(parsed.query)
  if parsed.path=='/parent.html':
   mode=query.get('mode',['normal'])[0]
   transform='perspective(500px) rotateY(18deg)' if mode=='perspective' else 'rotate(3deg) skewX(4deg)'
   body=f'''<title>AFFINE-PARENT</title><h1>竹林协作任务</h1><p>先在两层倾斜任务区完成验证和翻页，再把回执填入此处完成整轮。不要操作其它页面。</p><form id="finish"><label>最终回执 <input id="final" aria-label="最终回执" autocomplete="off"></label><button id="finish-button">完成整轮</button></form><output id="final-state">整轮未完成</output><div style="margin:30px 42px"><iframe title="旋转中间任务区" style="width:min(410px,100%);height:370px;transform-origin:top left;transform:{transform}" src="http://localhost:{port}/middle.html"></iframe></div>'''+script('PARENT')+f'''<script>document.querySelector('#finish').onsubmit=e=>{{e.preventDefault();const ok=document.querySelector('#final').value==={json.dumps(receipt)};document.querySelector('#final-state').textContent=ok?'整轮任务完成，回执正确':'回执不正确';log('final-submitted',e,{{accepted:ok}})}};</script>'''
   if mode=='cover':body+='<div id="cover" style="position:fixed;inset:200px 0 0;background:#f5ead8e8;z-index:99;padding:25px">任务区已被父页面覆盖，不能点击下层控件。</div>'
  elif parsed.path=='/middle.html':
   body=f'''<title>AFFINE-MIDDLE</title><h2>中间任务区</h2><p>下方第二层为验证任务。</p><div style="margin:25px 18px"><iframe title="倾斜验证任务" style="width:calc(100% - 20px);height:245px;transform-origin:top left;transform:rotate(-5deg) skewX(-6deg)" src="http://127.0.0.1:{port}/leaf.html"></iframe></div>'''+script('MIDDLE')
  elif parsed.path=='/leaf.html':
   body=f'''<title>AFFINE-LEAF</title><h2>第一步：验证</h2><p>本轮校验码：<strong>{code}</strong></p><form id="verify"><label>校验码<input id="verification" aria-label="校验码" autocomplete="off"></label></form><output id="status">输入后按 Enter 验证</output><div style="height:620px"></div><button id="continue" disabled>第二步：获取回执</button><p>验证成功后向下滚动，点击获取回执。</p>'''+script('LEAF')+f'''<script>document.querySelector('#verify').onsubmit=e=>{{e.preventDefault();const ok=document.querySelector('#verification').value==={json.dumps(code)};document.querySelector('#status').textContent=ok?'验证成功，请向下滚动获取回执':'校验码错误';document.querySelector('#continue').disabled=!ok;log('verified',e,{{accepted:ok}})}};document.querySelector('#continue').onclick=e=>{{log('navigate-requested',e);location.href='/receipt.html'}};</script>'''
  elif parsed.path=='/receipt.html':
   body=f'<title>AFFINE-RECEIPT</title><h2>第二步：回执</h2><p>回执码：<strong>{receipt}</strong></p><output>验证与翻页完成。将此回执填入父页面的最终回执框。</output>'+script('RECEIPT')
  else:self.send_error(404);return
  self.respond(('<!doctype html><meta charset="utf-8">'+style+body).encode(),'text/html; charset=utf-8')
 def respond(self,raw,kind):
  self.send_response(200);self.send_header('Content-Type',kind);self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  if self.path!='/event':self.send_error(404);return
  event=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096)));event.update(observed_ms=time.time()*1000,host=self.headers.get('Host'))
  with lock:
   with (folder/'events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps(event,ensure_ascii=False)+'\n')
  self.send_response(204);self.end_headers()
 def log_message(self,*args):pass
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}','code':code,'receipt':receipt}),encoding='utf-8')
def stop():
 while not (folder/'stop-server').exists():time.sleep(.3)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start()
server.serve_forever()
