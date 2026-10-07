# DSH生命周期轮询锁竞争修补与候选实操

0.2.89正式安装版在真实SWE请求接纳后更新calculator配置，启用返回host_interrupted，旧配置没有改变；旧请求仍正确返回192，因此该轮不能计配置过期负例通过。独立目录读取并发探针在63毫秒内同样触发dsh_runtime_interrupted。普通管理锁被取消轮询误判为生命周期撤销，是agent侧缺陷。

## 改动及职责边界

plugin-system的在途ticket/snapshot轮询改为直接读取持久设置、安装登记及启用世代，不争抢管理写锁或执行恢复。这里只维持已核验请求的在途撤销检查；新接纳、宿主启动、启用配置提交仍持管理锁执行完整来源/资源/源码核验。读取异常及真实世代变化继续撤销，不提升权限、不重放业务。

新增一项必要回归：持普通管理锁时，原ticket/snapshot继续有效，但新activation_ticket仍Busy；更新配置世代与停用后旧资格失效。修补前此用例因Busy失败，修补后插件43通过、1忽略；完整控制台1397通过、6忽略，实际offline build通过。无模型回复夹具参与实操。

## 源码候选真实结果

原SWE-2-medium、room-1791131523339、唯一island-kayak保持，未创建新云端会话。候选为debug后台及089正式外壳，不算正式安装版验收。

| 项目 | 结果 | 证据 |
| --- | --- | --- |
| 接纳后更新配置 | enabled=true、新activation；旧调用409拒绝，未派发实际插件执行，模型如实blocked，无心算或重试 | [时序](candidate/config-change-timing.json)、[台账](candidate/facts-read.json)、[原图](candidate/config-stale-result.jpg) |
| 更新后独立新请求 | calculator真实返回182，单工具/单审计成功 | [台账](candidate/fresh-facts.json)、[原图](candidate/config-fresh-result.jpg) |
| 宿主启动后取消 | 正常聊天interrupt接口返回；同一真实Node进程154毫秒内确认退出；外层interrupted、ACP cancelled/drained=1，插件host_interrupted，未返回计算值 | [进程与回执](candidate/cancel-observation.json)、[台账](candidate/cancel-facts.json)、[原图](candidate/cancel-result.jpg) |

取消仅证明execute模式宿主启动后的退出，不声称计算函数已经开始；manifest与execute envelope当时均未出现，函数执行中取消/超时保持待验。calculator不读取业务config，本轮只验证配置对象和启用世代的撤销边界。测试后config通过正常接口恢复为{}，新世代和历史保留，不复活旧请求。

首次取消观察器在UI发送前到期，未取消且不计通过；之后重新运行观察器捕捉同一新请求并成功取消，两份事实均保留。候选另一次无界目录压力探针超过HTTP超时，脚本缺finally导致继续等待；只停止精确身份的自有探针及配套候选，未计压力通过。正常候选重启后的配置与取消结果不能证明该压力场景已解决。

## 正式测试交接与开放项

下一正式版本须核包身份、正常安装后复测配置变化、新请求、取消与目录并发有界负载。保留原失败，不以候选覆盖正式验收。函数执行中超时、许可冻结窄竞争仍开放；不得为制造长任务修改真实远端calculator源码、伪造回复或另建云端会话。完整原始文件及摘要见[manifest](manifest.json)。
