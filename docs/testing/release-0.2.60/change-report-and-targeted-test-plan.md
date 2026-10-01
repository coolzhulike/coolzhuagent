# 0.2.60 改动、正式软件实操与定向测试报告

2026-10-01，主会话独立实施、审查和测试。使用既有真实 qwen3.8-flash、medium、百炼 Base URL 和已保存密钥，没有模型夹具、人工代模型点网页按钮或补画。微信不改不测；Devin 搁置；GPT-6 Pro 补审按用户决定暂停。PR #74 保持 Draft，四项任务尚未总体完成。

## 改动及模块职责

059 的真实失败暴露 Agent 窗口定位问题：仅提供 application=mspaint.exe 时，没有正确识别所属进程，观察取到前台控制台，后续笔画被时效闸门拒绝。060 在 UIA 模块独立增加 window_target：exe 文件名按实际进程名精确匹配；绝对 exe 路径按进程完整路径匹配；友好名称保留进程 stem、窗口标题与类名识别。显式 application 与 window 同时存在时取交集，优先于旧记事本目标推断。零匹配返回 target_not_found，多匹配返回 target_ambiguous，不能任取第一个或退回前台。

Windows 枚举器只查看可见窗口，通过 PID 查询所属进程路径，立即关闭查询句柄。查询失败不能用标题伪装成 exe。桌面桥在激活后再次核对选定 HWND 与实际前台 HWND，不一致即 stale_observation，零规划输入。无显式目标且无旧目标推断的前台模式保持既有语义。没有降低必填参数、帧守卫、输入许可、配额或释放检查。

具体文件：modules/vision/packages/uia-resolver/src/window_target.rs、windows_impl.rs、lib.rs，以及 modules/gui-web/packages/web-console/src/computer_use_desktop_bridge.rs。详见[窗口定位方案与边界](../../analysis/2026-10-01-desktop-application-target-review.md)。

## 发布、安装和工程检查

正常安装 MSI 退出 0，无 UAC 阻塞；正式入口启动后，唯一版本为 0.2.60、安装日期 20261001，Shell PID6268、Web PID5112，8765 唯一由正式 Web 服务监听，10 个关键安装产物摘要全部匹配。单独启动 bin 内壳程序不等于正式入口启动，该前置操作已正常退出纠正，不计作本版启动验收失败。

| 身份 | 事实 |
|---|---|
| MSI | CoolzhuAgent-0.2.60.msi，247020925 字节 |
| MSI SHA256 | de161e888b792758e4bd74dcaa53841ff26de8e9382c0d68cc93b12d58e5fe75 |
| 构建报告 | pkg-report-release-20261001-191951824-a4be5fee |
| 源快照 | a41c938b138e0dc29ecbec46e389a818ba35da209bd4932cd18a20eeb0cdc01b |
| 载荷摘要 | a82e90629da04603937d8dbef5da92405b3d4ba687cec204494d0c99dc8bee9c |
| 发布门 | 六项通过、安全扫描 0；859 文件、324548070 字节；全载荷及 10 个关键产物摘要核验通过 |

安装身份见 installed-native/installed-artifacts.json；六份原始构建身份文档在 evidence/build-identity。构建源快照是包身份依据，不能把报告中 dirty 源快照等同一个 Git HEAD，也不回写原收据。

UIA/Web 离线 cargo build 均通过。UIA lib 12 通过、0 失败、0 忽略；Web 桌面桥定向检查 17 通过、0 失败、1282 项过滤。未称本轮重新跑过全量 Web。源码提交 e6d88c732fca68b46570e7fd1d1e700177c1b234 的远端检查 36854392496、36854387297 均 completed/success，原始收据在 evidence/ci；不能外推到后续提交。

## 正式版真实 Qwen 实操

工程 ws-992cca993bc1a1a0，实际工作目录 C:/Users/zhupu/coolzhuagent；聊天室 room-1790567510595（聊天09），会话 session-1779459149988。保留既有历史和完全访问设置，不改安全库。每轮 facts 保存原请求、父运行、CU、步骤、规划/验图诊断和用量。父状态 completed、输入 released 和模型自述均不单独作为通过依据。

| 轮次及父耗时 | 实际结果 | 证据 |
|---|---|---|
| CU060-BODY-CS，97.262 秒 | 请求只指定 application=mspaint.exe、不提供 window，max_actions=1。真实选中 Paint HWND143198804/PID17600，一次五点连续闭合笔画 sent/released、路径完成；原图实际新增闭合小矩形，CU succeeded、1 动作、无重规划。本次 Agent 定位修复及闭合路径正向通过 | 03 原生前后图、04 控制台、CS-facts |
| CU060-MISSING-CT，18.761 秒 | 模型错传 maxActions 等参数，invalid_tool_input、0 规划/步骤/输入；未进入目标定位，不能当不存在 exe 的负向通过 | 05、CT-facts |
| CU060-MISSING-CU，14.290 秒 | 独立新请求字段正确，application 为不存在的 exe。observation/target_not_found、0 规划/步骤/输入，无前台回退。负向预期通过；工具本身仍 blocked/goal=false | 06、CU-facts |
| BU060-SINGLE-CV，30.968 秒 | 正式右栏原生网页由真实 Qwen 选择按钮并只点一次，次数 0→1，1 次 native click sent/released，新鲜观察 generation=2、1/1、succeeded。未用桌面 UIA 代替 Browser Use | 07 起点、08 终点、CV-facts |
| CU060-SPONGE-CW，141.569 秒，父 interrupted | 第一笔完整释放，但新增菱形跨越目标小矩形的上边线；模型后验图错误称 1/1。第二 CU 零动作又错误将旧图认作新右眼；第三 CU 达本轮两次调用上限，recursive_call_blocked、零输入。主会话发现原图不达标后在普通 UI 中止，未继续嘴/腿，不认领绘画完成 | 10 原生三图、11 中止界面、CW-facts |
| CU060-EYE-CX，27.431 秒 | 单部件新请求中模型漏 success_criteria，intent_guard/invalid_tool_input，0 规划/步骤/输入。没有新眼睛，未补发、未放宽校验 | 12、CX-facts |

CS 前验图 34906ms、规划 17218ms、笔画执行 5771ms、后验图 15684ms。窗口截图 2560×1152，screen_rect=(0,61,2560,1152)，canvas_rect=(11,106,2538,1096) 是可见客户区，包含工具栏，不能等同语义上的白色绘图区。五点为 [.35,.65]→[.55,.65]→[.55,.85]→[.35,.85]→[.35,.65]，1200ms。实际新小矩形原图约 x899–1406、y756–976；旧大矩形保留，质量不作精细要求。CS 前后 SHA256 为 c8d48987fe48f15ec4e73e80837a76fc2b8bb7141a1411a3388c25a737a04390、0e269c73b76a4784b91f4f040cc045bd12eae8c13dc8eae499cd723bb5b9da47。主会话按原始分辨率亲自复核：缩小预览会丢失一像素横线，不能用缩略图否定闭合。

![CS 正式 Paint 原始终态](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.60/installed-native/03-paint-body-native-02.png)

CW 第一 CU 前验图 9810ms、规划 30328ms、执行约 6389ms、后验图 19785ms；5 点 [.385,.63]→[.415,.66]→[.385,.69]→[.355,.66]→[.385,.63]。新增菱形上角原图约 y735，小身体上边线约 y756，独立复核明确越界。该原图 SHA256 fc0497a2bfaf69dd7f78dcd5e7748eaf285845e26ab8b66c5ce97be5705f31ea。机器历史 succeeded 原样保留，不能改写数据库使报告看起来通过；报告的独立审核结论为失败。中止请求时间 1790854738795，父终态 1790854738832，第三次额度阻断此前已发生；不能称中止发生在第二次观察期间。

![CW 越界菱形，不能算作合格眼睛](C:/Users/zhupu/Desktop/coolzhuagent/docs/testing/release-0.2.60/installed-native/10-paint-components-native-02.png)

绘画后续失败目前有直接证据的是模型字段遗漏、选点越界和视觉误判，执行接口已能完整投递并释放五点路径。按用户要求暂不针对纯模型能力改造，不更换模型、放宽参数或重写坐标制造通过。CW 的“最多五个部件”提示也超出正式宿主每父轮两次 CU 调用的上限，后续分为独立单部件请求；不扩大限额掩盖递归风险。

运行中主屏四边提示及准确英文 Coolzhu Agent is using your computer 原图、活动收据在 activity-060-CS。全部轮次结束后新读取 active=false、lease_ms=0；这是日常撤除证据，不等于实际按下期间取消、多屏或按下至释放期间关闭竞争已经通过。17 张原始图及字节索引在 original-images-index.json。

## DSH 真实远程插件 P1 结果

使用公开真实仓库 omdsh-dev/dsh-tool-calculator，固定 commit b2007a13f06bcf75bf07b9d277ee8d434a316490；package.json、实际 lib/index.js/evaluate.js/invariant.js、Cordis patch 和许可证均按 Git blob SHA1 与 SHA256 核验后原字节暂存。插件版本 0.0.1、private=true，因此不能把同名 npm 包当作已公开发行；后续安装须使用确认的固定 Git/source bundle。[真实插件源码](https://github.com/omdsh-dev/dsh-tool-calculator/tree/b2007a13f06bcf75bf07b9d277ee8d434a316490)

原 0.0.1-rc.1 SDK 依赖图存在公共源不可取得的 dsh-type-meta；没有使用 force、legacy-peer-deps 或假服务跳过。按真实候选锁文件固定官方 DSH SDK 0.1.1-rc.2、Cordis 4.0.1、Schemastery 3.18.1、Cosmokit 1.8.2，在临时目录安装 17 个依赖并关闭安装脚本。锁文件来源与 SHA512 全保存，dsh-tools 实际 tarball 另行下载并独立 SHA512 核对。[官方工具 SDK 包](https://www.npmjs.com/package/@deepseek-ai/dsh-tools/v/0.1.1-rc.2)

真实 Cordis Context 加载官方 SystemPrompt、ToolRuntime 和原插件后，实际注册 calculator schema，执行 15 + 27 * sqrt(9) 得到 value=96、content=96；非法表达式被真实解析器拒绝；预取消 AbortSignal 返回 ABORTED_BEFORE_DISPATCH；插件 dispose 后 schema 清空、调用 UNKNOWN_TOOL。最后三个子 fiber 都为 DISPOSED、registrySize=0，tools/systemPrompt 服务消失，进程正常退出。第一探针误用“根 fiber 必须 DISPOSED”作为核验条件，原收据保留：官方根 dispose 实际是 restart，不能将根 ACTIVE 误作插件未释放；修正后按实际子插件与服务事实核验。

完整证据在 dsh-p1，包括真实文件身份、SDK 锁文件、执行/取消/dispose 收据及探针。只通过首个工具包的 P1 依赖与运行可行性验证，未修改正式依赖、未在安装版市场安装、未调用真实 Qwen，不能称远程插件需求完成。正式 P2–P5 继续按[安装与宿主方案](../../analysis/2026-10-01-dsh-remote-plugin-runtime-plan.md)实施。

## 下一模型定向用例与未完成项

1. 窗口定位：本版只提供 mspaint.exe 的正向及不存在 exe 的负向已实操通过；还需软件现场覆盖同应用多窗口、应用与标题冲突、绝对路径不符、友好名称和无目标前台兼容。以实际 HWND、PID、进程路径、零输入及原图判断，不以单元结果替代。
2. Browser Use：060 原生单击已回归，读/输入/导航/滚动及同聊天室重开、跨聊天室/工程清空保持各自历史版本证据。按下至释放期间关闭竞争仍未覆盖；步骤之间关闭、晚于交付关闭、页面脚本延迟均不能追认。需要记录宿主下发/释放及资源撤销的真实时序。
3. Paint：整体海绵宝宝未完成。若再验，每父轮只发一个新部件，检查实际必填字段与 max_actions=1；每份原图都核对物理坐标和边线。released/hash 改变/模型 1/1 不足以证明成功，旧图或零动作不能证明新增。纯模型失误按用户要求暂不处理。
4. 确定 permit expired 的新分类已有工程检查，尚未在 058–060 正式包重现新的真实笔画到期现场。保留 partial/released 风险事实，不等同用户取消，不自动重试。
5. 动画 B 的日常 6.5 秒 Q 版舞剑、展卷、退场和控制台交接有 056 实拍；首次启动、减少动态效果、资源失败及四关键姿态逐帧尚未全验。泛光多屏和真实输入中止边界也未全验。
6. DSH：首个真实包 P1 已通过；P2 固定源安装事务、P3 Rust 工具桥与真实 Qwen、P4 正式右栏启停/卸载、P5 服务兼容扩展未完成。安装失败/取消/重启恢复、版本与宿主世代、工程隔离和停用旧调用均需真实验收。
7. 新发现聊天裸 URL 紧邻中文逗号和说明时，富文本链接将后续中文吞入地址；CV 的模型请求 URL 正确且不受影响，但聊天链接入口应修复并实际打开验收。不能将本轮 Browser Use 成功追认成链接解析通过。

本报告可供其它模型基于边界设计针对性功能测试。工程检查、P1 临时探针和正式安装版真实模型实操的证据范围分别明确，不回填历史失败，也不把本轮有限覆盖标成四项总体验收通过。
