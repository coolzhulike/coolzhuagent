"""正常网页导航只由只读真实步骤阶段触发，不访问宿主私有通道。"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,threading,time,sqlite3,os
folder=Path(__file__).resolve().parent
repo=Path('C:\\Users\\zhupu\\.codex\\worktrees\\input-recovery-20261004\\coolzhuagent')
db=repo/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
marker='BU-PENDING-CLOSE-111-20261008'
lock=threading.Lock()
def record(item):
    item.update(server_ns=time.perf_counter_ns(),received_unix_ns=time.time_ns())
    with lock:
        with (folder/'events.jsonl').open('a',encoding='utf-8') as stream:stream.write(json.dumps(item,ensure_ascii=False)+'\n')
def phase():
    with sqlite3.connect(db.resolve().as_uri()+'?mode=ro',uri=True,timeout=1) as c:
        source=c.execute("SELECT id FROM chat_room_messages WHERE role='user' AND content LIKE ? ORDER BY created_at DESC LIMIT 1",(marker+'%',)).fetchone()
        if not source:return None
        run=c.execute("SELECT r.id,r.state,r.legacy_turn_id FROM runtime_runs r JOIN runtime_run_events e ON e.run_id=r.id,json_each(e.payload_json,'$.message_ids') refs WHERE e.event_type='chat.source_messages' AND refs.value=? LIMIT 1",source).fetchone()
        if not run:return None
        cu=c.execute('SELECT call_id,state FROM computer_use_runs WHERE turn_id=? ORDER BY created_at_ms DESC LIMIT 1',(run[2],)).fetchone()
        if not cu:return None
        steps=c.execute('SELECT step_index,input_delivery,input_release_status,completed_at_ms FROM computer_use_steps WHERE run_id=? ORDER BY step_index',(cu[0],)).fetchall()
        if cu[1]=='observing' and len(steps)==1 and steps[0][1:3] == ('sent','released') and steps[0][3] is not None:
            requested=c.execute("SELECT id,created_at,payload_json FROM runtime_run_events WHERE run_id=? AND event_type='browser.observation_requested' AND created_at>=? ORDER BY id LIMIT 1",(run[0],steps[0][3])).fetchone()
            if requested:
                return {'run_id':run[0],'cu_call_id':cu[0],'cu_state':cu[1],'steps':steps,
                    'observation_requested':{'event_id':requested[0],'created_at':requested[1],'payload':json.loads(requested[2])}}
        return None
def page(old):
    role='OLD' if old else 'NEW'
    text='观察等待阶段导航' if old else '新页面已载入；仅观察'
    body=f'<!doctype html><meta charset="utf-8"><title>OBSERVE111-{role}</title><style>body{{font:19px system-ui;margin:24px;background:#eff7ef;color:#17422c}}button{{padding:15px;font:20px system-ui}}#target{{padding:24px}}</style><body data-role="{role}"><h1>{text}</h1>'
    if old:
        body+='<p>只点击主按钮一次。网页保持不导航；测试者仅在本轮请求登记后正常关闭右栏浏览器。</p><button id="target">交接观察验收</button><p id="state">尚未点击</p>'
        body+=''.join(f'<p><button>辅助条目{i:03d}</button></p>' for i in range(100))
    else:body+='<p>这里不需要任何旧动作，请保留未知效果，不重试或补发。</p>'
    body+='''<script>
function report(kind,e){navigator.sendBeacon('/event',new Blob([JSON.stringify({kind,role:document.body.dataset.role,page_ms:performance.timeOrigin+performance.now(),trusted:e?.isTrusted,target:e?.target?.id||e?.target?.tagName})],{type:'application/json'}));}
for(const kind of ['pointerdown','pointerup','click','input','keydown'])addEventListener(kind,e=>report(kind,e),true);
addEventListener('load',e=>report('loaded',e));addEventListener('pagehide',e=>report('pagehide',e));report('document-script-start');
</script>'''
    if old:body+='''<script>target.addEventListener('click',async e=>{if(!e.isTrusted)return;state.textContent='真实点击已完成，等待观察交接';const r=await fetch('/await-observation',{cache:'no-store'});const p=await r.json();report(p.ready?'phase-ready':'phase-not-found');});</script>'''
    return body+'</body>'
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_GET(self):
        if self.path in ('/await-observation','/watch-phase'):
            started=time.monotonic();found=None
            while time.monotonic()-started<15:
                found=phase()
                if found:break
                time.sleep(.01)
            record({'kind':'observing-phase-found' if found else 'observing-phase-not-found','facts':found})
            raw=json.dumps({'ready':bool(found),'facts':found,'returned_ms':time.time()*1000}).encode();mime='application/json'
        elif self.path in ('/old.html','/new.html'):
            record({'kind':'http-get','role':'OLD' if self.path=='/old.html' else 'NEW','path':self.path,'host':self.headers.get('Host')})
            raw=page(self.path=='/old.html').encode();mime='text/html;charset=utf-8'
        else:self.send_error(404);return
        self.send_response(200);self.send_header('Content-Type',mime);self.send_header('Cache-Control','no-store');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
    def do_POST(self):
        length=int(self.headers.get('Content-Length',0))
        if self.path!='/event' or not 0<length<=4096:self.send_error(400);return
        record(json.loads(self.rfile.read(length)));self.send_response(204);self.end_headers()
server=ThreadingHTTPServer(('127.0.0.1',0),Handler)
(folder/'address.json').write_text(json.dumps({'url':f'http://127.0.0.1:{server.server_port}/old.html','pid':os.getpid()},ensure_ascii=False),encoding='utf-8')
def stop():
    while not (folder/'stop-server').exists():time.sleep(.2)
    server.shutdown()
threading.Thread(target=stop,daemon=True).start()
print('ready',server.server_port,flush=True);server.serve_forever()
