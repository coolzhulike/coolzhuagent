from pathlib import Path
root=Path.cwd();p=root/'tmp/2026-10-10-browser-pending-cancel';p.mkdir(exist_ok=True)
old=root/'tmp/2026-10-09-release-125/pending-close'
assert not (p/'submitted.json').exists()
for name in ('server.py','send.py','collect.py'):
 t=(old/name).read_text(encoding='utf-8').replace('BU-PENDING-CLOSE125-20261009','BU-PENDING-CANCEL125-20261010').replace('OBSERVE125C-','OBSERVE125-CANCEL-')
 t=t.replace('parents[3]','parents[2]')
 if name=='send.py':
  t=t.replace("p.parent/'installed-125-standard-processes.json'","root/'tmp/2026-10-09-large-text-preview/restored-formal125.json'")
  a=t.index('text=marker+');b=t.index('\npayload=',a)
  t=t[:a]+'''text=marker+'：正式0.2.125 Browser输入已释放、下一观察请求仍在等待时的取消验收。右栏已正常打开'+address+'。只调用一次computer_use_perform，surface=browser、省略target、max_actions=1，objective=点击交接观察验收按钮一次并读取新观察，success_criteria=[看到真实点击已完成，等待观察交接]，constraints=[只点击一次；取消后立即结束；不得补发或重试]。测试者会在点击sent/released且下一个观察请求真正登记后，通过正常聊天停止接口取消本轮。不要自行操控关闭或取消按钮，不调用其它工具/HTTP/脚本，不改文件/配置/权限，不创建新云会话；完整读取分页、正常wait；收到取消立即如实结束，保留原投递和释放事实，不报告目标达成，不补发新调用。'\n''' + t[b:]
 if name=='collect.py':t=t.replace('0.2.125正式在途关闭验收','0.2.125正式在途取消验收')
 (p/name).write_text(t,encoding='utf-8')
print('唯一原SWE绑定的严格在途取消专项已准备，无新会话')
