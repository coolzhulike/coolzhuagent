# 0.2.42 原生浏览器单次点击：变更报告与定向验收

2026-10-01，主会话独立实现和测试。优先完成Browser Use，随后Paint。此报告区分源码、构建与真实操作；没有使用子代理或模型夹具进行软件验收。模型保持原qwen3.8-flash、百炼Base URL、已有密钥和medium。

## 本次问题及改动

正式0.2.41已安装核验，真实Qwen只读、动态内容变化拒绝及父取消先提交通过；它没有原生输入能力。新增单次Click通道，避免把长期存在的WebView宿主假装成短命输入helper，也避免借其它浏览器代操作右栏。

1. 接纳时冻结本轮内置浏览器范围及真实工程、聊天室、宿主实例和面板代次。历史用户消息只作为上下文，不能覆盖本轮任务。中文完整只读限制识别补齐。
2. 桌面宿主以实际frame/loader/root文档身份、AX及普通DOM登记随机节点引用；只向模型暴露节点引用、role/name。节点有数量和时效上限，不提供任意脚本、CDP方法名或外来坐标。
3. prepare-click复核原观察、文档、节点语义、控件状态、box/viewport及准确命中，生成两秒一次性票据。execute-click消费票据并再次核验；只支持可见普通DOM的单次点击，iframe/shadow/被遮挡目标不降级点击。
4. InputSafetyStore新增明确的PersistentNativePanel executor分支：实际OS PID、创建时间、可执行文件路径，随机host instance/boot和完整面板资源。原EphemeralHelper的READY/Job/退出契约继续独立执行。持久执行器可执行多次动作，每次许可只消费一次。
5. 复用输入所有权lease。先消费安全许可，再在真实父会话库短Immediate事务检查当前父关系、stop、期限、聊天室完全访问权限、CU/step状态及精确ticket/binding，CAS为dispatching；后者失败记录授权后未输入，不重放许可。这是两个有序事务，不声称跨库原子。
6. 宿主同一UI闭包对原controller依次排队按下和释放，不等待按下回调才发送释放。回调按本次request/attempt/executor和down/up独立聚合；单in-flight。两份成功回执才记Released，COM入队或模型回复不等于成功。已派发后取消不截断释放。未知回执进入既有安全隔离，禁止自动重放。
7. Click后重新观察，真实模型按目标逐项判断，再取样核文档/内容/环境；父取消和CU终态仍沿同库first-wins。动作Released不推导goal达成。只读请求仍为零输入能力。

## 职责与数据兼容

| 层 | 职责 | 主要位置 |
|---|---|---|
| CU运行时 | 冻结本轮、父归属/权限/取消/预算、动作和终态 | chat_run_admission、computer_use_executor/store/turn_scope |
| 安全持久层 | executor类型、真实进程实例、gate/epoch、一次性许可和隔离 | persistent_panel_executor、input_permit_store、native_panel_authorization |
| 桌面目标与输入 | 文档/节点/AX/box/hit-test，固定Click和阶段回执 | native_browser_nodes/target/input/devtools |
| 认证传输 | 有界、单请求、精确身份与阶段匹配 | native_browser_host/input、native-browser-protocol |
| 模型规划及验收 | 使用宿主事实选择节点、重新观察判定目标 | native_browser_adapter/verification、computer_use_planner |

安全库schema v4仅添加类型和绑定字段，不回填历史进程身份；会话库schema v28通过唯一初始化迁移添加dispatch状态、ticket及binding。旧数据字段为NULL；无新版可信身份的宿主可保持只读，但拒绝输入。迁移不把旧helper完成事件用于持久面板结算。

## 技术审查

既有[整合审查会话](https://chatgpt.com/c/6ab013c6-8dc4-83ea-a220-a33c9940783f)已返回第一阶段Click窄审；[实际结论截图](technical-review.png)保存。允许普通本地网页的低风险单次点击回归，要求单in-flight、attempt级回执、短票据重检、明确两库提交边界、Unknown隔离和点击后重观察。上述约束已补入源码。页面显示“极高”，未核完整模型版本，不把方案审查说成总体GPT6 Pro软件验收。

## 工程检查

最终源码离线Web build通过（81秒，133条警告），Shell build通过（20.20秒，1条警告）；完整Web回归1288通过、0失败、2项既有忽略（42.50秒），另lib8通过和宿主静态1通过；完整Shell回归64通过、0失败；模块接线8通过、0失败（4.43秒）；core-runtime输入安全定向回归35通过、0失败，tool-registry离线check通过。上述为工程结果，不调用模型或软件，不替代实操截图验收。

## 构建身份与安装交接

正常release构建完成，构建期间声明源码逐文件保持一致，6/6发布检查、10/10关键产物和859文件载荷通过独立核验；内容扫描safe=true、0发现。原始报告及独立核验收据见[evidence/build-identity/pkg-report-release-20261001-085753665-45d08181](evidence/build-identity/pkg-report-release-20261001-085753665-45d08181/staged-verification.json)。WebView2Loader沿实际producer输出复用并核摘要，不声称重新生成。

| 身份 | 实际值 |
|---|---|
| MSI | [CoolzhuAgent-0.2.42.msi](C:/Users/zhupu/Desktop/coolzhuagent/dist/CoolzhuAgent-0.2.42.msi)，246885757字节 |
| MSI SHA-256 | `fef3bbd4112ac5d264f5955fde58b8ee9ac18597c889b06e81a1d9d30882f6d6` |
| 报告ID | `pkg-report-release-20261001-085753665-45d08181` |
| 源快照 | `f81217c02c874e609850de4c6caed2edf3f062c6fc39b87f78edf506b3e7511d` |
| 构建输入 | `759aed3c2fbfd7f5ba3010f23f50176690985dcc1bdfed8e1ac15de1b8b8af99` |
| 载荷摘要 | `4418c2c4eff11343cb39791a1f4e0117210b3d0cfe229e95076aafd934acf8d5` |
| 参考提交 | `e284355b6e2c0556a36defe213c0c0088c15969e`，dirty工作树；参考种子，源码权威为快照 |

更新前真实父运行无未完成任务，正常窗口关闭已退出安装目录Web/Tauri。自动静默升级返回1603；日志定位为RemoveExistingProducts内旧版InstallInitialize错误1730（必须Administrator才能移除旧产品）。未修改系统权限或重试绕过；注册安装仍041。已请用户手动安装042并处理系统提权。用户随后已手动安装并启动；安装日期20261001，安装目录10/10关键文件与本包摘要一致，Web PID7984监听8765、Tauri PID19360均来自Program Files正式目录；[安装收据](installed-native/installed-artifacts.json)已归档。安装与进程身份通过，真实Qwen点击尚未开始。当前自动读取现场曾短暂出现控制台窗口后隐藏，后台持续运行；启动日志记录动画正常finished并交接console_visible=true，未据此推断窗口隐藏原因，已请用户保持控制台显示。普通网页服务为临时本地57159，进程32432；新页面click.html仅一个按钮/计数，无外部提交或模型响应。

## 可直接转为测试用例的验收矩阵（实际执行状态）

前置：正式安装身份与本包10产物一致；原工程/房间/Qwen不改；正常打开右栏并载入指定页面；当前输入资源安全且接受输入。普通页面只含本地交互，不是模型或宿主夹具。主会话只准备页面和输入测试任务，不代模型点击验收按钮。

| 用例 | 操作与判断标准 | 证据要求 | 当前状态 |
|---|---|---|---|
| BU042-IDENTITY | 正常安装后版本/文件身份/进程目录一致 | MSI与产物摘要、安装收据 | 正式安装10/10文件与运行进程核验通过；控制台可操作状态待恢复 |
| BU042-CLICK | Qwen在右栏click.html点击“增加次数”一次 | 次数0→1原图；真实CU Click、按下/释放、动作终态、S2 goal；无替代浏览器 | 待实操 |
| BU042-READONLY | 完整中文禁用动作任务只读页面 | 本轮cap0/动作0/正确AX答案原图 | 待新版回归 |
| BU042-CANCEL | 本步claim前父取消先提交 | 无输入；cancelled/goal=false；迟到回执不改终态 | 工程边界已测，真实交互待验 |
| BU042-RESOURCE | S1后关闭/隐藏/换环境或改变文档 | 旧目标拒绝；无错误输入；真实时序早于核验点 | 待验；041 AA关闭晚于S2不算通过 |
| BU042-RECEIPT | ticket/attempt/executor失配、到期或未知回执 | 不重放；Unknown沿既有隔离；原动作释放不被新资源覆盖 | 工程检查，不能伪造生产回执冒充实操 |
| BU042-TYPE/SCROLL/NAV | 后续真实文字输入、滚动与导航 | 每项独立动作/页面结果/截图与释放事实 | 本阶段未实现，不算通过 |
| CU042-PAINT | Browser闭环后Qwen在Paint绘画 | 闭合轮廓及简易海绵宝宝、真实释放、四边泛光/准确顶部提示和结束撤除 | 尚未续测 |

DSH远程插件实际安装运行与四项总体审核仍开放；微信保留不动不测试；Devin搁置。PR74保持Draft。不能把点击第一阶段或工程回归扩写为完整Browser Use/四项总体验收通过。
