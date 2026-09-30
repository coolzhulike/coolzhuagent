# 0.2.40 原生浏览器只读验收修复与测试交接

2026-10-01。主会话独立实现、检查与测试，不使用子代理。用户顺序：先闭环 Browser Use，后 Paint。当前实际运行仍为037候选；本页的源码与工程结果不能代替新包安装后的真实Qwen实操。

## 问题与修复

037真Qwen Q轮同时传URL/window而被surface_conflict拒绝；R轮仅传URL，已获得认证原生AX观察，但原生桥固定verify=false，外层只有证据引用，无标题正文，最终verification_failed。两轮均未通过，没有输入。原生窗口照片与具体运行ID见[Q/R实证](../release-0.2.37/candidate-joint/browser-read-q/q-r-real-qwen-results.md)。

039补入只读语义验收及宿主事实回传，已完成正常release构建；随后既有ChatGPT审核会话指出模型判断期间同URL的SPA变化、隐藏或换环境可能使旧观察失效。因此039保留为历史构建，不推荐用于本轮验收；040补齐结束前重新采样。审核原文见[实际页面DOM归档](../release-0.2.39/review/20261001-readonly-freshness-review-dom.txt)与[页面截图](../release-0.2.39/review/20261001-readonly-review.png)。页面仅显示“极高”，完整模型版本未核实；这是窄技术审查，不是GPT6 Pro或四项软件总体验收。

职责与最终行为：

- `native_browser_host.rs`：沿既有认证broker，返回已核验冻结父运行/工程/房间、短租约与同宿主资源的实际AX及独立观察请求ID。不增加公开路由、网页指令或输入能力。
- `native_browser_adapter.rs`：输出受限宿主事实及host/room/workspace/resource/generation/navigation身份；所有输入capability仍false。每次实际AX取样有独立凭据。
- `native_browser_verification.rs`：只接收有效、非空、精确目标URL与宿主只读标识的事实。严格判定schema、16KiB结果上限、逐项唯一索引、非空512字符证据，以及每项至多8个合法事实节点索引。回传宿主URL/标题、节点数/截断信息和相关片段，最多24节点/4096字符；不使用模型复述替换事实，不重复整个128节点快照。
- `computer_use_planner.rs`：复用当前真实模型、已有预算/取消/统计链路。验收请求纯文本、无tools，禁止执行页面指令。判断返回后在阻塞工作线程重新调用原生宿主采集，避免阻塞认证心跳；核对同宿主、工程、房间、资源代次、导航代次，以及有序URL/title/truncated/role/name投影。新旧观察ID必须不同。变化以`native_browser_observation_stale`结束，不自动另开模型循环。
- `computer_use_store.rs`：只读判定沿既有验收结构脱敏，保存索引/布尔/长度与字段类型，页面证据和模型原始回复不进入普通诊断。
- `computer_use_executor.rs`：规划器沿用适配器的同一冻结父上下文，不能由模型创建另一宿主作用域。

判定未满足时用`native_browser_verification_failed`返回有界宿主事实，不再进入固定false后的无效动作规划；无效schema、观察不可用、观察变更、取消与超时保持各自错误。只有实际完成新取样且匹配，才接受语义判定；仍只证明读取任务。`input_supported=false`与不证明点击/输入/滚动/导航的notice贯穿回传。

不改Qwen3.8-flash、百炼Base URL、API_KEY或medium，不转外部浏览器。038/039中的可见性、类型冲突、中文组合只读限制、输入前控件失效与NotSent回执修复随包保留。原始COM输出在复制Rust字符串前已有UTF16扫描上限；WebView2自身分配峰值并非由该限制保证。

## 验证记录

离线Web build退出0；最终完整Web回归1281通过、0失败、2项既有忽略（38.96秒），另lib8/0、宿主静态契约1/0通过。新增边界覆盖同URL内容变化、替换宿主、缺/复用观察ID、错误节点索引与正文片段上限；这些工程检查不模拟真实模型，不算软件验收。保留132条既有编译警告。0c4d150两项远端检查已通过，不外推后续提交的远端结果。候选身份待正常打包后补充。

## 真模型软件验收矩阵

| 编号 | 操作与前置条件 | 判定与截图标准 | 当前状态 |
|---|---|---|---|
| BU040-READ | 正常安装/入口，在原验收聊天室右栏打开普通HTML；真实Qwen仅只读标题、marker和正文 | 当前native-ax与两个不同观察ID；同工程/房间；实际回复由宿主片段支撑；原生软件截图；全程0输入 | 待新包实操 |
| BU040-SPA | S1后、模型判断期间人工改变同URL文本 | 结束前S2不同；ObservationStale；不能成功或自动重跑 | 待新包实操 |
| BU040-ENV | S1后隐藏/最小化、切房间/工程或替换宿主 | 原观察失效；不能跨环境成功；准确资源错误 | 待新包实操 |
| BU040-CANCEL | 模型判断或重采样期间取消/超时 | 迟到结果仅沿原记账；不成功、不追加请求、不输入 | 待新包实操 |
| BU040-CONFLICT | browser同时传URL与window/application | surface_conflict，0观察；不自动纠正目标 | Q旧轮失败证据已有，新包待复测 |
| BU040-FACTS | 无效schema、索引越界或目标未满足 | 区分VerificationInvalid/VerificationFailed；有界事实不被模型证据替换；诊断不含页面原文 | 工程边界检查，新包待实操 |
| BU040-INPUT | 只读真实通过后，按既有窄审实施click→type→scroll/navigation | 宿主短期节点、派发前重读/权限/取消/输入锁及释放回执；真实Qwen前后截图 | 类型化输入未实施，不称Browser Use完成 |
| CU040-PAINT | Browser闭环后，真Qwen在Paint画闭合轮廓与简易海绵宝宝 | 实际笔画/释放/视觉判定；四边玉石泛光与准确英文提示；取消/结束撤除、焦点正常 | 未续画；旧L形不算完成 |
| PKG040-INSTALL | 新包正常安装启动 | MSI/源快照/10产物身份、原配置会话附件保全、原生窗口 | 待新包 |

四项任务仍未总体完成；远程DSH插件实际安装运行等未验项保留。微信不改不测，Devin暂缓，PR74保持Draft。下一实操不复用旧轮成功/失败回执，也不因工程检查通过先开放浏览器输入。
