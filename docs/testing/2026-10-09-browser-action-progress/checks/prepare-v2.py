from pathlib import Path
import hashlib,json
p=Path(__file__).resolve().parent; root=p.parents[1]
# 保留第一候选失败、源码、日志与事件；第二候选独立标识和目录。
source=json.loads((p/'candidate-source.json').read_text(encoding='utf-8'))
for f in source:
 raw=(root/f['path']).read_bytes(); f.update(bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
(p/'candidate-source-v2.json').write_text(json.dumps(source,indent=2)+'\n',encoding='utf-8')
checks=(p/'checks.py').read_text(encoding='utf-8')
checks=checks.replace("kind=sys.argv[1]","kind=sys.argv[1]; command_kind=kind.removesuffix('-v2')")
checks=checks.replace('commands[kind]','commands[command_kind]')
(p/'checks-v2.py').write_text(checks,encoding='utf-8')
text=(p/'start-candidate.ps1').read_text(encoding='utf-8')
text=text.replace('tmp/2026-10-09-release-122/installed-122-standard-processes.json','tmp/2026-10-09-browser-progress/candidate-processes.json')
text=text.replace('web-build-result.json','web-build-v2-result.json').replace('web-result.json','web-v2-result.json')
text=text.replace('candidate-source.json','candidate-source-v2.json').replace("'candidate-bundle'","'candidate-bundle-v2'")
text=text.replace('candidate-web-','candidate-v2-web-').replace('candidate-shell-','candidate-v2-shell-')
text=text.replace('candidate-processes.json\') -Encoding','candidate-v2-processes.json\') -Encoding')
text=text.replace('122三项身份不符','第一候选三项身份不符').replace('本步效果源码候选Web与壳','AX索引漂移修正第二候选Web与壳')
(p/'start-candidate-v2.ps1').write_text(text,encoding='utf-8')
v2=p/'browser-ledger-v2';v2.mkdir(exist_ok=True)
assert not (v2/'submitted.json').exists()
for name in ('server.py','prepare.py','send.py','collect.py','watch.py','watch-steps.py','verify.py','verify-diagnostics.py','inspect-facts.py'):
 text=(p/'browser-ledger'/name).read_text(encoding='utf-8')
 text=text.replace('BU-CROSS-ORIGIN-LIVE-LEDGER-PROGRESS-CANDIDATE-20261009','BU-CROSS-ORIGIN-LIVE-LEDGER-PROGRESS-V2-CANDIDATE-20261009')
 text=text.replace('本步效果源码候选Web与壳','AX索引漂移修正第二候选Web与壳')
 if name=='verify.py':
  text=text.replace("assert any(x['action_kind']=='text_input' and x['effect_status']=='effect_observed' for x in steps)","assert sum(x['action_kind']=='text_input' and x['effect_status']=='effect_observed' for x in steps)==3")
 (v2/name).write_text(text,encoding='utf-8')
doc=root/'docs/analysis/2026-09-21-integration-review/browser-action-progress-design-2026-10-09.md'
text=doc.read_text(encoding='utf-8').replace('已结算click/keys之后的有效焦点变化只算交互准备','已结算click/keys之后只将焦点有无变化算交互准备；AX索引随DOM回执插入可能漂移，两个有效索引之间的变化仍为未知')
text=text.replace('当前文档是实施方案，尚无新代码/候选/正式通过结论。','第一候选11次真实动作完成三阶段订单及插件总额7443，根会话正常结束并解锁，但全部动作被算effect_observed，独立进展验收脚本正确拒绝通过。第二候选修正AX索引漂移，尚待重新回归、真实正反例和正式交付。')
doc.write_text(text,encoding='utf-8')
(p/'browser-ledger/verification-failed.json').write_text(json.dumps({'passed':False,'check_exit_code':1,'reason':'业务文字变化不应冒领本步效果','run_id':'run-chat-fb50740f365fd207b9924c3bab7efff0c93fdd565f467b1b','business_completed':True,'progress_acceptance_passed':False},ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print('第二候选驱动准备；第一候选失败证据完整保留')
