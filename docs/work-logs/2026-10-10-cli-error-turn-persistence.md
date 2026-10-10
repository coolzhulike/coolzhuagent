# CLI 失败回合保存会话事实

文本 run_turn 原先仅成功保存；JSON 临时 runtime 在 run_turn 错误时经 ? 提前返回而丢弃。两个入口现共享 persist_result，成功/失败均调用既有 Session::save_to_path；JSON 先保留 runtime 再处理结果。执行错误与保存错误同时发生会同时报告，不把保存失败转成成功。

真实 ConversationRuntime 测试先派发获最低权限声明的 Read 工具，再由模型夹具返回反馈请求失败，保存后重新加载，核对完整 Session、工具调用 ID、执行输出与非错误工具事实。另测试以目录作为目标文件触发实际保存失败，确认原执行错误仍可见且成功回合保存失败也返回错误。

离线 CLI 构建实际退出 0（tmp/analysis-cli-error-persistence-build.txt）；新增回归最终 2/0，完整 crate 测试实际退出 0、82 通过、0 失败、0 忽略（tmp/analysis-cli-error-persistence-final-tests.txt、tmp/analysis-cli-error-persistence-full-tests.txt）。首次回归 1/1 失败，原因是测试工具缺少最低权限 metadata，已补齐声明；原失败日志 tmp/analysis-cli-error-persistence-tests.txt 与失败夹具保存文件保留，不称工具已执行。

该修改不改变既有文件保存的原子性或错误恢复能力；内部斜杠命令独立临时会话、根取消信号接线、共享聊天其他入口仍开放。未完成真实 CLI 模型验收，也尚未进入已安装 0.2.128，32WBS 保持未闭环。
