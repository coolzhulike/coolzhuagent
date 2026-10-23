import json,subprocess,sys
from pathlib import Path
state=json.loads(Path('docs/plans/harness-alignment/state.json').read_bytes())
assert state['active_task']=='HA-05'
old=json.loads(Path('tmp/ha04-20261018-runs/web-regression-03/receipt.json').read_bytes())['command']
command=old+['ha05_tests','tool_loop_progress::tests','devin_acp::tool_wait::tests']
label=sys.argv[1] if len(sys.argv)>1 else 'web-regression-01'
raise SystemExit(subprocess.call([sys.executable,'tmp/ha05_run.py',label]+command))
