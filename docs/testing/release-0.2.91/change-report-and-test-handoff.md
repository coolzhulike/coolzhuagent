# 0.2.91 改动报告与针对性测试交接

本版修复DSH插件管理与执行轮询之间的锁竞争，以及首次迁移争锁造成的检查失败。正常安装版三项真实SWE-2-medium实测通过：配置更新拒绝旧调用、目录读取并发时新调用正常执行、真实插件宿主启动后取消并退出。原失败完整保留，0.2.90未公开发布；本报告不追认候选或旧版本整体通过。

主会话独立实现与测试；使用原Agent `session-1791131217833`、聊天室 `room-1791131523339`，继续唯一测试远端 `island-kayak`。未创建新云端测试会话。Paint不再测试、微信不改不测、Opus暂停。

## 改动及职责

1. `plugin-system/dsh_activation.rs`：在途已核验请求的撤销轮询直接读取持久设置、安装登记和启用世代，不抢普通管理写锁，不执行恢复。新接纳、启用提交与宿主启动完整复核仍在管理锁内；读取异常或真实世代变化继续撤销。
2. `plugin-system/install_transaction.rs`：取得管理锁时，仅对短暂 `WouldBlock` 最多等待1秒，每10毫秒再试。目录读取和快照复核能正常串行，避免ACP将瞬时Busy当作插件已经撤销。持续竞争或其它错误仍失败，不重试恢复、事务或第三方业务，不用旧缓存覆盖当前资格。
3. `web-console/schema_upgrade.rs`：迁移开始前的只读版本/完整标记检查及 `BEGIN IMMEDIATE` 遇SQLite `DatabaseBusy` 有界等待，接纳预算30秒。单次SQLite等待仍受既有5秒busy_timeout约束，最后一次等待可能越过预算；取得写锁后不重放迁移阶梯、业务或提交。
4. `devin_acp/process.rs`仅调整进程树回归的辅助程序：用轻量cmd子脚本自行写就绪回执，保留真实子进程、Job排空及排空后才释放绑定锁的断言。未改变生产协议、进程guard或安全权限。

前端未新增调试信息。没有清除输入安全历史、扩大权限或复活旧请求。根因、候选与原始失败见[本轮报告](../2026-10-07-plugin-lifecycle/report.md)。

## 正式安装与检查身份

冻结产品源码 `a366fbb2ec35bf75163ed643b79adfae176d8943`，源码快照 `148826ce31d0cc2b94b4e026043377609290c9244bbe775a352000fd074088ae`。正常完整构建六门通过，Windows正常安装返回0，1150个安装文件长度及SHA逐项匹配。MSI为276335234字节，SHA256 `a1c44f307beefda08ce776f3622b33f963dee1bbe37139ae0ca5b832c3a2fe9f`；桌面分发包 `C:/Users/zhupu/Desktop/coolzhuagent/dist/CoolzhuAgent-0.2.91.msi`。

实际offline build通过；主控制台1398通过、0失败、6忽略，插件44通过、1忽略，模块联动8通过，工具注册检查通过。冻结源码两条远端CI均success；先前b6的一路失败没有删除或改写。[原始检查日志清单](../2026-10-07-plugin-lifecycle/ci-repair/manifest.json)与[构建身份](evidence/build-identity/)可供复核。

## 正式安装版三项实操

测试运行Program Files中的正式web与shell文件，无调试浏览器参数。请求从聊天室普通消息输入发出；观察器只读查找本轮身份，修改配置与取消均走正常产品接口。各轮只有一个ACP attempt，effective为swe-2-medium、process_drained=1，继续原island-kayak。

| 用例及操作 | 预期与实际 | 证据 |
| --- | --- | --- |
| CONFIG：请求仅调用calculator一次算 `(72+24)*2`；接纳后正常更新config和启用世代 | 更新成功；旧调用409拒绝，工具failed、实际派发审计0，无心算、补发或重试。模型如实说明blocked，耗时54.4秒 | [变更时序](installed-plugin-lifecycle/installed091/config-change-timing.json)、[台账](installed-plugin-lifecycle/installed091/config-facts.json)、[原图](installed-plugin-lifecycle/installed091/config-result.jpg) |
| FRESH：独立新请求计算 `(72+19)*2`，同时有界读取市场目录 | 40次读取均成功；插件只执行一次，工具completed、审计ok，宿主真实返回182；42.6秒。目录临时持锁没有误撤销工具 | [目录读取](installed-plugin-lifecycle/installed091/bounded-catalog-read.json)、[台账](installed-plugin-lifecycle/installed091/fresh-facts.json)、[原图](installed-plugin-lifecycle/installed091/fresh-result.jpg) |
| CANCEL：独立新请求算 `(72+27)*2`；确认本轮execute模式真实Node启动后，调用正常interrupt | 确认同一进程183毫秒内退出；外层interrupted、ACP cancelled/drained，插件host_interrupted，聊天显示已中断，无计算结果或后续补发 | [精确进程与时序](installed-plugin-lifecycle/installed091/cancel-observation.json)、[台账](installed-plugin-lifecycle/installed091/cancel-facts.json)、[原图](installed-plugin-lifecycle/installed091/cancel-result.jpg) |

对应run依次为 `run-chat-63532f1c268727f64b25d4644f7fa8e643a7880c87fc4b14`、`run-chat-1f9fd0e7d6432d4ae2fd9dda6ceedccd1898baf3f010174c`、`run-chat-baf5e9210487188c3bdfa5394f0d3c97f451fbf43ef76b55`。完整请求前缀及原始身份在台账中。

取消时manifest及execute envelope尚未出现，只证明执行模式宿主启动后的取消，不声称计算函数已经开始或执行中超时通过。calculator忽略业务config，因此配置用例只证明配置对象与启用世代改变后的旧资格失效，不证明任意插件配置字段生效。40次有界读取也不等于无界压力测试。

## 恢复与历史保留

测试配置已通过正常接口恢复为 `{}`，新启用世代保留。测试配套按PID、UTC创建时间、路径和文件SHA核对后停止；正常桌面入口恢复原工程 `C:/Users/zhupu/coolzhuagent` 与原输入安全库，启动自检和实际Program Files进程一致。日常窗口原Qwen选择仅为恢复原环境，没有发送Qwen测试请求。

安全资源safe、accepts_new_input=1；历史2个outcome_unknown许可与9个closed记录保留。正式证据和恢复截图均收入[带摘要清单](installed-plugin-lifecycle/manifest.json)。PR84保持待审，不自动合入。

## 后续测试设计及开放范围

其它模型可按上述步骤设计定向测试：先正常安装核身份、确认原SWE和唯一远端，再分别发送独立请求，不复活旧轮。配置负例须证明“接纳在前、世代变更在后”，旧调用审计为0；新请求须核实际宿主结果和唯一成功审计，不能只信模型文字。取消须保存对应run/turn、真实子进程完整身份及退出事实；如果未观察进程或变化晚于执行，保留事实但不能计通过。锁测试区分短暂排队与持续Busy，迁移测试断言阶梯只执行一次，不能通过增加业务重放来消除争锁。

函数执行中取消/超时、许可冻结窄竞争仍未闭环。Browser复杂旋转/斜切/透视、严格按住跨URL与按下中关闭/面板替换，多屏、Goal/Relay附件及账号/模型切换边界、启动动画资源失败/减弱动作等仍见[总队列](../../analysis/2026-09-21-integration-review/current-acceptance-queue.md)。完整自动下载/安装/重启按既有范围延期；四项总体复核尚未完成，不能由本版三项外推全部验收。
