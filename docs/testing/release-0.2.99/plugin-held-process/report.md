# 正式099：插件停用后的独立进程退出确认

本轮只提交一次真实SWE-2-medium工具请求，沿用原聊天室和唯一Devin `island-kayak`。通过既有正常接口启用固定来源的真实DSH net-tools，临时只添加`net_fetch`工具白名单；没有更换模型、重绑远端或使用模型夹具。

真实网络函数进入前，测试脚本已捕获正式后台的直接子进程`C:/Program Files/CoolzhuAgent/bin/dsh-runtime/node/node.exe`，持有可查询且可等待的Win32进程句柄，核对实际映像路径、创建时间和二进制SHA。真实GET进入后，通过正常产品接口停用插件。原句柄初始Wait=258，随后Wait=0，独立确认对应进程退出；该测试脚本没有TerminateProcess或其它终止调用。

- [独立句柄事实](held-process-result.json)：PID16352、正式来源及Wait258→0。
- [真实运行、审计和网络事件](result.json)：单工具台账failed/cancelled、cleanup_confirmed=true；网络约1.199秒后断开、无响应体或补发；ACP正常end_turn/drained及唯一绑定解锁。
- [正式聊天室实拍](../installed-validation/plugin-held-process-completed.jpg)：模型如实报告取消与审计，没有用模型转述推断Windows进程退出。
- [正常配置恢复](model-tool-restored.json)：只撤销本次工具白名单，测试插件保持原停用状态，原计算器未动。自有本机服务器正常停止。

原始[观察脚本](watch.py)及[本机网络记录器](slow-server.py)归档供针对性复验。实际SDK插件有固定源码，不是工具应答夹具。观察脚本只枚举进程身份、持句柄、观察本机日志、调用正常停用接口和等待退出。

边界：只关闭“函数真实进入后停用”路径的独立宿主退出证明。其它启动/取消/截止阶段、后代进程全树、OS沙箱或句柄继承不由这一轮外推。完整瞬时工具回执未独立保存，原始审计和实际网络/Win32证据分别保留。

另发现：正常后台重启后，原页面保留旧聊天内容，本轮从其它客户端发起的真实消息未自动出现；正常Ctrl+R后#673/#674可见，[刷新前实拍](../installed-validation/after-reconnect-stale.jpg)与上述最终实拍对应。当前聊天只消费本次请求的SSE，既有全局工具事件不负责聊天历史同步；跨客户端/重连的权威消息同步仍需设计和补验，未将手动刷新称为自动恢复通过。
