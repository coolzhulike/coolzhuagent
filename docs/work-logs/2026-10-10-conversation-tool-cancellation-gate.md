# Conversation 目标派发前取消复查

实际 conversation 工具循环在 pre Hook 与权限重判之后直接调用执行器。新增目标执行前当前线程根控制复查，即使没有 Hook，或审批刚返回允许，取消/截止仍阻止目标执行。已有 Hook 事实保留在错误工具结果中。该检查不构成原子取消/执行提交事务，不宣称解决检查后竞态，也不改变已启动工具的取消能力。

新增测试在真实 PermissionPrompter 返回 Allow 前取消根令牌，使用无 Hook 的 ConversationRuntime 工具循环，验证目标执行器计数为零且结果明确未执行。正常授权、Hook 重判和反馈回归也通过。

离线构建实际退出 0；conversation::tests 最终实际退出 0，14 通过、0 失败、348 过滤。日志 tmp/analysis-conversation-cancel-build.txt、tmp/analysis-conversation-cancel-fixed-tests.txt。

首次测试编译错误误用了不存在的 AllowOnce，已按实际枚举修正，失败日志保留为 tmp/analysis-conversation-cancel-first-failure.txt。第二次测试发现 ReadOnly 模式直接拒绝升级、不进入审批，无法检验取消窗口；改用已有支持审批的 WorkspaceWrite→DangerFullAccess 路径后通过，第二次失败日志仍保留 tmp/analysis-conversation-cancel-final-tests.txt。不将前两次失败记为通过。

该候选尚未进入实装 0.2.128，所有入口根控制传递和正式模型取消矩阵仍未闭环。web 监督 worker 建立线程控制不能证明独立 ConversationRuntime 的每个宿主均建立相同控制。
