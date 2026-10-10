# Goal执行索引在途取消等待

GoalExecution保存与Live相同的取消令牌，CURRENT作用域内绑定CHAT_CANCELLATION，并以biased select观察取消；停止后丢弃阶段等待，不把等待结束声称外部副作用已撤销。run_goal_phase_once将停止原因交给原token-aware Goal finalizer，只有原运行终态提交后才同步索引。保持不自动重放与claim不外泄。

生产offline build实际exit0，1m02s，tmp/analysis-goal-index-cancel-build.txt。动态专项通过真实ExecutionService::cancel在运行中写stop_requested并通知令牌，永久等待在2秒期限内结束；stop_requested时finish_committed拒绝，随后fixture模拟原提交器写interrupted后索引同步interrupted。专项实际1通过0失败，0.12秒，tmp/analysis-goal-index-cancel-tests.txt。注意fixture最后的SQL终态不是调用真实Goal提交器，此项不能支持真实阶段联合取消已验收。

完整回归待终态。还待真实Goal模型/工具在途停止、迟到回复、原finalizer无消息/verdict污染、父子取消、跨工程/claim失效及外部副作用证据。正式服务、唯一SWE云端和版本未操作，32WBS仍未闭环。
完整回归actual exit0：1455通过0失败6忽略，37.77秒，tmp/analysis-goal-index-cancel-full-regression.txt。包含既有Goal stop-wins与late-result专项，但未形成原生/真实provider联合验收。
