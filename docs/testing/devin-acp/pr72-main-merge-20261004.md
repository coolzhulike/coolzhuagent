# PR72 主线冲突修复与 SWE-2 后续验收条件

PR72 原远端头为31d4d0bdc954a4607b474aa6753f02dc58db2e50；本次合入主线2a19c054df076d9224351fdbed6fc728fb6cde5b（已包含PR76）。保留原PR历史，不重置、强推或代替用户合并。

## 冲突与实际修复

| 位置 | 冲突根因 | 最终处理 |
| --- | --- | --- |
| main.rs模块声明 | Devin模块与主线DSH接线插入同位置 | 保留Devin及所有DSH模块；其余自动合并逐段审查 |
| plugin_runtime.rs工具定义 | Devin内置定义与DSH模型绑定并发增加 | 分别追加且共用重名拒绝，不覆盖其中一类 |
| extension_market.rs目录 | Devin条目追加与DSH真实启用/加载状态并发修改 | 可变类型化目录保留Devin条目，同时保留DSH状态及源码摘要 |
| devin_plugin/mod.rs目录兼容 | 主线PluginEntry新增dsh必填字段，原Devin JSON没有该字段；编译不会发现 | 明确非DSH并提供空DSH摘要，避免整个插件市场读取失败；新增一个真实目录生产者到消费类型的跨模块回归 |

旧模式为任取一侧会丢失主线DSH或Devin接线。本轮没有改变原生输入、恢复、权限与模型协议的责任边界，没有接入第二套HTTP模型循环。

## 工程验证

首次合并版本Web离线build通过；既有Web全回归1370通过、0失败、6忽略，另8项lib及1项native-host检查通过。之后语义审查补齐目录字段；最终版本离线build通过；Web主测试1371通过、0失败、6忽略，另8项lib及1项native-host通过；模块联动8通过，tool-registry离线check通过。前端Devin九项检查、UI静态契约和四文件JS语法检查通过。这些是工程验证，不作为真实模型或软件截图验收。

## 后续真实模型基线

用户最新明确要求后续测试使用Devin登录后的SWE-2，取代先前Qwen基线；旧Qwen原始报告不改写。使用当前账号实际可用并生效的SWE-2变体，保留本人设置的思考选择；缺省文本适配沿用swe-2-medium，不转收费或其它provider。连接不填HTTP Base URL/API Key，不接触或回显本机登录凭据。不得把目录的Free标记写成账单实测0元。

本轮只读核验CLI为devin 3000.10.48 (fcf7ba39)，SHA256 d8877ebf699499b1d0957a9fdd99cb596013fc3bfeb782b756496bbcf527bb1b，与适配固定摘要一致。但真实auth status返回未登录，models list退出1且需要认证；未发出SWE-2 prompt，未访问账号令牌，也未用历史认证记录或替身模型代替。登录需用户本人在官方流程完成。

## Browser Use / Computer Use的能力缺口

Devin ACP文本入口chat.rs使用受控独立目录、关闭内建工具及MCP，session/new不挂载工具，远端ToolObservation被拒绝。已有bridge.rs只暴露六类基础工具，明确不包含computer_use_perform；它没有接入聊天室文本路径。因此本轮合入冲突解决并不使SWE-2具备Browser Use或Paint能力。

后续先按已有整合计划P0/P3补齐固定CLI的受控工具桥：真实父run/turn、工具快照、当前请求CU范围、房间完全访问授权、取消与根预算必须传入同一生产执行器；CU规划/验图后端也必须显式具备真实图像输入与阶段完成事实，不能通过旧HTTP ProviderClient绕回Qwen。当前可验证的只有已登录SWE-2文本收发、上下文和停止；Browser/完整Paint及泛光联合截图在工具桥可用后执行。不会把模型在文本中描述操作认成实际输入，也不代画。

安装版仍保持当前载荷；本轮是源码PR更新，没有宣称已安装Devin版本或完成新模型实操。
