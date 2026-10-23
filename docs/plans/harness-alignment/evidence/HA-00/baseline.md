# HA-00 可复现源码基线

状态：已冻结当前工作区源码；业务功能验收未完成。

## 位置与身份

- 基线目录：`docs/plans/harness-alignment/baseline/HA-00-20261018/`
- 源码 HEAD：`8a13551b037f5f673188802f2c9128984245fab8`
- 当前 `main.rs`：3,652,979 B / SHA-256 `3d1aa95e28d0eb839279e4d2381ddd20e6a1f160ea6fb9193b08e7ca7e930a4d`
- 0.2.142 final-staged-source `main.rs`：同哈希
- 规划指纹 `baseline.json` 的 `main.rs`：3,652,853 B / `90ab3349…`（历史观测，不覆盖）
- Pro 审核引用 `main.rs`：`817f…`（历史观测，不覆盖）

## 内容

- `source.patch`：660,858 B；SHA-256 `4bec04dc89e403cf2dc4806a0c44d65cf91a777adaab8c5dd939155b3d14774e`
- changed text files：73
- untracked source files：29（原样复制到 `untracked/`）
- excluded：`.coolzhu` 运行态、session DB/附件、日志、`tmp/`、`target/`、`dist/`、输出、安装包、密钥、非 Harness 所需二进制资源
- 基线前的副本：`baseline/HA-00-20261018-pre-test-import-fix/`（保留两处测试导入修复之前的版本）

## 恢复与校验

`restore.md` 给出恢复步骤。实际校验：

1. 对每个 changed 文件用 `git show HEAD:<path>` 还原 HEAD 侧；
2. 在临时目录 `git apply --binary source.patch`；
3. 复制 `untracked/`；
4. 用 `git hash-object --path <rel> <restored>` 与 manifest 的 `git_blob_sha1` 比较；
5. 对 untracked 文件用 SHA-256 比较。

结果：`restored_files=102`，`errors=0`，`git_apply_exit=0`。

> 说明：Git patch 对文本行尾做 LF 归一化，manifest 同时记录 `sha256_worktree` 与 `git_blob_sha1`；恢复后的文本可能与工作区仅差 CRLF/LF。

## 基线期间的一处修复

首次 `cargo test -p coolzhu-web-console` 在测试目标报：

- `main.rs:79996`：`use super::ToolInvocationExecutor` 未解析
- `tool_dispatch_audit.rs:2`：`super::ToolOutcome` / `ToolOutcomeStatus` 未解析

已做两处不改变业务语义的导入修复：

- `tool_dispatch_audit.rs`：`use super::{tool_audit_log_path, ComputerUseAuditRecord};` + `use runtime::{ToolOutcome, ToolOutcomeStatus};`
- `main.rs` 测试函数：`use runtime::ToolInvocationExecutor;`

修复后基线重新冻结。生产 `cargo check -p coolzhu-web-console --offline` 通过（exit 0，1m07s）。web-console test 目标随后在 rustc 代码生成/链接阶段遇到 `memory allocation of 21233680 bytes failed` / `STATUS_STACK_BUFFER_OVERRUN`，见 `test-baseline.md`（待补）。

## 未包含

- 未生成新的模型实验材料；HA-11 的 15 组配对仍待模型/额度/费用授权。
- 未启动隔离服务；隔离参数见 `current-gap-map.md` 第 6 节，启动前需重新核对端口与 `stop_owned_instance`。
