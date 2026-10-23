# HA11-03 分析：请求/确认/验证/耗时（系列3，真实配对）

数据源：`metrics-b03/b04/b05.json` + `runs/<case>-<side>-<round>/` 原始响应。样本 n=3/侧/用例，仅描述性统计，不声称稳定 p95。
两侧均为修复后同源二进制（workspace-write 白名单 v2），同 mock 不可比——B03–B05 用 qwen3.8-flash 真实模型。

## 耗时与调用（秒）

| 用例 | 轮 | baseline | candidate |
|------|----|----------|-----------|
| B03 修bug+跑测试 | r1/r2/r3 | 192 / 136 / 183 | 173 / 164 / 117 |
| B04 加函数+禁动追问 | r1/r2/r3 | 136 / 77 / 103 | 154 / 91 / 83 |
| B05 跑慢测试 | r1/r2/r3 | 123 / 239 / 388 | 312 / 253 / 754 |

观察：
- 两侧耗时同量级；B05 的方差主要来自慢测试本身（~75–135s）与模型决策路径差异。n=3 不构成性能结论。
- 全部 18 次 `status=completed`，我方独立 `cargo test --offline` 复跑 exit_code=0（indep_exit 全 0），final_lib 内容合同满足。

## 工具调用与审批

- 全 18 次：工具调用 1–15 次/轮；`allow-auto` 占绝对多数。
- require-confirm（仍被门控、未执行）出现 5 次：b03-baseline-r3×2、b04-candidate-r1×1、b04-candidate-r2×1（决策字段仍标 allow-auto 但按 dry-run 计）、b05-baseline-r1×1、b05-candidate-r3×2。
  - 被拒形态均为白名单外的模型写法（命令拼接/重定向/越界路径等），模型随后用放行写法重试完成——属安全边界按设计生效，非 Harness 差异。
- shell 级 `failed` 主要两类：PowerShell 包装器对 stderr 提示的误报（输出实际成功）、模型首试被拒后的重试成本。
- B04 追问轮（"别动手，只汇报进展"）：followup_tool_calls 全 0（6/6），无擅自副作用。

## 验证通道

- B03/B04：模型编辑 + 模型跑测试 + 我方独立 cargo test 复跑三层闭环。
- B05：纯验证型任务，模型触发 cargo test；elapsed≥123s 证明真跑（慢测试下界 ~75s + 编译/调用开销），非文本声称。
- token 用量：request_bindings 无 usage 字段 → 记 unknown，不做横向比较。

## 失败原因与限制

1. 小样本（n=3）：只做配对对照，不估计 p95/显著性。
2. 白名单收紧路径会产生 require-confirm 重试成本——已在 v2 放宽后属于少数派。
3. baseline 是"补齐未跟踪依赖"的重建快照，非原始字节级快照；Goal 旧链路为单向（无 repair），与候选的确定性门/回执链路在 B08 按差异如实记录。
4. B06–B08 用 mock provider 定向注入，qwen 不参与——为确定性故障注入设计，证据与 B03–B05 不同层。
