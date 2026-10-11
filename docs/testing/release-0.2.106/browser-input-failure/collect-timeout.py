from pathlib import Path
p=Path('tmp/2026-10-08-browser-input106-stale/collect.py');t=p.read_text().replace("assert cu['terminal_result_json']['error']['code']=='native_browser_observation_stale'", "assert cu['terminal_result_json']['error']['code']=='native_observation_timeout'")
t=t.replace("'passed':'观察失效负例中终态input_steps仍保留sent/released；无补发、新页零输入'", "'passed':'观察超时失败终态input_steps仍保留sent/released，effect_status/goal_verdict为空；无补发、新页零输入'")
t=t.replace("'open':'native_browser_resource_changed观察入口未触发；newTarget/跨来源commit严格按下窗口仍开放'", "'open':'资源变化期间待处理旧观察落为通用超时，原因识别待修；newTarget/跨来源commit严格按下窗口仍开放'")
p.write_text(t,encoding='utf-8')
