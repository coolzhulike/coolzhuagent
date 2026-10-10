# 真实HTTP接纳不得绕过模型快照失败

chat_run_admission::accept原本filter_map丢弃捕获错误，真实HTTP执行随后可回退动态provider配置。现在真实模型开启且房间允许模型调用时，HTTP目标捕获失败返回409，已建立的guard负责失败收尾，模型与工具不执行。Devin独立协议与离线演示保留原路径。房间能力查询使用接纳已固定的db_path，不吞SQL错误。

扩展已有真实本地HTTP/SSE路由动态测试：正常两个入口读取两个真实文件、历史与重复tool ID断言保留；然后设置HTTP服务商与backend_kind不一致，两个入口均必须返回“无法冻结本轮会话”，网络总请求仍6、读取工具总数仍4。

offline build实际exit0，56.01秒，日志tmp/analysis-admission-model-freeze-build.txt；专项及全量回归待实际终态。

此处只修补HTTP快照捕获失败的接纳约束，不证明所有身份/取消/恢复矩阵、配置跨锁原子快照、非Web入口及32WBS正式验收完成。正式服务和唯一真实云端未操作。
首次专项失败（0通过/1失败，5.95秒）；诊断重跑失败（5.89秒），确认第二个负例使用同文本而被重复提交保护抢先拒绝。保留tmp/analysis-admission-model-freeze-tests.txt及tmp/analysis-admission-model-freeze-test-diagnosis.txt；已区分两个入口负例文本，保持去重保护与原快照拒绝断言。

最终专项实际exit0，1通过/0失败，5.98秒，日志tmp/analysis-admission-model-freeze-tests-final.txt；最终完整回归实际exit0，1452通过/0失败/6忽略，37.67秒，日志tmp/analysis-admission-model-freeze-full-regression.txt。
