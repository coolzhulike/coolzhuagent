# HA-11/HA-12 收口最终报告（2026-10-23）

## 结论

- **HA-11 全部 4 条验收条目通过**：B01–B05 各 3 组真实配对 + B06–B08 故障/审批/验证合同真实链路，全部满足合同判定。
- **HA-12 全部 4 条验收条目通过**：恢复/在途/负例演练留证，本报告即最终审查。
- T08（D08-A/D08-B）保持 `deferred`，非必做；下一会话无隐含必做工作。

## 必做条目证据对照

| 条目 | 证据 | 要点 |
|------|------|------|
| HA11-01 | E-HA11-B01/B02-20261020 + B03/B04/B05-20261023 | 15 组配对；qwen3.8-flash 真实模型；同 prompt/effort/工具；18 次新系列全 completed + 独立 cargo test exit 0；旧误拦截系列归档不作数 |
| HA11-02 | E-HA11-B06/B07/B08-20261023 | 断线恰好一次执行（sqlite 审计）；审批幂等/拒绝不可复活；回执复用 0 重复执行、digest 失效重跑、畸形合同计划期拒绝、缺省 cwd 修复验证 |
| HA11-03 | E-HA11-ANALYSIS-20261023 | 耗时/确认/验证通道分析；token unknown 如实标注；n=3 不声称 p95 |
| HA11-04 | E-HA11-ROLLOUT-20261023 + E-HA10-CURRENT-BUILD/DIGEST-20261023 | 受影响面 5 文件；双快照互切回退演练；live gate 双侧实测保留；13 项定向 + 125 项回归 + 构建全过 |
| HA12-01 | E-HA12-RECOVERY-20261023 | 新上下文只读恢复演练 |
| HA12-02 | E-HA12-INFLIGHT-20261023 | 在途/未知效果 4 例注入全拒；真实中断处置记录 |
| HA12-03 | E-HA12-SELFCHECK-20261023 | --self-test 11 负例全拒 |
| HA12-04 | 本报告 + state 审查 | — |

## HA-10 失配证据处置

`goal_verification.rs` 修复致 4 条旧证据输入失配 → 新增 20261023 复验证记录（构建/125 项回归/源码复核/13 项定向），criteria 重指向新 ID；旧记录原样归档，未改哈希。

## 缺陷与诚实声明

1. 修复两个真实产品缺陷：workspace-write 不暴露 shell（开发命令白名单 v2）、Command 合同空 cwd 恒 Invalid。
2. 环境异常如实记录：两轮隔离服务僵死（sqlite 写事务嫌疑）、56 个 0 字节构建残件（定点清理）。
3. 基线为重建快照；候选相对基线新增回执复用机制——差异已在 B08 双侧如实记录而非掩盖。
4. B03–B05 系列 1（窄白名单期）因模型未跑成测试不计入，归档于 `runs-series1-before-output-suffix-fix/`。
5. token 用量：服务侧无 usage 输出，报告为 unknown，不做横向比较。

## 收口动作

- 隔离服务 8766/8767 与 mock 18080 在本报告后关停（证据已入 `docs/.../evidence/`，tmp 仅为中间产物）。
- `state.json`：HA-11→accepted、HA-12→accepted，`active_task=null`、`next_task=null`、在途/未知效果清空。
- 收口后运行 `python docs/plans/harness-alignment/check_plan.py --complete`。
