# 内置浏览器输入与导航接入方案及主会话审查

当前正式045已通过真实Qwen单次滚动、只读观察、正常重启和关闭页面时输入前拒绝。文字输入与导航仍缺Agent执行接口。本方案由主会话审查、实施；用户暂停Pro复审且要求不使用子代理。

## 实施边界

- 复用现有冻结父运行、物理输入所有者、两阶段随机票据、单次许可、持久面板结算与真实页面复查。不新增权限系统，不使用外部浏览器，不向模型暴露CDP方法名或任意脚本。
- TextInput只支持顶层普通input[type=text/search]和textarea。模型先明确点击目标获得焦点，再选择新观察中的textbox引用；不暗中聚焦、不设置DOM.value、不自动提交。宿主固定只读函数取得真实焦点、选区及当前值，Prepare与Execute比较同一字段；原值仅保留在短期宿主内存，不进入回执、日志或模型。密码、只读、禁用、iframe、shadow及contenteditable暂不扩展。
- 实际插入使用固定Input.insertText。插入前在同一controller回调核对最新焦点、选区、值、资源和期限，再排队一次输入。ACK仅说明投递，不等于文本已显示；目标仍由动作后的新观察验收。任何未知回执保持隔离，不自动重放。
- Navigate由模型选择当前RootWebArea原引用并提供HTTP(S)目标URL。沿用控制台的URL限制，拒绝带凭据地址、非网页协议和本地控制台；不增加网页IPC权限。宿主在最后UI线程检查源资源后更新导航revision并调用固定Navigate，返回源动作回执及确切目标资源。导航不按住输入，单独记录ACK。
- 导航后目标URL与新revision必须匹配本次授权回执，再采集新文档证据。验收保留原任务URL约束；只允许由本次适配器已结算动作产生的导航来源记录解释目标URL变化。任意人工切换、关闭、异聊天室/工程、新页面或未声明重定向不能借用旧成功判断。多次导航沿用最初任务来源并逐次更新授权目的地。

## 风险与选择

焦点、选区、原值可能在等待期间变化，因此输入预检票据仍只有2秒，执行时再次比较；不能以较长节点缓存替代输入前检查。固定读取函数使用Runtime.callFunctionOn的throwOnSideEffect和returnByValue，拒绝异常及超长字段；不会接受请求提供的脚本或执行页面写操作。只读函数结果只作为宿主校验资料，页面文字不授予权限。

导航派发与页面完成不是同一事实。回执保持绑定原资源，不能用导航后的页面替换原动作结算。目标页加载、模型验收和验收期间再次取样各保留独立证据；加载失败或跳到未授权地址明确失败。初期不把所有重定向和高级表单动作宣称支持。

模块责任：protocol规定类型；Shell target/editor验证真实控件、Shell input派发一次动作；browser_panel维护显示和导航状态；Web adapter转换模型动作并持有本任务导航来源；authorization/store继续独立裁决许可及结算；verification仅验证页面目标。禁止跨模块直接改运行数据库。

## 回归与正式验收

定向工程检查覆盖字段/回执失配、选区或焦点变化拒绝、危险URL及未授权导航不可验收。改完离线编译Web、Shell和协议，必要既有回归空闲运行。正式新包核验安装后，使用原qwen3.8-flash及百炼配置实测：点击输入框→输入短文本→页面显示结果；从当前页导航到指定新页→实际URL、标题及正文匹配；只读cap0、滚动和关闭资源拒绝回归。真实截图和动作账本是功能通过标准，工程测试不替代实操。随后继续Paint联合验收。

官方依据：[Input.insertText](https://chromedevtools.github.io/devtools-protocol/tot/Input/#method-insertText)、[Runtime.callFunctionOn](https://chromedevtools.github.io/devtools-protocol/tot/Runtime/#method-callFunctionOn)。本地通过agent-reach网页读取核对，原资料存tmp/browser-use-priority/cdp-*-official.txt。

## 源码与工程结果

已按上述边界接入四类原生动作。普通文字输入使用独立editor和edit_input模块，selection/value未加到通用AX投影；导航使用独立的顶层文档校验，不因页面中心含iframe而额外拒绝导航。模型仅提供text/url和原随机引用，Shell票据同时绑定具体操作载荷，业务许可保持冻结动作digest。

Web离线build通过；完整既有回归1291通过、0失败、2项既有忽略，另lib8与宿主静态1通过，39.73秒。Shell离线build通过，最后调整后的64项回归通过、0失败，1.19秒。回执交叉使用、导航错房间/宿主/修订及只读借用交互来源均被拒绝。日志：tmp/browser-use-priority/type-nav-*.log。工程通过仍不代表正式软件验收；046待打包、安装、原Qwen截图验证。未扩展通用字段私密值、自动聚焦、提交、标签页、未授权重定向或链接跳转的成功推导。
