# 验收证据与状态更新规范

状态唯一来源是 [state.json](state.json)，行为合同唯一来源是 [ACCEPTANCE.md](ACCEPTANCE.md)。本目录用于项目实施交接，不是新增的产品审批系统，也不会在每次模型调用前运行。

## 证据最小内容

每次有价值的验证保留一份回执；同一回执可以支持多个条目。路径相对仓库根目录，文件保存在 `docs/plans/harness-alignment/evidence/<工作包>/`。`tmp/` 适合中间输出，不能作为已通过条目的唯一长期证据。

| 字段 | 要求 |
|---|---|
| `id` / `kind` / `result` | 稳定 ID；`test`、`integration`、`benchmark`、`review` 或 `build`；`pass`/`fail`/`unknown` |
| `observed` | 实际观察及对应验收结论；不写“应该通过” |
| `method` | 真实命令或具体复核步骤、工作目录、执行者/会话、开始与结束 UTC；命令记录 exit_code，测试记录 tests_run |
| `artifacts` | 原始输出、事件/数据、检查报告的相对路径及 SHA-256；不只保存模型结论 |
| `inputs` | 与证据适用性有关的源码/测试/配置等文件路径及 SHA-256；说明相关范围；不要求每步扫描全仓 |
| `limitations` | mock/实链路、未覆盖范围、环境与模型身份不确定性；无局限可留空数组 |

命令退出 0 不自动证明行为正确；测试须匹配至少 1 项且断言对应合同。基线失败也可以是合格的基线采集证据，需明确 `observed` 中证明的是“成功采集真实结果”，不能改写失败为功能通过。构建记录证明可构建，单独不能关闭行为条目。

没有命令的源码复核可以用 `kind: review`，`exit_code/tests_run` 为 null。它只能支持源码/范围/接替等复核合同；撤销竞态、实际副作用、并发、运行状态与真实性能需要执行证据。人工复核原始数据是否满足合同是必须工作，脚本只核对元数据与完整性，不替代行为判断。

源变化后，先识别是否相关。若只有无关范围变化，新增复核证据，记录所比较差异、旧回执及仍适用理由，保存当前输入哈希；原回执和旧哈希保持不动。若相关或不确定则复测；不能直接覆盖旧证据的 input 哈希。实验结果受外部条件影响时，哈希相同也不自动有效。

## 状态字段

`tasks[].status`：`pending`、`in_progress`、`implemented`、`accepted`、`blocked`、`needs_recheck`；延期候选为 `deferred`。`criteria[].status`：`pending`、`running`、`passed`、`failed`、`blocked`、`needs_recheck`。失败/阻塞要写 `note`，通过项填写 `evidence_ids`。`accepted` 要求依赖全部 accepted、条目全部 passed，不能有未知效果被掩盖。

`handoff` 保存 checkpoint、当前 owner、最近具体结果、下一动作、在途命令和未知效果。长命令记录确切命令、PID/会话 ID、开始时间、工作目录、日志路径和操作是否有副作用。普通本地源码读取无需逐条记入。

更新采用先保存证据、再更新状态的顺序。保存前重读 state 并确认 `updated_at_utc` 未被其他执行者修改；先写 `state.next.json`、解析确认、保留 `state.previous.json` 后替换。每个工作包结束检查一次即可，不增加每步审核模型。

## 证据对象示意

下列是填写格式，不是已执行记录。不要原样登记成通过。

```json
{
  "id": "E-HAxx-实际运行编号",
  "kind": "test",
  "result": "pass",
  "observed": "实际断言、计数与结论",
  "method": {
    "command": "实际执行的完整命令；源码复核时可为 null",
    "procedure": "验证怎样对应合同",
    "cwd": "D:/coolzhuagent",
    "actor": "实际执行会话/执行者",
    "started_at_utc": "实际开始时间",
    "ended_at_utc": "实际结束时间",
    "exit_code": 0,
    "tests_run": 1
  },
  "artifacts": [{"path": "docs/plans/harness-alignment/evidence/HA-xx/实际日志.txt", "sha256": "实际文件哈希"}],
  "inputs": [{"path": "实际相关源码路径", "sha256": "执行时相关内容哈希"}],
  "limitations": []
}
```

对不再适用的旧证据保持原记录；将相应条目标记 `needs_recheck` 并移除其“当前有效”关联，保留说明。校验器只把当前 passed 条目的证据视为有效声明，历史失败、过期证据可归档而不妨碍继续工作。

## 检查工具能证明什么

`check_plan.py` 不调用模型、不开启业务进程、不执行证据中的命令。它核对任务/条目映射、依赖无环、通过/关闭是否附证据、文件哈希、相关输入哈希及命令基本结果。`--complete` 额外要求全部 13 项/60 条关闭、无在途操作和未知效果。

哈希变化只表明证据需重新判断，不能机械等同于重跑全部测试。通过结构检查也不能证明测试设计足够、记录未造假或运行时所有情况都正确；验收执行者仍须审查断言与原始回执。
