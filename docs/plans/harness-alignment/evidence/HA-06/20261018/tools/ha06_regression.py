import json,sys,subprocess
from pathlib import Path
state=json.loads(Path('docs/plans/harness-alignment/state.json').read_bytes());assert state['active_task']=='HA-06'
command=json.loads(Path('docs/plans/harness-alignment/evidence/HA-05/20261018/runs/web-regression-02/receipt.json').read_bytes())['command']
command+=['ha06_tests','tool_recovery::tests','tool_dispatch_settlement::tests','tests::hook_budget_acceptance','--skip','ha06_tests::post_hook_process_probe','--skip','tests::hook_budget_acceptance::hook_process_probe']
label=sys.argv[1]
raise SystemExit(subprocess.call([sys.executable,'tmp/ha06_run.py',label]+command))
