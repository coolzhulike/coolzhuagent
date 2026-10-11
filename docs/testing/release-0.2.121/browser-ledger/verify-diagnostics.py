"""仅投影本轮工具诊断，禁止归档其它日志或隐藏思考。"""
from pathlib import Path
import json,re

p=Path(__file__).resolve().parent
start=json.loads((p/'diagnostic-start.json').read_text(encoding='utf-8'))
path=Path(start['log_path'])
assert path.stat().st_size>=start['offset'],'日志已轮转，须另核本轮跨度'
with path.open('rb') as f:
    f.seek(start['offset'])
    segment=f.read().decode('utf-8')
allowed={'computer_use_perform','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab'}
records=[]
for line in segment.splitlines():
    if '[TOOL-CHAIN] run_model_tool_dispatch: name=' not in line:
        continue
    match=re.search(r'\[TOOL-CHAIN\] run_model_tool_dispatch: name=([^,]+), input=(.*)$',line)
    assert match and match[1] in allowed,'本轮工具名或诊断格式异常'
    assert re.fullmatch(r'\[object fields=\d+；参数内容已隐藏\]',match[2]),'参数诊断不是内容无关投影'
    records.append({'tool':match[1],'input_shape':match[2]})
facts=json.loads((p/'facts.json').read_text(encoding='utf-8'))
actual={x['tool_name'] for x in facts['calls']}
assert len(records)==len(facts['calls']) and {x['tool'] for x in records}==actual,'须与本轮实际派发一致'
record={'passed':True,'source_log_offset':start['offset'],'source_log_end':path.stat().st_size,
        'dispatches':records,'raw_segment_not_saved':True,
        'scope':'仅本轮实际root派发日志；未调用的计算器、其它生产者、历史和导出未据此验收'}
(p/'diagnostic-verification.json').write_text(json.dumps(record,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(record,ensure_ascii=False))
