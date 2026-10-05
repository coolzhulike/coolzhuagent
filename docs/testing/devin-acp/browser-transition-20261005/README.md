# Browser 自然导航、弹窗与 Paint 接地实操

日期：2026-10-05。对应 PR #78 的未合并源码；正式安装版仍是 0.2.71，本页不能作为新安装版全量通过证明。实际操作模型固定为 `swe-2-medium`，架构讨论固定为 `claude-opus-5-5-high`。未使用子代理、模型夹具或主会话代操作目标网页/代绘画。

## Browser 已验证行为与失败保留

| 场景 | 实际结果 | 证据 |
|---|---|---|
| I：一次点击产生同源自然导航 | 1 次真实 click，2/2 页面条件满足，目标输入 0 | [联合截图](I-success-chat-and-page.png)、I-reply.md |
| J：一次点击产生跨源导航 | 127.0.0.1 → localhost，1 次 click，2/2 满足，目标输入 0 | [截图](J-cross-site-success.png) |
| K：Click → Navigate，原约束误判只读 | 原任务零动作失败，保留失败；“不点击目标页”被误判为禁止所有输入 | [原失败](K-readonly-failure.jpg) |
| K2：保持 K 原始限制复测 | 2 次真实动作、2/2 满足 | [截图](K2-click-navigate-success.jpg) |
| L：Navigate → Click | 2 次真实动作、2/2 满足 | [截图](L-navigate-click-success.jpg) |
| M：SPA 同文档变更 URL | 模型动作与实际页面条件通过；临时 8767 的地址显示不同步，不能称 UI 全通过 | [截图](M-spa-success.jpg) |
| N：实际起始页与要求来源不符 | 零输入 blocked，保持边界；不是目标达成 | [截图](N-wrong-start-zero-input.jpg) |
| P：标准 8765 同源导航 | 1 次 click、2/2 满足，地址和页面一致，目标输入 0 | [联合截图](P-standard-address-success.jpg) |
| Q：标准 8765 SPA 地址错误 | 真实目标通过但地址仍显示 source；确认为产品显示缺陷 | [失败截图](Q-spa-address-stale.jpg) |
| Q2：原生 SourceChanged 后复测 | 1 次 click、2/2 满足，实际页与地址均为 spa-target | [页面截图](Q2-spa-address-synchronized.jpg)、Q2-reply.md。截图左侧为旧 O 回复，不能当联合新回复截图 |
| O：原始 popup 任务 | “不向目标页发送输入”误判外发，零动作审批阻断 | [原失败](O-approval-misclassification.jpg) |
| O2：测试准备失误 | 主会话打开来源页动作被拒后仍发送任务，起始 about:blank，零输入失败；不算有效弹窗复测 | O2-reply.md |
| O3：机械语义修复后 | click 已释放，实际 popup 目标已显示、目标输入 0；后续观察因资源 revision 被误增而超时，goal=false | [实际页截图](O3-popup-page-observation-failure.jpg)、O3-reply.md |
| O4：弹窗队列与输入 revision 分离后 | 同一 O 限制，1 次 click、2/2 满足，目标输入 0，回复在聊天室可见 | [联合截图](O4-popup-success.jpg)、O4-reply.md |

O4 runtime 为 `run-chat-0dd1a4244eae6b27d1410c6a5ddd446c7b1d8d056a783c34`，真实 input 为 `6ee49aee24f0202918b79616a1527679`。网页记录 source down=1791176618444、up=1791176618445、pagehide=1791176618453、target load=1791176618458。**释放早于卸载，不能算按下/释放微区间页面替换通过。** I/J 等同样保留此限制。

标准 8765 正常页面更新、8767 不更新的环境原因是壳的远端事件监听许可仅允许标准控制台源；没有拓宽许可。Q 仍在标准端口重现，故独立修复同文档地址监听。

## 实现职责和风险边界

1. 适配器仅对真实已结算 Click/Keys/Text 记录待观察状态，新观察固定实际 URL/文档。下一动作前封闭待观察，不让旧输入追认后续定时器变化。任意新动作仍核本轮实际资源、文档、节点和授权；未知释放不生成证明，Scroll 不生成自然导航证明。
2. 自然导航记录 `after_settled_input`，只表示先后关系，不宣称已证明点击因果；晚到定时器或人工操作仍是已知因果局限。来源变化二次失配撤销证明，不将释放当目标成功。
3. 壳的 popup 队列使用独立序号取消旧队列；手动地址切换、显式 Navigate 被 popup 覆盖时仍使原输入来源失效。原世代/作用域/节点校验继续有效。
4. SourceChanged 只同步真实同文档 Source 到显示。读取和验收仍基于实时 URL/document_token，地址缓存不作为输入来源证明；退役时解除 COM 回调。监听注册失败后台留日志并降级，不使网页功能整体失效，不添加前端调试信息。
5. 只读限制辨别对象限定，“不点击目标页”不再误判为禁所有输入；机械输入豁免只补完整宾语“输入”，节点和动作参数继续按原文判定，不豁免发送输入内容给外部对象。
6. 桌面观察增加 `drawing_region_candidates`，仅整理已有 UIA 元素，原始 rect 和相交可见范围分别标明。拖拽仍相对原始 rect，原 window-canvas/FrameRef 契约不变。候选不能证明绘图区语义、权限或完成；缺失不拒绝旧接口。

尚未验收：前端停止/关闭/手动替换恰好发生在 down/up 之间；显式导航被 popup 覆盖的实际竞争；fragment 与同文档历史前进/后退；导航取消后遗漏的 Source 同步；缩放、多屏与当前新包正式回归。纯队列检查和代码审查不替代这些实操。

## Paint 实际进展

R 为两腿脚任务。开始截图有旧身体/眼睛/笑嘴与身体内错误笔迹。[起始](R-paint-before.jpg)与[结束](R-paint-after-failed.jpg)显示两笔真实新增，但一笔在右侧旧横线下，另一笔在脸部，未位于要求的身体底边下方。两笔均使用真实 UIA 引用 `uia-eee5940e25c5adae`，均 released；2 次图像变化、0/2 条件，最终 `no_progress`、goal=false。[实际回复](R-reply.md)保留未通过，不用输入完成替代人物完成。

这证明拖拽执行接口已经存在并可用，尚不能仅据此断言所有失败都是模型能力。当前观察表明模型选点混用了全图与较小画布的比例；几何换算说明与候选须继续在真实任务中验证。

S 为另外一项独立的新人物任务，主会话只说明通用换算、不提供落点、不补发旧调用。实际六笔完成后最终仍 `no_progress`、goal=false。[结束截图](S-paint-after-failed.jpg)显示右侧新增大矩形与内部嵌套小矩形、身体外菱形，没有完整新人物。保留[起始](S-paint-before.jpg)、实际动作和[S 回复](S-reply.md)。模型回复只列回执节选，六笔全量以数据库动作/步骤为准，不依据其节选推断只有三笔。

同时确认 Agent 视觉验收协议缺口：旧 before 是上一步，无法稳定证明更早步骤仍属于本轮新增。新增独立 `computer_use_visual_baseline` 模块，每个 job 保存起始图像、几何和世代，不保存旧 UIA 引用，不用于输入或 FrameRef；每次同一视觉请求按明确标签提供起始/上一步/当前，起始=上一步时去重。新增目标比较起始→当前，进展仍比较上一步→当前，不累计旧 met。几何变化不能证明新增，但不阻断观察或动作。无跨 job 复用，分段等待不重建后台执行器/规划实例。编译已通过；T 为后续独立两步真实基线回归，结果收尾后补充，不能追认 S 成功。

## Opus 讨论与工程检查口径

本轮第六至第十二轮真实 Opus 宿主只读调用分别为 8、11、0、5、0、0、0；与此前五轮 81 合计 **105**。第八/第十/第十一轮仅根据提供的说明审查，没有读源码，不能称全部新代码完整审查。原始意见分别见 opus-design-review.md、opus-implementation-review.md、opus-next-review.md、opus-popup-review.md、opus-paint-region-review.md、opus-paint-baseline-review.md、opus-image-transport-review.md，实际台账见 runtime-receipt.json。

采纳：职责分离、原坐标兼容、候选无权威、原图核验、注册失败降级、退役注销、显式导航覆盖使旧来源失效。未采纳凭空添加 IsUserInitiated/多套 popup 回执或候选缺失即拒绝，未扩浏览器源许可；回调使用显式注销，未额外增加弱引用注册器。

编译和旧测试仅是工程检查。Browser 阶段 Web 串行检查为 1384 通过、0 失败、6 忽略，Shell 浏览器相关检查 8 通过。首次画布修改后完整检查与真实 R 任务并行，27 项因桌面 input owner busy 等竞争失败，保留 tests-drawing-region-final.log；真实 CU 排空后已重新串行执行，最新基线阶段 1386 通过、0 失败、6 忽略，另有库 8 项、入口 1 项通过；不能据并发失败结果宣称代码通过或删除测试。

证据索引将保留 runtime/CU/step/实际动作/诊断/ACP 精确模型和停止排空事实、普通网页事件、截图、真实请求及回复。不会复制 API_KEY 或配置文件。构建摘要、最终检查、收尾日常恢复和 S 结果补充后再刷新 evidence-manifest.json。

## T：多步基线与出站错误新发现

T 是独立两笔小菱形任务，运行 `run-chat-b69bd46e43c45123f4d6e262e0111324d2d31c64c8a9a35a`，耗时 90.5 秒。两笔 released，宿主终态 failed/goal=false；[结束原图](T-paint-after-failed.jpg)显示第一笔接到旧人物底边，第二笔独立菱形。模型称“两处新笔迹”不能替代两个独立菱形的目标。第一步视觉 0/2；第二步验收返回 planner_backend_unavailable，旧错误描述“任务无响应”保留在 [T 回复](T-reply.md)，不能追认 T 通过。

代码复核确认 timeout/oneshot/ApiError 嵌套分支错置：真实 ApiError 被吞成接收任务丢失；已区分供应商错误、接收通道关闭、超时。只有 ACP 固定安全诊断带入回执，不输出 HTTP 原始响应。T 第五个内部 ACP 为 not_sent/drained；原代码的出站图片提示仍共用入站 1 MiB 限额，三图很可能超限，但旧 T 没有序列化大小计数，**不能单凭 not_sent 断言唯一原因**。

经 Opus 第十二轮讨论后，合法 session/prompt 图片块出站单独限 8 MiB；入站、握手、工具消息、文本限额继续 1 MiB。提交前完整 JSON-RPC 包与实际 write 使用同一大小规则；原图不缩放、不删、不补发。台账新增只含数字的 prompt_payload 大小/图片数/额度收据，超限仍 not_sent，不改变权限、预算或释放。第一次工程检查发现新增收据阶段未接到 Journal 的阶段目录（7 项失败），已修复阶段接线，保留 tests-image-transport.log，不将其误归因输入锁竞争。新真实 U 结果另行补充。

## U：三帧链路与两个新增图形通过

U 的精确模型为 `swe-2-medium`，运行 `run-chat-906d1e255b59c1d7e696d07f660c33028a72ed6b1485144a`，87.3 秒，1 次 perform、同句柄 2 次 wait、2 次真实 drag，最终 succeeded/goal=true、2/2。对照[起始](U-paint-before.jpg)与[结束](U-paint-after.jpg)，本轮新增菱形分别在旧人物下方和白画布右侧下方，两图形分离，没有借旧线条。第一笔使用 window-canvas，第二笔使用新鲜 UIA 画布引用，均 5 点、800ms、released，主会话未提供坐标或代画。

第一次视觉 2 张原图 923092 字节、第二次 3 张 1358145 字节；两者 end_turn/drained，固定起始 generation=1，逐代 1/2→2/2，不累计旧 met。实际三帧请求超过旧 1 MiB 上限，真实 Devin 接收并返回完整回复，证明新增出站容量和任务起始/逐步基线链路可用；不能倒推 T 旧失败全部是超限，也不替完整人物验收。[真实回复](U-reply.md)及数字大小计数见 runtime-receipt.json。

最终完整 Web 串行检查 1387 通过、0 失败、6 忽略；库 8 和入口 1 另过。首次新增大小计数漏阶段接线造成的 7 项失败已修复，原失败日志保留。工程检查与真实 CU 不并行。真实完整人物 V 是后续独立回归，另记软件结果，不以 U 两图形通过代替。


## 本轮收尾：U 通过、V 未通过与候选包计划

U 的两个新增独立菱形通过，真实两笔、2/2、goal=true，三帧原图提示 1358145 字节完整返回，未以 released 替代目标。V 的完整人物仍未通过：四笔 released、0/3、no_progress，270.2 秒；新矩形与旧横线/旧人物交叠，眼嘴和两腿未完成。三次三帧请求 1384625/1387135/1388890 字节均 end_turn/drained；这次没有图片发送故障。Paint 截图和 SWE-2 实际聊天室回复见本轮证据目录，不借旧人物追认新成果。动作接口、图片链、多步简单图形可用与完整人物能力未达标分别记录。

Opus 第十三/十四轮实际只读工具调用为 27（read 14/grep 13）与 10（read 4/grep 6），此前 105，累计 142。原始审查分别在 opus-final-code-review.md 与 opus-followup-review.md。第十四轮模型的文字统计含列工具，工程台账以 read/grep 10 项为准。采纳同文档 SourceChanged 不清除普通加载状态、首次观察固定验收基线、本轮“新画/新绘”按新增处理；主会话补充在首次可规划桌面动作前固定原图，覆盖最早观察无图的情况，不增加输入拒绝分支。生产 planner 在每次 run 独立构造，同一个 executor/future 保留分段等待状态。

最后小修已离线编译：完整 Web 1387 通过、0 失败、6 忽略，另有库 8/入口 1；Shell 浏览器相关 8 通过。最后动作规划前捕获基线的改动又完成真实 build 和现有两项基线检查；不把工程检查说成新版本软件回归。U/V 实操的实际二进制摘要与最终源码/工程二进制摘要分列在 review-build-manifest.json，最后小修尚未软件实操。

临时验收 ACP 全部排空，外壳正常关闭，精确后台和自有网页服务退出，临时配置恢复原字节，安全库不清不换。日常正式版 0.2.71 恢复启动自检通过，健康 10 正常/1 工作区提示/0 错误。日常显示原 Qwen/聊天09 是环境恢复，**没有改用 Qwen 进行本轮实操**；所有 I～V 实际 requested/effective 均 SWE-2。后续验收继续指定 swe-2-medium，审查指定 claude-opus-5-5-high。

优先后续：将本轮收敛代码/证据提交 PR #78 草稿，生成 0.2.72 候选包；安装版先复跑自然链接、popup、SPA 地址同步与两笔新增图形，再补停止/关闭/手动替换恰 down/up 内的竞争、显式导航覆盖 popup、同文档 fragment/历史和多屏缩放。完整人物保留未通过，当前证据未发现尚缺 drag 执行接口；不通过追加画图引擎或放松验收强行闭环。新包总回归和其余侧栏/市场/开机动画收尾仍待完成，微信保持不动。

V 原图：[开始](V-paint-before.jpg)、[结束](V-paint-after-failed.jpg)；[实际回复](V-reply.md)、[聊天室终态](V-visible-chat-settled.jpg)。日常恢复实拍：[截图](daily-restored.jpg)。
