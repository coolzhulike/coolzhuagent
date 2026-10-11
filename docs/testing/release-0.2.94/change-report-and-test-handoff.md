# 0.2.94 改动与验收交接

0.2.94 已完成正常完整构建和 Windows MSI 安装。本文只记录已有事实；Browser关键长程与负例已正式通过，综合任务与剩余边界仍在执行，不将构建或安装摘要通过当成功能验收。

## 版本与安装身份

- 冻结产品提交：`54fe2d317d1c0022e88a95533f538b172ae30593`。
- 源码快照：`715efcbc968f074fd8fb094eff09e97b23db2dcc8e238eedc32ad19792059f10`。
- 正常命令：`scripts/build-msi.ps1 -Version 0.2.94 -Configuration release -PackageRoot tmp/release-0.2.94-package`，未跳过构建、未事后替换 EXE。
- 六项发布资格门禁均通过；1159 个打包文件与实际安装文件逐长度/SHA 一致。
- MSI：`dist/CoolzhuAgent-0.2.94.msi`，285769223 bytes，SHA256 `2795079a3005fb76da5db1d1cca04eb9edac4fe31263b5b794dbbc1297396ce1`。
- 正常管理员安装退出码 0；安装目录 CLI 自报 0.2.94、上述冻结提交。实际后台和桌面壳均来自 `C:/Program Files/CoolzhuAgent/bin`，文件摘要与清单一致。
- MSI 当前未签名；本版本尚未上传 GitHub。本地安装不等于远端交付。

安装前已只读确认日常与验收库没有活动运行，原 SWE 唯一远端 `island-kayak` 无占用，internal 绑定仍空。只停止经过路径、开始时间和摘要核对的本轮候选进程，没有清除安全记录或重新创建云会话。

启动核验脚本在启动瞬间读取进程 Path 失败，保留进程；随后系统进程清单、Get-Process 和文件 SHA 重新核对均确认正式程序身份，补记收据。不是重启应用掩盖错误。安装版启动明确移除源码静态根覆盖，使用正常打包资源。

## 主要产品变更

| 变更 | 触发与结果 | 关键边界 |
| --- | --- | --- |
| 玉石控件与上传居中 | 统一图标资源；上传箭头绕中心对称、固定24px尺寸，避免旧造型视觉偏移。 | 49 SVG、3 PNG实际安装HTTP内容一致；高DPI、低高度与全部动态状态尚未全验。 |
| 规划快照分页 | 第三方规划请求超过6000字节时，通过只读桥按offset顺序取得完整快照。 | 绑定job/request，拒绝跨请求、越读、取消后读；读全后才允许回答。未启用原生文件读取。 |
| Browser仿射映射 | 支持两层跨来源iframe的旋转和斜切映射，统一Layout尺寸/Visual偏移。 | 非平行透视、坍塌、厚度过小、越界仍明确拒绝，不近似投递。 |
| OOP候选可见性 | 根据逐层owner映射与命中判断离屏候选。 | 有限只读预算，无法证明保留未知；观察阶段不滚动、不输入。 |
| 子文档滚动进展 | 为独立CDP target根输出document_viewport，同一文档token内的有效变化计入进展。 | 不以随机ref变化判断进展，不把缺失viewport当0；同进程子滚动独立事实仍待扩展。 |
| 当前右栏后端偏好 | 提交时前端附带当前可见原生面板偏好，服务端核验并冻结本轮选择。 | 不自动提权；普通偏好不禁用桌面，正文只读约束仍约束桌面，禁止绕过。 |

ChatGPT订阅登录最小实验单独见[调研报告](../../analysis/2026-09-21-integration-review/chatgpt-subscription-connectivity-research-2026-10-08.md)。独立OAuth、真实目录和单次无工具Responses已通过；没有将实验令牌或未完成的Provider写入正式包。

## 实机验收事实

| 项目 | 状态与证据 |
| --- | --- |
| 正常构建/安装/文件身份 | 通过；[安装摘要](installed-validation/installed-094-verification.json)、[安装退出码](installed-validation/install-094-result.json)、[完整构建身份](evidence/build-identity/pkg-report-release-20261008-084033165-2ed8a082/package-report.json)。 |
| 图标实际资源与主窗口 | 49 SVG及3 PNG全部HTTP200且与安装文件逐字节一致；[资源摘要](installed-validation/installed-icons-http.json)、[主窗口实拍](installed-validation/01-installed-icons.jpg)。仅证明当前尺寸实际画面与资源完整，不外推全部DPI。 |
| Browser复杂仿射长程 | 首轮 `BU-INSTALLED094-AFFINE-LONGRUN-20261008` 失败：前8步投递且观察到效果，最后提交前仅剩394ms的600秒CU预算，节点预检超时，最后点击not_sent/not_needed，父页未完成。保留[失败结果](installed-validation/affine-timeout-result.json)和[实拍](installed-validation/05-installed-affine-timeout.jpg)。原轮正常收尾后，仅将既有controller.timeout_seconds由600调整900，并正常重启正式程序，再提交新轮 `BU-INSTALLED094-AFFINE-BUDGETED-20261008`；独立重试已通过：父轮completed、CU succeeded/goal_achieved=true，9/9投递且effect_observed，需要释放的动作均released，no_progress=0/replan=0；父子完成条件同时可见、可信final-submitted accepted=true。SWE正常end_turn/process_drained=1，唯一island-kayak绑定释放。见[通过结果](installed-validation/affine-budgeted-result.json)、[完成实拍](installed-validation/08-installed-budgeted-completed.jpg)。 |
| 透视/遮挡/同步替换 | 透视与父覆盖层已正式通过：真实SWE规划click后分别被native_browser_frame_transform_unsupported / native_browser_target_hit_mismatch拒绝，1次尝试、0步骤、not_sent/not_needed、页面事件为空，正常end_turn/drained/解锁；负例通过不表示CU任务成功。见[perspective](installed-validation/perspective-result.json)与[cover](installed-validation/cover-result.json)。同步替换正式也已通过，1步sent/released，新文档只有pointerup(HTML)，没有click/input；[结果](installed-validation/replace-result.json)、[实拍](installed-validation/11-installed-document-replaced.jpg)。 |

| 跨来源真实HTTP导航 | 已正式通过：pointerdown触发127.0.0.1→localhost真实导航；navigation-started与beforeunload在pointerup前，HTTP请求及新文档load在释放后；原动作sent/released，新文档无任何click/input/key、无补发。CU succeeded/goal_achieved=true，唯一远端正常收尾。[结果](installed-validation/network-result.json)、[实拍](installed-validation/12-installed-network-navigation.jpg)。不能将此描述为新文档commit发生于按下期间。 |

| SKILL/插件/Browser综合反馈流程 | 正式通过：正常选中插件回执核验SKILL，真实DSH calculator单次completed/ok，再单次perform完成5个输入动作。页面可信business-rejected(delivery)→delivery-confirmed→order-completed，最终结算单BAMBOO-ORDER-094、815、包邮；CU succeeded/goal_achieved=true，SWE正常end_turn/drained/解锁。[结果](installed-validation/integration-result.json)、[工具台账](installed-validation/integration-tool-ledger.json)、[业务反馈实拍](installed-validation/14-installed-integration-feedback.jpg)、[完成实拍](installed-validation/15-installed-integration-completed.jpg)。完整瞬时插件响应未单独持久化，不伪造该响应体；已有真实台账、页面输入及回执交叉核对。结束后通过正常接口恢复SKILL未选择状态。 |

| 真实插件内外超时 | 正式通过：inner timeoutMs=1500，实际GET→EOF约1.506秒；插件业务ok:false/空body，宿主正常返回使audit=ok、台账completed，不表示网络抓取成功。[内层结果](installed-validation/plugin-inner-deadline-result.json)、[实拍](installed-validation/16-installed-plugin-inner-deadline.jpg)。独立outer timeoutMs=60000、服务延迟45秒，实际GET→EOF约29.948秒，宿主audit=timeout/elapsed30702ms、台账failed，未返回迟到body、未重试；远端正常end_turn/drained/解锁。[外层结果](installed-validation/plugin-outer-budget-result.json)、[实拍](installed-validation/17-installed-plugin-outer-budget.jpg)。完整瞬时响应未独立持久化，模型转述与宿主审计/网络事实分别标明；独立Win32句柄及许可竞争全矩阵仍未全验。 |

## 后续测试设计依据

复杂长程在当前右栏进行：两层跨来源倾斜子文档读本轮码→输入/Enter→最内层滚动→按钮正常翻页→读取回执→父文档填写并提交。最终宿主必须同时看到父“整轮任务完成，回执正确”和子“验证与翻页完成”；核对可信页面事件、真实CU步骤与投递/释放事实、远端正常收尾和绑定锁释放。正文仅使用“当前右栏”，同时验证面板偏好，不能人工代做步骤。

透视与覆盖层分别验证实际规划到点击后宿主明确拒绝、零投递、页面无可信输入；同步文档替换核对原动作释放、新文档不出现旧点击或补发。跨来源网络导航已单独采证；关闭与整个面板替换仍须补窄时序证据，不能用同源history变化代替。

其余仍按[总验收清单](../../analysis/2026-09-21-integration-review/acceptance-summary-2026-10-08.md)推进。Paint免测、微信不改不测、Opus暂停，正式Browser与工具长程沿用SWE-2-medium和唯一远端，不新增多个测试云会话。

## 本轮长程预算与严格时序说明

首轮失败不能追认为成功：真实步骤为8/9，父页保持未完成。600秒CU截止前394ms才进入最终预检，且最后点击明确not_sent。新轮使用既有工程controller配置900秒，仍受本轮15分钟chat deadline限制；配置变更在旧轮终态、绑定释放后进行，正常重启生效，没有修改运行中的deadline或授权。见[配置事实](installed-validation/longrun-budget-configuration.json)。模型逐页读取与规划耗用墙钟，后续需结合真实规划时延评估默认任务预算与前端提示。

输入路径源码复核：click在同一UI闭包向同一原controller顺序入队down/up，不等待down回调再排队up；面板retire先隐藏、等待当前execution_gate释放后close。该设计用于保留释放机会，但仅为源码依据，不能替代真实导航与关闭时序演练。跨来源网页另记录pointerdown、navigation-started、pointerup、beforeunload、HTTP GET及新文档load；必须根据真实时间戳说明导航发生在释放前还是之后，未命中按下期间窄窗口时不冒称已覆盖。

跨来源正式时间线：BEFORE pointerdown 1791476463617.6ms；navigation-started 3618.0、beforeunload 3618.5、pointerup 3619.2、原页click 3619.6；localhost新页HTTP GET 3633.572，新页load 3710.3。上述后几项省略共同前缀179147646。新文档没有输入，不能把原页click误计为新文档click。
