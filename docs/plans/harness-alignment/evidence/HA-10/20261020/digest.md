# HA10 任务内有限证据复用

## 实现

- `goal_command::Receipt` 新增可选 `input_digest`，旧记录为空时保守地不复用。
- `verification_digest` 只哈希源码/构建配置/锁文件（`.rs`、`Cargo.toml`、`Cargo.lock`、`.toml/.lock/.yaml/.yml/.ps1/.bat/.cmd`），排除 `target/.git/.coolzhu/node_modules`；根路径先 canonicalize。
- `take_matching_receipt` 除 command/cwd/shell/timeout 外，要求 `input_digest` 等于当前摘要；旧/失败/无 shell 回执不复用。
- 复用只通过同阶段 `repair_result.command_verification.receipts` 消费；没有跨任务通用缓存。
- FilesExist 仍返回 NotRequired，不会因空失败列表升级为 Command passed；文件存在性由 `goal_required_file_is_ready` 直接复查。

## 实跑

- `input_digest_reuses_unchanged_and_invalidates_modified_source`：1 passed；无变化复用原回执，改 `.rs` 后旧回执失效并新执行。
- `ha09_repair_result_receipt_is_reused_without_second_process`：1 passed；repair 回执复用不启动第二个进程。
- 125 项定向回归：125 passed / 0 failed / 0 ignored，197.92 秒，无源码变化。
- `cargo build -p coolzhu-web-console --offline` exit 0。

## 局限

- digest 不包含运行态 `.json`（避免会话库自身变化导致每次失效）；若某验证只依赖 JSON 配置，变化不会自动失效。
- 状态追问/换角色/任务 revision 没有单独的验证入口；它们不会清空同阶段 prior，因此不会单独触发重跑，但没有真实 UI 状态事件测试。
- 没有“关闭复用”显式开关；未匹配/旧记录自然回退为 runtime 执行。
