# 启动器成功观测历史收敛

复用成功原先不调用 record_observations，首次新启动的保存错误被忽略。两条成功入口现统一 record_successful_observations，记录当前 launch_id、实际后台工作区、构建及实际数据库绑定；观测缺少工作区/构建或工作区与请求不一致时不更新，保存错误明确输出。

仅更新 observations，既有 revision CAS 与用户选择保存机制保留。数据库观测为空时保持未知而非沿用旧工作区绑定，不将观测记录当新的业务配置权威，也不为历史保存失败回收已启动实例。

实际落盘回归核对最新 launch/build/db/workspace，用户选择与 legacy selection 不变；不同工作区观测不覆盖已保存历史。完整 launcher 测试实际退出 0，库 64/0、binary 9/0，共 73 通过；离线最终构建实际退出 0。日志 tmp/analysis-launcher-observation-history-final-build.txt 与 tmp/analysis-launcher-observation-history-tests.txt。

当前测试使用独立 tmp 用户状态，不修改实际用户启动选择。真实两实例切换竞态、安装包升级复用、断电及磁盘满仍需正式矩阵，实装 0.2.128 尚不含候选，32WBS 未闭环。
