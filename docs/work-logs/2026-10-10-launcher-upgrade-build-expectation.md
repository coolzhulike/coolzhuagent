# 升级后的后台版本复用期望

启动器源码原先直接取用户状态 last_background_build_version 作为复用版本期望。升级后该字段可以仍是旧包版本，因此同工作区的新后台可能被旧历史版本拒绝，旧后台反而与历史版本匹配。此问题独立于用户截图中确实存在的跨工作区冲突，不改变截图归因。

修改为优先取当前启动器编译期 SHA 和日期，按后台相同规则产生 SHA 前 12 字符及日期。支持后台相同的四个 SHA 来源并跳过空字符串，日期来源顺序亦一致。未嵌入 SHA 的本地构建保留历史观测回退。工作区、数据库、协议与不同版本的既有冲突规则不变。

回归覆盖升级前历史身份与升级后当前身份；并调用真实 decide_reuse，确认同工作区当前包后台可复用、旧包后台仍冲突。cargo build -p coolzhu-app-launcher --offline -j1 实际 exit 0；全 crate 测试实际 exit 0：库 64、二进制 9，失败 0。日志为 tmp/analysis-launcher-current-identity-build.txt 和 tmp/analysis-launcher-current-identity-tests.txt。

本修改是安装版 0.2.128 之后的源码候选，未重新打包安装，未完成真实升级启动矩阵。此前后台启动的自动审批拒绝继续保留，不以单元回归代替正式运行验收。完整 32WBS 继续开放。
