"""仅本机的受控慢页面，用真实网络请求证实函数已经进入；不是工具或模型夹具。"""
import http.server, threading, pathlib, json, time, datetime, select, socket, hashlib
folder=pathlib.Path(__file__).resolve().parent
lock=threading.Lock()
def record(event,**fields):
    with lock:
        with (folder/'slow-events.jsonl').open('a',encoding='utf-8') as out:
            out.write(json.dumps({'event':event,'utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'monotonic':time.monotonic(),**fields},ensure_ascii=False)+'\n')
class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args): pass
    def do_GET(self):
        if self.path not in ('/revoke-20261008',):
            self.send_error(404); return
        record('function_network_entered',path=self.path)
        deadline=time.monotonic()+45
        while time.monotonic()<deadline:
            ready,_,_=select.select([self.connection],[],[],0.15)
            if ready:
                try: data=self.connection.recv(1,socket.MSG_PEEK)
                except (ConnectionResetError,OSError): data=b''
                if not data:
                    record('connection_closed_before_response',path=self.path); return
        body=b'DELAYED_LOCAL_RESPONSE'
        try:
            self.send_response(200); self.send_header('Content-Length',str(len(body))); self.end_headers(); self.wfile.write(body)
            record('late_response_sent',path=self.path)
        except (ConnectionError,OSError): record('connection_closed_before_response',path=self.path)
server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
server.daemon_threads=True
thread=threading.Thread(target=server.serve_forever,daemon=True); thread.start()
plugin='dsh-aa909f79795799f288de8a67@external'
tool='dsh__'+hashlib.sha256(json.dumps([plugin,'net_fetch'],ensure_ascii=False,separators=(',',':')).encode()).hexdigest()[:32]
identity={'url':f'http://127.0.0.1:{server.server_port}/revoke-20261008','host':'127.0.0.1','port':server.server_port,'delay_seconds':45,'tool_timeout_ms':60000,'host_timeout_seconds':30,'tool_name':tool,'plugin_id':plugin}
(folder/'address.json').write_text(json.dumps(identity,indent=2)+'\n',encoding='utf-8')
record('server_started',**identity)
print(json.dumps(identity),flush=True)
deadline=time.monotonic()+1200
while not (folder/'stop-server').exists() and time.monotonic()<deadline: time.sleep(0.2)
server.shutdown();server.server_close();record('server_stopped')
