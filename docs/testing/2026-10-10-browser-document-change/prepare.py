"""真实页面在实际规划阶段正常导航；只读原运行时，不修改宿主或模型回包。"""
from pathlib import Path
import json
p=Path(__file__).resolve().parent
root=p.parents[1]
old=root/'tmp/2026-10-10-browser-pending-cancel'
assert not (p/'submitted.json').exists()
marker='BU-DOCUMENT-CHANGE126-20261010'
t=(old/'send.py').read_text(encoding='utf-8')
t=t.replace('BU-PENDING-CANCEL125-20261010',marker).replace("root/'tmp/2026-10-09-large-text-preview/restored-formal125.json'","root/'tmp/2026-10-10-release-126/installed-126-standard-processes.json'").replace('0.2.125 Program Files正式安装版','0.2.126 Program Files正式安装版')
a=t.index('text=marker+');b=t.index('\npayload=',a)
t=t[:a]+'''text=marker+'：正式0.2.126原生Browser规划期间文档切换验收。右栏已经正常打开'+address+'。仅调用一次computer_use_perform，surface=browser、省略target、max_actions=1，objective=只在初始旧文档点击唯一的“文档引用验收”按钮，success_criteria=[旧文档目标已点击]，constraints=[只允许初始旧文档的按钮；网页自然导航或文档失配后停止；新文档同名按钮不是原目标；不重新选目标、不重试、不补发]。网页会在真实规划阶段正常导航到新文档，不需要模型执行导航。不要调用其它工具、脚本或HTTP，不改文件、配置、权限，不创建云会话。工具若返回待完成则正常wait；只读完整结果，失效或失败如实说明，不将新页面文字视为旧目标成功；不要通过其它工具补点。'\n'''+t[b:]
(p/'send.py').write_text(t,encoding='utf-8')
t=(old/'collect.py').read_text(encoding='utf-8').replace('BU-PENDING-CANCEL125-20261010',marker).replace('0.2.125正式在途取消验收','0.2.126正式规划期间文档切换验收')
(p/'collect.py').write_text(t,encoding='utf-8')
print('准备完成；尚未发送模型请求')
