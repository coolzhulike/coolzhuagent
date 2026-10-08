# 同进程子文档滚动反馈修复与候选实操

本项为源码候选验收，尚未纳入正式安装包。0.2.94正式包的已通过项目保持独立记录。

## 问题与修复

原观察路径仅为顶层或拥有独立CDP目标的子文档输出视口。同进程iframe与父文档共享目标时，其自身滚动位置缺失；文字和AX节点不变的中间滚动可能被误判为无进展。

观察模块现在复用滚动模块的固定宿主只读查询，读取该子文档自己的scrollingElement，输出合法宽高及左右/上下位置。负RTL和小数位置保留，不借用父视口。固定查询不接收页面或模型脚本、不发送输入；临时对象读取完成后沿原释放路径处理，不用外层超时中途丢弃释放。原文档身份复核及最终资源复核保留；查询失败仍不伪造视口。

## 真实验收

沿用原SWE-2-medium与唯一Devin远端island-kayak，提交一次computer_use_perform。受控真实网页的中间滚动只改变子视口，末端才更新完成文字；未人工代做滚动。

- 运行 `run-chat-e8abb031f690062a3348831d981b68d9161136c550a51213` completed；单attempt正常end_turn、process_drained=1、绑定解锁，内部远端保持空。
- 两步scroll均sent/effect_observed/visible_progress=true，无需按键释放；首步completed_unverified表示尚未达最终目标，不能误读为动作失败，第二步verified。
- 可信子滚动事件位置为600、898.666687，最终出现“子滚动验收完成”；最终宿主父视口page_y=0。CU succeeded/goal_achieved=true，no_progress/replan均0。
- [结构化结果](result.json)与[实际软件截图](01-candidate-completed.jpg)相互核对。完整逐轮观察原文未独立持久化，不把模型转述替代原始视口证明。

实际cargo build --offline通过；native_browser_scroll两项针对性检查通过，包括负RTL、小数及非法视口拒绝。未扩展重复的实现镜像测试。

## 复现与交接

将[真实网页服务器](server.py)复制到仓库tmp内新目录后运行，地址写在该目录address.json；不要直接在本证据目录运行。通过正常右栏打开parent.html，让真实模型仅在SAMEPROCESS-CHILD新鲜RootWebArea引用滚动到完成。

待正式完整构建、正常安装、文件摘要核验及正式实操复验。该结果不覆盖按下期间关闭/替换整个面板、所有DPI或其余总体验收。
