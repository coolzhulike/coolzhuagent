"""补传正式工作区参数后，单独重验SSE/迟到工具；首次脚本400记录不覆盖。"""
import importlib.util, json, time, urllib.request, urllib.parse
from pathlib import Path
p=Path(__file__).with_name('controller-remaining-http-matrix-20261016.py')
spec=importlib.util.spec_from_file_location('matrix',p); m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
m.REPORT=p.with_name('controller-reply-reconnect-matrix-20261016.json')
m.result['boundary']='完整135发布包隔离真实HTTP/SSE、合成回复及工具，不写生产数据或调用外部模型'
s=m.Server('reply-reconnect',29466)
try:
    s.launch();room=s.room('隔离SSE与迟到工具矩阵');ws=s.workspace['workspace_id']
    query=urllib.parse.urlencode({'workspace':s.workspace['workspace']})
    m.require(s.request(f'/api/chat/rooms/{room}/trace')[0]==200)
    with m.dbopen(s.db) as c:
        c.execute("INSERT INTO runtime_runs(id,kind,workspace_id,session_id,chat_room_id,state,claim_token,created_at) VALUES('owned-run','chat_turn',?,'fixture-session',?,'completed','fixture-only',1)",(ws,room))
        c.execute("INSERT INTO chat_room_messages(room_id,id,author,role,target,content,kind,attachments_json,created_at) VALUES(?,'reply','合成夹具','assistant','chat','原始中文正文','assistant-reply','[]',1)",(room,))
        c.execute("INSERT INTO runtime_run_events(run_id,event_type,payload_json,created_at) VALUES('owned-run','chat.context_outputs',?,1)",(json.dumps({'messages':[{'id':'reply'}]}),))
        c.execute("INSERT INTO tool_calls(tool_call_id,run_id,tool_name,arguments_digest,status,created_at_unix_ms,updated_at_unix_ms) VALUES('owned-call','owned-run','read_file','fixture-only','running',1,1)")
    def trace(expected):
        code,b=s.trace(room,'reply');m.require(code==200,b)
        m.require([r['run_id'] for r in b['runs']]==['owned-run'],b)
        calls=b['runs'][0]['calls'];m.require(len(calls)==1 and calls[0]['call_id']=='owned-call' and calls[0]['status']==expected,calls)
        return {'run_id':'owned-run','call_id':'owned-call','status':expected}
    def reconnect():
        events=[]
        with urllib.request.urlopen(s.base+f'/api/chat/rooms/{room}/history-events?{query}',timeout=8) as r:
            while len(events)<2:
                line=r.readline().decode().strip()
                if line.startswith('event:'):events.append(line[6:].strip())
        m.require(events==['hello','history-changed'],events);return events
    m.case('4.2 SSE首次连接携带工作区并产生初始失效通知',reconnect)
    m.case('4.2 迟到工具前精确运行状态',lambda:trace('running'))
    def disconnected_write():
        with m.dbopen(s.db) as c:
            c.execute("UPDATE tool_calls SET status='completed',updated_at_unix_ms=1001 WHERE tool_call_id='owned-call'")
            c.execute("UPDATE chat_room_messages SET content='断线期间已提交的新中文正文' WHERE id='reply'")
        events=reconnect();code,b=s.request(f'/api/chat/rooms/{room}/history-messages?{query}')
        m.require(code==200,b)
        rows=[msg for msg in b['messages'] if msg['id']=='reply']
        m.require(len(rows)==1 and rows[0]['content']=='断线期间已提交的新中文正文',b)
        return {'events':events,'one_latest_body':True,'trace':trace('completed')}
    m.case('4.2 断线期间提交正文和工具终态，重连重读不串轮次',disconnected_write)
    def incorrect_scope():
        observed=[]
        for route in ['history-events','history-messages']:
            code,b=s.request(f'/api/chat/rooms/{room}/{route}?workspace='+urllib.parse.quote(str(m.ROOT/'other-project')))
            m.require(code==409,(code,b));observed.append({'route':route,'status':code})
        return observed
    m.case('4.2 SSE与重读消息跨工程拒绝',incorrect_scope)
except Exception as e:m.result['harness_error']=repr(e)
finally:s.stop()
m.result['passed']=sum(c.get('state')=='passed' for c in m.result['cases'])
m.result['failed']=sum(c.get('state')=='failed' for c in m.result['cases'])
m.result['state']='completed' if not m.result['failed'] and 'harness_error' not in m.result else 'failed'
m.save();print(json.dumps({k:m.result[k] for k in ['state','passed','failed']},ensure_ascii=False))
raise SystemExit(0 if m.result['state']=='completed' else 1)
