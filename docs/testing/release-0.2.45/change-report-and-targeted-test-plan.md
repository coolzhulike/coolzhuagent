# 0.2.45：节点引用有效期与人工接受累计

2026-10-01，主会话独立审查、实现与测试，不使用子代理。指定Pro复审按用户要求暂缓；真实Qwen配置保持不变，微信不改不测，Devin搁置，PR74保持Draft。

## 触发及改动

044真实Qwen AK正确走当前URL原生分支，但初始验证16.235秒加规划6.903秒超过20秒节点缓存；Scroll预检`native_browser_node_expired / not_sent`，页面仍0。正式044本身另暴露空清单审批覆盖过去已接受的遗留运行，需要第二次正常确认。不能将这些失败追改为通过。[044实操](../release-0.2.44/installed-native/acceptance-and-open-issues.md)及[方案审查](../../analysis/2026-10-01-browser-node-lifetime-and-recovery-review.md)保留。

| 模块 | 变化 | 保留的边界 |
|---|---|---|
| Shell native_browser_nodes | 随机观察引用有界保留120秒，覆盖默认CU总预算 | 引用不是授权；最多4份/128节点；异文档/房间/导航拒绝；真实AX/几何/命中重检；预检及执行票据2秒；动作后旧引用撤销；总deadline不延长 |
| Web input_safety_store | 同scope已落库决定的运行ID累计、去重排序 | 空清单不撤销过去接受；未声明的新运行与阻断继续等待复核；不改Unknown许可、事故、审批或恢复资格；latest仍只展示最近决定 |

不增加schema、Provider参数、Agent循环或权限系统；不通过直接写生产DB修复运行状态。已授权的常规权限审批由主会话使用Computer Use正常操作。

## 工程验证

Web离线build退出0（34.31秒）；Shell离线build退出0（23.98秒）。既有节点测试模拟25秒等待后仍可解析，过期/异身份/撤销拒绝；既有人工决定测试覆盖后续空清单、去重、不同scope及未声明阻断。

首次Web完整回归与真实AK同时占用物理输入范围，1281通过/9失败/2忽略，出现`input_owner_busy`和恢复协调Busy，原日志保留。软件空闲时重跑：Web1290通过/0失败/2既有忽略（47.45秒），Shell64通过/0失败（1.26秒）。工程检查不计为真实模型实操通过。

## 包与软件验收

正常release构建与独立核验通过，报告`pkg-report-release-20261001-102523661-6dfb9189`。MSI 246930813字节，SHA256 `f83287014d64ba47a0cf02738e64652e07d4df3926af15a07873e26b82cd128a`；源码快照`e07e736fb16bd42bd41d27b9ac214890ab306abd85a93b43ccf02a09f96675ce`，载荷摘要`d3775c4952bd1fc51f51173a3e67ed9ee0e6db3877b618cdc0f1b862ad966bc0`。六发布门、10关键产物、859文件/324140518字节的完整载荷与归档原件摘要均通过，内容扫描safe且0 findings。种子HEAD不代替当前源码快照。

Windows Installer正常安装返回0，注册唯一045。2026-10-01 10:56首次启动后核实正式Shell PID25000、Web PID29980，8765唯一监听归Web，安装目录10关键产物摘要与包报告一致。11:00正常重启后为Shell30328/Web30716，重新核验10产物及唯一监听通过，[安装事实](installed-native/installed-artifacts.json)保存最新身份。[包证据](evidence/build-identity/pkg-report-release-20261001-102523661-6dfb9189/staged-verification.json)保存原件。旧本地样本服务已停止，正常重启静态HTML服务至127.0.0.1:58420；真实Qwen测试前通过右栏正常载入新地址，未修改模型、Base URL或API KEY。

| 用例 | 前置与触发 | 通过证据 | 状态 |
|---|---|---|---|
| BU045-SCROLL-AL | 原房间、原Qwen、当前普通长页位置0；恰好一次向下Scroll amount1 | 实际page_y=289.3333435058594、顶部289、1 verified步骤、sent/not_needed/effect_observed/passed、动作后generation2新观察，前后原图 | 正式045真实Qwen通过，父completed，CU succeeded/goal=true；43.3秒 |
| BU045-READONLY-AM | 同一实际页面，禁止全部输入 | input_supported=false、步骤/动作0、真实标题和page_y=289.3333435058594，原图与账本 | 正式045真实Qwen通过，父completed，CU succeeded/goal=true，2/2标准；25秒 |
| BU045-RESTART | 完成动作后正常退出启动 | settled持久面板不新建错误在途阻断；safe且待人工复核不复现 | 正常退出/启动通过，safe/accepts=true，待恢复0、未确认阻断0、未确认遗留0、已接受遗留2；没有新增审批 |
| REC045-FOLLOWUP | 新正常审批无新增遗留清单 | 历史接受不丢失，新未声明运行仍拒绝 | 工程通过，正式触发待验；不人为破坏生产输入 |
| BU045-RESOURCE-AN | 已取得原生观察且在模型验证/规划期间正常关闭右栏 | 原计划输入前重新观察失败，blocked/goal=false，0实际动作/步骤、not_sent/not_needed、禁止重试，原图 | 正式045安全拒绝通过，返回native_observation_timeout；关闭前原生观察及真实Qwen规划均已完成，不计点击目标成功 |
| BU-TYPE/NAV | 焦点/selection、敏感字段及新文档身份 | 真实Qwen输入/导航、准确效果、范围/资格回执、原图 | 尚未实现 |
| CU-PAINT | Browser优先项后正常Paint | 连续闭合轮廓/简易海绵宝宝、释放、有效图像验收；四边泛光、准确英文提示、结束撤除原图 | 尚未续测 |

DSH远程插件实际安装运行、交互中资源变化及四项总体审核仍开放。后续按软件真实事实更新，不外推现有Click/只读或旧Paint结果。
