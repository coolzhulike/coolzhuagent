"""只读本轮已释放后的真实登记事件，限时返回供正常GUI操作。"""
import json,time,runpy
from pathlib import Path
folder=Path(__file__).resolve().parent
server=runpy.run_path(str(folder/'phase-only.py'))
started=time.monotonic()
while time.monotonic()-started<55:
    facts=server['phase']()
    if facts:
        (folder/'close-gate.json').write_text(json.dumps({'facts':facts,'detected_unix_ns':time.time_ns()},ensure_ascii=False,indent=2),encoding='utf-8')
        print(json.dumps(facts,ensure_ascii=False),flush=True)
        break
    time.sleep(.01)
else:print('本轮登记窗口尚未出现',flush=True)
