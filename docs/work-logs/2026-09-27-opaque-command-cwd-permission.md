# 2026-09-27 不透明命令的 cwd 权限边界

`tool-registry/path_effect.rs` 原先把 `bash`、`PowerShell`、`REPL`、`Agent` 入参中的 `cwd` 抽成 `Execute` 路径。`permission_gate` 的 `WorkspaceAuto` 分支据此把工程内 `cwd` 判为工程内操作并自动允许。此推论不成立：命令或子工具仍可访问工作目录外的路径。

只读核对执行链：Windows 的 `bash` 通过 `cmd /C`、PowerShell 通过 `-Command` 启动，两者仅设置进程 `current_dir`；REPL 直接启动对应解释器；Agent 子任务使用父工具快照和权限上下文，并没有一个由 `cwd` 构成的操作系统文件边界。Linux 可选的 `unshare` 也不能让单独的 `cwd` 证明命令只访问工程内。没有更强的实际隔离证据，因此不能继续以 `cwd` 作为自动放行凭据。

最小修复只改不透明命令的路径提取：始终返回“范围未知”，不改命令入参、启动目录或现有审批系统。对最低权限为 `DangerFullAccess` 的 bash/PowerShell/REPL，`WorkspaceAuto` 下即使 `cwd` 在工程内也需原有双确认；已有双确认可执行，`FullAccess` 仍走原有自动允许。Agent 入口本身只建立子任务，其工具使用继续受父快照、子工具名单与各次工具门禁约束，并没有新增一套 Agent 授权。

`coolzhu-tool-registry` 的 8 个路径提取定向测试通过，新增用例覆盖工程内 `cwd` 仍需确认、双确认允许及 `FullAccess` 允许；独立离线 build 通过。组合 Web 离线 build 与隔离 MCP 8K 调用实证亦通过。后续打包时仍需统一复查交互授权文字；本轮没有对用户实际运行目录发出命令或授予权限。
