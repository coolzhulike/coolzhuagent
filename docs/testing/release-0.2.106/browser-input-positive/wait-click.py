from pathlib import Path
import time,json
p=Path('tmp/2026-10-08-browser-input106/events.jsonl')
start=time.monotonic()
while time.monotonic()-start<180:
 events=[json.loads(l) for l in p.read_text().splitlines()]
 found=[e for e in events if e.get('role')=='OLD' and e.get('kind')=='click' and e.get('trusted') is True]
 if found: print(json.dumps(found[-1]),flush=True);break
 time.sleep(.1)
else:raise SystemExit('180秒内未收到可信click，保留现场不导航')
