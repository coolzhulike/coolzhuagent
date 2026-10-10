# CLI 内部命令独立回合快照

内部提示词原先在临时 ConversationRuntime 执行，成功/失败均未保存。现于建立 runtime 后、执行前，在主会话父目录的 internal-turns/<主会话文件名>/ 下用 create_new 独占预留独立文件；名字包含时间、PID 与递增序列，碰撞重试有上限。预留失败时不执行模型/工具。

内部回合成功或失败均复用 persist_result 保存，stderr 显示路径，主会话不写入内部提示词；此前快照不覆盖。文件保存失败会保留执行错误信息，但预留的空文件可能存在，不能把仅有文件名当完整证据。仍使用既有 Session 保存格式与行为，不宣称具备断电原子性、完整回合终态 metadata 或快照 GC/全局配额。

新增磁盘回归验证失败内部回合工具事实重新加载、连续快照路径不同，以及主会话和第一快照字节不变。离线 CLI 构建实际退出 0；完整 crate 测试实际退出 0，83 通过、0 失败、0 忽略。日志 tmp/analysis-cli-internal-persistence-build.txt 与 tmp/analysis-cli-internal-persistence-tests.txt。

该路径覆盖 run_internal_prompt_text_with_progress 的内部命令调用，不证明原生模型/工具正式验收完成。CLI 根取消接线、权限撤销、完整 usage 分账以及安装包升级仍开放，实装 0.2.128 不含本候选。
