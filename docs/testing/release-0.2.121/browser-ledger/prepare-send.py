from pathlib import Path
p=Path(__file__).resolve().parent;root=p.parents[2]
old=root/'tmp/2026-10-09-release-120/browser-live'
marker='BU-CROSS-ORIGIN-LIVE-LEDGER-INSTALLED121-20261009'
for name in ('send.py','collect.py','watch.py'):
 text=(old/name).read_text(encoding='utf-8').replace('BU-NON-TARGET-LIVE-REPAINT-INSTALLED120-20261009',marker).replace('0.2.120','0.2.121')
 if name=='send.py':
  start=text.index('text=marker+');end=text.index('\npayload=',start)
  text=text[:start]+'''text=marker+'：这是跨来源内嵌网页、动态行情和插件计算的综合长程验收。当前右栏原生内置浏览器已正常打开'+address+'。沿用当前唯一Devin云端会话。先只调用一次computer_use_perform(surface="browser",省略target,max_actions=16)，完成“竹林订单与账本”的三项订单流程：每步从当前真实跨来源内嵌订单页读取本步订单码，填入订单码后按Enter，通过后前两步点下一步，第三步按Enter。忽略非目标行情变化，必要时按真实文档引用滚动以显示控件或最终结果；不得复用被替换的表单引用。Browser成功标准必须包含可见整单已完成三项均通过，并把外层已验证回执中三项物品的单价与数量保留为可读证据。真实终态成功后，根据本轮工具回传的三项价格和数量，调用一次DSH计算器 dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab，算三项单价乘数量的总和；最终如实报告逐项算式和总额。两个工具严格顺序、每个只调用一次；CU或证据失败则不猜数字、不补发计算器。完整read_request分页后respond，并wait到真实终态。禁止直接HTTP或脚本操作网页、外部浏览器、其它工具、修改文件或权限、创建云端会话；阻止或超时即如实收尾，输入投递/释放与效果分开，未知保持未知，不读取或输出历史思考。' '''.rstrip()+text[end:]
  text=text.replace("and 'computer_use_perform' in s['parameters']['tool_allowlist']","and {'computer_use_perform','dsh__3596dc2eaf5d6f03a00cbaa53d42a8ab'}<=set(s['parameters']['tool_allowlist'])")
  text=text.replace("assert not (p/'submitted.json').exists()", "assert not (p/'submitted.json').exists()\nlog=root/'tmp/2026-10-04-devin-models/workspace/err.log'\n(p/'diagnostic-start.json').write_text(json.dumps({'log_path':str(log),'offset':log.stat().st_size if log.exists() else 0}),encoding='utf-8')")
 (p/name).write_text(text,encoding='utf-8')
print('本轮唯一绑定/两实际工具顺序综合发送及只读采证脚本备妥')
