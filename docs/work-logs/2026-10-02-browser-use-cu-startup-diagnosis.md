# Browser Use续验：本会话CU启动故障

本轮优先响应“先解决Codex Computer Use再自主验收”。实际线程01a0fadf…与日志相符：cua_repl/codex_app在08:23:40Z启动failed，只有笼统错误，底层原因未提供；父线程01a0f094…最新cua_repl为ready。本地插件启用、Node/模块/@oai/sky依赖存在，当前工具目录仍无CU入口。未重装、扩大权限或使用私有协议。

后续端点纠正：父会话明确其CU连接dot云端Linux，不能控制本地Windows。日志双方均为hostId=durable，但环境ID不同；撤回“由父ready CU直接观察本地”的建议。当前失败无底层错误码，邻近-32601属于另一个线程的unsubscribe。现有Node实际--version返回v24.21.0/exit 0；基础依赖健康不等于CU握手健康。CLI无restart/reconnect命令，当前没有服务重启工具，因此未尝试私有重连或实际窗口枚举。

最小外部步骤为在本地客户端当前任务查看/mcp verbose；仅当设置确实有失败服务独立Restart入口时重试一次，再由本地node_repl枚举Windows窗口确认。服务重试可能清空REPL状态/中断该服务调用，不要求退出被测应用；若无入口，保留诊断交由客户端支持，不重启整个客户端。配置哈希未变。默认模型配置不是活动模型证明，Astra/High/标准速度仍须由活动会话状态核验。

独立继续工程检查：Shell原生Browser定向5通过/59过滤，Web输入传输2通过/1303过滤，均退出0。无生产代码修改、无真实输入/项目模型请求。原正式063隔离1、Shell3864原创建身份仍存活；未退出或恢复。064候选未安装。

[详细诊断、具体事故与审核建议](../analysis/2026-10-02-browser-use-cu-availability-review.md)，[证据索引](../testing/browser-use-resume-2026-10-02/evidence-index.json)。真实Close/Cancel/导航竞争、未知释放恢复、旧文档代次和连续真实模型闭环保持未验收。
