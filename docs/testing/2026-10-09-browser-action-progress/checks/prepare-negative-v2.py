from pathlib import Path
import json
p=Path(__file__).resolve().parent;src=p/'browser-negative';dest=p/'browser-negative-v2';dest.mkdir(exist_ok=True)
assert not (dest/'submitted.json').exists()
for name in ('server.py','prepare.py','send.py','collect.py','watch.py','watch-steps.py','inspect-runs.py','inspect-facts.py','verify.py','verify-diagnostics.py'):
 text=(src/name).read_text(encoding='utf-8').replace('BU-NON-TARGET-REPAINT-NEGATIVE-CANDIDATE-20261009','BU-NON-TARGET-REPAINT-NEGATIVE-V2-CANDIDATE-20261009').replace('无关刷新负例源码候选','请求字段明确的无关刷新负例源码候选')
 if name=='send.py':
  needle="payload={'expected_workspace_id':workspace['workspace_id']"
  assert needle in text
  text=text.replace(needle,"text+=' 工具请求必须使用这些正式字段：'+json.dumps({'objective':'使页面目标完成','surface':'browser','success_criteria':['页面完整显示目标已完成'],'constraints':['只点击尝试完成按钮，最多两次，未完成即停止','忽略动态行情'],'max_actions':4},ensure_ascii=False)+'；不要使用task或instruction字段。仅一次工具调用，参数错误也立即如实结束，不重试、不补发。'\n"+needle)
 (dest/name).write_text(text,encoding='utf-8')
(src/'verification-failed.json').write_text(json.dumps({'passed':False,'check_exit_code':1,'reason':'有两条CU调用，首条错误task字段零执行，模型违规补发第二条；第二条两次真实输入后no_progress停止，仅关闭该部分观察，完整单调用验收未通过','run_id':'run-chat-a0de96ce135f6e9c912e9effd9d7684f3d1ee2784a98113b'},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('新的独立反例明确正式字段；不修改工具协议或原失败')
