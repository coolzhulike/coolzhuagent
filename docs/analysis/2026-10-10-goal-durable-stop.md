# Goal等待观察持久停止与claim失效

直接api_cancel_goal/暂停入口可只写数据库stop_requested，不通知GoalExecution专属令牌。此前只观察索引cancel，导致这些入口的在途模型等待不能及时结束。

现阶段scope同时观察captured DB中的同run/claim/workspace/kind状态，250ms轮询；查询放spawn_blocking。仅running继续，持久停止/结束、claim失效或查询无法确认均终止等待，向CHAT_CANCELLATION传播request，由原Goal提交器处理终态，不自行重放。查询不读当前工程路径、不输出claim。

新增动态专项：不经执行页cancel，另一路径调用真实interrupt_runtime_run_sqlite；另组替换claim。两组永久等待需在3秒内退出且工具上下文令牌requested。生产offline build actual0/57.43秒，tmp/analysis-goal-durable-stop-build.txt；专项及全量待实际终态。

仍待真实模型/外部工具在途取消及原生联合验收；查询错误采用停止等待语义，不声称外部副作用撤销。32WBS与正式验收未完成。
专项actual exit0，1通过0失败0.62秒，tmp/analysis-goal-durable-stop-tests.txt；完整actual exit0，1456通过0失败6忽略37.60秒，tmp/analysis-goal-durable-stop-full-regression.txt。
