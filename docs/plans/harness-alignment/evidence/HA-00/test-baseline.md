# HA-00 确定性基线回执

状态：已采集到一个真实通过的确定性样例；web-console 测试目标仍受 Windows 提交内存限制。

## 1. 生产构建检查

- 命令：`cargo check -p coolzhu-web-console --offline`
- 工作目录：`D:\coolzhuagent`
- 结果：exit `0`；`Finished dev profile ... in 1m 07s`
- 日志：`tmp/ha00-check.log`（105,706 B，SHA-256 `0a22abcb7d17c3f1dfda1127e1d7c2d48d386ed2b7f2ac0e4bd5dd0c3ac72ac2`）
- 说明：只有 warning，没有 error；证明当前生产源码可编译，不等于行为通过。

## 2. web-console 测试目标

### 第一次

- 命令：`cargo test -p coolzhu-web-console --offline dispatch_hook_budget_post_deadline_preserves_target_effect -- --nocapture --test-threads=1`
- 结果：exit `101`；测试目标编译失败：
  - `main.rs:79996`：`use super::ToolInvocationExecutor` 未解析
  - `tool_dispatch_audit.rs:2`：`super::ToolOutcome` / `ToolOutcomeStatus` 未解析
- 日志：`tmp/ha00-baseline-test.log`（6,686 B，SHA-256 `87329a1e29ecd52426d5dfd2e7551e0baa4233d93c054c8fc8780efbd34f870e`）

### 修复

只改导入，不改业务语义：

- `tool_dispatch_audit.rs`：拆为 `use super::{tool_audit_log_path, ComputerUseAuditRecord};` + `use runtime::{ToolOutcome, ToolOutcomeStatus};`
- `main.rs` 测试函数：`use runtime::ToolInvocationExecutor;`

### 第二次

- 同一命令重跑。
- 导入错误消失；rustc 在 test 目标代码生成/链接阶段报：
  - `memory allocation of 21233680 bytes failed`
  - `STATUS_STACK_BUFFER_OVERRUN`（`0xc0000409`）
  - `cargo` 报 `could not compile ... (bin "coolzhu-web-console" test)`；`TEST_EXIT=101`
- 日志：`tmp/ha00-baseline-test-2.log`（61,998 B，SHA-256 `eb0d4dd045138aaa42931623d8a4e4274044c9773267fb0789e67d54dc2ff762`）
- 判定：**环境资源阻塞**，不是测试断言失败；与既有工作记录中“Windows 提交内存不足、分八组注册测试”一致。不能据此宣称该测试通过。

### 第三次（关闭 test debuginfo）

- 命令同第二次，额外设置 `CARGO_PROFILE_TEST_DEBUG=0`、`CARGO_BUILD_JOBS=1`。
- 结果仍为 `TEST_EXIT=101`；rustc/LLVM 报 `rustc-LLVM ERROR: out of memory`、`Allocation failed`，随后 `STATUS_STACK_BUFFER_OVERRUN`。
- 日志：`tmp/ha00-baseline-web-test-debug0.log`（72,410 B，SHA-256 `5dce23f36977d8c45dcce9fe0be7489502759de645f51df9fa28ae504cbeff67`）
- 判定：单纯关闭 debuginfo 不足以让该 test 目标在当前 Windows 内存条件下完成链接；需要按既有分组/测试分片方式，或减少测试目标体积，才能继续 HA-01 的行为测试。

## 3. 小 crate 确定性样例（通过）

- 命令：`cargo test -p coolzhu-core-runtime --offline workspace_write_outside_needs_approval -- --nocapture --test-threads=1`
- 结果：exit `0`；`1 passed; 0 failed; 385 filtered out`；测试名 `permission_gate::tests::workspace_write_outside_needs_approval`；`Finished test profile ... in 6m 10s`
- 日志：`tmp/ha00-baseline-core-test.log`（6,710 B，SHA-256 `211019ecc9971a0414b390e773d56f10924f30df62f2fd278caf8b7a5bad6306`）
- 说明：证明“workspace-write 对工作区外路径需要审批”的确定性权限行为；不能替代 HA-07/HA-11 的 Web/模型验证。

## 4. 内存释放后重跑（通过）

用户关闭部分占内存程序后，内存从约 5.06GB 空闲/5.25GB 虚拟空闲升至约 7.72GB 空闲/8.53GB 虚拟空闲；同一命令（`CARGO_PROFILE_TEST_DEBUG=0`、`CARGO_BUILD_JOBS=1`）重跑成功：

- `tests::hook_budget_acceptance::dispatch_hook_budget_post_deadline_preserves_target_effect ... ok`
- `1 passed; 0 failed; 0 ignored; 0 measured; 1559 filtered out`
- `Finished test profile ... in 3m 55s`；`TEST_EXIT=0`
- 日志：`tmp/ha00-baseline-web-test-after-memory.log`（48,828 B，SHA-256 `cdd34ea2b2e1fda7c320c60af7249293d54d57fe20bd631c2159a6eb6edbe6db`）

结论：此前的 OOM 确认为**可用内存/提交限制**导致；内存释放后 test 目标可编译并执行。该测试只是 HA-06 的 hook 预算样例，不代表 HA-01 已通过。

## 5. 结论与限制

1. HA-00-03 的“确定性基线有实际记录”由第 3 节真实回执满足；第 2 节的失败也如实保留，不写成通过。
2. web-console 测试目标的内存失败是 HA-01 起持续测试的前置风险；需要按既有工作记录的分组方式或降低调试信息/并发，先解决测试可执行性，再进入业务修改。
3. 本回执没有运行模型实验；HA-11 的 15 组配对仍未开始，等待模型/额度/费用授权。
