# HA-10 复验证源码复核（2026-10-23）

触发：`goal_verification.rs` 在 HA-10 验收后再次修改（空/缺省 cwd 修复 + 畸形 Command 合同显式 Invalid），四条既有证据输入哈希失配。本复核确认修复后语义仍满足原证明对象。

## 复核点

1. `evaluate_with_prior`：仅消费同阶段已保存的 passed 回执；command/cwd/shell/timeout 全匹配且 `input_digest` 与当前工作区 digest 一致才复用——未变。
2. `verification_digest`（goal_command.rs）：只对 `.rs/.toml/.lock/.yaml/.yml/.ps1/.bat/.cmd/Cargo.toml/Cargo.lock/build.rs` 计数，排除 `target/.git/.coolzhu/node_modules`——源改动→digest 变→旧回执失效。
3. **新修复语义**：`cwd` 为 None 或空/空白 → 解析为工作区根（与计划归一化写入 `""` 的行为一致）；`commands` 缺失/空数组/纯空白 → `Invalid`，不再是可静默通过的合同。
4. 失败/中断/旧格式/无 shell 回执仍不复用；FilesExist 等非 Command 合同不会升级为 Command 成功。
5. 集成复核：b08b 双侧真实链路（evidence/HA-11/20261021/b08-analysis.md）以计数器证明同输入 0 重复执行、相关修改后真实重跑、畸形合同计划期拒绝。

结论：原 E-HA10-CURRENT-* 证明对象在新源码上仍成立；差异点（空 cwd 语义）为缺陷修复而非语义削弱。哈希以本记录 inputs 为准。
