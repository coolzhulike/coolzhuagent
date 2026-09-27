# 最终 head 的 CI 测试夹具跟进（2026-09-28）

提交 `e62733adc26f040ab4f2540487134a89951d260c` 的 [push 检查](https://github.com/coolzhulike/coolzhuagent/actions/runs/36339744003) 与 [PR 检查](https://github.com/coolzhulike/coolzhuagent/actions/runs/36339747908) 均完成 Web 构建及 S0 六类路径、三项夹具拒绝回放，随后在 `Run web console regression tests` 失败。push 结果为 **1258 通过、1 失败、2 忽略**；PR 为 **1257 通过、2 失败、2 忽略**。两次运行的 fixture contract 与 workspace linkage 因前一步失败而跳过，不能算通过。

共同失败的 `web_frontend_chat_context_controls_live_in_compact_left_sidebar` 仍断言左栏顺序为“收件人→权限→工作区”，但已批准的单套聊天室权限入口位于顶部。测试现核对顶部权限弹层及唯一的选择器/保存按钮，去除失效的左栏几何顺序要求。PR 另有 `legacy_recovery_driver::tests::driver_refuses_without_a_real_commit_candidate_check` 在 `legacy_recovery_driver.rs:885` 报 `Busy { scope: "windows-session-2" }`。该测试传入的夹具 `coordination_scope` 在 `run_legacy_recovery(None)` 分支没有参与取锁；该分支调用生产 `InputSafetyCoordinator::begin`，请求真实交互会话锁。日志没有记录占锁者，**不能证明具体竞争者**，也不能把 push/PR 同时运行直接当成根因。

这次只改 `#[cfg(test)]`：六个 legacy recovery 用例以各自临时 `input_safety_root` 派生唯一协调范围，用现有 `begin_with_coordination_scope` 取得协调器后传入 `Some`；重放用例在检查当前 epoch 后显式释放，结账再放用例在两次调用之间释放并按同一范围重新取得。生产恢复取锁路径、输入门禁及运行时限均未更改。首次本机六例复验为 **5 通过、1 失败**：夹具过早释放新协调器，使重放用例的当前 epoch 断言看不到持有资格；保持资格到断言后才释放，第二次 **6/6 通过**。顶部权限定向测试 **1/1 通过**，`cargo build -p coolzhu-web-console --offline` 通过。完整 CI 的后续结论以本 PR 新 head 的 Checks 为准，不以本机定向通过代替。

已安装的 **0.2.23** 包仍对应打包当时的源码快照 `b910ca61d3d01a16c3c358072e824199777409e274835877ea12556f3f434e09`；此后测试源码有变化，不能称当前整树与旧包快照一致。本次不重打包、不更改已安装产品行为，保留原失败及新验证各自身份。
