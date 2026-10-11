# Hook 子进程中文解码

现场源码核对：core-runtime 的 Bash stdout/stderr 已调用 managed_process::decode_console_output，交接中的“仍待补 Bash”已过时，不重复修改。hooks.rs 的两个输出入口仍使用 String::from_utf8_lossy，中文 Windows GB18030 输出会出现替换字符。

本轮将 hook stdout/stderr 接入本 crate 已有解码函数，UTF-8 优先、GB18030 回退，保留原 trim、退出码 0 允许/2 拒绝/其它警告以及 hook 独立授权边界。未修改 MCP/协议字节解码。

Windows 专项回归使用显式独立 hook 授权，实际 PowerShell 子进程将合成中文以 GB18030 原始字节分别写入 stdout（退出 2）和 stderr（退出 1），验证中文消息和拒绝/警告语义。该测试不是外部模型或正式 GUI 验收。

cargo build -p coolzhu-core-runtime --offline -j1 实际 exit 0，耗时 1 分钟；日志为 tmp/analysis-hook-chinese-build.txt。完整 hooks::tests 最终实际 exit 0，7 passed/0 failed，包含两路 GB18030 子进程回归；其它 351 项被过滤，不能称 core-runtime 全量通过。日志为 tmp/analysis-hook-chinese-final-tests.txt。本源码候选不在已安装 0.2.128 中，整体 32WBS 保持未完成。

测试修复沿革：首次及诊断复现实际 exit 101，嵌套 cmd/PowerShell 内联引号将脚本当成字符串展开，没有得到预期退出码；改用临时 ps1 后第二类失败实际 exit 101，PowerShell 无法打开仍持有的临时文件。使用 into_temp_path 关闭文件句柄且保留自动清理后通过。原日志分别为 tmp/analysis-hook-chinese-tests.txt、tmp/analysis-hook-chinese-tests-diagnostic.txt、tmp/analysis-hook-chinese-tests-fixed.txt，未覆盖或删除。测试脚本在 crate 的 ignored tmp/ 下，不用实际用户凭据。
