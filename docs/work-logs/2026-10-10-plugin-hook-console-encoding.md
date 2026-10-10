# 插件钩子中文输出解码

插件系统的钩子 stdout/stderr 原先直接使用 UTF-8 lossy 解码，中文 Windows 的 GB18030 输出会损坏拒绝或警告消息。该 crate 新增独立解码函数与 encoding_rs 依赖，优先 UTF-8，失败回退 GB18030，无法解码时才使用 lossy；仅接入钩子的两处输出，不改变退出码与授权规则。

真实 Windows PowerShell 子进程分别向 stdout/stderr 写入 GB18030 字节，并以 2/1 退出，测试确认中文完整、无替换符、拒绝与警告语义保持。

验证：`cargo build -p coolzhu-plugin-system --offline -j1` 实际退出 0；完整 crate 测试实际退出 0，46 通过、0 失败、1 忽略。日志位于 `tmp/analysis-plugin-hook-build.txt` 与 `tmp/analysis-plugin-hook-tests.txt`。

该修改属于集成候选源码，尚未进入已安装的 0.2.128，也不证明真实模型、正式 Browser 输入零事件、Hook 取消预算或其他 32WBS 门禁通过。正式后台启动先前被自动审批拒绝，运行验收仍未闭环。
