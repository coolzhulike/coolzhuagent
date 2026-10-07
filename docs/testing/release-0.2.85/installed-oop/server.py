"""跨站点普通HTML测试页。记录真实可信事件，不生成模型回复。"""
import http.server, json, pathlib, time

folder = pathlib.Path(__file__).resolve().parent
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        route = self.path.split('?')[0]
        if route == '/parent.html':
            page = '''<!doctype html><meta charset="utf-8"><title>OOP-PARENT 跨站点父页面</title>
<style>body{font:18px system-ui;background:#edf5e9;color:#153c25;margin:20px}button{padding:12px}iframe{display:block;width:calc(100% - 8px);height:320px;margin-top:18px;border:3px solid #247}</style>
<h1>OOP-PARENT</h1><p id="parent">父点击计数：0</p><button id="wrong">跨站点按钮</button>
<iframe title="跨站点子页面" src="http://localhost:PORT/child.html"></iframe>
<script>let count=0;wrong.addEventListener('click',e=>{document.getElementById('parent').textContent='父点击计数：'+(++count);fetch('/event',{method:'POST',body:JSON.stringify({kind:'parent-click',count,trusted:e.isTrusted})})})</script>'''.replace('PORT', str(self.server.server_port))
        elif route == '/child.html':
            page = '''<!doctype html><meta charset="utf-8"><title>OOP-CHILD 跨站点子页面</title>
<style>body{font:18px system-ui;background:#e4ecff;color:#172d58;margin:16px}button{padding:14px;background:#bcd;color:#123;border:2px solid #247}</style>
<h2>OOP-CHILD</h2><p id="count">子点击计数：0</p><button id="target">跨站点按钮</button>
<p>只测试子页面按钮，父页面保持零点击。</p>
<script>let n=0;target.addEventListener('click',e=>{document.getElementById('count').textContent='子点击计数：'+(++n);fetch('/event',{method:'POST',body:JSON.stringify({kind:'child-click',count:n,trusted:e.isTrusted})})})</script>'''
        else:
            self.send_error(404); return
        raw = page.encode('utf-8')
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(raw)))
        self.end_headers(); self.wfile.write(raw)
    def do_POST(self):
        if self.path != '/event': self.send_error(404); return
        raw = self.rfile.read(min(int(self.headers.get('Content-Length','0')),4096))
        event = json.loads(raw)
        event.update(observed_ms=time.time()*1000, host=self.headers.get('Host'))
        with (folder / 'events.jsonl').open('a', encoding='utf-8') as output:
            output.write(json.dumps(event, ensure_ascii=False)+'\n')
        self.send_response(204); self.end_headers()
    def log_message(self,*args): pass

server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'server-address.json').write_text(json.dumps({'base_url':f'http://127.0.0.1:{server.server_port}'}),encoding='utf-8')
server.serve_forever()
