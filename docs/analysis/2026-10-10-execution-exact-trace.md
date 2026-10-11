# 执行详情按真实运行定位轨迹

修复execution_service::api_trace返回房间整页、前端最多扫描20页的问题。服务端从已验证工程的执行记录取得bound run或真实parent_run，覆盖外部run_id并清除before，再按workspace+room+run精确查询。没有绑定run明确409，不存在或不同工程房间404。普通房间trace仍保留原分页；新增run_id用于精确查询。执行卡改为一次请求，保留工程/详情代际迟到响应检查。

新增真实临时SQLite动态测试：121条同房间运行中直接取最旧trace-0；调用for_run覆盖外部错误run及invalid cursor；跨工程、跨房间及missing均404。不能用此测试推断全部后代树与Goal索引已实现。

生产offline build实际exit0（1m03s），tmp/analysis-execution-exact-trace-build.txt；JS语法检查通过。首次专项编译失败E0425（测试锁路径遗漏），保留tmp/analysis-execution-exact-trace-tests.txt，已改crate::tests::config_test_guard；重跑及全量结果待实际终态。

正式UI/真实provider验收仍未执行；GoalPhase索引、取消及后代usage聚合仍未闭环。未操作正式服务/云端/版本。
最终专项actual exit0，1通过0失败，0.54秒，tmp/analysis-execution-exact-trace-tests-retry.txt；完整actual exit0，1453通过0失败6忽略，37.37秒，tmp/analysis-execution-exact-trace-full-regression.txt。原工程代际异步前端模拟通过：冻结scope头、切工程清理、迟到详情/取消错误/轨迹废弃和dispose。
