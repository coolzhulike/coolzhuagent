"""真实三层跨来源页面：双反射、斜切、旁路同名控件及跨来源导航。"""
import argparse,http.server,json,pathlib,secrets,threading,time,urllib.parse
folder=pathlib.Path(argparse.ArgumentParser().parse_args().output_dir) if False else pathlib.Path(__file__).resolve().parent
assert not (folder/'stop-server').exists()
code='JADE-'+str(secrets.randbelow(9000)+1000)
receipt='SCROLL-'+str(secrets.randbelow(9000)+1000)
lock=threading.Lock()
style='<style>body{font:16px system-ui;margin:8px;background:#eef6ef;color:#153b29}h1,h2{font-size:19px;margin:6px 0}p{margin:6px 0}input,button{font:inherit;padding:9px;box-sizing:border-box;max-width:95%}iframe{border:2px solid #487a61;background:white;box-sizing:border-box}output{display:block;padding:8px;background:#cfe7d5}</style>'
def events(role):
 return '<script>const role='+json.dumps(role)+''';function log(kind,e,data={}){navigator.sendBeacon('/event',new Blob([JSON.stringify({role,kind,trusted:e.isTrusted,page_ms:performance.timeOrigin+performance.now(),...data})],{type:'application/json'}));}for(const kind of ['pointerdown','pointerup','click','keydown','input','wheel'])document.addEventListener(kind,e=>log(kind,e,{target:e.target.id||e.target.tagName,key:e.key,value:e.target.value,scrollX,scrollY}));addEventListener('scroll',e=>log('scroll',e,{scrollX,scrollY}));</script>'''
class Handler(http.server.BaseHTTPRequestHandler):
 def do_GET(self):
  route=urllib.parse.urlparse(self.path).path;p=self.server.server_port
  if route=='/parent.html':
   body=f'''<title>THREE-LAYER-PARENT</title><h1>竹林主线：三层卷轴任务</h1><p>只完成“主线卷轴”的验证、滚动、翻页，把回执交回此处。旁路控件不要操作。</p><form id="final-form"><label>最终回执<input id="final" aria-label="最终回执" autocomplete="off"></label><button id="finish">完成整轮</button></form><output id="final-status">整轮未完成</output><div style="margin:24px 25px"><iframe title="主线卷轴：第一层镜像" style="width:min(500px,100%);height:390px;transform-origin:center;transform:rotate(2deg) skewX(3deg) scaleX(-1)" src="http://localhost:{p}/middle.html"></iframe></div><details><summary>旁路任务（本轮不使用）</summary><label>校验码<input id="decoy-code" aria-label="校验码"></label><button id="decoy-continue">第二步：获取回执</button></details>'''+events('PARENT')+f'''<script>document.querySelector('#final-form').onsubmit=e=>{{e.preventDefault();const ok=document.querySelector('#final').value==={json.dumps(receipt)};document.querySelector('#final-status').textContent=ok?'整轮任务完成，三层回执正确':'回执不正确';log('final-submitted',e,{{accepted:ok}})}};</script>'''
  elif route=='/middle.html':
   body=f'''<title>THREE-LAYER-MIDDLE</title><div style="margin:10px"><iframe title="主线卷轴：第二层反射" style="width:calc(100% - 16px);height:350px;transform:scaleX(-1) rotate(-3deg) skewX(-4deg)" src="http://127.0.0.1:{p}/bridge.html"></iframe></div>'''+events('MIDDLE')
  elif route=='/bridge.html':
   body=f'''<title>THREE-LAYER-BRIDGE</title><h2>主线卷轴</h2><div style="margin:14px"><iframe title="主线卷轴：第三层校验" style="width:calc(100% - 12px);height:265px;transform-origin:center;transform:rotate(2deg) skewX(2deg)" src="http://localhost:{p}/leaf.html"></iframe></div>'''+events('BRIDGE')
  elif route=='/leaf.html':
   body=f'''<title>THREE-LAYER-LEAF</title><h2>第一步：验证</h2><p>本轮校验码：<strong>{code}</strong></p><form id="verify"><label>校验码<input id="verification" aria-label="校验码" autocomplete="off"></label></form><output id="status">输入后按 Enter 验证</output><div style="height:640px"></div><button id="continue" disabled>第二步：获取回执</button><p>验证成功后，向下滚动到这里。</p>'''+events('LEAF')+f'''<script>document.querySelector('#verify').onsubmit=e=>{{e.preventDefault();const ok=document.querySelector('#verification').value==={json.dumps(code)};document.querySelector('#status').textContent=ok?'验证成功，请向下滚动获取回执':'校验码错误';document.querySelector('#continue').disabled=!ok;log('verified',e,{{accepted:ok}})}};document.querySelector('#continue').onclick=e=>{{log('navigate-requested',e);location.href='http://127.0.0.1:{p}/receipt.html'}};</script>'''
  elif route=='/receipt.html':
   body=f'<title>THREE-LAYER-RECEIPT</title><h2>第二步：回执</h2><p>回执码：<strong>{receipt}</strong></p><output>三层验证和翻页完成。把此回执填到最外层“最终回执”并提交。</output>'+events('RECEIPT')
  else:self.send_error(404);return
  raw=('<!doctype html><meta charset="utf-8">'+style+body).encode();self.send_response(200);self.send_header('Content-Type','text/html; charset=utf-8');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
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
threading.Thread(target=stop,daemon=True).start();server.serve_forever()
