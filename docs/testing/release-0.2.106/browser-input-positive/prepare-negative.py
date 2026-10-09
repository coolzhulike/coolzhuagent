from pathlib import Path
import json,sqlite3
root=Path.cwd();folder=root/'tmp/2026-10-08-browser-input106'
p=folder/'result.json';r=json.loads(p.read_text());nav=json.loads((folder/'ui-navigation-times.json').read_text())
with sqlite3.connect((root/'tmp/2026-10-04-devin-models/workspace/.coolzhu/web-sessions.sqlite3').as_uri()+'?mode=ro',uri=True) as c:
 terminal=c.execute('select updated_at_ms from computer_use_runs where call_id=?',(r['facts']['computer_use'][0]['call_id'],)).fetchone()[0]
assert terminal<nav['before_action_ms']
r['terminal_updated_at_ms']=terminal;r['ui_before_action_ms']=nav['before_action_ms'];p.write_text(json.dumps(r,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({'terminal_ms':terminal,'navigation_later_ms':nav['before_action_ms']-terminal,'visible_reply':r['visible_reply']['content']},ensure_ascii=False))
new=root/'tmp/2026-10-08-browser-input106-stale';new.mkdir(exist_ok=True)
for name in ['server.py','status.py','send.py']:
 t=(folder/name).read_text().replace('INPUT106-','INPUT106-STALE-').replace('BU-INPUT-106-20261008','BU-INPUT-106-STALE-20261008')
 (new/name).write_text(t,encoding='utf-8')
# 负例沿用原105收集逻辑，新增终态投影核对；不覆盖第一轮正例或原未命中事实。
t=(root/'tmp/2026-10-08-browser-observation105/collect.py').read_text().replace('BU-OBSERVATION-105-20261008','BU-INPUT-106-STALE-20261008').replace('0.2.105','0.2.106')
t=t.replace("assert cu['terminal_result_json']['error']['code']=='native_browser_observation_stale'", "assert cu['terminal_result_json']['error']['code']=='native_browser_observation_stale'\nprojection=cu['terminal_result_json']['input_steps']\nassert len(projection)==1 and projection[0]['input_delivery']=='sent' and projection[0]['input_release_status']=='released'")
t=t.replace("'passed':'原观察失效、一次sent/released、零补发、新页零输入'", "'passed':'观察失效负例中终态input_steps仍保留sent/released；无补发、新页零输入'")
t=t.replace("'open':'native_browser_resource_changed分支未触发；模型最终回执缺逐步释放字段，且误述导航发生于按下期间；不能以模型转述取代独立时序'", "'open':'native_browser_resource_changed观察入口未触发；newTarget/跨来源commit严格按下窗口仍开放'")
(new/'collect.py').write_text(t,encoding='utf-8')
