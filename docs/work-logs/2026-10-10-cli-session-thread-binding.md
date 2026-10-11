# CLI 会话线程身份绑定

实际检查发现 CLI 的交互文本、JSON 及内部模型回合均未设置线程身份，继承 thread-default。本次在三个 run_turn 调用前显式绑定持久会话文件身份。普通文本与 JSON 使用主会话文件；内部回合使用独占内部快照文件，避免内部执行冒充主会话回合。

ThreadId::from_session_path 规范化已存在路径，对平台原生路径字节做 SHA-256，不把本地路径直接放进事件标识。内容更新与相对路径中的点别名保持身份；不同规范化路径产生不同身份。身份读取失败通过问号返回，未发起模型回合。新增 setter 使宿主在开始回合前显式绑定，而无需移动现有运行时。

实际验证：core-runtime 与 CLI 离线串行 build 退出 0；agent_event 定向测试 3 通过、0 失败；CLI 完整回归 90 通过、0 失败。工具会话 31453 最终退出 0。日志为 tmp/analysis-cli-thread-binding-build.txt、tmp/analysis-cli-thread-binding-runtime-tests.txt、tmp/analysis-cli-thread-binding-cli-tests.txt。

边界：这是路径绑定，文件移动或复制到另一规范化路径将获得另一身份；不是随文件迁移的内嵌会话 UUID，也未验证 Windows 所有大小写、硬链接及跨平台路径别名。未在真实模型正式入口执行恢复矩阵；后台启动此前自动审批拒绝仍未解除。默认 core-runtime 调用方与 Web/其他入口的身份仍需各自验收。当前安装版 0.2.130 不含该源码，32WBS 未完成。
