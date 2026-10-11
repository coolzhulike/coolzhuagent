"""独立核对正式104的大回执业务完成，不以父run完成替代成功。"""
from pathlib import Path
import json,hashlib,sqlite3
folder=Path(__file__).resolve().parent
facts=json.loads((folder/'result.json').read_text(encoding='utf-8'))
expected=facts['expected'];original=(folder/'original-tool-result.json').read_bytes()
receipt=json.loads(original);value=receipt['dispatch_plan']['dry_run_input']['result']['value']
body=value['body'].encode('utf-8')
assert body==(folder/'body.json').read_bytes()
assert hashlib.sha256(body).hexdigest()==expected['body_sha256']
assert len(body)==expected['body_bytes'] and value['status']==200 and value['truncated'] is False
tail=json.loads(body)['final_record']
assert all(tail[k]==expected[k] for k in ('sentinel','left','right'))
network=[json.loads(line) for line in (folder/'network-events.jsonl').read_text().splitlines()]
gets=[x for x in network if x['event']=='real_plugin_get']
assert len(gets)==1 and gets[0]['sha256']==expected['body_sha256']
assert len(facts['calls'])==2 and all(x['status']=='completed' for x in facts['calls'])
assert {x['tool_name'] for x in facts['calls']}=={'dsh__1ef9be67fd4ddb42d93d68c084b38983','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab'}
assert len(facts['attempts'])==1
attempt=facts['attempts'][0]
assert attempt['state']=='terminal' and attempt['protocol_stop']=='end_turn' and attempt['process_drained']==1
assert json.loads(attempt['model_json'])['effective']=='swe-2-medium'
assert facts['run']['state']=='completed'
pages=[json.loads(e['payload_json']) for e in facts['events'] if e['event_type']=='tool.result_page_read']
assert len(pages)==1 and pages[0]['next_offset']==len(original) and pages[0]['offset']>0
assert pages[0]['source_tool']=='dsh__1ef9be67fd4ddb42d93d68c084b38983'
assert not any(x['locked_attempt'] for x in facts['bindings'])
assert sum(x['remote_session_id']=='island-kayak' for x in facts['bindings'])==1
reply=facts['visible_reply']['content']
assert all(str(expected[k]) in reply for k in ('sentinel','left','right','result'))
db=folder.parents[1]/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3'
with sqlite3.connect(db.as_uri()+'?mode=ro',uri=True) as c:
    columns=[r[1] for r in c.execute('PRAGMA table_info(tool_calls)')]
    assert 'result_json' not in columns
    row=c.execute('SELECT arguments_digest,status FROM tool_calls WHERE tool_call_id=?',(facts['calls'][1]['tool_call_id'],)).fetchone()
    assert row and row[1]=='completed'
    calculator={'arguments_digest':row[0],'status':row[1],
        'result_in_final_reply':expected['result'],'independent_product':tail['left']*tail['right'],
        'limitation':'小工具正文未另存；独立账本证明calculator完成，返回值由最终回复与独立算式对照，不宣称另有持久化完整calculator回执。'}
    assert calculator['independent_product']==expected['result']
    (folder/'calculator-evidence.json').write_text(json.dumps(calculator,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
summary={'passed':True,'version':'0.2.104','run_id':facts['run']['id'],'attempt_id':attempt['attempt_id'],
    'source_response_bytes':len(body),'receipt_bytes':len(original),'receipt_sha256':hashlib.sha256(original).hexdigest(),
    'http_get_count':len(gets),'successful_page_reads':len(pages),'executed_tools':facts['calls'],
    'final_tail':tail,'calculator_result':expected['result'],'visible_reply_id':facts['visible_reply']['id'],
    'duration_ms':facts['events'][-1]['created_at']-facts['run']['created_at'],
    'binding':'island-kayak','new_cloud_sessions':0,'read_file_calls':0,
    'model_wording_correction':'模型将111227字节工具回执误称HTTP响应体；实际HTTP响应体44498字节。非法UTF-8偏移拒绝仅由最终回复转述，不追认独立事件证明。'}
(folder/'verification.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(summary,ensure_ascii=False))
