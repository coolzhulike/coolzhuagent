# Browser Use 在途停止与恢复实操

日期：2026-10-05。实际执行模型为 Devin `swe-2-medium`；方案讨论为 `claude-opus-5-5-high`。没有使用模型夹具、子代理或主会话代模型点击验收网页。普通 HTML 测试页记录浏览器真实事件，不提供模型回复或宿主输入替身。

## 已完成与未完成

**已闭环本轮后端在途停止：真实 pointerdown → 正式停止接口 → 原页面 pointerup → 宿主确认 released → 无后续文本 → 新独立任务成功。** 另外修复停止后被误报为“父运行变化”的原因分类。该结果来自独立调试版，不能替代新安装包的正式回归。

仍未证明前端停止按钮、关闭面板、用户导航或页面自行导航都命中 down/up 微区间。关闭测试发生在释放之后。导航测试 G 在执行前发现语义审批误判；修复后 H 确实跳转且释放，但来源 URL 校验使目标验收失败。Paint 完整绘图、本轮接地改进和正式版 HUD 联合回归仍待完成。

## 版本与运行环境

- 实施工作树：`C:/Users/zhupu/.codex/worktrees/input-recovery-20261004/coolzhuagent`；提交基线 `f054833`，功能基线 `438f6ec`。
- 调试后台使用 8767，独立验收工程及“SWE-2 Browser与Paint真实验收”聊天室，沿用已授权的完全访问。
- A～D 使用原调试后台；E、F、G 使用修复后 Web 二进制，SHA256 `CF35F3ADB9931F40E6212D75BC92566342F7130AD3E3C3274420F1106AA8CA4D`。
- H 使用再修复机械输入语义的后台，SHA256 `5842A1A526496CC1D308BF3AECAA19B7534D36D00EDB08C6040118F6DFA9D6E9`。
- 日常安装版仍是 0.2.71，载荷源码 `49d0a1b6d5e5`，不含本轮修复。共享输入安全目录始终使用原目录，没有清库、更换状态根、修改安全历史或追认未知释放。

## 实操结果

| 场景 | 实际结果 | 验收结论与截图 |
|---|---|---|
| A 正常点击和输入 | 3 动作成功，次数 0→1，输入 `SWE2-BOUNDARY-A` | 基础执行通过；没有提交停止，不算中断。[截图](A-normal-real-success.png) |
| B 仅写“右栏关闭” | 正文未命中现有原生浏览器关键词，外部通道报 `extension_unavailable`，0 输入；原生页面仍可见 | 路由措辞限制，不能归因为原生宿主断连。[截图](B-native-disconnected-failure.png)（历史文件名保留） |
| C 释放后关闭右栏 | 1 次点击 sent/released 后，观察报 `native_browser_panel_unavailable`；后续文字未发生 | 关闭后停止通过；关闭晚于 pointerup 约 9 秒，微区间未覆盖。[截图](C-close-between-actions.png) |
| D 修复前在途停止 | down 后 23ms 提交停止；原页面随后 up，宿主 released；无后续文字，父运行 interrupted | 释放通过；CU 原因误报 `native_browser_parent_changed`。[截图](D-cancel-released-page.png) |
| E 修复后在途停止 | down 后 50ms 提交停止；原页面随后 up，宿主 released；无后续文字 | 释放与分类复测通过，CU 原因 `native_observation_cancelled`，retry_owner=none。[截图](E-fixed-cancel-released-page.png) |
| F 新任务恢复 | 新独立目标仅点击一次，次数 1→2；新鲜页面目标 1/1，goal=true | 新任务恢复通过，回复在聊天室可见。[截图](F-recovered-click-success.png) |
| G 页面按下后自然导航 | 0 动作，`approval_required/external_communication`；来源页未跳转 | 发现 objective 中“不向目标页发送任何输入”触发“发送”关键词；尚未测到页面竞争。[原始回包](G-navigation-response.json)。截图返回缓存与 F 相同，已剔除，不作为 G 证据 |
| H 修复误拦后自然导航 | 1 次 click sent/released；目标页面可见，目标输入事件 0；CU verification 因来源 URL 不符而 blocked | 机械输入审批误判修复生效；跳转后的目标验收仍失败，页面替换发生在 up 之后。[截图](H-navigation-page-only.png) |

D、E 使用真实聊天创建任务，再由普通测试驱动调用与前端停止按钮相同的 `/api/chat/turn/interrupt`。驱动只监听页面真实日志和读取当前轮身份；没有直接调用 CU、注入鼠标或写数据库。因此这证明后端取消链路在途竞争，**不冒充人工点击停止按钮也命中了相同区间**。

## 可复核的时间与身份

页面日志保存两个时钟：`page_ms` 是真实页面 Date.now，`server_ms` 是本机服务器接收时间；不能把二者当完全同一时刻。本机时间顺序如下：

| 场景 | 来源页 down（服务器收到） | 停止/关闭请求 | 来源页 up（服务器收到） |
|---|---:|---:|---:|
| C | 1791168754671 | 1791168765107（实际隐藏 1791168765320） | 1791168756277 |
| D | 1791169112949 | 1791169112972（ACK 1791169112979） | 1791169114557 |
| E | 1791169526605 | 1791169526655（ACK 1791169526677） | 1791169528210 |

D 原文档 ID `847dae7c-1df2-4c87-a320-67a29e6d9087`，见 [page-events.jsonl](page-events.jsonl) 和 [停止收据](D-interrupt-receipt.json)。E 原文档 ID `516026a0-6594-47c3-a28b-f02514d31004`；来源页 `pointerdown` 均标记 trusted=true。A、C、D、E、F 的具体动作投递、释放、终态和 ACP 模型回执见 [runtime-receipt.json](runtime-receipt.json)。

| 场景 | 聊天轮 |
|---|---|
| A | `chat-turn-1791168407997-0` |
| B | `chat-turn-1791168573590-1` |
| C | `chat-turn-1791168735327-2` |
| D | `chat-turn-1791169091679-4` |
| E | `chat-turn-1791169504447-0` |
| F | `chat-turn-1791169584065-1` |
| G | `chat-turn-1791170191900-2` |
| H | `chat-turn-1791170763175-0` |

## 本轮代码修改与检查

`native_browser_host.rs` 把父轮的 `stop_requested/interrupted` 分类为观察已取消，其他身份/房间失配仍返回父关系变化。观察开始和收到回执后均保留取消、根预算及父关系校验。`native_browser_adapter.rs` 对取消给出实际停止说明，不要求用户为取消重新授权，也不自动重试。

没有修改输入派发、释放期限、身份/资源校验、权限配置或安全隔离恢复。CU 终态仍是 blocked，原因 cancelled；父聊天运行 interrupted，不能说终态枚举已改为 cancelled。

- `cargo build -p coolzhu-web-console --offline`：通过。
- `cargo test -p coolzhu-web-console native_browser_host --offline`：6 通过、0 失败；这是分类及宿主相关检查，不是整套工程回归。
- `git diff --check`：通过。
- 修复后真实 E、F：分别证明在途停止和新轮恢复。ACP 外层与内部规划均可按 scope 关联，requested/effective 为精确 `swe-2-medium`，收尾以排空收据为准。

G 之后另修复 `computer-use-core/controller.rs` 的机械输入描述识别，只增加完整宾语“任何输入”，沿用原句尾边界。真实目标节点、动作参数和敏感动作分类原样检查，不做泛化否定句豁免。新的离线 Web build 通过；一项纯规则检查覆盖实际目标原句、记录/结果外发、真实发送按钮、post/share 参数和顿号边界，通过。真实 H 证明同样目标原句不再被误拦；没有使用模型响应夹具。肯定的“发送任何输入。”也按机械输入解释，真实动作参数仍须检查；繁体或更多变体不在本次扩展范围。

H 的真实页面时间：pointerdown=1791170789097、pointerup=1791170789099、pagehide=1791170789106、新文档 load=1791170789111。模型点击触发页面自身 `location.assign`，但 up 在旧文档离开之前完成，不能算替换竞争。[原始页面事件](navigation-events.jsonl)。`native_browser_verification::observed_page` 仅允许原请求 URL 或正式 Navigate 回执对应的目标 URL；Click 触发自然导航没有 Navigate 回执，因此产生 `native_browser_observation_unavailable`。没有把目标可见直接改成宿主 goal=true。

## Opus 第四轮：机械输入误拦

请求与原文见 [opus-classifier-request.json](opus-classifier-request.json)、[opus-classifier-review.md](opus-classifier-review.md)。同意上述最小词汇修复，并明确来源 up 早于 pagehide 只算正常释放；ReleaseUnknown 不通过。宿主核实 4 次只读（read 2、grep 2），精确 Opus 模型、end_turn、进程排空，无桌面动作。

## Opus 第五轮：自然跳转的最终页面验收

请求与原文见 [opus-transition-request.json](opus-transition-request.json)、[opus-transition-review.md](opus-transition-review.md)。宿主核实 10 次只读（read 6、grep 4），精确 Opus 模型、end_turn、进程排空。源码快照是 438f6ec，未读取主工作树新补丁；没有将旧快照审查写成对新补丁的完整代码审核。

主会话确认：页内 `location` 跳转不递增 navigation_revision；宿主换地址和弹窗路径会递增。现有 `execute_authorized` 也不强制页面 URL 等于原请求，所以这里是最终观察范围设计的问题，不是删除输入权限检查的理由。

计划收敛为：已结算动作来源记录由适配器产生；观察层只提供真实后续页面；验证器处理起始 URL 链和新鲜目标事实；保留现有 AuthorizedNavigation 协议。不使用一个“本轮曾有输入”的粗标志放宽所有 URL，也不把新网页自述当来源证明。需要同时处理 Click→Navigate、Navigate→Click 和正常中间点击；记录的是输入后变化，不宣称证明点击与跳转的唯一因果。跨源及弹窗应有明确范围，不凭本地同源成功外推。

尚未实施这项兼容变更。Opus 关于“适配器没有 key 动作”的说法不符合主会话当前源码：KeyCombination 已存在，Enter 也可能触发提交/跳转。后续设计不能按这个错误结论遗漏键盘路径。目标固定终点、每步重置来源的建议还须避免破坏没有跳转的合法中间动作，不能照抄形成新误拦。

实施后的实际门槛：自然导航最终目标引用真实节点通过；无新页孤立输入；跨源/用户换地址的范围明确；Click→Navigate 与反向链路真实回归；取消/未知不生成已完成来源证明。先有可审查的最小职责方案，再修改，不堆叠多份重复导航状态。

## Opus 第三轮方案讨论与裁决

原文见 [opus-boundary-review.md](opus-boundary-review.md)，请求见 [opus-boundary-request.json](opus-boundary-request.json)。模型自估 18 次只读调用，宿主实际是 **19 次（read 11、grep 8）**，以台账为准。精确模型 `claude-opus-5-5-high`，end_turn 且进程排空。只读审查不是实操验收。

1. 采纳显式浏览器后端选择方向：结构化选择负责路由，权限、正文约束和资源绑定保持独立；空值保留兼容关键词。界面交互尚未设计实施，不能写成 B 已修复。
2. 用户导航/弹窗导航应协调在途执行，但不能简单“等锁后导航”就声称已释放：执行门退出也可能是 ReleaseUnknown，迟到释放需单独处理。此次未修改这些生产路径。
3. 不采纳“旧网页必须收到 pointerup 才能认 released”。CDP 释放确认是鼠标状态事实，与网页目标成功分开；新文档的孤立事件仍应真实记录，不能将释放当成功。
4. Paint 采用可选真实 UIA 绘图区引用、保留旧坐标契约、本轮基线与逐步进展分离的方向。不采纳“没有候选就一律禁止图像验收”，避免新的通用过度防护。
5. 不采纳机械重复约 20 次、关闭晚于 up 也算竞争或未知算正常通过。未命中的边界保持未覆盖。

## 下一项执行门槛

先修复 G 暴露的机械输入语义误判并真实复测自然导航；核对来源文档 down/up、pagehide、目标文档事件、宿主释放及后续派发。出现未知就保存并停止，不清记录重跑洗掉失败。再完成用户导航/弹窗/关闭边界，然后 Paint 接地与完整粗略人物；收敛新包后正式安装版统一回归。

本目录中的网页及取消脚本只用于复现实际边界。截图是软件真实界面，消息由真实模型产生；测试页延时不在生产代码中。

脚本中的会话 ID、端口和时序属于本次原始测试上下文，不能盲目复用为永久测试入口；新验收必须读取当前运行身份，不重放已结束的调用。归档代码不是模型夹具或新增产品功能。

## 收尾与日常环境

三轮 Opus 补充审查、全部 SWE-2 调用已排空后，正常关闭调试外壳，按精确可执行文件身份停止验收后台，停止两个自有普通 HTML 服务。临时配置恢复原字节；正式启动器自动写回 8765 地址。日常正式版启动自检通过，原用户数据工程、Qwen 和“聊天09”保留。健康检查 10 项正常、0 错误，1 项为既有用户工程不是源码仓库的提示，不据此改变工程。

[收尾事实](cleanup-receipt.json)、[正式版恢复截图](formal-daily-restored.png)。已恢复的正式版仍是 0.2.71，不含本轮修复；没有让验收后台冒充日常正式版，也没有生成或宣称安装新版本。
