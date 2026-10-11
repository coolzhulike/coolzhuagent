"""正常双页面导航，用于截图验证当前地址与标签；记录真实HTTP读取。"""
import http.server, json, pathlib, time
folder = pathlib.Path(__file__).resolve().parent
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        name = self.path.split('?')[0]
        if name not in ['/tab-a.html', '/tab-b.html']:
            self.send_error(404)
            return
        with (folder / 'page-events.jsonl').open('a', encoding='utf-8') as log:
            log.write(json.dumps({'url': self.path, 'observed_ms': time.time() * 1000}) + '\n')
        marker = 'TAB-A' if name == '/tab-a.html' else 'TAB-B'
        other = '/tab-b.html' if marker == 'TAB-A' else '/tab-a.html'
        raw = f'''<!doctype html><meta charset="utf-8"><title>{marker} 当前地址验证</title>
<style>body{{font:20px system-ui;background:#edf5e9;color:#163b28;margin:24px}}a{{display:block;padding:20px;border:2px solid #526;margin-top:30px}}</style>
<h1>{marker}</h1><p>真实独立HTML页面，页签和地址栏应与本页路径一致。</p><a href="{other}">切换到另一页面</a>'''.encode('utf-8')
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Content-Length', str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)
    def log_message(self, *args): pass
server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
(folder / 'server-address.json').write_text(json.dumps({'base_url': 'http://127.0.0.1:' + str(server.server_port)}), encoding='utf-8')
server.serve_forever()
