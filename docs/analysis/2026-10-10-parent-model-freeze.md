# 父模型快照贯穿共享执行

审查发现父执行虽然已捕获HostModelSnapshot，普通反馈轮、SSE首轮及文本恢复仍从活动会话读取provider与请求设置，会在回合中修改配置后漂移。已让三处优先使用同一父快照的client、request及父工具定义；子执行仍使用自身已冻结scope；无快照旧入口保留既有行为。

动态测试扩展为父/子执行×普通/首响应续跑四组：捕获快照后将会话端点改为127.0.0.1:1，工具反馈必须继续原本localhost服务，父执行恢复也必须成功。真实历史及重复provider tool ID断言保留；子请求账本计数保留。

生产offline build实际exit0，58.73秒，日志tmp/analysis-parent-model-freeze-build.txt。专项与全量回归结果待实际终态追加。

此变更不证明所有入口都已捕获快照，不覆盖SSE网络首轮配置切换动态验收，也不证明配置读取跨锁原子性、Goal索引/恢复/取消或正式provider验收闭环。32WBS保持未完成。
专项实际exit0，1通过/0失败，2.33秒，日志tmp/analysis-parent-model-freeze-tests.txt；最终完整回归实际exit0，1452通过/0失败/6忽略，37.93秒，日志tmp/analysis-parent-model-freeze-full-regression.txt。chat_run_admission.rs亦已有捕获逻辑；未对所有入口穷尽审计。
