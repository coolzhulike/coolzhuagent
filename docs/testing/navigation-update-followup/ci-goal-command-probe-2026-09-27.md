# Goal 验证命令 CI 探针记录（2026-09-27）

同一提交 `9eae50ebd857f95ea44fd542c9a8399a374a385f` 的两次 Windows CI 结果不一致：[push 运行](https://github.com/coolzhulike/coolzhuagent/actions/runs/36330165080)通过，Web 控制台单测为 `1259 passed, 0 failed, 2 ignored`；[PR 运行](https://github.com/coolzhulike/coolzhuagent/actions/runs/36330167698)失败，结果为 `1258 passed, 1 failed, 2 ignored`。唯一失败的是 `goal_command::tests::gate_exit_and_expired_root_use_the_same_managed_path`，停在首个 `exit 0` 探针的 `is_ok()` 断言。原断言没有打印 `Err`，因此现有日志不能判定失败的实际原因。CI 负载使原 30 秒预算耗尽是可能解释，但尚无直接证据。

已对 `modules/gui-web/packages/web-console/src/goal_command.rs` 做仅限 `#[cfg(test)]` 的窄修：正常退出与非零退出探针各使用 90 秒预算，断言失败时打印完整 `Result` 和实测耗时；非零退出必须包含明确的 `失败（exit 1）` 标记。旧断言只查找 `exit 1`，即使错误仅在命令文本中带有该字样也可能误通过。根执行预算过期分支仍用 30 秒。生产代码及其执行时限、排队计时和取消行为未改。

本地仅运行该单测：`cargo test -p coolzhu-web-console --offline --bin coolzhu-web-console goal_command::tests::gate_exit_and_expired_root_use_the_same_managed_path -- --exact`，结果为 `1 passed, 0 failed`，测试本身耗时 `0.51s`；`git diff --check` 通过。原始命令输出保存在忽略目录 `tmp/candidate-0.2.22/goal-test-narrow.log`，SHA-256 为 `79F6C117C410709A544EBCB8C75F15FDFFF2FD273961B5427E1A874A3D0B373A`。这只证明单独探针通过；修复是否消除远端不稳定性，要由下一次同头 CI 判断。
