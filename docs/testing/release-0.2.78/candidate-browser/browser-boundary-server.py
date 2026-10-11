"""SVG图标按钮与独立覆盖层真实页面，不提供模型回复。"""
import http.server,json,pathlib,time
dest=pathlib.Path(__file__).resolve().parent;events=[]
page='''<!doctype html><html lang="zh"><meta charset="utf-8"><title>普通图标按钮验收</title>
<style>body{background:#edf5ef;color:#18392a;font:18px sans-serif;padding:22px;margin:0}.container{position:relative}button{width:100%;height:140px;background:#16674d;color:white;border:0;border-radius:14px}svg{display:block;width:100%;height:100%}output{display:block;font-size:24px;margin:25px 0}.overlay{position:absolute;inset:0;background:#8c5e3df0;color:white;display:flex;align-items:center;justify-content:center;border-radius:14px}</style>
<h1>普通图标按钮验收</h1><div class="container"><button id="target" aria-label="确认图标按钮"><svg viewBox="0 0 320 140"><rect id="glyph" width="320" height="140" fill="#16674d"/><path d="M126 70l25 25 45-50" fill="none" stroke="white" stroke-width="10"/></svg></button>OVERLAY</div>
<output id="result">待点击，次数0</output><script>let count=0;for(const type of ['pointerdown','pointerup','click'])document.getElementById('target').addEventListener(type,e=>{if(type==='click'){count++;document.getElementById('result').textContent='SVG-PASSED-078，次数'+count;}fetch('/event',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({case:location.pathname,type,target:e.target.id,count,time:Date.now()})});});</script></html>'''
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        data=json.dumps(events,ensure_ascii=False).encode() if self.path=='/events' else page.replace('OVERLAY','<div class="overlay" id="cover">独立覆盖层</div>' if self.path=='/covered.html' else '').encode()
        self.send_response(200);self.send_header('Content-Type','application/json; charset=utf-8' if self.path=='/events' else 'text/html; charset=utf-8');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_POST(self):
        if self.path!='/event':self.send_error(404);return
        value=json.loads(self.rfile.read(int(self.headers['Content-Length'])));value['received_at_ns']=time.time_ns();events.append(value)
        (dest/'boundary-events.json').write_text(json.dumps(events,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(dest/'boundary-server.json').write_text(json.dumps({'port':server.server_port,'svg_url':f'http://127.0.0.1:{server.server_port}/svg.html','covered_url':f'http://127.0.0.1:{server.server_port}/covered.html'},indent=2)+'\n',encoding='utf-8')
server.serve_forever()
