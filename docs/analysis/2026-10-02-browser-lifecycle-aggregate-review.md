# Browser生命周期聚合验证与真实UI边界

## 结果与新增工作

承接输入deadline修复，本轮完成聚合和模块链接检查，补足真实SQLite派发事务的5种遗漏场景。没有再修改生产状态机、超时值或安全隔离；新增代码仅扩展`computer_use_executor.rs`已有测试，从4种场景扩为9种，仍计为1项测试。

新增场景：派发前内存取消、票据到期、绑定失配均拒绝claim；派发后父运行停止时拒绝下一步claim，但仍允许原动作释放结算；release_unknown落库后拒绝迟到released覆盖，未知事实保持不变。后两项直接查SQLite状态，不以断言错误码代替持久事实检查。

| 实际检查 | 结果与限制 |
| --- | --- |
| Web全套，首次默认存储环境 | 主程序1295通过、11失败、3忽略，exit101；库8与宿主1通过。失败原样归档 |
| 首次11个失败项，逐项独立进程和临时存储复核 | 11项分别通过；只给测试子进程覆盖session DB/legacy JSON/attachment路径 |
| Web全套，独立`tmp/`存储，串行测试 | 主程序1306通过、0失败、3忽略；库8通过、宿主1通过，exit0，主程序64.60秒 |
| 根`module_linkage_smoke` | 8通过、0失败，exit0；含受控输入入口、帧绑定、测试环境守卫接线 |
| 桌面`browser_panel::tests` | 7通过、0失败、57过滤，exit0；涉及资源可见性、权限边界和面板布局，不含真实WebView生命周期 |
| 新增场景所在派发CAS测试 | 1通过、0失败、1308过滤，exit0；9种模式，其中5种新增；之后也包含于通过的全套 |
| Web离线build | exit0，15.78秒；既有warning保留 |
| `git diff --check` | exit0；既有inventory CRLF提示 |

首次失败中9项报会话数据库无法打开，1项附件上传返回500，1项开发工具列表断言失败。相同测试二进制分别运行于临时存储时全部通过；随后同进程全套在临时存储下也通过。证据支持默认测试存储/状态环境影响，不能把首次失败删掉或当作deadline缺陷，也不据此声称所有共享状态风险已消除。测试覆盖仅为子进程环境中的`COOLZHU_WEB_SESSION_DB`、`COOLZHU_WEB_SESSION_STORE`、`COOLZHU_WEB_ATTACHMENT_STORE`，没有修改用户配置、安全权限或运行实例数据库。

## 状态路径覆盖与剩余边界

| 路径 | 工程证据 | 必须真实UI确认的部分 |
| --- | --- | --- |
| claim前取消/停止/权限收紧/过期/票据或绑定错误 | 真实临时SQLite事务拒绝派发，保留pending；已有受控入口接线检查 | 真实用户停止与UI派发的时点关系 |
| 宿主领取前Close或navigation资源撤销 | 传输在锁内以NotDispatched结算；到期领取被拒绝 | 真实host轮询与面板事件的先后关系 |
| 已领取后Close/navigation | 不能伪造零交付；期限内原资源回执仍结算；重复、旧代次、错误执行实例回执拒绝 | WebView隐藏/销毁是否保留原controller回调，必须命中按下至释放区间 |
| claim后Cancel/父停止 | 原释放结算保留，下一步派发拒绝；适配器执行阶段继续等待原回执，不使用普通取消截断等待 | UI停止是否发生在真实输入期间，控制器是否停止后续输入 |
| ReleaseUnknown后迟到成功 | SQLite原未知终态不可覆盖；传输到期返回409；不延长期限、不重放 | 实际3秒超时、晚到8秒事件与隔离后的物理状态 |
| 导航引起旧观察失效 | 全套中的来源/目的资源、同URL SPA和替换宿主验证通过；源码核对revision先变化且loading撤资格 | 实际载入/重定向、文档frame/loader变化与旧节点拒绝 |

源码审查：`browser_panel::invalidate`先使资源失效再调用`retire_view`；后者隐藏视图并异步等待执行gate后关闭。执行动作使用try_lock防重入；点击在同一原controller的UI闭包顺序提交按下/释放，不等down回调才提交up。宿主把handle任务独立spawn，普通后台轮询变化不会主动abort它。以上是代码路径事实，不是实际COM回调或窗口销毁成功证据；关闭整个进程/崩溃也不能由gate单元测试推导安全释放。

在本轮已审查范围内，没有再发现可独立复现的生产缺陷；无需为模拟WebView销毁而新增一个与真实平台无关的替身状态机。下一关键验收仍依赖本地Windows CU恢复、原事故核对和正式恢复流程。父云端Linux工具ready不提供本地验收能力。

## 源码与安装包对应

分支`codex/cu-preinput-followup-20260930`，HEAD `8181f08ae9c50d3e41aabd32a6af85343dcd92e8`，workspace版本0.2.0。当前未提交变更含上一轮`native_browser_input.rs`期限修复，以及本轮`computer_use_executor.rs`测试扩展。精确源文件SHA256与HEAD文件SHA256记录在[本轮证据索引](../testing/browser-lifecycle-aggregate-2026-10-02/evidence-index.json)。

- 正式安装仍是0.2.63；本轮没有重启或替换正式实例。
- 旧0.2.64是此前生成的DSH候选，不包含新deadline修复；未重打包、未安装。
- 0.2.63/0.2.64 MSI哈希和原事故记录均复核未变。不能把当前测试结果标在旧0.2.64包上。
- 本轮没有真实GUI输入、项目Qwen调用、Paint验收或安全放行；此前人工复核记录继续保留。

相关：[期限修复报告](2026-10-02-browser-input-deadline-review.md)、[CU端点/占用诊断](2026-10-02-browser-use-cu-availability-review.md)。
