"""自有HTTP资料端点，仅供应真实插件大回执，不替代模型或工具执行。"""
from pathlib import Path
from http.server import ThreadingHTTPServer,BaseHTTPRequestHandler
import json,secrets,threading,time,hashlib
folder=Path(__file__).resolve().parent
assert not (folder/'body.json').exists()
nonce=secrets.token_hex(12)
left=secrets.randbelow(5000)+10000;right=secrets.randbelow(5000)+10000
body=json.dumps({'document':'竹林资料批次','records':[{'index':i,'note':'资料内容须原样保留；中文𠮷😀与Unicode末尾校验。','data':secrets.token_hex(24)} for i in range(240)],'final_record':{'sentinel':'竹林𠮷😀-'+nonce,'left':left,'right':right,'operation':'multiply'}},ensure_ascii=False,indent=2).encode()
(folder/'body.json').write_bytes(body)
(folder/'expected.json').write_text(json.dumps({'sentinel':'竹林𠮷😀-'+nonce,'left':left,'right':right,'result':left*right,'body_bytes':len(body),'body_sha256':hashlib.sha256(body).hexdigest()},ensure_ascii=False,indent=2),encoding='utf-8')
lock=threading.Lock()
def record(event,**fields):
 with lock:
  with (folder/'network-events.jsonl').open('a',encoding='utf-8') as out:out.write(json.dumps({'event':event,'unix_ms':time.time()*1000,**fields})+'\n')
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  record('real_plugin_get',path=self.path,bytes=len(body),sha256=hashlib.sha256(body).hexdigest())
  self.send_response(200);self.send_header('Content-Type','application/json; charset=utf-8');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
address={'port':server.server_port,'url':f'http://127.0.0.1:{server.server_port}/actual-large-document','plugin_id':'dsh-aa909f79795799f288de8a67@external','tool_name':'dsh__1ef9be67fd4ddb42d93d68c084b38983'}
(folder/'address.json').write_text(json.dumps(address,indent=2),encoding='utf-8');record('server_started',**address)
def stop():
 deadline=time.monotonic()+1200
 while not (folder/'stop-server').exists() and time.monotonic()<deadline:time.sleep(.2)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start();print(json.dumps({'server':address,'body_bytes':len(body)}),flush=True);server.serve_forever();server.server_close();record('server_stopped')
