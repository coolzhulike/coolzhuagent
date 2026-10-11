"""普通验收网页与真实事件记录；不模拟模型、不发送键鼠输入。"""
import json,time,pathlib,threading,sys
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
root=pathlib.Path(__file__).resolve().parent
lock=threading.Lock();events=[]
common="""const doc=crypto.randomUUID();function log(kind,e){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,doc,url:location.href,page_ms:Date.now(),trusted:e?.isTrusted})],{type:'application/json'}))}for(const k of ['pointerdown','pointerup','click','keydown','keyup','input'])document.addEventListener(k,e=>log(k,e));window.addEventListener('pagehide',e=>log('pagehide',e));window.addEventListener('popstate',e=>log('popstate',e));window.addEventListener('hashchange',e=>log('hashchange',e));log('load');"""
def page(title,content,script=''):
    return '<!doctype html><html lang="zh-CN"><meta charset="utf-8"><title>'+title+'</title><style>body{font:21px sans-serif;padding:20px;background:#edf7f3;color:#122c22}button,input{font:21px sans-serif;padding:16px;margin:10px 0}a{display:block;padding:14px}label{display:block}</style><h1>'+title+'</h1>'+content+'<script>'+common+script+'</script></html>'
target=page('TARGET-072','<p>目标页输入事件：<b id="count">0</b></p>',"let n=0;for(const k of ['pointerdown','pointerup','click','keydown','keyup','input'])document.addEventListener(k,e=>count.textContent=++n)")
pages={
'/source.html':page('SOURCE-072','<button id="go">进入目标页</button>',"go.onpointerdown=()=>location.assign('/target.html')"),
'/cross.html':None,
'/target.html':target,
'/popup.html':page('POPUP-SOURCE-072','<button id="go">打开新页面</button>',"go.onpointerdown=()=>window.open('/popup-target.html','_blank')"),
'/popup-target.html':target.replace('TARGET-072','POPUP-TARGET-072'),
'/spa.html':page('SPA-SOURCE-072','<button id="go">切换SPA页面</button>',"go.onpointerdown=()=>{history.pushState({},'','/spa-target.html');document.querySelector('h1').textContent='SPA-TARGET-072'}"),
'/fragment.html':page('FRAGMENT-SOURCE-072','<a href="#destination">跳转到目标段落</a><div style="height:500px"></div><h2 id="destination">FRAGMENT-TARGET-072</h2>'),
'/history.html':page('HISTORY-072','<p id="stage">阶段：起点</p><p id="trail">操作顺序：起点</p><button id="push">进入第二阶段</button><button id="back">历史后退</button><button id="forward">历史前进</button>',"push.onclick=()=>{history.pushState({stage:2},'','/history.html?stage=2');stage.textContent='阶段：第二阶段';trail.textContent+='→进入第二阶段'};back.onclick=()=>history.back();forward.onclick=()=>history.forward();window.addEventListener('popstate',()=>{stage.textContent=location.search?'阶段：第二阶段':'阶段：起点';trail.textContent+=location.search?'→历史前进':'→历史后退'})"),
'/form.html':page('FORM-072','<label>验收姓名<input id="name" aria-label="验收姓名"></label><div style="height:500px"></div><label><input id="agree" type="checkbox">确认验收</label><button id="submit">提交本地表单</button><p id="result">尚未提交</p>',"submit.onclick=()=>result.textContent=document.getElementById('name').value==='SWE2-072'&&agree.checked?'FORM-PASSED-072':'FORM-FAILED-072'"),
}
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path=self.path.split('?')[0]
        if path=='/events':
            with lock:body=json.dumps(events,ensure_ascii=False).encode()
            mime='application/json;charset=utf-8'
        elif path in pages:
            content=pages[path]
            if path=='/cross.html':content=page('CROSS-SOURCE-072','<button id="go">进入跨源目标页</button>',f"go.onpointerdown=()=>location.assign('http://localhost:{server.server_port}/target.html')")
            body=content.encode();mime='text/html;charset=utf-8'
        else:self.send_error(404);return
        self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Cache-Control','no-store');self.end_headers();self.wfile.write(body)
    def do_POST(self):
        if self.path!='/event':self.send_error(404);return
        item=json.loads(self.rfile.read(min(int(self.headers.get('Content-Length',0)),4096)));item['server_ms']=time.time_ns()//1000000
        with lock:
            events.append(item)
            with (root/'page-events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(item,ensure_ascii=False)+'\n')
        self.send_response(204);self.end_headers()
    def log_message(self,*args):pass
server=ThreadingHTTPServer(('127.0.0.1',int(sys.argv[1]) if len(sys.argv)>1 else 0),Handler)
(root/'server.json').write_text(json.dumps({'port':server.server_port,'base':f'http://127.0.0.1:{server.server_port}'},indent=2))
print(server.server_port,flush=True);server.serve_forever()
