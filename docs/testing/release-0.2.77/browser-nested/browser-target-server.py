"""真实网页交互验收页，记录原生输入事件；没有模型回复夹具。"""
import http.server, json, pathlib, threading, time

dest=pathlib.Path(__file__).resolve().parent
events=[]
lock=threading.Lock()
page='''<!doctype html><html lang="zh"><meta charset="utf-8"><title>嵌套控件验收077</title>
<style>body{margin:0;background:#edf5ef;color:#18392a;font:18px sans-serif;padding:22px}button{display:block;width:100%;height:126px;margin:22px 0;background:#16674d;color:white;border:0;border-radius:16px;font-size:24px;padding:0}button span{display:flex;width:100%;height:100%;align-items:center;justify-content:center}small{display:block;color:#3c6150}output{display:block;font-size:24px;font-weight:bold;margin:20px 0}</style>
<h1>嵌套控件验收077</h1><small>按钮中有普通文字与图标容器。</small>
<button id="nested" aria-label="确认嵌套按钮"><span id="label">确认嵌套按钮</span></button>
<output id="result">待点击，次数0</output>
<script>
let count=0;
for(const type of ['pointerdown','pointerup','click'])document.getElementById('nested').addEventListener(type,e=>{
 if(type==='click'){count++;document.getElementById('result').textContent='NESTED-PASSED-077，次数'+count;}
 fetch('/event',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({type,target:e.target.id,count,time:Date.now()})});
});
</script></html>'''.encode('utf-8')

class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_GET(self):
        data=json.dumps(events,ensure_ascii=False).encode() if self.path=='/events' else page
        self.send_response(200);self.send_header('Content-Type','application/json; charset=utf-8' if self.path=='/events' else 'text/html; charset=utf-8');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_POST(self):
        if self.path!='/event': self.send_error(404);return
        value=json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        value['received_at_ns']=time.time_ns()
        with lock:
            events.append(value)
            (dest/'browser-target-events.json').write_text(json.dumps(events,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
        self.send_response(204);self.end_headers()

server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(dest/'browser-target-server.json').write_text(json.dumps({'port':server.server_port,'url':f'http://127.0.0.1:{server.server_port}/nested.html'},indent=2)+'\n',encoding='utf-8')
server.serve_forever()
