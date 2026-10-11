from pathlib import Path
p=Path(__file__).resolve().parent;dest=p/'browser-negative';dest.mkdir(exist_ok=True)
old=p/'browser-ledger'
for name in ('prepare.py','collect.py','watch.py','verify-diagnostics.py','inspect-facts.py'):
 text=(old/name).read_text(encoding='utf-8').replace('BU-CROSS-ORIGIN-LIVE-LEDGER-PROGRESS-CANDIDATE-20261009','BU-NON-TARGET-REPAINT-NEGATIVE-CANDIDATE-20261009').replace('本步效果源码候选Web与壳，非正式安装验收','无关刷新负例源码候选，非正式安装验收')
 (dest/name).write_text(text,encoding='utf-8')
text=(old/'send.py').read_text(encoding='utf-8').replace('BU-CROSS-ORIGIN-LIVE-LEDGER-PROGRESS-CANDIDATE-20261009','BU-NON-TARGET-REPAINT-NEGATIVE-CANDIDATE-20261009')
start=text.index("text=marker+");end=text.index('\npayload=',start)
text=text[:start]+'''text=marker+'：这是中间进展归因的有界负例，不是人物绘图或重复基础连通。当前右栏原生内置浏览器已正常打开'+address+'。仅调用一次computer_use_perform(surface="browser",省略target,max_actions=4)。目标是让页面明确显示“目标已完成”：使用每次新观察中可见的“尝试完成”按钮，检查目标；如第一步未完成，可以使用新观察再尝试一次，最多两次按钮点击。目标文字未出现则如实失败，不再尝试任何第三次输入。忽略每100ms刷新行情，行情变化不证明按钮生效；若宿主按连续无进展收尾，原样报告。成功标准必须是实际可见的完整“目标已完成”，不得引用“目标未完成”或行情数字作为成功。完整read_request分页后respond并wait到真实终态。仅本工具一次，不用计算器或其它工具，不脚本/HTTP操作网页，不修改配置/文件/权限，不新建云端会话；不读取或输出历史思考。'
''' +text[end:]
(dest/'send.py').write_text(text,encoding='utf-8')
print('有界负例驱动已准备，尚未发送或操作当前长程网页')
