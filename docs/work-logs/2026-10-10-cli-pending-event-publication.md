# 回合轨迹提交状态区分

此前独占最终 JSON 文件后直接写入，崩溃可能留下与完整历史同名的半份记录。本次独占创建 pending 扩展名记录，写完并 sync_all 后使用同目录硬链接发布最终 JSON，再删除 pending。硬链接创建失败不覆盖已有 JSON，保留 pending 供恢复处置；清理失败返回错误，不冒称保存成功。

恢复入口遇到 pending 明确报未提交、结果未知，不尝试解析为完成历史或自动重放工具。即使完整 JSON 已发布但 pending 清理前崩溃，也保持未知，后续需专门恢复判断。

实际 Windows 文件系统验证：碰撞发布保留旧 JSON 与新 pending 原字节，撤销碰撞后发布内容一致且 pending 消失；有效回合保存/读取继续通过，遗留 pending 被明确拒绝。最终离线 build 与完整 CLI 回归退出 0，91 通过、0 失败。日志 tmp/analysis-cli-pending-events-build-2.txt、tmp/analysis-cli-pending-events-tests-2.txt。

不宣称断电目录项持久化、非硬链接文件系统兼容或消息/事件原子事务。pending 的人工/自动恢复处理仍待实现；写入大小与历史读取上限的一致性仍待完善，不能据本次状态区分宣称全部存储恢复闭环。当前安装版 0.2.130 未含该改动，32WBS 继续。
