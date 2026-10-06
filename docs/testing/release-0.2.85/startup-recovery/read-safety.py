"""只读原安全库状态和计数，不触碰历史许可或恢复操作。"""
import json, pathlib, sqlite3, sys, time
folder = pathlib.Path(__file__).resolve().parent
db = pathlib.Path('C:/Users/zhupu/AppData/Local/CoolzhuAgent/input-safety/input-safety.sqlite3')
c = sqlite3.connect(db.as_uri() + '?mode=ro', uri=True)
c.row_factory = sqlite3.Row
data = {'observed_ms': time.time_ns() / 1e6,
    'resource': [dict(r) for r in c.execute('SELECT * FROM input_safety_resource_state')],
    'permits': [dict(r) for r in c.execute('SELECT state,count(*) count FROM input_safety_permits GROUP BY state')],
    'blocks': [dict(r) for r in c.execute('SELECT state,count(*) count FROM input_safety_resource_blocks GROUP BY state')]}
assert all(r['state'] == 'safe' and r['accepts_new_input'] == 1 for r in data['resource'])
assert next(r['count'] for r in data['permits'] if r['state'] == 'outcome_unknown') == 2
assert data['blocks'] == [{'state': 'closed', 'count': 9}]
(folder / sys.argv[1]).write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(json.dumps(data, ensure_ascii=False))
