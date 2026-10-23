# B06/B07 真实效果计数分析（fault/concurrency 配对证据）

证据目录: `evidence/HA-11/20261021/runs/`
两侧二进制同源补丁（workspace 开发命令白名单 v2），模型=mock-model（provider=custom @127.0.0.1:18080），同 effort/输入/工具集。

## B06 —— 断线/掉线后副作用（b06a 掉一次后恢复、b06b 全掉终态失败）

mock `b06` 模式：首轮返回 1 个 PowerShell 调用 `cargo test --offline`（cwd=隔离 ws），收到 tool 结果后的 post-tool 调用按剧本断流。

| 腿 | 侧 | mock 请求序列 | 运行终态 | 效果计数 |
|----|----|----------------|-----------|-----------|
| b06a-drop1 | baseline | respond → drop(has_tool_result) → respond | completed | cargo test 真实执行 1 次（req1→req2 ≈31s，fixture 已编译） |
| b06a-drop1 | candidate | respond → drop → respond | completed | 同上（elapsed 136s 含首轮编译） |
| b06b-dropall | baseline | respond → drop → drop → drop | failed | 效果已执行 1 次；post-tool 3 次尝试全部被断 |
| b06b-dropall | candidate | respond → drop → drop → drop | failed | 同上（elapsed 156s） |

判定要点：
- **效果真实发生且只发生一次**：每条链 req2 起 `has_tool_result=true`，证明 PowerShell 已在服务端执行完毕并把结果回传模型；断流发生在"结果已回传之后"的汇总阶段。
- **运行时结果不可知时如实终态**：b06b 三两侧均为 `failed` + `本地回退` 提示（"api failed after 3 attempts"），没有虚假通过；重试计数真实（attempt 1→3）。
- **无重放/无重复效果**：恢复重试是向 provider 重发同一上下文，不是重放工具调用；effect 文件与 target/ 产物无重复追加。
- 原始记录：`runs/b06a-drop1-{baseline,candidate}-1/`、`runs/b06b-dropall-{baseline,candidate}-1/`（response.json / mock-requests.jsonl / mock-scenario-final.json / meta.json / tool-calls-audit.json）。
- **服务端审计复核**：隔离 sqlite `tool_calls` 表按 run_id 查询，四次运行各恰好 1 条 PowerShell 记录且 status=completed——即断线/重试全程工具只真实执行一次，无重复效果、无复活。

## B07 —— 审批合并/撤销/重复（baseline/candidate 结构一致）

mock `b07` 模式：单轮返回 3 个 `PowerShell` 追加写调用：A1/A2 → `effect-A.txt`，B1 → `effect-B.txt`。workspace-write 下 3 条全部 `require-confirm` 入 pending（pending_after_turn=3，两侧一致）。

操作序列与实测效果（两侧 flow 文件一致，见 `runs/b07-{baseline,candidate}-1/b07-flow.json`）：

| 步骤 | 结果 | effect-A | effect-B |
|------|------|----------|----------|
| approve A1 | PowerShell status=ok | 1 | 0 |
| 再次 approve A1 | "未找到对应待审批的工具调用，可能已过期或已被处理" | **仍 1**（幂等，无重复执行） | 0 |
| reject B1 | ok | 1 | 0 |
| reject 后 approve B1 | 同上 not-found | 1 | **仍 0**（拒绝不可复活） |
| approve A2 | status=ok | 2 | 0 |

终态两侧相同：`effectA_final=2, effectB_final=0, pending_final=0`。
- unauthorized_effects=0（拒绝的 B1 从未执行）
- duplicate_effects=0（A1 重复审批不产生第二次执行）
- resurrected=0（被拒调用无法被再次批准拉起）
- `confirmed_twice:true` 表明审批走的是确认通道而非旁路。

## 已知干扰与诚实标注

- 第一次 b08 运行（`runs/b08-candidate-1/`）期间 mock 场景曾停留在 `b07`，导致阶段 Agent 收到 PowerShell 提案而非 b08 剧本的 write_file——该次运行仅作为归档，b08 判定以 b08b 系列为准。
- b06 的"效果"是命令执行（cargo test），可观测产物为 ws 内 target/ 与运行耗时；response.json 的 messages 只含 user/assistant 两条，工具执行证据以 mock 侧 `has_tool_result=true` + 服务端 TOOL-GATE/TOOL-EXEC 日志为准（已在 meta/reqlog 中留痕）。
