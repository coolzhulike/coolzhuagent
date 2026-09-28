# PR69检查、浏览器及DSH市场接续计划

用户最新范围：微信保留原有功能，仅保留页面迁移，本轮不测试微信；优先内置浏览器和DSH远程插件市场。沿用用户已授权的技术方案自主审查与实施，代码由GPT-6 Sol xhigh完成，测试执行指定GPT-5.6 Luna max，不使用模型夹具。

## 已核证据与方案

PR69的2291328两条CI失败。已读失败日志：1253通过、5失败、2忽略。失败涉及旧浏览器高级控件静态断言、Goal事件流上限旧值，以及目录必含local-plugin的假设。先检查当前真实目录与UI职责，再更新过期约定，不能skip测试或恢复用户要求删除的控件。

微信回退以abaf055为行为基线，精确撤销2830570新增生命周期、实例标识、provider_kind及前端实时状态分支。保留共用连接修复及页面迁移，不回退整个app.js/main.rs。

浏览器：025原生target=_blank停留A、window.open到C，仍无证据证明是否外开。026只补了请求版本防护，尚未确定根因。检查Wry/WebView2事件、URL校验、请求与加载事件关联；不能添加定时等待猜测回调顺序，不能使用DOM脚本改链接掩盖宿主问题。新增诊断如有必要只记录内部事件类型/请求ID，不暴露带密钥查询的网页地址。

市场：本机DSH核心通过pnpm管理带dsh.bundle的包，没有自身统一远程目录。已核社区DSH Market源码`src/regions.ts`及`src/registry.ts`：公开目录为`https://awesome-dsh-plugin.com/plugins.json`，2026-09-28实际返回updated=2026-09-27、4377项、23类。已询问用户具体市场；在回复前以该来源进行实现准备，适配器独立便于替换。

DSH的Node/Cordis插件与coolzhu的.claw-plugin/plugin.json进程插件使用不同协议。直接接入市场页面不能产生运行能力；不能把npm下载、DSH版本兼容或市场收录当作coolzhu可加载证明。当前方案保留现有本地运行链，增加同源远程发现/搜索/分类/详情与来源说明，后端检查远程仓库实际清单。现有Git安装器不固定提交，也没有拉取超时和体积上限，因此这一阶段不开放远程安装，避免检查通过后实际安装内容已变。完整DSH运行适配是未完成边界，需按实际服务/工具协议逐步接入，不能借此启动第二个Agent主循环。

## 执行单元及职责

源码已推送PR69提交`137546a`。本地构建、6项定向检查及真实市场接口详情见[测试简报](../testing/pr69-browser-market/pr69-browser-market-api-and-regressions.md)。2026-09-28本次记录时远端检查仍在执行，尚不写为全部通过。

- [x] 本地CI修订与微信回退：精确恢复原微信行为、删除新增托管模块，保留页面迁移；Web与sidecar离线构建通过。此前5条失败检查和1条市场来源限制检查均通过。没有微信实操或模型夹具测试；远端最终结果另列。
- [ ] 浏览器：`browser_panel.rs`、`native_browser_panel.js`；先证据再修复，失败/未知结论照实保留。
- [x] 远程目录代码：独立`dsh_market.rs`通过`extension_market.rs`挂接，固定HTTPS来源、16MiB上限和15秒连接预算，服务端搜索/分类/24项分页，失败保留标明过期的缓存。真实接口已返回4377条目录，响应内容验收见测试报告。
- [x] 市场页面代码：独立`dsh_market.js`、发布资源内嵌及市场刷新事件已接线；显示远程目录与已安装插件，沿用玉石风格。搜索、分类、分页、详情、清单检查及内置浏览器打开仓库均已接线，JavaScript语法检查通过；原生界面验收未完成。
- [ ] 安装适配与原生验收：先核实际包元数据/运行协议后决策；无兼容执行能力不能宣称完成“一键安装可用”。界面实测需原生截图，当前会话未提供Computer Use工具，不以接口读取冒充界面通过。
- [ ] 更新同一PR69及报告，读取新提交CI结果，候选包与实际安装状态分开记录。

## 主要风险与检查

### DSH运行兼容的后续实施边界

本机DSH源码核实：`dsh.bundle.patch`指定叠加的`cordis.patch.yml`，其中的插件通过Cordis服务依赖挂载，工具由`ctx.tools.register`动态注册。仅获取npm包或解析目录不能完成运行接入。DSH ToolRuntime提供schemas和execute接口，自身还依赖systemPrompt服务。

可实施方案是独立Node工作进程，仅挂载Cordis、ToolRuntime及选定插件需要的服务，向Rust暴露listTools/callTool；模型会话与主循环仍由现有运行时负责。首批优先支持依赖tools的MCP客户端，以及依赖明确的小型工具插件；文件工具需映射fs与当前工程权限。依赖Agent/session的goal、subagent、workflow、skill管理和DSH自身插件管理工具暂不列为可用，逐类补服务适配。

交付前必须固定Node和包版本、固定安装内容、支持取消和进程回收、停用时卸载服务并刷新工具清单，调用前重新验证工程和启用状态。不得引用开发机Desktop/dsh作为发布依赖，也不得执行远程目录中的安装命令或任意bundle配置。此部分尚未实现；验收应使用真实DSH插件和既有qwen3.8-flash验证发现、调用、停用与重启，不新增模型夹具。完成前远程市场只能标记发现/兼容检查阶段，不能声称与DSH安装运行能力一致。

### 浏览器调查补充

只读核对025测试页 `tmp/browser-redirect-acceptance-20260928/server.py`：anchor的`href="/c" target="_blank"`与按钮的`window.open('/c', '_blank')`指向同一个静态200页面；不存在把该链接重定向回A的页面逻辑。anchor未显式设置rel，隐式noopener是否影响实际WebView2回调尚未证实。既有服务日志没有保存对应GET请求，不能据其断言浏览器发出或未发出请求。

已加入默认关闭的`COOLZHU_BROWSER_NAV_DIAGNOSTICS=1`开关，记录新窗口回调、调度、导航和加载阶段以及请求版本/视图代次，不记录完整URL、查询参数或聊天室标识。Tauri离线构建通过，但这只证明诊断代码可构建；必须结合新的原生操作及截图定位故障阶段，NAV02503保持开放。

第三方市场不是DeepSeek官方服务。目录和包清单可能含脚本/恶意文本，渲染必须使用textContent；不执行目录install字段的shell文本。远程包校验由后端掌握，前端不得传任意本地路径/URL跳过目录约束。切工程后旧响应不能显示为新工程状态，安装变更沿用expected_workspace归属检查。网络错误应收尾并恢复操作，不能把空列表当成功。

资料：[DSH Market](https://github.com/dsh-market/dsh-market)、[来源配置](https://github.com/dsh-market/dsh-market/blob/main/src/regions.ts)、本机`C:/Users/zhupu/Desktop/dsh/packages/boot/plugin-manager/src/index.ts`。远端原文/实时目录样本留在tmp，作为调查材料，不作为产品内置快照或模型夹具。
