# Goal索引归属检查与终态写入原子化

审查发现finish_committed先query阶段state，再调用Guard::finish通过另一连接写索引，存在检查后claim/状态变化的窗口。绑定同样在初检后无条件关联run。

现绑定UPDATE原子核对run/claim/kind/workspace/room/session/phase/running，影响行非1拒绝。终态同步单条UPDATE以EXISTS核对相同claim及真实completed/failed/interrupted，并从同语句子查询取得状态；索引也核对bound run及工程和可收尾状态。失败不设guard.finished，drop保留未知结果，不自动重放。

专项扩展旧claim面对新所有者completed时必须拒绝且索引仍running，恢复有效fixture claim后failed同步通过；保留错误claim、重复绑定、提前完成、CURRENT和不外泄断言。生产offline build actual0，55.21秒，tmp/analysis-goal-index-cas-build.txt；专项及完整回归待终态。

此测试是身份负例，不冒充高并发跨进程故障矩阵；索引创建与绑定仍分两步，失败会留下outcome_unknown，不能据此重放。正式provider、原生取消与32WBS正式验收仍未完成。
专项actual0：1通过0失败0.07秒，tmp/analysis-goal-index-cas-tests.txt；完整actual0：1456通过0失败6忽略37.79秒，tmp/analysis-goal-index-cas-full-regression.txt。
