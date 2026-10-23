# HA09-04 源码变更复核（对 HA00～HA06 旧证据适用性）

本次源码变更范围：

- `modules/gui-web/packages/web-console/src/goal_command.rs`：Goal Command 使用与 PowerShell 工具相同的解释器解析；Receipt 增加 `shell` 字段；加入诊断回执的 call_id 暂存/消费。
- `modules/gui-web/packages/web-console/src/goal_verification.rs`：Command 结果可消费当前 tool message 的真实进程回执；repair 持久化回执可被同阶段消费；继续拒绝旧/空/不匹配记录。
- `modules/gui-web/packages/web-console/src/main.rs`：只在 Goal 验证消费者传入 tool messages、持久化 `repair_result.command_verification`；未改普通聊天路由、权限门、请求绑定或 HA01～HA06 的语义实现。
- `modules/core-runtime/.../bash.rs` 与 `modules/tooling/.../tool-registry/src/lib.rs`：只增加可选的宿主进程回执字段/填充，不改既有 stdout/stderr/exit 行为。
- `ha09_tests.rs`：新增真实进程、repair、phase finalizer 测试。

复核结论：

- HA00 的四路径/调用链结论没有被这些 Goal 验证消费者改动推翻。
- HA01～HA06 的旧验证由本次 `ha09-04-regression-03` 重新覆盖：124 项通过、0 失败、0 忽略，184.47 秒，运行期间无源码变化；命令与源文件哈希在 `runs/ha09-04-regression-03/receipt.json`。
- HA09-01/02/03/05 也在同一回归中重新执行；HA09-04 另有三条专项真实进程/phase 证据。
- 旧 E-HA09-WEB/E-HA09-BUILD/E-HA00-HA09-REVIEW 保留为历史，不修改其输入哈希；当前通过项改用新的回归/构建/复核证据。

限制：本复核不把“源码存在”当行为通过；行为结论来自同目录的真实测试回执。
