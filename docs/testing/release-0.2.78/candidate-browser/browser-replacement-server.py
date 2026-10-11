"""实测按下时同步替换整个HTML内容后的释放，独立于跨URL导航竞争。"""
import http.server,json,pathlib,time
dest=pathlib.Path(__file__).resolve().parent;events=[]
after='''<!doctype html><html lang="zh"><meta charset="utf-8"><title>替换后的页面</title><style>body{background:#edf5ef;color:#18392a;font:20px sans-serif;padding:24px}output{display:block;margin:24px 0;font-size:26px}</style><h1>整页内容已经替换</h1><output id="status">已替换，等待释放</output><script>
function log(type){navigator.sendBeacon('/event',JSON.stringify({type,page:'after',time:Date.now(),absolute_ms:performance.timeOrigin+performance.now()}));}
log('replacement-ready');document.addEventListener('pointerup',()=>{document.getElementById('status').textContent='REPLACEMENT-RELEASED-078';log('replacement-pointerup');},{once:true});
</script></html>'''
before='''<!doctype html><html lang="zh"><meta charset="utf-8"><title>按下时整页内容替换验收</title><style>body{background:#edf5ef;color:#18392a;font:20px sans-serif;padding:24px}button{width:100%;height:140px;color:white;background:#16674d;border:0;border-radius:14px;font-size:24px}</style><h1>按下时整页内容替换</h1><button id="target">按下后替换整页内容</button><p>只点击一次；本页不会访问外部网址。</p><script>
document.getElementById('target').addEventListener('pointerdown',()=>{
 navigator.sendBeacon('/event',JSON.stringify({type:'original-pointerdown',page:'before',time:Date.now(),absolute_ms:performance.timeOrigin+performance.now()}));
 document.open();document.write(REPLACEMENT);document.close();
},{once:true});
</script></html>'''.replace('REPLACEMENT',json.dumps(after,ensure_ascii=False).replace('</script>','<\\/script>'))
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        data=json.dumps(events,ensure_ascii=False).encode() if self.path=='/events' else before.encode()
        self.send_response(200);self.send_header('Content-Type','application/json; charset=utf-8' if self.path=='/events' else 'text/html; charset=utf-8');self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
    def do_POST(self):
        if self.path!='/event':self.send_error(404);return
        value=json.loads(self.rfile.read(int(self.headers['Content-Length'])));value['received_at_ns']=time.time_ns();events.append(value)
        (dest/'replacement-events.json').write_text(json.dumps(events,ensure_ascii=False,indent=2)+'\n',encoding='utf-8');self.send_response(204);self.end_headers()
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
(dest/'replacement-server.json').write_text(json.dumps({'port':server.server_port,'url':f'http://127.0.0.1:{server.server_port}/replace.html'},indent=2)+'\n',encoding='utf-8')
server.serve_forever()
