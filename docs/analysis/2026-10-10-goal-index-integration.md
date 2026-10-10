# Goal真实阶段运行接入执行索引

新增GoalPhase来源，Devin ACP Goal明确拒绝。GoalExecution绑定已有runtime_runs(kind=goal_phase)时核对run/claim/workspace/room/session/phase与running状态，不创建第二个运行；claim只保存在内部guard，不进入spec。CURRENT覆盖原阶段模型/工具，后代可沿bound run关联索引。finish_committed只读取同claim原运行的completed/failed/interrupted状态，拒绝提前完成；drop保留outcome_unknown，不重放。

真实run_goal_phase_once已在阶段claim捕获后、模型前绑定索引，绑定拒绝走原阶段失败finalizer；原record_goal_phase_model_result_at_path_with_claim提交后更新索引，再执行既有memory流程。执行页增加Goal来源标签与过滤。

初始生产build0/1m09s，绑定专项actual1通过0失败/0.07秒；入口整合build0/1m04s。日志tmp/analysis-goal-index-binding-build.txt、tmp/analysis-goal-index-binding-tests.txt、tmp/analysis-goal-index-integrated-build.txt。全量回归待实际终态。

当前取消沿ExecutionService既有interrupt_runtime_run_sqlite及signal_runtime_run_after_interrupt作用于真实Goal run，但在途模型/工具取消、迟到提交、Goal后代树与usage仍需要动态联合验证。此处未认定Goal执行中心完整闭环，也未执行正式原生/真实provider验收。
整合完整回归actual exit0：1454通过0失败6忽略，37.92秒，tmp/analysis-goal-index-integrated-full-regression.txt。无真实原生/云端验收外推。
