# 共享 CLI 断流身份与原始状态

断流确认原有 run/turn/session/room/workspace 比较保留；新增真实本机 HTTP 回归逐个替换五个身份字段，确认外来 completed 响应不设置 receipt 终态、不输出完成事件。正确 completed 正对照与 orphaned 对照均使用真实查询接口路径。

确认函数入口现在要求 run 与 turn 完整；未知接纳仍不查询/重试业务。orphaned 展示仍映射 failed，但生成的 done JSON 增加 runtime_state 原始状态，保留权威恢复诊断事实；不推断此前工具是否投递或业务可安全重跑。

离线 CLI 构建实际退出 0；完整 crate 测试实际退出 0，88 通过、0 失败、0 忽略。日志 tmp/analysis-cli-disconnect-scope-build.txt、tmp/analysis-cli-disconnect-scope-tests.txt。新增函数内包含七组实际 HTTP 场景，不是七个独立测试函数。

本回归直接覆盖断流确认函数，不替代完整断流前后模型/工具/停止实机矩阵；正式产品后台仍未恢复验收，0.2.128 尚不含候选。32WBS 保持开放。
