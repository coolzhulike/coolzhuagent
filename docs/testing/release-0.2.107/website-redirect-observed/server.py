"""正常正式UI地址栏导航；同一HTTP进程记录真实页面事件，不改产品等待。"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json, threading, time, os

folder = Path(__file__).resolve().parent
lock = threading.Lock()

def record(event):
    event.update(server_ns=time.perf_counter_ns(), received_unix_ns=time.time_ns())
    with lock:
        with (folder/'events.jsonl').open('a',encoding='utf-8') as stream:
            stream.write(json.dumps(event,ensure_ascii=False)+'\n')

def page(role):
    body = '<!doctype html><meta charset="utf-8"><title>REDIRECT107-'+role+'</title><style>body{font:19px system-ui;margin:24px;background:#eff7ef;color:#17422c}button{padding:24px;font:22px system-ui}</style><body data-role="'+role+'">'
    if role=='OLD':
        body += '<h1>旧网页</h1><p>只点击一次。真实按下处理持续2400毫秒，本轮由网页在可信click之后正常跳转到另一个来源。</p><button id="target">投递事实验收</button><p id="state">尚未点击</p>'
    else:
        body += '<h1>新网页已加载</h1><p>只观察，不接受旧动作，不继续点击或输入。</p><button id="new-target">新网页，禁止点击</button>'
    body += '''<script>
function report(kind,e){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,role:document.body.dataset.role,page_ms:performance.timeOrigin+performance.now(),trusted:e?.isTrusted,target:e?.target?.id||e?.target?.tagName})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])addEventListener(kind,e=>report(kind,e),true);
addEventListener('load',e=>report('loaded',e));addEventListener('pagehide',e=>report('pagehide',e));
report('document-script-start');
</script>'''
    if role=='OLD':
        body += '''<script>target.addEventListener('pointerdown',e=>{if(!e.isTrusted)return;const stop=performance.now()+2400;while(performance.now()<stop){};report('down-handler-finished',e);});target.addEventListener('click',()=>{state.textContent='旧网页真实点击完成';setTimeout(()=>location.href='http://localhost:'+location.port+'/new.html',150);});</script>'''
    return body+'</body>'

class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_GET(self):
        if self.path not in ['/old.html','/new.html']:self.send_error(404);return
        role='OLD' if self.path=='/old.html' else 'NEW'
        record({'kind':'http-get','role':role,'path':self.path,'host':self.headers.get('Host')})
        raw=page(role).encode();self.send_response(200);self.send_header('Content-Type','text/html;charset=utf-8');self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
    def do_POST(self):
        if self.path!='/event':self.send_error(404);return
        length=int(self.headers.get('Content-Length',0))
        if length<1 or length>4096:self.send_error(400);return
        record(json.loads(self.rfile.read(length)));self.send_response(204);self.end_headers()

server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'old_url':f'http://127.0.0.1:{server.server_port}/old.html','new_url':f'http://localhost:{server.server_port}/new.html','pid':os.getpid()},ensure_ascii=False),encoding='utf-8')
def stop():
    while not (folder/'stop-server').exists():time.sleep(.2)
    server.shutdown()
threading.Thread(target=stop,daemon=True).start();print('ready',server.server_port,flush=True);server.serve_forever()
