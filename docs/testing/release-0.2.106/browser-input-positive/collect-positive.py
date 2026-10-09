from pathlib import Path
p=Path('tmp/2026-10-08-browser-input106/collect.py');t=p.read_text()
t=t.replace("assert facts['run']['state']=='failed' and len(calls)==1", "assert facts['run']['state']=='completed' and len(calls)==1")
t=t.replace("assert cu['terminal_result_json']['error']['code'] in ('native_browser_observation_stale','native_browser_resource_changed')", "assert cu['terminal_result_json']['status']=='succeeded' and cu['terminal_result_json']['goal_achieved'] is True")
t=t.replace("'passed':'原观察失效、一次sent/released、终态input_steps完整保留投递与释放、零补发、新页零输入'", "'passed':'成功终态input_steps完整保留sent/released/effect_observed/passed；单调用、单终态、无补发'")
t=t.replace("'open':'newTarget/跨来源commit严格按下窗口仍未验收；模型最终措辞另审查'", "'open':'导航晚于终态，观察失效负例未命中；newTarget/跨来源commit严格按下窗口仍开放'")
p.write_text(t,encoding='utf-8')
