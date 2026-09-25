# 实施契约与验收矩阵交叉 QA

日期：2026-09-21。范围：`implementation-contracts.md`、`execution-detail-supplement.md`、`acceptance-test-matrix.md`（38例）及机器表头。只读审查，未修改上述文件、未执行案例。

共有 **5 条需要落实的行动项**。它们影响接口编码、崩溃接管、截止行为或测试门禁；没有发现需另列的来源归属问题。相对链接当前可解释，最终文档重命名/复制后由集成步骤统一重写和校验即可。

1. **冻结输入事实的完整合法值与组合，避免出现隐含第四枚举。** [契约第11行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/implementation-contracts.md:11)列出 `not_sent / may_have_been_sent / sent`，随后写“不知道时写unknown”；[CU-07第141行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/acceptance-test-matrix.md:141)又写 `may_have_been_sent/unknown`，容易被实现为两个发送状态。请明确 `input_delivery` 只有三值，`unknown`究竟是独立字段值、JSON null，还是说明文字；给出发送前拒绝、确认部分发送、全部发送、发送后失联四条JSON样例，并规定 `partial`、确认点数及坐标的可空性。例如“确认部分输入”可为 sent+partial=true，“是否输入均不确定”为may_have_been_sent+partial=null，但最终映射应统一冻结。同步CU-05/06/07及CSV，断言非法组合被拒绝，不再让枚举名取决于测试作者解释。

2. **独占崩溃测试必须覆盖原生helper成为孤儿而继续输入。** [契约第41行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/implementation-contracts.md:41)及[CU-08第151行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/acceptance-test-matrix.md:151)目前是“A死亡→OS回收→B新观察后取得租约”。但现有执行路径由Rust宿主启动PowerShell/C#输入helper，父进程死亡不天然保证helper同时退出；若租约只由父进程持有，B可能与仍执行路径的helper重叠。请在契约要求租约覆盖实际发输入的进程树，使用可证明的进程树终止或执行端代际检查，回收确认前不允许B发新输入；扩展CU-08：A在鼠标down后退出、helper故意继续存活，验证B等待/拒绝、旧helper停止及必要释放完成后才接管。仅观测命名互斥体被OS回收不足以通过。

3. **明确deadline到达时进行中笔画的行为，而不仅是“不能启动新输入”。** [契约第45行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/implementation-contracts.md:45)约束输入启动，[CU-10第170行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/acceptance-test-matrix.md:170)测试模型超时与释放，但未覆盖deadline前已按下、deadline后仍在路径上移动。请固定两者之一：启动前保证剩余预算覆盖路径及释放；或deadline传播到helper，中途停止新增路径点后进入仅释放/审计宽限。不要把继续绘画算成释放收尾。将此边界并入CU-05/10，分别记录最后实际移动、deadline与mouseup时刻，断言不会在宽限期继续生产性输入，释放失败则保持未知效果且禁止自动续画。

4. **对齐Paint阶段进入门禁，并预注册图形目标判据。** [补充细则第48行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/execution-detail-supplement.md:48)要求P-B稳定通过后进入复杂任务归因；[PAINT-03第199行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/acceptance-test-matrix.md:199)仅要求前两级“可解释”，后者允许短线/矩形仍失败也进入正式A/B归因。请统一为：探索性失败复现随时可做，但正式复杂任务收益/模型归因须PAINT-01/02满足预注册的稳定性门槛，明确样本与准入规则。PAINT-03还应在执行前固定最低语义图形判据与盲判规则（允许低画质，但不是任意线条即“海绵宝宝”），分别记录有效落笔和完整目标，不在输出结果后临时解释什么算通过。

5. **为已确认的run计数字段失真增加直接回归断言。** [契约第17行](C:/Users/zhupu/Desktop/coolzhuagent/tmp/2026-09-21-integration-review/implementation-contracts.md:17)要求区分多类计数，但38例现有计数断言主要覆盖后端发送次数和前端工具call去重，未明确针对0.2.14的 `computer_use_runs.action_count/replan_count` 长期为0而步骤/终态有值的缺陷。无需增加第39例，可扩展CU-07或BASE-01：制作R6式3次尝试、1次实际发送、1次重规划的fixture，核对步骤、run摘要、终态JSON和统计API的权威来源与含义；旧冗余列若保留须正确回填，若废弃则所有消费者不得继续读取。再含partial/unknown和重连重放，保证尝试数、确认输入数、部分输入数、验收通过数互不混淆。该缺陷不能只靠CU通道执行正确而视为闭环。

上述均可合并到已有案例，不建议仅为增加数量拆出镜像测试。修订时同步Markdown与CSV；最终计划引用新的输入契约版本，旧审查报告的 `none/partial/complete/unknown` 仅保留为历史候选方案，不再与已冻结三维事实模型并列成为实施要求。
