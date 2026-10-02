# 0.2.59 改动、正式软件实操与定向测试报告

2026-10-01，主会话独立实施和测试。真实qwen3.8-flash、medium、既有百炼Base URL和保存密钥不变；没有模型夹具、人工代点击或补画认领模型成功。微信不改不测，Devin搁置，Pro补审按用户决定暂停。

## 改动与职责

ComputerUseRequest增加可选max_actions，0非法、省略兼容旧行为；控制器取请求与宿主上限较小值，复用既有RunBudgetGuard。在规划前和输入前检查，一次连续drag算一次动作尝试，预输入失败也消耗额度。最后一次动作后仍重新观察验收：未达成则budget_exhausted、retry_owner=none，不允许内部补画或补点。上限不限制模型HTTP次数，不替代本轮契约、调用限额或120秒截止时间。

工具schema公开参数，内部规划携带值；显式本轮JSON契约不允许删/增额度。普通自然语言不猜测任意数字，必须看真实请求是否携带max_actions。未新增执行循环、表或正式界面调试栏。详见[实施审查](../../analysis/2026-10-01-computer-use-request-action-limit-review.md)。

## 发布、安装、工程验证

MSI正常安装退出0，唯一版本0.2.59、10个关键产物摘要匹配；Shell PID30024、Web PID21536，8765唯一由正式Web监听。MSI247033213字节，SHA256 `493245b7caa0cf4cba89b2d7c665fc2b02baf6c312fa7ded7422b9d96fa5f374`。报告 `pkg-report-release-20261001-184630302-5529b643`；源快照 `2629565c4154f345c13d071cdd0ecb05ba83a0ebd6ccb1401a4b3d0e3d3796cf`；载荷 `0dbb88b6c221a5dbc0ec1bab472a935ae31fda4036c27eee7b4e295eafa23964`，859文件、324546022字节、六项发布门通过、安全扫描0、全载荷摘要匹配。构建身份原件在evidence/build-identity，不回写包身份。

核心/Web离线build通过，核心lib141/0/0（66.56秒）、本轮契约6/0、工具schema1/0。源码提交e6992ffee003ded3d5c19e2e6eee31ad6f8134bf的两项CI36850859852/36850854906均completed/success，原始收据在evidence/ci；不能外推到后续窗口定位改动。

## 真实Qwen结果

聊天室room-1790567510595（聊天09）、session-1779459149988、workspace ws-992cca993bc1a1a0。父运行completed不等于目标通过；facts保存请求、CU、步骤、投递释放、规划/验图诊断及用量。

| 轮次 | 结果与意义 | 原图 |
|---|---|---|
| BU059-LIMIT-CM，27.410秒 | 起点0、目标2、max_actions=1。一次真实click只到1，验收0/1后budget_exhausted；1次规划、1次只读验收，没有第二次规划/输入。动作边界通过，目标2未达成 | 01–03及CM-facts |
| BU059-SINGLE-CN，23.669秒 | 新独立请求从1到2，max_actions=1。一次click sent/released，实际页面2、验收1/1、succeeded，无重规划。正向通过 | 04及CN-facts |
| CU059-COLOR-CO，20.369秒 | Qwen实际参数缺success_criteria，intent_guard/invalid_tool_input、0动作0步骤；不撤必填校验，不计Paint通过 | 06及CO-facts |
| CU059-COLOR-CP，17.663秒 | 提示把success_criteria放在首位，实际仍缺字段，0动作0步骤。未证明适配器删除字段，后续普通语言成功；失败保留 | 07及CP-facts |
| CU059-COLOR-CQ，62.253秒 | 普通自然语言，模型构造完整参数，指定mspaint.exe及“无标题 - 画图”。一次UIA点击黑色颜色控件sent/released，后验图1/1，正式Paint原图可见黑色选中。准备步骤通过 | 08及09原生前后图，CQ-facts |
| CU059-BODY-CR，44.915秒 | 仅指定mspaint.exe，模型请求完整、max_actions=1。观察却是控制台，窗口选择None。规划一次5点drag，输入前stale_observation/not_sent/not_needed，未落笔；尝试限额1随后阻止第二规划。发现Agent窗口定位缺陷，身体未完成 | 11/12终态、13原生控制台图、CR-facts |

CQ前验图11783ms、规划9146ms、后验图14134ms；两份2560×1152原生图SHA256为 `8287158eceb7dff5fba1d18bb7ec241262ae4a185303f462c3f2012bfe078212` 和 `7164993b05ce4e7f867720eec4fc3d4068b03f7b901143a61fa98e55529211f8`。主会话仅正常激活窗口和截图，没有修改颜色或绘图。

CR前验图10509ms、规划11432ms，plan点为[.3,.6],[.5,.6],[.5,.8],[.3,.8],[.3,.6]、1200ms；对应的是控制台2182×1355图，并非Paint。因此这次不能归因模型选点能力。DB已发送action_count=0、结果尝试数/监督预算1并不矛盾：未发送尝试也消耗限额。13原图按来源原始字节复制和校验，不能用后续Paint截图掩盖错误观察。

## 下一执行者的定向验收

1. 新版修复仅指定exe的路由：控制台在前台、Paint已打开，真实Qwen只指定mspaint.exe，检查每份原生观察窗口身份为Paint，模型自主一次闭合笔画后原图新增矩形。不要给window提示绕过修复，不人工补画。
2. 不存在exe/同应用多窗口/应用与窗口冲突必须明确target_not_found或target_ambiguous，零规划零输入，不能退回前台。无目标前台和旧记事本推断仍兼容。详见[窗口定位审查](../../analysis/2026-10-01-desktop-application-target-review.md)。
3. 新安装版重跑浏览器单次点击，不将UIA检查当作Browser Use通过。浏览器读、输入、导航、滚动和上下文清理有历史独立证据；按下至释放期间关闭竞争尚未覆盖，步骤间关闭不能追认。
4. Paint完整简化海绵宝宝仍未通过；一次toolcall不等于一次动作。保留输入、释放、路径点数、验图逐项结果；released、图像hash改变及模型自述不能单独证明画成。
5. 确定permit expired的分类工程验证通过，058/059尚未重新触发真实笔画到期现场。动画日常舞剑、Windows泛光及准确英文有历史/本轮实拍，首次/减少动态/资源失败、多屏和取消仍开放。
6. DSH真实远程安装及Qwen运行P1–P5、四项总体仍开放。PR74保持Draft。远程SDK已只读调查，依赖树冲突需固定兼容版本，不用force/legacy-peer-deps或占位实现宣称接通。

15张实操原图、字节索引和全部失败facts均在installed-native；060修复在新报告中验收，不回填本版成功。
