# 官方SDK函数进入后的父预算截止：最终独立补证

本轮完成此前唯一列明的离线补证：真实官方calculator已进入求值函数后，父预算到期，生产Rust桥拒绝迟到成功并确认进程回收。新增1项集成测试覆盖3场景，最终实际运行1通过、0失败、0忽略，27.02秒。没有修改生产代码或官方资源，**不重新打包0.2.66**。

分支`codex/cu-preinput-followup-20260930`，参考HEAD`8181f08ae9c50d3e41aabd32a6af85343dcd92e8`；实际测试对应保留的未提交工作树。精确新增文件SHA及22份记录见[证据索引](../testing/dsh-official-deadline-2026-10-02/evidence-index.json)。本报告关闭[恢复专项](2026-10-02-dsh-recovery-review.md)最后一节的官方函数截止开放项，旧报告不追改。

## 真实执行范围

使用0.2.66实际MSI只读展开的固定运行时：Node 24.15.0、原生产`process.mjs/host.mjs`、原官方SDK；calculator为`@deepseek-ai/dsh-tool-calculator@0.0.1`，固定提交`b2007a13f06bcf75bf07b9d277ee8d434a316490`。执行前及每场景后均通过生产来源回执和完整运行资源锁复核。SDK锁SHA256为`af9313e4938c2c94596b702e13f48dfe430b47ec52eab98855d6707a5bdb1307`。

calculator是同步且限制500字符的纯计算，不能可靠依赖超长表达式形成自然慢调用。测试入口用Node内置、同进程inspector.Session，在官方`lib/evaluate.js:79`的`parse`内部设断点，仅注入有界调度暂停。没有开放调试端口、替换SDK/工具函数、修改官方文件、伪造协议结果或调用模型。原生产脚本返回后，测试入口只复制其实际终态，不写回IPC。

三份进入记录均发生在父预算到期前，保留同次调用的workspace/room/run/call身份。实际栈包含`parse → evaluate → calculator.execute → @deepseek-ai/dsh-tools.execute → dispatchToolBody`，证明已进入工具函数，不是仅完成握手或收到execute。官方求值文件SHA256为`3f0bfded394d92c98c18e187a6508e71b1d303494f847e3f9eebf9c93feb03c8`。

**这是未修改官方函数中的执行时序故障注入，不是自然慢函数、真实Qwen或GUI验收。** Rust使用真实生产进程桥，根ExecutionControl预算8秒，局部30秒不能续期。

## 最终结果

| 场景 | 实际观察 | 结论 |
| --- | --- | --- |
| 正常对照 | 函数内暂停120毫秒后恢复，390毫秒返回`value=96` | 相同官方路径可正常完成，测试没有替换计算结果 |
| 到期后恢复 | 暂停至请求截止后250毫秒；恢复时真实cancel.json已存在；生产Node终态仍为`executed/value=96`，写出时间晚于deadline | Rust在8293毫秒返回`host_interrupted/timed_out`、`cleanup_confirmed=true`，明确“迟到结果不再采纳；未自动重放”，没有把96计为成功 |
| 仍在函数内暂停 | 计划截止后4秒才恢复；父预算加生产1秒收尾宽限后，在9056毫秒返回超时且确认清理 | 原Node实例已退出；观察至调用起点12.5秒，超过预定恢复时间，没有resumed或终态写入 |

进程验证使用现有安全Windows接口：在函数暂停期间记录存活PID、创建FILETIME和映像路径；返回后同接口确认该实例已退出。本次三组均为“原PID不存在或进程句柄已退出”，没有把访问拒绝当作退出，也没有仅凭PID关联不同进程。测试工作线程已join；最终独立TEMP目录为空，生产IPC目录已清理。calculator无额外子进程，未将此扩大为任意插件派生进程树的验收。

首次实测三场景通过后，将实际迟到终态`executed/96`与其时间关系增为强断言，重新运行得到上表结果。两次测试编译失败分别来自测试直接使用unsafe、误用ProcessIdentity私有字段和序列化；修正均只在测试，采用仓库已有安全API。失败日志和首轮通过日志全部保留。

`cargo build -p coolzhu-core-runtime --offline`通过，0.34秒；`git diff --check`通过。已有未使用代码警告保留，未重复运行未受影响的Web全套。

可复跑：设置`DSH_OFFICIAL_RUNTIME`为上述MSI解包的`bin/dsh-runtime`，`DSH_OFFICIAL_SOURCE`为`tmp/2026-10-02-dsh-activation/probe-final/fixed-source-copy`，`DSH_OFFICIAL_RECEIPT`为同级`probe-final-result.json`，`DSH_OFFICIAL_OUTPUT`为新建空目录，然后运行`cargo test -p coolzhu-core-runtime --offline --test dsh_official_deadline -- --ignored --nocapture`。该测试默认忽略以避免缺少固定资源时意外运行；本轮明确运行了它，最终忽略数为0。

## 授权范围的统一收束

历史证据仍不足以唯一还原“原五项”的编号；沿用最近候选报告的五个已知工作面，不能编造完成率。以下合并恢复专项与本轮新证据，不再留笼统的“DSH离线补测待办”。

| 工作面 | 已有独立工程证据 | 仍未验收的真实条件 |
| --- | --- | --- |
| Browser use | 单调截止及ACK窗口修复、四项回归和SQLite生命周期聚合已通过 | 实际WebView释放期间Close/Cancel/navigation及销毁交错；旧8秒pointerdown后才关闭不能算覆盖，晚到计数1不能追认 |
| Paint computer use | 现有许可与敏感动作门禁保持，原人工复核证据完整 | 完整绘画和结果图验收；需要可用本地CU及原安全条件，不能由工程测试代替 |
| DSH | 固定来源、默认停用安装、显式启用/世代快照、父轮派发/取消/预算、官方96；恢复专项6测试15场景及本轮官方执行中截止已补齐 | 安装后市场UI和真实Qwen；Goal缺少完整接纳身份时仍拒绝，不声称已支持该路径 |
| 启动动画 | 绘制/清理异常交接修复及受控生命周期回归已进入0.2.66 | 原生连续画面、动作效果、主控制台真实可用性 |
| 泛光及总体审核 | 物理屏幕布局、缩放、主屏标记、租约/代次及静态边光已有限核对；候选MSI资源/来源/升级表已只读核验 | 多屏/DPI、点击穿透和生命周期显隐；真实安装/升级/失败恢复及总体产品验收 |

已明确列出的、无需人工恢复和付费模型的独立工程项现已收束；这不表示所有潜在缺陷已穷尽。当前缺少本地Windows CU，诊断不重复；无GUI、正式安装或模型执行的成功声明。下一验收点需要先明确正式安装范围，并具备本地CU及原安全流程的前置条件，才可进行真实产品验收。本轮没有请求解除隔离，也不替用户签署复核。

## 保留性

0.2.66仍为275651099字节，SHA256`ac7c1ec5c01b76f2132ce2a9276e3096389450b0be1b373f632e72f4bac52f25`，未安装、未重打包。1283冻结源码相对恢复专项无新增变化，426份原工作区文件保留（恢复专项的三个测试专属扩展按已记录哈希核验），旧交付和证据未变。

正式0.2.63的Web/Shell二进制、PID2572/3864及创建时间保持。只读安全API仍`isolated`、`accepts_new_input=false`，原未放行阻断`native-panel-fd88ef6ac7802d80f61b9c375dbad320`保留。原事故记录和Codex配置SHA不变。没有安装、升级、结束正式进程、重启、清库、解除隔离、付费调用或GitHub上传。
