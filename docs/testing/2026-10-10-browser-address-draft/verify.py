"""核对原生地址栏观测与独立网页日志，不将GUI回归冒充模型工具验收。"""
import json
from pathlib import Path
from datetime import datetime
p=Path(__file__).resolve().parent
read=lambda name:json.loads((p/name).read_text(encoding='utf-8-sig'))
stamp=lambda value:datetime.fromisoformat(value.replace('Z','+00:00')).timestamp()*1000
formal=read('formal-draft-overwritten.json')
first=read('candidate-draft-first.json');held=read('candidate-draft-held.json')
opened=read('candidate-target-opened.json');back=read('candidate-back-resynchronized.json')
events=[json.loads(line) for line in (p/'events.jsonl').read_text(encoding='utf-8').splitlines()]
assert any('/pulse.html?tick=' in x for x in formal['observed'])
assert not any(formal['intended'] in x for x in formal['observed'])
assert any(held['intended'] in x for x in held['observed'])
elapsed=stamp(held['at'])-stamp(first['at']);assert elapsed>=8000
pulses=[x for x in events if x['kind']=='same-document-url' and stamp(first['at'])<=x['received_ms']<=stamp(held['at'])]
assert len(pulses)>=10
targets=[x for x in events if x['kind']=='request' and x['path']=='/target.html?source=draft']
assert len(targets)==1 and targets[0]['received_ms']>stamp(held['at'])
assert any('标题 用户草稿目标页已打开' in x for x in opened['observed'])
assert any('/pulse.html?tick=' in x for x in back['observed'])
assert read('build-receipt.json')['actual_exit_code']==0
result={'passed':True,'draft_hold_ms':round(elapsed,3),'independent_url_updates_during_hold':len(pulses),'target_http_requests':len(targets),'candidate_web_sha256':read('candidate-processes.json')['web_sha256'],'model_calls':0,'new_cloud_sessions':0,'scope':'仅原生GUI地址编辑、Enter导航与后退同步；候选未正式安装'}
(p/'verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps(result,ensure_ascii=False))
