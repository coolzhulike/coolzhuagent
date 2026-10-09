"""正常重启后的真实领取/并发扫描证据，不改数据库或历史状态。"""
import concurrent.futures,json,time
from pathlib import Path
import importlib.util
folder=Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('live',folder/'scheduler-live.py')
live=importlib.util.module_from_spec(spec);spec.loader.exec_module(live)
before=live.counts()
assert before['active_runs']==0 and not any(row[3] for row in before['bindings'])
with concurrent.futures.ThreadPoolExecutor(max_workers=5) as pool:
    scans=list(pool.map(lambda _:live.api('/api/task-schedules/run-due',{}),range(5)))
assert all(not scan['ran'] for scan in scans),scans
after=live.counts()
assert before==after,(before,after)
result={'stage':'正式102正常重启后并发扫描','observed_ms':time.time()*1000,'before':before,'scans':scans,'after':after,'model_request_increment':0}
(folder/'scheduler-no-replay-result.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'scans':len(scans),'all_empty':True,'model_request_increment':0,'binding':'island-kayak'},ensure_ascii=False))
