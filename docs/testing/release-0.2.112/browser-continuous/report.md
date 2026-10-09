# 112正式Browser连续观察复验：控制动作仍未命中在途窗口

正式Program Files 0.2.112，冻结72be2b5；原SWE-2-medium/revision51/唯一island-kayak。仅一次真实模型提交、一次CU、一点击，无补发、重试或新云端。先启动独立SQLite连续只读观察器，再提交模型任务；观察器不操作宿主，不修改产品预算、等待期限或网页执行节奏。

观察登记3362/request33e46de1d2c3ba84850a99bed25103bc发生于1791522824144ms，观察器在4.6152ms后捕获（10268次轮询，最大实际间隔548.9261ms）。此前分段等待的盲区已消除。随后正常刷新AX、检查关闭控件，再在独立工具调用中关闭右栏；动作开始1791522835104ms，返回1791522835189ms，分别晚10960ms/11045ms，超过产品底层观察5秒上限。登记时点不能证明11秒后仍处于该等待分支，不能据此宣布在途撤销通过。

旧页产生可信down/up/click；终态parent failed/CU blocked/verification/native_browser_panel_unavailable，input_steps保留sent/released/partial=false，效果和目标结论保持未知。runtime_events没有browser.observation_stopped。单次ACP end_turn/process_drained=1、绑定解锁；SSE正常EOF，服务器和连续观察器正常退出。模型将“登记后关闭”描述为面板撤销，仅是最终展示，精确阶段以独立时间和事件为准。[正常软件终态实拍](final-native.jpg)。

结论：连续观察驱动已捕获阶段，实际控制工具往返仍超过窗口，严格在途资源撤销未验收通过；严格新原生Target/跨来源commit按下窗口也继续开放。不重复简单点击碰窗口，不延长产品期限、不注入暂停；推进其它独立验收项。Paint免测，微信不动。原事实、独立时钟、驱动及截图保留。
