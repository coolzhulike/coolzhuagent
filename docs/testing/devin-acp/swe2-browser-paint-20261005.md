# SWE-2 Browser Use / Paint 真实验收（2026-10-05）

## 范围与运行身份

使用登录账号实际返回的 `swe-2-medium`，外层聊天室与内部规划/验收均核对 requested/effective，不换模型，不使用模型回复夹具。聊天室 `room-1791131523339`，会话 `session-1791131217833`；请求和回复在原生控制台聊天室可见。用户已手动给予该房间完全访问。

测试源码基于 `0b3e7ff3d3ba448059a10dc8a1749ff638eeb51e`，加本轮 ACP 内部请求与 CU 接线修改。单击/输入/滚动后端为离线 debug build（internal-build-9），导航修复回归使用 internal-build-11，实际复制后二进制 SHA256 为 `C4E4D2A9259448E2EE5E06291B316658D5B638FA3D68B6271FF5DBF6E7F53C28`。后端与既有原生外壳放在同目录启动，使用原共享输入安全库，不清除隔离或伪造释放。不是已安装正式版本；既有 0.2.70 MSI 不含这些修改。

本地网页为普通 HTML，计数、输入状态、滚动位置与导航由真实页面事件产生；网页服务记录事件，不提供模型响应或伪造动作。端口 63657 的既有服务保持运行。独立任务失败后不在同轮自动补发，失败历史保留。

## 本轮实现

- 会话请求服务按 HTTP / Devin ACP 分发；ACP 内部请求无 MCP、禁止原生工具、图片按协议发送，沿用已接纳父运行、预算和取消身份。
- 内部请求独立 lane、attempt、进程收尾与模型回执；迁移保留外层既有绑定及锁。没有远端历史重复加载。
- Devin 配置增加显式 Computer Use 复选框；宿主桥复用正式监督执行与台账，不从远端工具通知推导本地执行。
- 补齐正式 CU 工具的权限等级映射，修复它被 MCP 目录过滤的问题；未知工具仍不自动授权。
- 聊天完成判断读取同父运行的 ACP CU 台账，修复实际调用后仍附加“模型没有发起工具调用”的错误提醒。
- 模型目录保留设置页实时发现；每轮不再重复网络目录查询，仍逐轮核对 CLI 及 ACP 精确生效模型。
- ACP 不提供的 token 细分显示未知，不把零值当实际消耗。

## 实操结果（持续补充）

| 标识 | 当前结论 | 事实与截图 |
|---|---|---|
| TEXT-02 | 通过 | 真实回复“SWE2会话就绪。”，9.5 秒，end_turn、进程排空；`evidence/20261005-swe2-cu/swe2-text-chat.png`。 |
| CLICK-04 | 通过 | 次数 0→1；32.6 秒；一次 click，零重规划；step verified、input_delivery=sent、release=released、effect_observed、goal passed；`browser-swe2-click-pass.png`。 |
| TEXT-03 | 通过 | 页面实际显示“SWE2-真实输入-1005”；40.0 秒；一次 text_input（上一轮已聚焦），零重规划；step verified、sent、release=not_needed、effect_observed、goal passed；`browser-swe2-text-pass.png`。 |
| SCROLL-01 | 通过 | 真实页面滚动位置 0→约867 CSS像素；35.7 秒；一次 scroll，零重规划；`browser-swe2-scroll-pass.png`。 |
| NAVIGATE-03 | 修复后通过 | 44.4 秒；target.html→click.html，一次 navigate，零重规划；URL与标题2/2，step verified、sent、release=not_needed、effect_observed、goal passed；`browser-swe2-navigation-pass.png`。 |
| READONLY-01 | 通过 | 30.9秒；零动作，标题与真实次数0两项2/2；新观察，未沿用历史次数1；`browser-swe2-readonly-pass.png`。 |
| CANCEL-01 | 未命中停止区间 | 点击停止时单击和模型终态已完成，不算停止通过；`browser-swe2-cancel-after-input.png`。 |
| CANCEL-02 | 停止回复通过，输入竞争未覆盖 | 页面已从1变2后停止，前端显示中断、提示撤除，外层进程排空，未补发；但旧版消费者关闭导致ACP未知且漏记取消，不能算按下/释放间取消通过；`browser-swe2-cancel-running.png`。 |
| CANCEL-03 | 停止区间未命中 | 单击0→1已经完成，点击停止前回复接近终态；不算规划中取消。 |
| CANCEL-04 | 多步流程取消及下一轮恢复通过，按下/释放竞争未覆盖 | 一次click使次数1→2，尚未填写SWE2-CANCEL-04时停止；CU cancelled、goal=false、1步sent/released，后续聚焦和文本输入没有派发，页面输入仍空白。英文提示随后撤除。截图 `browser-swe2-cancel-04-settled-ui.png`。PAINT-LEGS-05新独立轮实际执行；旧工具投影依据释放与排空事实结账failed，旧ACP unknown历史仍保留。 |
| PAINT-STROKE-02 | 单笔通过 | 一次真实drag、5个确认点、path_completed=true、sent/released、截图变化和横线1/1验收、goal_achieved=true；`paint-swe2-stroke-visible.png`。新用户回合在原聊天室执行，旧取消未知记录保留。 |
| 电脑控制提示 | 调试版活动与撤除通过，正式版待验收 | Paint前台真实任务进行时顶部显示英文“Coolzhu Agent is using your computer”，四边绿色泛光均可见；截图 `paint-swe2-indicator-reassert-active.png`。05结束后提示窗口撤除、Paint保留焦点与画布，截图 `paint-swe2-legs-05-final-indicator-withdrawn.png`。不据此宣称绘图目标通过。 |
| PAINT-SPONGEBOB-01 | 多步失败 | 真实画出身体轮廓和左眼，2次drag均sent/released；工具等待约119.8秒后被丢弃，目标未达成。截图 `paint-swe2-multistep-interrupted.png`。 |
| PAINT-SPONGEBOB-02 | 多步失败，SSE未解决 | 真实补出右眼和笑嘴，2次drag均sent/released；客户端仍约120秒重新initialize。衣裤和手脚缺失，不算完整绘图。截图 `paint-swe2-sse-still-interrupted.png`。 |
| PAINT-SPONGEBOB-03 | 等待修复有效，绘图未完成 | 同任务持续约300秒，未再120秒重连；五次真实drag均sent/released/path_completed。已补衣裤横线及双手，腿脚缺失，规划剩余12125ms耗尽而blocked。截图 `paint-swe2-long-wait-budget-stop.png`。 |
| PAINT-LEGS-04 | 输入链路完成，模型定位失败 | internal-build-19，四次drag均sent/released，动作预算耗尽，goal=false。模型规划把腿脚线条画到身体内；没有将发送成功或图片变化算目标达成。外层及内部ACP全部end_turn、排空。 |
| PAINT-LEGS-05 | 取消后恢复通过，腿脚目标失败 | 新独立任务给予通用坐标换算说明（有指导，不算无提示自主绘图）；1分53秒，2次drag各3点，sent/released/path_completed，goal=false，no_progress停止。第一笔落到白画布外，第二笔仍在身体内。宿主未把图片变化或释放判为人物完成，回复在聊天室可见；`paint-swe2-legs-05-chat-result.png`。 |
| 长程等待适配 | 实际超过120秒通过 | 一次perform创建同父运行任务；每次最多25秒，running时通过computer_use_wait等待同一句柄，不重发动作。03持续约300秒、04约四分钟；桥退出丢弃未完成future，由正式执行器取消收尾；父预算与权限不扩大。不是完整绘图通过。 |
| BROWSER-FORM-01 | 环境未就绪 | 新外壳重启后未打开右栏，零输入，native_browser_panel_unavailable。随后通过真实界面重新打开页面，不改权限。 |
| BROWSER-FORM-02 | Agent 接口缺失 | 一次导航和两次下拉框点击均真实投递，但原生能力未提供键盘、select/check；页面仍步行、未勾选、未提交。no_progress，goal=false；`browser-swe2-form-02-missing-keys.png`。不能归为纯模型能力。 |
| BROWSER-FORM-03 | 按键真实生效，流程误停 | internal-build-20；click 与 end 两步 sent/released，菜单显示自行车，但未 Enter 确认。进展只比较文字而漏掉焦点，连续无进展误停；`browser-swe2-form-03-key-sent-progress-stop.png`。没有勾选、提交事件，不算整套表单通过。 |
| BROWSER-FORM-04 | 选择通过，整套失败 | internal-build-21；click、end、enter 后页面实际显示自行车，网页服务收到真实 select=bicycle。随后模型连续两次重复 enter，未点击复选框或提交按钮，5步均 sent/released 后 no_progress、goal=false。全部ACP end_turn并排空。`browser-swe2-form-04-repeated-enter-final.png`；不能将单项选择通过算整套通过。 |
| BROWSER-FORM-05 | Agent 路由遗漏 | internal-build-22；本轮写“右栏表单”，范围识别仅匹配“右栏网页”等，误走未连接的外部扩展，extension_unavailable，零动作。`browser-swe2-form-05-routing-failure.png`。已补明确的右栏/右侧表单同义表述；普通右栏设置、统计或轨迹不能据此改成浏览器操作。 |
| BROWSER-FORM-06 | 路由修复通过，顺序与整套目标失败 | internal-build-24；真实进入原生右栏，先click产生check=true事件，然后navigate重置表单，再click下拉框和两次end；5次输入均有明确结算，但no_progress停止，goal=false。导航后的新表单没有select/submit事件，不能算整套通过。`browser-swe2-form-06-sequence-failure.png`。 |
| BROWSER-FORM-07 | 整套通过（有明确步骤指导） | internal-build-25；初始步行/未勾选/未提交，5步依次click→end→enter→click复选框→click提交；全部sent/released，零重规划、无导航/刷新/重试/补发。网页服务依次记录select=bicycle、check=true、submit={mode:bicycle,ready:true}；新鲜页面逐项3/3，goal=true、succeeded。完整真实回复在聊天室可见，2分25秒；`browser-swe2-form-07-pass.png`。步骤指导测试通过不等于所有任意网站自主规划通过。 |
| 取消/关闭竞争 | 待执行 | 必须真实命中输入区间，完成后才关闭面板不算竞争通过。 |
| 正式安装版 | 0.2.71启动及表单通过，Paint误报待修 | 安装身份及实际软件截图核验；C71表单5步3/3，Paint零动作把旧横线当新增，不算通过。详见0.2.71交接报告，不将调试成功推广到全部安装版功能。 |

成功输入的 `not_needed` 是此文本动作的正式回执，不改写成鼠标按键 Released。台账完成、输入发送和目标达成分别记录，不互相替代。

## 失败记录与定位

| 标识 | 阻塞点 | 处理与验证边界 |
|---|---|---|
| TEXT-01 | 每轮远程目录查询30秒超时，未提交 | 从请求路径移除重复目录查询；设置页发现和 ACP 生效核验保留。TEXT-02 实际成功。 |
| CLICK-01 | MCP tools/list 没有 CU | CU 不在同步 registry，权限映射原为 Unspecified；补精确工具名映射后出现真实调用。 |
| CLICK-02 | 临时服务未注入 input-safety root | 使用正式 launcher 自检返回的原共享根；不另建库、不删历史、不绕隔离。 |
| CLICK-03 | 后端和外壳不在可信同目录 | 按原正式目录结构启动。保留宿主来源核验，CLICK-04 实际成功。 |
| TEXT-01（浏览器） | 未明确右栏，选中未连接的外部扩展 | 测试提示遗漏本轮后端选择；新任务明确右栏原生浏览器，不放宽路由保护。截图 `browser-swe2-text-backend-selection.png`。 |
| TEXT-02（浏览器） | 一动作预算被聚焦 click 用尽 | 只有聚焦，无输入；sent/released、目标失败。新任务允许聚焦+输入两步；TEXT-03 成功。截图 `browser-swe2-text-focus-budget.png`。 |
| NAVIGATE-01 | 认证 URL 引用附带有效正文索引时，被误判为杜撰正文 | URL、标题、视口原本来自独立宿主字段；改为直接与对应字段核对。任意伪造文本、越界索引仍拒绝。保留旧失败，不能用新规则回写旧目标。 |
| NAVIGATE-02 | 明确允许导航，同时禁止点击/输入/滚动，却被判为只读 | 范围解析保留明确导航许可；绝对禁止发送输入仍优先，否定“允许导航”不反转权限。用正式同链路新任务回归。 |
| PAINT-STROKE-01 | 新任务提交前被旧ACP未知绑定阻止 | 零Paint动作，7.3秒失败。宿主取消时间1791135712410已持久保存，旧外层进程已排空且CU completed；ACP取消字段却为0。这是消费者关闭竞争，不能归类为模型绘图能力失败。 |
| PAINT-SPONGEBOB-01/02 | HTTP工具请求约120秒重连，导致宿主执行future丢弃 | 根轮900秒、CU尚未到期，没有宿主人工停止记录。build16加SSE进度仍失败，不能归类为模型绘图能力或Windows必须审批。分段等待同一任务已由03/04实际超过120秒验证。 |
| PAINT-SPONGEBOB-03 | 配置600秒被代码静默截为300秒 | `ConfigComputerUse::budgets`原hardcap300。现尊重显式5秒至24小时配置，默认120秒不变；真实执行仍取CU与根deadline的较小值，不续期。新增较短根deadline截断600秒预算的检查。 |
| PAINT-LEGS-04 | 模型视觉定位不正确 | 第一笔points为[[0.271,0.67],[0.268,0.77],[0.24,0.779]]，对应真实frame的screen_rect=[0,61,2560,1152]、canvas_rect=[11,106,2538,1096]。宿主映射落点与原图笔迹相符，实际位于身体内；后续笔划仍误定位。保留模型能力失败，不把它误归因于未提供绘图接口。 |
| PAINT-LEGS-05 | 公式指导仍未正确定位 | 第一笔points=[[0.0745,0.4005],[0.0745,0.4505],[0.068,0.4505]]落在白画布左方；第二笔[[0.295,0.568],[0.295,0.671],[0.327,0.671]]落在身体内。原图2560×1152、screen_rect与canvas_rect和04一致。传输、输入释放、原图回读均有记录；绘图质量失败不再以扩大权限或修改输入映射处理。 |

## 停止收尾修复与风险检查

原生页面按键新增独立 `native_browser_key_input` 模块。协议仅接受单个 home/end/tab/enter/escape；与点击复用原父运行、资源、节点、预检票据、一次性许可、执行实例及取消结算。宿主只读核对实际 activeElement 后，通过 WebView2 CDP 投递真实 rawKeyDown/keyUp；在同一 UI 闭包排队释放，不用 JS 写控件值、不发送全局系统快捷键。回执必须确认按下和释放；无按住输入 ACK 不能用于按键结算。规划快照仅增加焦点布尔/索引，不传原控件值。

表单03暴露监督进展缺口：点击聚焦和下拉暂选尚未改变正文。修复只将宿主真实焦点、视口、URL、标题及节点变化作为中间进展，随机观察 ID 或节点句柄更新不计进展，目标达成仍逐项核对新鲜页面事实。焦点未选中时 null 合法；复核期间发生焦点变化仍使旧观察失效。原 no_progress 上限、权限、输入隔离和根预算均保留。

表单04的重复Enter来自实际SWE-2规划，不是宿主确定性动作替换。规划说明现在明确：capabilities描述可用动作接口，select/check/submit=false不禁止click操作对应控件；上一步整体目标failed也不等于已确认输入失败，应依据最新页面继续未满足目标。前端失败提示改为“本轮任务未完成，详情见回复与运行轨迹”，避免在模型回复已完整结束时错误宣称回复未完成。失败终态、工具事实及历史不改写。

表单06继续重复End，暴露上一步反馈只有key_combination类型而没有具体按键，无法区分暂选和确认。internal-build-25增加同运行、精确上一步的只读投影：仅接受已保存动作中的单个home/end/tab/enter/escape，作为previous_step_feedback.keys；不传URL、输入文本、动作原文或其它历史，也不修改动作或放宽预算。07已按真实5步完整回归通过；没有靠删除no_progress限制通过。

表单与按键回归二进制：internal-build-22后端SHA256 `06AF680A7A1C6ECED6D06E5BA11580F9C08E7FB62682119DE7658921740FC36E`；internal-build-24为 `ECD87E0BDCE10F5D9A319A999D02F0EC3DAB24A8E74C73F3C7C74BF23E9642DC`；键盘原生外壳SHA256 `A9E68C2BAFCF1B2C7667B224C633E219E0F5E9C08A88D60061CAEF9F6F03FA6D`。均为调试版，不能替代正式安装验收。

前端停止后关闭消息消费者，原增量投影函数把关闭当作协议读取错误，导致worker无法继续接收远端终态。新实现只在取消已请求且消费者已关闭时丢弃迟到展示增量，继续按原5秒宽限读取ACP终态和回收进程；正常消费者的队列溢出仍报错。

历史未知回合不改写为成功或取消确认。新用户回合使用新的远端连接：仅当旧进程实际排空、精确旧回合已有ACP取消或同库宿主持久停止记录、全部同来源工具结账，才归档旧绑定并建立新绑定；旧远端ID不得复用，旧派发身份失效，不自动重发提示。未停止、跨房间、仍在途或工具状态不明时保持阻止。这个变更只处理会话收尾，不修改输入安全隔离或权限。

CANCEL-02的旧未知事实将保留；Paint的新独立请求用于验证同一聊天室可继续执行，而不是修正旧任务的目标达成结论。

长程失败只在同父运行全部ACP进程排空、CU确有cancelled终态、每步输入与释放均有确定记录时，将工具等待投影结账为failed；追加`tool.worker_settled`审计保留之前的`cancelled_outcome_unknown`。跨房间、仍有worker、NULL/unknown释放均不允许结账。原ACP未知历史及输入步骤不改写为成功。新用户回合可以在这些事实齐备后归档旧绑定，不复用旧远端会话或补发旧任务。

Paint活动时找到原生外壳的空标题提示窗口，截图`paint-swe2-control-overlay-active.png`只确认下沿及左下泛光，没有确认顶部英文和完整四边。该项仍是部分验收，需与完整绘图一起回归。

新编译外壳确认主屏2560×1440、DPI scale=1.5、加载computer-use-indicator.html。外壳新增不抢焦点的置顶重申；05已在Paint前台实际看到顶部英文及四边泛光，任务结束后撤除。外壳SHA256为 `F7E8B03AC7AB36BA1555127BBF9A1870C4AB6A1105C7F908B6ACB9B1D5786AED`；这是新编译调试外壳，尚未正式安装版验收。

官方参考：[Devin MCP配置](https://docs.devin.ai/cli/extensibility/mcp/configuration)、[MCP Streamable HTTP](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)、[MCP进度通知](https://modelcontextprotocol.io/specification/2025-06-18/basic/utilities/progress)。SSE心跳不能作为完成证据，也不能据此宣称客户端时限已被取消。

修复回归后端 internal-build-14 SHA256：`D7B86835312D7E6AC93F15D914287C46A409E337346A071A3181316925E0A5CE`。ACP定向检查51通过、0失败、3忽略；单笔实际绘图使用该二进制。截图与协议图片的源图摘要、帧坐标绑定、5点笔划及释放均在正式CU回执保存，不能只依赖模型文字结论。

## 回归检查与后续测试设计

已执行离线 Web build（最新 internal-build-19，SHA256 `81C1C55B3DF2032899BF1CBA769FF66BBC3AF88C85A21900960CECE808CFC3B4`）、ACP定向检查（53通过、3忽略）、最新bridge检查（7通过）、CU存储与预算（45通过）、前端Devin配置（3通过）、范围解析（7通过）、原生浏览器证据（5通过），以及正式CU权限映射检查。忽略项和契约测试不算真实模型实操通过。原远端HEAD的CI成功不代表本轮未提交源码通过。

最新internal-build-25已离线编译，SHA256 `574E1F395BF49C9B5D34CC1EE6AD4CF599B5749F5411A6F531CF15C4CB47EBF7`。完整Web二进制原有检查串行执行：1379通过、0失败、6忽略；另有库8项和可执行入口1项通过。原生外壳64通过、0失败，模块连接8通过，Devin配置JS 3通过。未新增模型回复夹具，软件实操仍使用真实SWE-2。首次全量检查和真实CU并发，27项失败；实操退出后并行检查剩1项环境共享冲突；按远端CI原有串行方式复核全部通过。保留这两次失败日志，不能删除互斥锁使测试通过，也不能在真实软件正在受控时并发执行占用同一物理输入锁的检查。

临时验收配置沿用通用controller参数，timeout_seconds=600，根轮预算900秒。build18之前代码仍截为300，build19起尊重600；源码默认120秒未修改，不增加Paint专用权限或预算特例。

后续测试应围绕：开关保存与工具子集、内层精确模型及图片能力、超时/取消进程收尾、当前来源页绑定、输入焦点及预算、不同输入释放类型、Paint 画布与提示生命周期、正式包身份。不要用大量映射实现的单元用例取代软件截图；失败应分别归类为运行环境、接线、预算、模型规划或输入释放，不能统一算模型能力问题。

## 正式包交接（0.2.71）

本轮实现与实操证据已提交为 `49d0a1b6d5e5995a3d75bdb1cbd78c97ceb0c28c`，PR #78 的该提交 push 检查（37230846704）与 PR 检查（37230849525）均已成功。发布包采用正常 locked/offline release 构建，未跳过组件编译；固定 DSH 运行资源按现有锁核验后复用。编译期间源码快照保持不变，包安全扫描无发现。

`CoolzhuAgent-0.2.71.msi` 已生成，SHA256 为 `C7CD1580A42805B7AE127807E8092D9950A9BF7CC68562741DCB4686ED65DDCF`。源码快照为 `414d6ffcdd04e7479346f0abf2b5528a6065c187fe24196575fc05e4c2213440`，载荷摘要为 `f4d7d7b07320632c87269f781da93c95c7e96eb98e7ce3b1f2782925045a2bf7`。安装收据与构建身份已归档到 `docs/testing/release-0.2.71/evidence/build-identity/`。发布证据文档的后续提交不改变此安装包所引用的源码提交。

自动普通安装尚未成功。首次使用正斜杠路径返回1619；修正为系统原生路径后正确打开安装包，但在移除旧版本阶段返回1603，详细原因是 Windows Installer 错误1730（旧版卸载需要管理员身份）。注册表仍显示0.2.70。这与聊天室完全访问不同，不能通过清除输入隔离、改写会话权限或运行调试二进制替代正式安装。已交由用户手动完成管理员安装，正式安装实操仍待执行；不把包生成或CI成功当作正式版功能通过。

详细变更、剩余验收及可复用测试步骤见[0.2.71改动与验收交接报告](../release-0.2.71/change-and-acceptance-report.md)。

用户随后完成0.2.71管理员安装。安装版关键二进制与载荷清单一致；关闭精确已结束的验收后台后，日常工作区启动恢复。原验收聊天室正式版表单通过（1分9秒、5步、3/3），但Paint新增笔迹验收暴露零动作误报，原协议成功终态保留、软件验收不通过。正在修复桌面初始单帧不能证明本轮新增成果的问题；后续源码与调试结果不冒充0.2.71安装版已含修复。

## 本轮新增条件修复与实操（internal-build-26）

明确要求“本轮新增”的桌面成功条件，不再用动作前同一观察帧直接判完成；动作后仍交给同一真实模型对照原图验收。普通存在性/只读检查保留零动作完成。只识别明确条件，不宣称解析所有自然语言语义；不替模型生成坐标，不改变权限、预算、释放或历史终态。后端SHA256为 `05EDE018A037C848F959A20D51ACB1F7FF38B0CCE72340706E90FFD97CDFC53B`，外壳为 `A9E68C2BAFCF1B2C7667B224C633E219E0F5E9C08A88D60061CAEF9F6F03FA6D`。离线build通过，完整Web串行检查1380通过、0失败、6忽略；新增检查仅验证这个纯前置条件，不使用模型回复夹具。

| 任务 | 真实结论 | 边界 |
|---|---|---|
| SWE2-PAINT-NEW-EFFECT-26-01 | 观察阶段 `target_ambiguous`，0输入，未完成 | 仅指定应用时目标不唯一；保留失败，不补发。 |
| SWE2-PAINT-NEW-EFFECT-26-02 | 指定实际“无标题 - 画图”窗口后，1次真实drag，3点、1200ms、sent/released；最终 `budget_exhausted`、criteria 0/1 | 不再初始零动作误报。模型选相对点 `[0.62,0.16]→[0.66,0.16]→[0.70,0.16]`，宿主容器rect `[11,106,2538,1096]` 是窗口内容容器，并非白色绘画区；映射落在工具栏附近，软件截图未确认新增横线。图片变化包含键位提示撤除，不能作为新笔迹证据；外层回复“有真实新笔迹”也不能替代截图。保持未完成，不按输入释放算成功。 |

第二轮耗时1分2秒。两轮共4个内外层ACP尝试均requested/effective=`swe-2-medium`、end_turn并排空。活动时顶部 `Coolzhu Agent is using your computer` 与完整四边泛光可见，终态后提示窗口撤除。证据为 `paint-new-effect-26-initial.png`、`paint-new-effect-26-hud-active.png`、`paint-new-effect-26-final-indicator-withdrawn.png`、`paint-new-effect-26-model-reply.png`、`paint-new-effect-26-receipts.json`，位于本报告证据目录。初始误报修复的实操生效，不等于Paint绘图通过；这些调试证据不代表0.2.71已包含修复。

## 后续 Browser 边界验证

新增 A～H 真实 SWE-2 场景见 [Browser 边界完整报告](browser-boundary-20261005/README.md)：后端在途停止、原页面释放、无后续输入、新独立任务恢复通过；停止原因误报和机械输入语义误拦均已修复并实测。前端按钮/关闭/页面替换微区间仍未覆盖，Click 自然导航虽实际跳转，最终来源 URL 校验失败，保持未通过。没有本轮新增 Paint 绘画或新 MSI。

Opus 三轮补充只读审查及主会话裁决一并归档，原始事件、模型/运行回执、软件截图可供后续测试设计复用。临时 8767 后台和 HTML 服务已收尾，原日常 8765 正式版、Qwen/聊天09 已恢复；正式版仍不包含这些新补丁。

## 后续推进：自然导航通过与 Paint 新缺口

详见 [本轮真实过程](browser-transition-20261005/README.md)。I/J/K2/L/P/O4 分别实际验证同源、跨源、Click→Navigate、Navigate→Click、标准地址更新及 popup，目标满足，真实目标页输入 0；Q2 的同文档 URL 显示已修复并实拍。K/Q/O/O2/O3 原失败全部保留。网页释放都早于卸载，不能算 down/up 微区间竞争通过。停止/关闭/手动替换、显式导航被 popup 覆盖、fragment/历史切换、缩放多屏与新包正式回归仍待验收。

代码职责：适配器记录已结算输入后的实际页面来源，目标仍基于新观察逐项判定；popup 排队独立序号，不再误增普通自然跳转的输入 revision；同文档 SourceChanged 只同步地址显示，不授予输入权限；对象限定的禁止点击不误当全局只读。没有移除新鲜节点/资源/授权校验，没有在前端增加调试卡片。

Paint R 两腿与 S 完整人物分别真实 2 笔/6 笔，均未通过。已给模型现有 UIA 绘图区候选及明确坐标空间，未改原 points 容器契约，主会话未给落点或代绘。确认新增基线的 Agent 缺口，独立模块保留任务起始图/几何，仅用于验收；progress 仍比较上一帧。T 两笔回归未通过，首笔与旧图相连、第二笔独立；第二步又暴露出站错误描述被嵌套结果分支吞掉。已修错误分类与合法图片提示出站容量，新增仅数字大小/图片数计数；T 的具体旧超限原因尚无字节证据，不追认成功。结构化新增目标字段与完整人物仍待完成。

新增 Opus 第六至第十二轮只读调用为 8、11、0、5、0、0、0；加此前五轮，共 105 项。零工具轮次只按提供的事实讨论，不称完整源码审查。所有新补丁仍是独立编译验收版，正式 0.2.71 不含；PR #78 保持草稿，不能把旧 HEAD CI 当新修改通过。最终部署摘要、U 结果、最新工程检查和日常恢复在本轮证据目录补充。


## 本轮收尾：U 通过、V 未通过与候选包计划

U 的两个新增独立菱形通过，真实两笔、2/2、goal=true，三帧原图提示 1358145 字节完整返回，未以 released 替代目标。V 的完整人物仍未通过：四笔 released、0/3、no_progress，270.2 秒；新矩形与旧横线/旧人物交叠，眼嘴和两腿未完成。三次三帧请求 1384625/1387135/1388890 字节均 end_turn/drained；这次没有图片发送故障。Paint 截图和 SWE-2 实际聊天室回复见本轮证据目录，不借旧人物追认新成果。动作接口、图片链、多步简单图形可用与完整人物能力未达标分别记录。

Opus 第十三/十四轮实际只读工具调用为 27（read 14/grep 13）与 10（read 4/grep 6），此前 105，累计 142。原始审查分别在 opus-final-code-review.md 与 opus-followup-review.md。第十四轮模型的文字统计含列工具，工程台账以 read/grep 10 项为准。采纳同文档 SourceChanged 不清除普通加载状态、首次观察固定验收基线、本轮“新画/新绘”按新增处理；主会话补充在首次可规划桌面动作前固定原图，覆盖最早观察无图的情况，不增加输入拒绝分支。生产 planner 在每次 run 独立构造，同一个 executor/future 保留分段等待状态。

最后小修已离线编译：完整 Web 1387 通过、0 失败、6 忽略，另有库 8/入口 1；Shell 浏览器相关 8 通过。最后动作规划前捕获基线的改动又完成真实 build 和现有两项基线检查；不把工程检查说成新版本软件回归。U/V 实操的实际二进制摘要与最终源码/工程二进制摘要分列在 review-build-manifest.json，最后小修尚未软件实操。

临时验收 ACP 全部排空，外壳正常关闭，精确后台和自有网页服务退出，临时配置恢复原字节，安全库不清不换。日常正式版 0.2.71 恢复启动自检通过，健康 10 正常/1 工作区提示/0 错误。日常显示原 Qwen/聊天09 是环境恢复，**没有改用 Qwen 进行本轮实操**；所有 I～V 实际 requested/effective 均 SWE-2。后续验收继续指定 swe-2-medium，审查指定 claude-opus-5-5-high。

优先后续：将本轮收敛代码/证据提交 PR #78 草稿，生成 0.2.72 候选包；安装版先复跑自然链接、popup、SPA 地址同步与两笔新增图形，再补停止/关闭/手动替换恰 down/up 内的竞争、显式导航覆盖 popup、同文档 fragment/历史和多屏缩放。完整人物保留未通过，当前证据未发现尚缺 drag 执行接口；不通过追加画图引擎或放松验收强行闭环。新包总回归和其余侧栏/市场/开机动画收尾仍待完成，微信保持不动。


## 0.2.72 候选交付更新

远端 PR #78 已在此前合并，本轮继续推送的增量改为[草稿 PR #79](https://github.com/coolzhulike/coolzhuagent/pull/79)，当前无冲突，不转正式评审。代码提交 f3bb374 的[远端检查](https://github.com/coolzhulike/coolzhuagent/actions/runs/37271308515)已成功，后续报告提交不更改构建源码。

0.2.72 正常完整 release 构建成功，MSI 摘要 c0c75f28b610574906f3fe05f1ca50258428a04073b03794ace2618b1792aa0c，已复制至 Desktop/coolzhuagent/dist 并核对。一次正常静默安装返回1603，日志明确 Windows1730：移除旧版需要 Administrator。注册和磁盘仍0.2.71；安装过程停止了原服务，回滚后已正常重新启动并核验自检/健康0错误，没有留下失效控制台。

具体改动、精确版本身份、已过/未过矩阵及其它模型测试记录要求见 [候选改动与测试交接报告](../release-0.2.72/change-report-and-test-handoff.md)。新包尚未安装，不声称安装版回归已过；后续真实测试仍精确使用 SWE-2，不用日常 Qwen 会话替代。
