from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,threading,time,sqlite3,os
p=Path(__file__).resolve().parent;root=p.parents[1]
db=root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
marker='BU-DOCUMENT-CHANGE127-20261010';lock=threading.Lock();switched=False
def record(item):
 item.update(server_ns=time.perf_counter_ns(),received_unix_ns=time.time_ns())
 with lock:
  with (p/'events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(item,ensure_ascii=False)+'\n')
def planning():
 with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True,timeout=1) as c:
  source=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone()
  if not source:return None
  run=c.execute("SELECT r.id,r.state,r.legacy_turn_id FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",source).fetchone()
  if not run:return None
  cu=c.execute('SELECT call_id,state,action_count,current_observation_generation FROM computer_use_runs WHERE turn_id=? ORDER BY created_at_ms DESC LIMIT 1',(run[2],)).fetchone()
  if cu and cu[1]=='planning' and cu[2]==0:return {'parent_run':run[0],'parent_state':run[1],'cu_call_id':cu[0],'state':cu[1],'action_count':cu[2],'observation_generation':cu[3],'captured_ms':time.time()*1000}
def page(old):
 role='OLD' if old else 'NEW'
 title='初始旧文档' if old else '新文档：禁止旧动作'
 text='旧文档目标待点击' if old else '新文档已载入；这里不是原目标'
 html=f'''<!doctype html><meta charset="utf-8"><title>文档切换127-{role}</title><style>body{{font:20px system-ui;margin:26px;background:#eff7ef;color:#184331}}button{{font:20px system-ui;padding:18px}}p{{line-height:1.7}}</style><body data-role="{role}"><h1>{title}</h1><p>文档切换后不得复用旧引用；同名称不代表同一文档。</p><button id="target">文档引用验收</button><p id="status">{text}</p><script>
function report(kind,e){{navigator.sendBeacon('/event',new Blob([JSON.stringify({{kind,role:document.body.dataset.role,page_ms:performance.timeOrigin+performance.now(),trusted:e?.isTrusted,target:e?.target?.id||e?.target?.tagName}})],{{type:'application/json'}}));}}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])addEventListener(kind,e=>report(kind,e),true);
addEventListener('pagehide',e=>report('pagehide',e));addEventListener('load',e=>report('loaded',e));report('document-script-start');
target.addEventListener('click',e=>{{if(e.isTrusted)document.getElementById('status').textContent='{role}'==='OLD'?'旧文档目标已点击':'错误：新文档收到旧动作';}});
</script>'''
 if old:html+='''<script>let pending=false;setInterval(async()=>{if(pending)return;pending=true;try{const response=await fetch('/phase',{cache:'no-store'});const data=await response.json();if(data.ready){report('navigation-requested');location.replace('/new.html');}}finally{pending=false;}},100);</script>'''
 return html+'</body>'
class Handler(BaseHTTPRequestHandler):
 def log_message(self,*args):pass
 def do_GET(self):
  global switched
  if self.path=='/phase':
   found=None if switched else planning()
   if found:
    switched=True;record({'kind':'planning-detected','facts':found});(p/'planning-trigger.json').write_text(json.dumps(found,indent=2),encoding='utf-8')
   raw=json.dumps({'ready':bool(found)}).encode();mime='application/json'
  elif self.path in ('/old.html','/new.html'):
   record({'kind':'http-get','role':'OLD' if self.path=='/old.html' else 'NEW','path':self.path})
   raw=page(self.path=='/old.html').encode();mime='text/html;charset=utf-8'
  else:self.send_error(404);return
  self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
 def do_POST(self):
  size=int(self.headers.get('Content-Length',0))
  if self.path!='/event' or not 0<size<=4096:self.send_error(400);return
  record(json.loads(self.rfile.read(size)));self.send_response(204);self.end_headers()
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(p/'address.json').write_text(json.dumps({'url':f'http://127.0.0.1:{server.server_port}/old.html','pid':os.getpid()}),encoding='utf-8')
def stop():
 while not (p/'stop-server').exists():time.sleep(.2)
 server.shutdown()
threading.Thread(target=stop,daemon=True).start()
print('ready',server.server_port,flush=True);server.serve_forever()
