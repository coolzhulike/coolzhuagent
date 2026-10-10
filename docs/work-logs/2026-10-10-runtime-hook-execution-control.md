# 运行时 Hook 继承根执行控制

运行时 Hook 原先自行 spawn、写 stdin 并 wait_with_output，未继承当前根执行的取消与截止。现在调用本 crate 的 managed_process::output_with_input，保留原环境变量、stdin JSON、stdout/stderr 和正常退出码语义，使用现有受控子进程及管道回收路径。

已确认取消/截止及入场 Interrupted 转为拒绝工具继续派发，不能作为普通 hook 警告放行；拒绝会停止后续钩子。其它启动错误仍保持原警告语义。executed_commands 保留原尝试次数口径，不能当实际启动进程数量。

验证：离线 core-runtime 实际构建退出 0；hooks::tests 实际退出 0，9 通过、0 失败、351 过滤。新增已取消根令牌不启动钩子，以及真实 Windows 长睡眠子进程受 200ms 根截止中止、后续标记钩子不执行的验证；既有授权、退出码和中文输出回归也通过。日志：tmp/analysis-hook-control-build.txt、tmp/analysis-hook-control-tests.txt。

本修改只覆盖运行时 Hook 使用当前线程执行控制的路径。不证明所有调用者都建立根控制，也未覆盖插件系统独立 Hook 路径或真实模型正式取消矩阵。Windows 子树回收使用已有 Job 实现，非 Windows 不扩称同等子树能力。当前候选尚未进入已安装 0.2.128，WBS 1.5 与 2.3 保持未闭环。
