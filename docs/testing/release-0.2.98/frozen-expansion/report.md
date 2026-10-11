# 0.2.98正式版：旧轮授权不因后续完全访问而扩大

原SWE-2-medium/原聊天室/唯一island-kayak，一轮真实模型、一次真实read_file调用。目标为本轮自有工程外无敏感内容文件；没有模型夹具或人工代做工具动作。

接纳前通过正常模型设置临时加入read_file，短时关闭调试放行并将聊天室置为目录权限，确认effective_full_access=false。真实ACP进入submitted后恢复该房间完全访问：完成1791490593876.533ms；唯一read_file台账创建1791490602695ms，晚8.818秒。旧轮仍返回dry-run-only/require-approval/read-only-outside-workspace，审计elapsed_ms=0、summary为“接纳时的权限未允许该工具，未执行”。[真实台账、权限变更、审计及终态](result.json)、[正式聊天室实拍](formal-result.jpg)。

父run completed只表示回复正常收尾，读文件业务被拒绝。单attempt terminal/end_turn/process_drained=1、原远端解锁、internal仍空；未知文件内容未进入回复。没有重试、补发或新建云会话。完整瞬时MCP工具响应没有独立旁路截取；引用的机器证据来自实际宿主审计和台账，模型回复只是转述，不把它单独当原始工具回执。[流终态](stream-terminal-events.json)。

结束正常恢复原完全访问、原调试开关及原工具列表，配置revision按产品正常递增至24，没有回写旧revision；自有文件按字节SHA确认后删除。首次采证误把workspace_id传给期望路径的权限接口，409且未派发模型，finally完整恢复；保留[失败及恢复记录](harness-workspace-identity-failure.json)，修正采证参数后才进行本轮。

本项关闭共享ACP冻结授权的“接纳后扩大不能追溯授权旧轮”实际路径；**不是DSH所有插件授权矩阵**，插件自身来源/activation/配置并发与全部独立进程清理仍保持开放。LSP启动竞争和工程浏览候选有各自专项，不能混为正式新代码通过。
